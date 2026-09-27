//! A constant written inside a loop, moved to the entry block.
//!
//! What a constant costs is the machine's, and a language may make one a call: a heap
//! value its runtime holds by index is not bits. Such a constant inside a loop is a call
//! on every pass for an answer that cannot change. Moved to the entry block, it is asked
//! once per activation. The entry dominates every block, so the value reaches every
//! site that read the one it replaces.
//!
//! # Which constants, and why the language says
//!
//! Only the language knows which of its constants cost anything, so it passes the
//! predicate and this pass does not guess. Hoisting one that costs nothing is not free:
//! the value is then live from the entry to its last use, across every call in
//! between. The running emitter measured that cost with every literal of a body hoisted
//! to its entry: a function of 4 000 literals took 1.7 GB to compile. So only a constant
//! inside a LOOP moves, where the call it saves is paid per pass rather than once.
//!
//! # Not in a body that parks
//!
//! A suspending body is rewritten around every suspension, and a value defined at the
//! entry and read after one is not the value it was. The running emitter learned this
//! at the cost of 37 generator files, and this pass is gated the same way.
//!
//! # What counts as a loop
//!
//! A block on a cycle of ordinary edges, found as the strongly connected components of
//! the successor graph. An exception edge is not an edge here, so a handler that jumps
//! back into its loop is not counted as inside it. That only means a constant in the
//! handler stays where it is.

use std::collections::{BTreeMap, BTreeSet};

use crate::cfg::{BlockId, Const, Func, InstId, Op, ValueId};

/// Moves every constant `costly` accepts, from a block on a cycle to the end of the
/// entry block, one per distinct constant. Returns how many were moved.
pub fn hoist_loop_constants(func: &mut Func, costly: impl Fn(&Const) -> bool) -> usize {
    if func.may_suspend {
        return 0;
    }
    let looping = on_a_cycle(func);
    let mut hoisted: Vec<(Const, ValueId)> = Vec::new();
    let mut moved: Vec<InstId> = Vec::new();
    let mut dropped = BTreeSet::new();
    let mut replace = BTreeMap::new();
    for block in looping {
        for &inst in &func.blocks[block.0 as usize].insts {
            let Op::Const(held) = func.insts[inst.0 as usize].op else {
                continue;
            };
            if !costly(&held) {
                continue;
            }
            let result = func.insts[inst.0 as usize].result;
            // COMPARED BY BITS, so `0.0` and `-0.0` stay two constants.
            match hoisted.iter().find(|(seen, _)| same(seen, &held)) {
                Some((_, first)) => {
                    replace.insert(result, *first);
                    dropped.insert(inst);
                }
                None => {
                    hoisted.push((held, result));
                    moved.push(inst);
                    dropped.insert(inst);
                }
            }
        }
    }
    if dropped.is_empty() {
        return 0;
    }
    super::unlist(func, &dropped);
    let entry = func.entry();
    func.blocks[entry.0 as usize].insts.extend(moved.iter().copied());
    super::rewrite_uses(func, &replace);
    dropped.len()
}

fn same(a: &Const, b: &Const) -> bool {
    match (a, b) {
        (Const::Float(a), Const::Float(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

/// The blocks on a cycle of the successor graph, in block order: Tarjan's strongly
/// connected components, iteratively, so a large function does not recurse.
fn on_a_cycle(func: &Func) -> Vec<BlockId> {
    let count = func.blocks.len();
    let successors: Vec<Vec<usize>> = func
        .blocks
        .iter()
        .map(|block| {
            block
                .terminator
                .as_ref()
                .map(|end| end.successors().iter().map(|held| held.0 as usize).collect())
                .unwrap_or_default()
        })
        .collect();
    let mut index = vec![usize::MAX; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut next = 0usize;
    let mut cyclic = vec![false; count];
    for root in 0..count {
        if index[root] != usize::MAX {
            continue;
        }
        // Each frame: the block and how many of its successors have been visited.
        let mut frames = vec![(root, 0usize)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (block, ref mut at)) = frames.last_mut() {
            if let Some(&target) = successors[block].get(*at) {
                *at += 1;
                if index[target] == usize::MAX {
                    index[target] = next;
                    low[target] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    frames.push((target, 0));
                } else if on_stack[target] {
                    low[block] = low[block].min(index[target]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[block]);
            }
            if low[block] == index[block] {
                let mut members = Vec::new();
                while let Some(held) = stack.pop() {
                    on_stack[held] = false;
                    members.push(held);
                    if held == block {
                        break;
                    }
                }
                let looped = members.len() > 1 || successors[block].contains(&block);
                for member in members {
                    cyclic[member] = looped;
                }
            }
        }
    }
    (0..count)
        .filter(|at| cyclic[*at])
        .map(|at| BlockId(at as u32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{FuncBuilder, Terminator};
    use crate::effect::Effect;
    use crate::guard::Tier;
    use crate::verify::verify;

    fn declared(build: &mut FuncBuilder, which: u32) -> ValueId {
        build.push(Op::Const(Const::Declared(which)), Effect::PURE, Default::default())
    }

    /// `entry -> header <-> body`, `header -> exit`, with two copies of one declared
    /// constant and one of another in the body, and one in the exit.
    fn looped() -> Func {
        let mut build = FuncBuilder::new(Tier::Generic);
        let entry = build.current();
        let flag = build.param(entry);
        let header = build.block();
        let body = build.block();
        let exit = build.block();
        build.end(Terminator::Jump {
            target: header,
            args: Vec::new(),
        });
        build.switch_to(header);
        build.end(Terminator::Branch {
            condition: flag,
            then_block: body,
            then_args: Vec::new(),
            else_block: exit,
            else_args: Vec::new(),
        });
        build.switch_to(body);
        declared(&mut build, 7);
        declared(&mut build, 7);
        declared(&mut build, 8);
        build.end(Terminator::Jump {
            target: header,
            args: Vec::new(),
        });
        build.switch_to(exit);
        let once = declared(&mut build, 7);
        build.end(Terminator::Return(Some(once)));
        build.finish()
    }

    #[test]
    fn a_constant_inside_a_loop_moves_to_the_entry_once() {
        let mut func = looped();
        assert_eq!(hoist_loop_constants(&mut func, |_| true), 3);
        assert_eq!(verify(&func), Ok(()));
        // Two distinct constants at the entry, none left in the body, and the one
        // outside the loop where it was.
        assert_eq!(func.blocks[0].insts.len(), 2);
        assert!(func.blocks[2].insts.is_empty());
        assert_eq!(func.blocks[3].insts.len(), 1);
    }

    #[test]
    fn the_language_decides_which_constants_move() {
        let mut func = looped();
        assert_eq!(
            hoist_loop_constants(&mut func, |held| *held == Const::Declared(8)),
            1
        );
        assert_eq!(func.blocks[2].insts.len(), 2);
    }

    #[test]
    fn a_body_that_parks_keeps_its_constants() {
        let mut func = looped();
        func.may_suspend = true;
        assert_eq!(hoist_loop_constants(&mut func, |_| true), 0);
    }
}
