//! Types by abstract interpretation, to a fixed point.
//!
//! The pass that could not exist before this crate did. A syntax-directed
//! traversal can compute a type for `x + 1`; it cannot compute one for `x` inside
//! a loop, because the answer depends on the back edge and the back edge has not
//! been walked yet. Iterating until nothing changes is the whole difference, and
//! it needs a graph to iterate over.
//!
//! # Termination
//!
//! A worklist over blocks, joining into each block's parameters from every
//! predecessor, stopping when no block's answer changed. It terminates because
//! [`Domain::join`] never narrows: each value's type only moves up the lattice, so
//! the number of rounds is bounded by the lattice's height.
//!
//! A domain whose `join` is not monotone does not terminate, and that is the
//! front end's bug rather than this pass's. `ROUNDS` bounds it anyway — not to
//! paper over such a domain but so the failure is a panic naming the cause instead
//! of a compiler that hangs, which is what `lost-roots.md` calls the difference
//! between a defect and a mystery.

use crate::cfg::{Func, Op, Terminator, ValueId};
use crate::domain::Domain;

/// What is known about every value of a function.
pub struct Types<T> {
    of: Vec<T>,
}

impl<T: Clone> Types<T> {
    /// What is known about one value.
    pub fn of(&self, value: ValueId) -> &T {
        &self.of[value.0 as usize]
    }
}

/// How many times a block may be re-analysed before this is called a bug.
///
/// Generous rather than tight: a legitimate lattice is a handful of levels deep,
/// so a hundred rounds over one block means `join` narrowed somewhere.
const ROUNDS: usize = 100;

/// The type of every value, computed to a fixed point.
///
/// Values a block's parameters receive are joined from every predecessor's
/// arguments; everything else follows from its operation. A guard narrows.
pub fn infer<D: Domain>(func: &Func, domain: &D) -> Types<D::Type> {
    let mut of = vec![domain.bottom(); func.values as usize];
    // The entry block's parameters are the function's own, and nothing in this
    // function supplies them: they are as unknown as the domain allows.
    for param in &func.block(func.entry()).params {
        of[param.0 as usize] = domain.top();
    }
    let mut visits = vec![0usize; func.blocks.len()];
    let mut worklist: Vec<_> = func.block_ids().collect();
    worklist.reverse();
    while let Some(block) = worklist.pop() {
        visits[block.0 as usize] += 1;
        assert!(
            visits[block.0 as usize] <= ROUNDS,
            "a block was re-analysed {ROUNDS} times, which means the domain's join narrows somewhere"
        );
        let mut changed = false;
        // The parameters, joined from every predecessor that supplies them.
        if block != func.entry() {
            let params = func.block(block).params.clone();
            for (at, param) in params.iter().enumerate() {
                let mut joined = domain.bottom();
                for predecessor in func.predecessors(block) {
                    let Some(end) = &func.block(predecessor).terminator else {
                        continue;
                    };
                    for supplied in arguments_to(end, block, at) {
                        joined = domain.join(&joined, &of[supplied.0 as usize]);
                    }
                }
                if joined != of[param.0 as usize] {
                    of[param.0 as usize] = joined;
                    changed = true;
                }
            }
        }
        // The instructions, in order, each from what it reads.
        for inst in &func.block(block).insts {
            let held = func.inst(*inst);
            let computed = match &held.op {
                Op::Const(value) => domain.of_const(value),
                Op::Prim { prim, args } => {
                    let of_args: Vec<_> =
                        args.iter().map(|held| of[held.0 as usize].clone()).collect();
                    domain.transfer(*prim, &of_args)
                }
                Op::Call { callee, .. } => match callee {
                    crate::cfg::Callee::Entry(entry) => domain.of_entry(*entry),
                    // A call to a function of this program or to whatever a value
                    // holds: nothing is known until an interprocedural pass says
                    // so, and answering `top` is the sound half of not knowing.
                    crate::cfg::Callee::Func(_) | crate::cfg::Callee::Dynamic(_) => domain.top(),
                },
                Op::Guard { assertion, on, .. } => {
                    domain.narrow(*assertion, &of[on.0 as usize])
                }
            };
            if computed != of[held.result.0 as usize] {
                of[held.result.0 as usize] = computed;
                changed = true;
            }
        }
        if changed {
            if let Some(end) = &func.block(block).terminator {
                for successor in end.successors() {
                    if !worklist.contains(&successor) {
                        worklist.push(successor);
                    }
                }
            }
        }
    }
    Types { of }
}

/// The values a terminator supplies to `target`'s parameter at `at`.
///
/// A vector rather than an `Option` because a branch may take both arms to the
/// same block with different arguments, and joining only one of them would answer
/// something no path produces.
fn arguments_to(end: &Terminator, target: crate::cfg::BlockId, at: usize) -> Vec<ValueId> {
    let mut found = Vec::new();
    match end {
        Terminator::Jump { target: to, args } if *to == target => {
            found.extend(args.get(at).copied());
        }
        Terminator::Branch {
            then_block,
            then_args,
            else_block,
            else_args,
            ..
        } => {
            if *then_block == target {
                found.extend(then_args.get(at).copied());
            }
            if *else_block == target {
                found.extend(else_args.get(at).copied());
            }
        }
        _ => {}
    }
    found
}
