//! Resolving a run's row once, ahead of a loop, for an owner the loop does not change.
//!
//! # What this removes, and what it does not
//!
//! [`super::run_read`] and [`super::run_write`] look the row up from the owner on
//! every access: header, rows, row, then start / length / kind. For an owner that
//! is the same value on every pass of a loop that chain is the same answer every
//! time, and it was measured at ~2 ns of a ~3 ns access.
//!
//! # Why this is NOT the withdrawn raw-address hoist
//!
//! `rts_cranelift::ir::inst::Terminator::RunRead` records an element-address load
//! that was hoisted out of a loop and withdrawn on 2026-08-29: nothing said when the
//! address stopped being true, so the collector freed the run under the loop. What is
//! hoisted here is guarded by the one fact that does say: the table's GENERATION
//! ([`RunTable::GENERATION_OFFSET`]), which the client bumps on every change to any
//! row (publish, freeze, retract, disarm, reset). Each access loads the generation and
//! compares it with the one the row was resolved under; equal means no row has changed
//! since, so the hoisted start / length / kind ARE the row. Not equal -- a call in the
//! loop that grew the array, detached the buffer, froze it or freed its cell -- sends
//! that access to the full walk, which is the program as it was. No claim is made about
//! what the loop body contains, so there is no invalidation set to get wrong.
//!
//! The owner is still an operand of every access it was an operand of (the guard that
//! narrows it stays where it was), so it stays live through the loop exactly as before.
//!
//! # What qualifies
//!
//! A loop (a natural loop: a header that dominates the source of a back edge) with a
//! single entry edge from a block that ends in a plain jump to the header, in a
//! function with no protected region and no suspension. A `RunRead` / `RunWrite` in it
//! whose owner is defined outside the loop, or is the narrowing of a generic value
//! defined outside it by a guard that is that block's only entry. Anything else keeps
//! the full walk.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::value;
use crate::ir::{BlockId, Function, Terminator, ValueId, ValueOrigin};
use crate::mem::RunTable;
use crate::repr::{RefKind, Repr};

/// Which values to resolve in which preheader, and which sites use them.
#[derive(Default)]
pub(super) struct HoistPlan {
    /// Preheader block -> the owner roots to resolve at the end of it.
    pub(super) pre: HashMap<BlockId, Vec<ValueId>>,
    /// A `RunRead` / `RunWrite` block -> (the preheader, the root) it reads.
    pub(super) sites: HashMap<BlockId, (BlockId, ValueId)>,
}

/// A row as resolved in a preheader, with the generation it was resolved under.
#[derive(Clone, Copy)]
pub(super) struct HoistedRow {
    pub(super) generation: Value,
    pub(super) start: Value,
    pub(super) length: Value,
    pub(super) writable: Value,
    pub(super) kind: Value,
}

struct Loop {
    body: HashSet<usize>,
    preheader: usize,
}

/// Decides, for one function, which accesses read a hoisted row.
pub(super) fn plan(func: &Function) -> HoistPlan {
    let mut out = HoistPlan::default();
    if !func.regions.is_empty() || func.signature.may_suspend {
        return out;
    }
    let n = func.blocks().count();
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut has_site = false;
    for (id, data) in func.blocks() {
        if let Some(t) = &data.terminator {
            if matches!(t, Terminator::RunRead { .. } | Terminator::RunWrite { .. }) {
                has_site = true;
            }
            for s in t.successors() {
                succs[id.index()].push(s.index());
                preds[s.index()].push(id.index());
            }
        }
    }
    if !has_site {
        return out;
    }

    // Reverse post-order of what the entry reaches, then dominators (Cooper, Harvey,
    // Kennedy).
    let order: Vec<usize> = func.control_order().into_iter().map(|b| b.index()).collect();
    let mut reach = vec![false; n];
    let mut stack = vec![func.entry.index()];
    while let Some(b) = stack.pop() {
        if std::mem::replace(&mut reach[b], true) {
            continue;
        }
        stack.extend(succs[b].iter().copied());
    }
    let rpo: Vec<usize> = order.into_iter().filter(|&b| reach[b]).collect();
    let mut position = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        position[b] = i;
    }
    let entry = func.entry.index();
    if rpo.first() != Some(&entry) {
        return out;
    }
    let mut idom: Vec<Option<usize>> = vec![None; n];
    idom[entry] = Some(entry);
    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new: Option<usize> = None;
            for &p in &preds[b] {
                if !reach[p] || idom[p].is_none() {
                    continue;
                }
                new = Some(match new {
                    None => p,
                    Some(cur) => {
                        let (mut x, mut y) = (p, cur);
                        while x != y {
                            while position[x] > position[y] {
                                x = idom[x].expect("processed");
                            }
                            while position[y] > position[x] {
                                y = idom[y].expect("processed");
                            }
                        }
                        x
                    }
                });
            }
            if new != idom[b] {
                idom[b] = new;
                changed = true;
            }
        }
    }
    let dominates = |a: usize, mut b: usize| -> bool {
        loop {
            if a == b {
                return true;
            }
            match idom[b] {
                Some(up) if up != b => b = up,
                _ => return false,
            }
        }
    };

    // Natural loops, one per header.
    let mut latches: HashMap<usize, Vec<usize>> = HashMap::new();
    for &u in &rpo {
        for &h in &succs[u] {
            if reach[h] && dominates(h, u) {
                latches.entry(h).or_default().push(u);
            }
        }
    }
    let mut loops: Vec<Loop> = Vec::new();
    let mut headers: Vec<usize> = latches.keys().copied().collect();
    headers.sort_unstable();
    for h in headers {
        let mut body: HashSet<usize> = HashSet::new();
        body.insert(h);
        let mut work = latches[&h].clone();
        while let Some(x) = work.pop() {
            if body.insert(x) {
                work.extend(preds[x].iter().copied().filter(|&p| reach[p]));
            }
        }
        let outside: Vec<usize> = preds[h]
            .iter()
            .copied()
            .filter(|p| reach[*p] && !body.contains(p))
            .collect();
        // Exactly one way in, and every other way to the header is a back edge.
        let unreachable_preds = preds[h].iter().any(|p| !reach[*p]);
        if outside.len() != 1 || unreachable_preds {
            continue;
        }
        let pre = outside[0];
        if body.contains(&pre) || preds[h].iter().filter(|&&p| p == pre).count() != 1 {
            continue;
        }
        let ends_in_jump = matches!(
            func.block(BlockId(pre as u32)).and_then(|b| b.terminator.as_ref()),
            Some(Terminator::Jump(call)) if call.block.index() == h
        );
        if !ends_in_jump || !body.iter().all(|&b| dominates(h, b)) {
            continue;
        }
        loops.push(Loop {
            body,
            preheader: pre,
        });
    }
    if loops.is_empty() {
        return out;
    }
    // Outermost first: the longest-lived row is resolved the fewest times.
    loops.sort_by_key(|l| std::cmp::Reverse(l.body.len()));

    // Where each value is defined.
    let mut inst_block: HashMap<usize, usize> = HashMap::new();
    for (id, data) in func.blocks() {
        for inst in &data.insts {
            inst_block.insert(inst.index(), id.index());
        }
    }
    let def_block = |v: ValueId| -> Option<usize> {
        match func.value(v)?.origin {
            ValueOrigin::BlockParam(b) => Some(b.index()),
            ValueOrigin::InstResult(i) => inst_block.get(&i.index()).copied(),
        }
    };

    for (id, data) in func.blocks() {
        let site = id.index();
        let owner = match &data.terminator {
            Some(Terminator::RunRead { owner, .. }) | Some(Terminator::RunWrite { owner, .. }) => {
                *owner
            }
            _ => continue,
        };
        if !reach[site] {
            continue;
        }
        for l in loops.iter().filter(|l| l.body.contains(&site)) {
            let root = root_of(func, owner, &def_block, &l.body, &preds);
            let Some(root) = root else { continue };
            let pre = BlockId(l.preheader as u32);
            let list = out.pre.entry(pre).or_default();
            if !list.contains(&root) {
                list.push(root);
            }
            out.sites.insert(id, (pre, root));
            break;
        }
    }
    out
}

/// The value to resolve a row from, if the owner does not change in the loop.
fn root_of(
    func: &Function,
    owner: ValueId,
    def_block: &dyn Fn(ValueId) -> Option<usize>,
    body: &HashSet<usize>,
    preds: &[Vec<usize>],
) -> Option<ValueId> {
    let outside = |v: ValueId| def_block(v).is_some_and(|b| !body.contains(&b));
    if outside(owner) && matches!(func.repr_of(owner), Repr::Ref(_)) {
        return Some(owner);
    }
    // The narrowing of a generic value by a guard that is the block's only entry.
    let ValueOrigin::BlockParam(block) = func.value(owner)?.origin else {
        return None;
    };
    let data = func.block(block)?;
    if block == func.entry || data.params.len() != 1 || data.params[0] != owner {
        return None;
    }
    let from = &preds[block.index()];
    if from.len() != 1 {
        return None;
    }
    let guard = func.block(BlockId(from[0] as u32))?.terminator.as_ref()?;
    match guard {
        Terminator::Guard {
            input,
            expect: Repr::Ref(RefKind::Opaque),
            ok,
            fail,
        } if ok.block == block && fail.block != block && ok.args.is_empty() => {
            if outside(*input) && func.repr_of(*input) == Repr::Tagged {
                Some(*input)
            } else {
                None
            }
        }
        _ => None,
    }
}

impl Body<'_> {
    /// Resolves the rows a loop entered from `block` will read, then continues.
    ///
    /// Emitted at the end of the preheader, before its jump. A root that is not a
    /// reference, has no row yet, or when there is no table at all, resolves to an
    /// all-zero row: its length is zero, so no access ever takes the hoisted path,
    /// and the generation is still read so nothing about it is special.
    pub(super) fn emit_hoisted_rows(&mut self, builder: &mut FunctionBuilder, block: BlockId) {
        let Some(roots) = self.hoist.pre.get(&block).cloned() else {
            return;
        };
        let Some(heap) = self.heap.as_ref() else {
            return;
        };
        let Some(table) = heap.bases.runs() else {
            return;
        };
        for root in roots {
            let flags = MemFlags::trusted();
            let heap = self.heap.as_ref().expect("checked above");
            let header = super::memory::baked(builder, heap, super::memory::Baked::Runs, table);
            let generation = builder
                .ins()
                .load(types::I64, flags, header, RunTable::GENERATION_OFFSET);
            let x = self.value(root);
            let (cell, usable) = if self.func.repr_of(root) == Repr::Tagged {
                let held = value::test(builder, x, Repr::Ref(RefKind::Opaque))
                    .expect("a reference test");
                let cell = value::narrow(builder, x, Repr::Ref(RefKind::Opaque))
                    .expect("a reference narrowing");
                let held = builder.ins().icmp_imm(IntCC::NotEqual, held, 0);
                (cell, Some(held))
            } else {
                (x, None)
            };
            let count = builder
                .ins()
                .load(types::I64, flags, header, RunTable::COUNT_OFFSET);
            let in_table = builder.ins().icmp(IntCC::UnsignedLessThan, cell, count);
            let ok = match usable {
                Some(held) => builder.ins().band(held, in_table),
                None => in_table,
            };

            let resolve = builder.create_block();
            let joined = builder.create_block();
            let params: Vec<Value> = (0..4)
                .map(|_| builder.append_block_param(joined, types::I64))
                .collect();
            let zero = builder.ins().iconst(types::I64, 0);
            let none = vec![BlockArg::Value(zero); 4];
            builder.ins().brif(ok, resolve, &[], joined, &none);

            builder.switch_to_block(resolve);
            let rows = builder
                .ins()
                .load(types::I64, flags, header, RunTable::ROWS_OFFSET);
            let offset = builder.ins().imul_imm(cell, RunTable::ROW_BYTES);
            let row = builder.ins().iadd(rows, offset);
            let start = builder
                .ins()
                .load(types::I64, flags, row, RunTable::ROW_START_OFFSET);
            let length = builder
                .ins()
                .load(types::I64, flags, row, RunTable::ROW_LENGTH_OFFSET);
            let writable = builder
                .ins()
                .load(types::I64, flags, row, RunTable::ROW_WRITE_LENGTH_OFFSET);
            let kind = builder
                .ins()
                .load(types::I64, flags, row, RunTable::ROW_KIND_OFFSET);
            builder.ins().jump(
                joined,
                &[
                    BlockArg::Value(start),
                    BlockArg::Value(length),
                    BlockArg::Value(writable),
                    BlockArg::Value(kind),
                ],
            );

            builder.switch_to_block(joined);
            self.rows.insert(
                (block, root),
                HoistedRow {
                    generation,
                    start: params[0],
                    length: params[1],
                    writable: params[2],
                    kind: params[3],
                },
            );
        }
    }

    /// The hoisted row an access reads, if its loop resolved one.
    pub(super) fn hoisted_row(&self, site: BlockId) -> Option<HoistedRow> {
        let key = self.hoist.sites.get(&site)?;
        self.rows.get(key).copied()
    }
}

