//! Scalar replacement: a literal that never has to become an object.
//!
//! # What it is, and whose it was first
//!
//! `const o = { x: i, y: i }; a += o.x` allocates an object, writes two properties
//! and reads one back -- and nothing else ever sees `o`. The running emitter answers
//! that with `emit/escape.rs`, which turns such a literal into one local per property
//! before emitting anything. `bench/analytic.ts` measured what its absence costs here:
//! `object literal 2` at 65.8 ns through the MIR stage against 2.6 ns on the running
//! emitter, `array literal 4` at 109 ns against 3.6 ns (2026-09-25, release).
//!
//! Here it is a pass over the GRAPH rather than over the tree, which is the point of
//! having one: SSA already says every place the object's value goes, so "does it
//! escape" is a scan of its uses instead of a walk of the syntax with the language's
//! scope rules restated.
//!
//! # What is replaced
//!
//! A `NewObject` whose every use is a `FieldRead` or `FieldWrite` of a key the
//! compiler fixed, and a `NewArray` whose every use is an `IndexRead` at a constant
//! index inside the literal. Each read becomes the value last written under its key,
//! and the allocation and the accesses leave the graph.
//!
//! # Why each condition, and what it would cost to drop it
//!
//! - **Every use is an access by a fixed key.** A call, a store into another object,
//!   a return, a block argument, a guard -- any other use lets the object be seen, and
//!   then it has to exist.
//! - **A read of a key the literal does not own is refused.** It would find the
//!   prototype's property, which is not a value this pass holds.
//! - **With a WRITE, every use sits in the literal's own block.** Straight-line code
//!   has one order, so "the value last written" is a fact of position; across blocks it
//!   would be a merge, which is SSA construction and a different pass.
//! - **An own data property of a fresh literal has no getter or setter** -- an
//!   accessor in a literal is built by the running emitter in a helper, so it never
//!   reaches `NewObject` -- which is what makes the read the value and the write a
//!   rebind.

use std::collections::{BTreeMap, BTreeSet};

use rts_mir::cfg::{BlockId, Const, Func, InstId, Op, ValueId};

use crate::domain::{Js, JsConst, JsPrim};
use crate::names::Name;

/// What the pass removed, counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replaced {
    /// Allocations taken out of the graph.
    pub allocations: usize,
    /// Accesses to them turned into the values they would have answered.
    pub accesses: usize,
}

/// A key or an index an access names, when the compiler fixed it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Key(Name),
    Index(u64),
}

/// Replaces every literal of the two shapes above, and says how many.
pub fn replace_scalars(func: &mut Func, domain: &Js) -> Replaced {
    let mut out = Replaced::default();
    let mut placed: BTreeMap<InstId, (BlockId, usize)> = BTreeMap::new();
    let mut defined: BTreeMap<ValueId, InstId> = BTreeMap::new();
    for block in func.block_ids() {
        for (position, inst) in func.block(block).insts.iter().enumerate() {
            placed.insert(*inst, (block, position));
            defined.insert(func.inst(*inst).result, *inst);
        }
    }
    let literals: Vec<InstId> = placed
        .keys()
        .copied()
        .filter(|inst| {
            matches!(&func.inst(*inst).op, Op::Prim { prim, .. }
                if matches!(domain.meaning(*prim), Some(JsPrim::NewObject | JsPrim::NewArray)))
        })
        .collect();

    let mut replace: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut dropped: BTreeSet<InstId> = BTreeSet::new();
    for literal in literals {
        if let Some((values, accesses)) = plan(func, domain, &placed, &defined, literal) {
            out.allocations += 1;
            out.accesses += accesses.len();
            replace.extend(values);
            dropped.insert(literal);
            dropped.extend(accesses);
        }
    }
    if !dropped.is_empty() {
        rts_mir::passes::unlist(func, &dropped);
        rts_mir::passes::rewrite_uses(func, &replace);
    }
    out
}

/// What replacing one literal takes -- the value each access becomes, and the
/// accesses -- or `None` where it cannot be replaced.
fn plan(
    func: &Func,
    domain: &Js,
    placed: &BTreeMap<InstId, (BlockId, usize)>,
    defined: &BTreeMap<ValueId, InstId>,
    literal: InstId,
) -> Option<(BTreeMap<ValueId, ValueId>, Vec<InstId>)> {
    let object = func.inst(literal).result;
    let Op::Prim { prim, args } = &func.inst(literal).op else {
        return None;
    };
    let slot_of = |value: ValueId| -> Option<Slot> {
        let Op::Const(held) = &func.inst(*defined.get(&value)?).op else {
            return None;
        };
        match held {
            Const::Declared(index) => match domain.declared(*index)? {
                JsConst::Key(name) => Some(Slot::Key(*name)),
                _ => None,
            },
            Const::Int(at) if *at >= 0 => Some(Slot::Index(*at as u64)),
            Const::Float(at) if *at >= 0.0 && at.fract() == 0.0 && *at < 9.0e15 => {
                Some(Slot::Index(*at as u64))
            }
            _ => None,
        }
    };

    // WHAT THE LITERAL HOLDS, by slot. A repeated key keeps the last value, which is
    // what defining the pairs in order leaves.
    let array = domain.meaning(*prim) == Some(JsPrim::NewArray);
    let mut held: BTreeMap<Slot, ValueId> = BTreeMap::new();
    if array {
        for (at, element) in args.iter().enumerate() {
            // A HOLE is not an element, and reading it reaches the prototype.
            if let Some(JsConst::Hole) = defined
                .get(element)
                .and_then(|inst| match &func.inst(*inst).op {
                    Op::Const(Const::Declared(index)) => domain.declared(*index),
                    _ => None,
                })
            {
                return None;
            }
            held.insert(Slot::Index(at as u64), *element);
        }
    } else {
        for pair in args.chunks(2) {
            let [key, value] = pair else { return None };
            held.insert(slot_of(*key)?, *value);
        }
    }

    // EVERY USE, and each has to be an access this pass can answer.
    let mut accesses: Vec<(usize, InstId, Slot, Option<ValueId>)> = Vec::new();
    let (home, _) = placed.get(&literal)?;
    let mut writes = false;
    for (inst, (block, position)) in placed {
        let reads = func.reads(*inst);
        if !reads.contains(&object) || *inst == literal {
            continue;
        }
        let Op::Prim { prim, args } = &func.inst(*inst).op else {
            return None;
        };
        let meaning = domain.meaning(*prim)?;
        let access = match (meaning, args.as_slice(), array) {
            (JsPrim::FieldRead, [on, key], false) if *on == object && *key != object => {
                (slot_of(*key)?, None)
            }
            (JsPrim::FieldWrite, [on, key, value], false)
                if *on == object && *key != object && *value != object =>
            {
                writes = true;
                (slot_of(*key)?, Some(*value))
            }
            (JsPrim::IndexRead, [on, index], true) if *on == object && *index != object => {
                match slot_of(*index)? {
                    Slot::Index(at) => (Slot::Index(at), None),
                    Slot::Key(_) => return None,
                }
            }
            _ => return None,
        };
        let order = if block == home { *position } else { usize::MAX };
        accesses.push((order, *inst, access.0, access.1));
    }
    // A TERMINATOR that hands the object on lets it be seen.
    for block in func.block_ids() {
        if func.block(block).terminator.as_ref().is_some_and(|end| hands_on(end, object)) {
            return None;
        }
    }
    if writes && accesses.iter().any(|(order, ..)| *order == usize::MAX) {
        return None;
    }

    // IN ORDER, so a read answers the value last written before it.
    accesses.sort_by_key(|(order, inst, ..)| (*order, *inst));
    let mut replace = BTreeMap::new();
    let mut gone = Vec::with_capacity(accesses.len());
    for (_, inst, slot, written) in accesses {
        let result = func.inst(inst).result;
        match written {
            Some(value) => {
                held.insert(slot, value);
                // A write answers the value written.
                replace.insert(result, value);
            }
            None => {
                replace.insert(result, *held.get(&slot)?);
            }
        }
        gone.push(inst);
    }
    Some((replace, gone))
}

/// Whether a terminator reads `value`.
fn hands_on(end: &rts_mir::Terminator, value: ValueId) -> bool {
    use rts_mir::Terminator;
    match end {
        Terminator::Jump { args, .. } => args.contains(&value),
        Terminator::Branch {
            condition,
            then_args,
            else_args,
            ..
        } => *condition == value || then_args.contains(&value) || else_args.contains(&value),
        Terminator::Return(Some(held)) | Terminator::Raise(held) => *held == value,
        Terminator::Return(None)
        | Terminator::Fall(_)
        | Terminator::CleanupDone
        | Terminator::Unreachable => false,
    }
}

#[cfg(test)]
#[path = "scalar_tests.rs"]
mod tests;
