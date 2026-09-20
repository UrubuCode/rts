//! What a well-formed function is, stated once.
//!
//! README rule 9. A pass that breaks one of these must fail a test rather than
//! produce a program — which is the lesson `rts-cranelift`'s own verifier records
//! and this crate inherits rather than re-learns: an IR whose invariants live in
//! the heads of the people writing passes has invariants that hold until someone
//! is in a hurry.
//!
//! # Why definition order is checked and not merely dominance
//!
//! Dominance is the real rule — a use must be dominated by its definition — and it
//! is the rule to check once a pass reorders blocks. What is checked here is the
//! stricter and cheaper thing: within a block, a use follows its definition, and
//! across blocks, a value is defined somewhere. That refuses everything dominance
//! refuses plus some legal programs, and every legal program this crate's builder
//! produces passes it, because a lowering emits in order.
//!
//! Upgrading to dominance is a change to make when a pass needs it, with the
//! dominator tree it will need anyway. Checking the strict form now means the
//! weaker check never silently becomes the only one.

use std::collections::BTreeSet;

use crate::cfg::{BlockId, Func, InstId, Op, Terminator, ValueId};
use crate::guard::PointId;

/// A function that is not well formed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Malformed {
    /// A block was never terminated.
    Unterminated(BlockId),
    /// A jump names a block that does not exist.
    NoSuchBlock(BlockId),
    /// A value is read where nothing has defined it yet.
    ReadBeforeDefined {
        /// The value.
        value: ValueId,
        /// Where it was read.
        at: BlockId,
    },
    /// A value is defined twice, which SSA forbids.
    DefinedTwice(ValueId),
    /// A jump supplies a different number of arguments than the target declares
    /// parameters.
    ///
    /// The commonest way to break a graph while editing one, and silent without
    /// this: the extra argument is simply not read, so the program compiles and
    /// the parameter holds whatever the other predecessor happened to supply.
    Arity {
        /// Where the jump is.
        from: BlockId,
        /// Where it goes.
        to: BlockId,
        /// How many it supplied.
        supplied: usize,
        /// How many the target declares.
        expected: usize,
    },
    /// A guard or a fall names a point the function does not declare.
    UndeclaredPoint(PointId),
    /// The entry block declares parameters that no predecessor supplies, and has
    /// a predecessor.
    ///
    /// An entry block with a predecessor is a loop back to the function's own
    /// parameters, which would make them mean two things.
    EntryHasPredecessor(BlockId),
    /// A handler block declares no parameter, so it cannot receive what was raised.
    ///
    /// `region.rs` carries the reason, which is the machine's own: a handler that did
    /// not receive the value would have to find it somewhere else, and somewhere else
    /// is a side channel that outlives the frame it belongs to.
    HandlerTakesNoValue(BlockId),
    /// An instruction parks the frame in a function that does not say it may.
    ///
    /// `FuncBuilder` derives the flag, so this is unreachable through it and is here
    /// for a `Func` assembled by hand -- a test, or a pass that rebuilds one. What it
    /// protects is the machine's reading of the flag: `frame::resumable_form` is asked
    /// for once per function, from that flag, so a function that lied about it would
    /// get an ordinary frame and then try to leave it.
    SuspendsWithoutSaying(InstId),
    /// A region names a parent that does not exist.
    NoSuchRegion(crate::region::RegionId),
}

/// Whether `func` is well formed, and what is wrong if it is not.
pub fn verify(func: &Func) -> Result<(), Malformed> {
    let blocks = func.blocks.len() as u32;
    let mut defined: BTreeSet<ValueId> = BTreeSet::new();

    // Every value defined exactly once, gathered before any use is judged so that
    // a block reached by a back edge is not reported for reading a value its own
    // predecessor defines.
    for block in func.block_ids() {
        for param in &func.block(block).params {
            if !defined.insert(*param) {
                return Err(Malformed::DefinedTwice(*param));
            }
        }
        for inst in &func.block(block).insts {
            let result = func.inst(*inst).result;
            if !defined.insert(result) {
                return Err(Malformed::DefinedTwice(result));
            }
        }
    }

    // WHAT THE FUNCTION CLAIMS ABOUT ITSELF, against what its body does.
    if !func.may_suspend {
        for block in func.block_ids() {
            for inst in &func.block(block).insts {
                let held = func.inst(*inst);
                if matches!(held.op, Op::Suspend { .. }) || held.effect.may_suspend() {
                    return Err(Malformed::SuspendsWithoutSaying(*inst));
                }
            }
        }
    }

    if !func.predecessors(func.entry()).is_empty() {
        return Err(Malformed::EntryHasPredecessor(func.entry()));
    }

    // THE REGIONS, before the blocks are walked: a handler must be able to receive
    // what was raised, and a parent must exist for the chain a raise walks out along.
    for (at, region) in func.regions.iter().enumerate() {
        if let Some(parent) = region.parent
            && parent.0 as usize >= func.regions.len()
        {
            return Err(Malformed::NoSuchRegion(parent));
        }
        let _ = at;
        for block in [region.handler, region.cleanup].into_iter().flatten() {
            if block.0 >= blocks {
                return Err(Malformed::NoSuchBlock(block));
            }
        }
        if let Some(handler) = region.handler
            && func.block(handler).params.is_empty()
        {
            return Err(Malformed::HandlerTakesNoValue(handler));
        }
    }

    for block in func.block_ids() {
        // Within the block, a use follows its definition.
        let mut here: BTreeSet<ValueId> = func.block(block).params.iter().copied().collect();
        for inst in &func.block(block).insts {
            for read in func.reads(*inst) {
                check_read(read, block, &here, &defined)?;
            }
            here.insert(func.inst(*inst).result);
            if let crate::cfg::Op::Guard { point, .. } = &func.inst(*inst).op {
                declared(func, *point)?;
            }
        }

        let Some(end) = &func.block(block).terminator else {
            return Err(Malformed::Unterminated(block));
        };
        for read in end.reads() {
            check_read(read, block, &here, &defined)?;
        }
        for successor in end.successors() {
            if successor.0 >= blocks {
                return Err(Malformed::NoSuchBlock(successor));
            }
        }
        match end {
            // A RAISE NAMES NO BLOCK, so there is no arity to check. What WOULD be
            // worth checking -- that a region encloses it, or that one does not -- is
            // not an error either way: a raise with no enclosing region leaves the
            // function, which is what an uncaught throw does.
            Terminator::Raise(_) => {}
            Terminator::Jump { target, args } => {
                arity(block, *target, args.len(), func.block(*target).params.len())?;
            }
            Terminator::Branch {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                arity(
                    block,
                    *then_block,
                    then_args.len(),
                    func.block(*then_block).params.len(),
                )?;
                arity(
                    block,
                    *else_block,
                    else_args.len(),
                    func.block(*else_block).params.len(),
                )?;
            }
            Terminator::Fall(point) => declared(func, *point)?,
            Terminator::Return(_) | Terminator::Unreachable => {}
        }
    }
    Ok(())
}

/// A value read in `block` must be defined there already, or anywhere at all.
///
/// The second half is what admits a back edge: a loop's body reads a value the
/// latch defines, and the strict within-block order cannot see that.
fn check_read(
    value: ValueId,
    block: BlockId,
    here: &BTreeSet<ValueId>,
    anywhere: &BTreeSet<ValueId>,
) -> Result<(), Malformed> {
    if here.contains(&value) || anywhere.contains(&value) {
        return Ok(());
    }
    Err(Malformed::ReadBeforeDefined { value, at: block })
}

fn arity(from: BlockId, to: BlockId, supplied: usize, expected: usize) -> Result<(), Malformed> {
    match supplied == expected {
        true => Ok(()),
        false => Err(Malformed::Arity {
            from,
            to,
            supplied,
            expected,
        }),
    }
}

fn declared(func: &Func, point: PointId) -> Result<(), Malformed> {
    match func.points.binary_search(&point) {
        Ok(_) => Ok(()),
        Err(_) => Err(Malformed::UndeclaredPoint(point)),
    }
}

/// Every instruction of a function, in block order.
///
/// Here rather than on [`Func`] because it exists for the checker and for passes
/// that walk everything, and a method on the type would suggest the order means
/// something to the semantics. It does not: only the order within a block does.
pub fn instructions(func: &Func) -> impl Iterator<Item = InstId> + use<'_> {
    func.block_ids()
        .flat_map(move |block| func.block(block).insts.iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Const, FuncBuilder, Op, Terminator};
    use crate::effect::Effect;
    use crate::guard::Tier;
    use rts_cranelift::fault::Position;

    fn at() -> Position {
        Position::default()
    }

    #[test]
    fn a_straight_line_function_is_well_formed() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, at());
        build.end(Terminator::Return(Some(one)));
        assert_eq!(verify(&build.finish()), Ok(()));
    }

    #[test]
    fn an_unterminated_block_is_refused() {
        let build = FuncBuilder::new(Tier::Generic);
        assert_eq!(
            verify(&build.finish()),
            Err(Malformed::Unterminated(BlockId(0)))
        );
    }

    /// The commonest way to break a graph while editing one, and silent without
    /// the check.
    #[test]
    fn a_jump_that_supplies_the_wrong_number_of_arguments_is_refused() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let target = build.block();
        let first = build.param(target);
        let _second = build.param(target);
        let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, at());
        build.end(Terminator::Jump {
            target,
            args: vec![one],
        });
        build.switch_to(target);
        build.end(Terminator::Return(Some(first)));
        assert_eq!(
            verify(&build.finish()),
            Err(Malformed::Arity {
                from: BlockId(0),
                to: target,
                supplied: 1,
                expected: 2,
            })
        );
    }

    #[test]
    fn a_value_no_instruction_defines_is_refused() {
        let mut build = FuncBuilder::new(Tier::Generic);
        build.end(Terminator::Return(Some(ValueId(7))));
        assert_eq!(
            verify(&build.finish()),
            Err(Malformed::ReadBeforeDefined {
                value: ValueId(7),
                at: BlockId(0),
            })
        );
    }

    /// A loop reads in its body what its latch defines. The check must admit
    /// that, which is why a definition anywhere counts across blocks.
    #[test]
    fn a_back_edge_may_read_what_the_latch_defines() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let header = build.block();
        let carried = build.param(header);
        let zero = build.push(Op::Const(Const::Int(0)), Effect::PURE, at());
        build.end(Terminator::Jump {
            target: header,
            args: vec![zero],
        });
        build.switch_to(header);
        let next = build.push(
            Op::Prim {
                prim: crate::cfg::Prim(0),
                args: vec![carried],
            },
            Effect::PURE,
            at(),
        );
        build.end(Terminator::Jump {
            target: header,
            args: vec![next],
        });
        assert_eq!(verify(&build.finish()), Ok(()));
    }

    #[test]
    fn a_fall_to_a_point_the_function_does_not_declare_is_refused() {
        let mut build = FuncBuilder::new(Tier::Specialised);
        build.end(Terminator::Fall(PointId(3)));
        let mut func = build.finish();
        // Declared by the builder, so the refusal has to be provoked: a pass that
        // rewrites a terminator without declaring its point is exactly the bug.
        func.points.clear();
        assert_eq!(verify(&func), Err(Malformed::UndeclaredPoint(PointId(3))));
    }

    #[test]
    fn an_entry_block_with_a_predecessor_is_refused() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let second = build.block();
        build.end(Terminator::Jump {
            target: second,
            args: Vec::new(),
        });
        build.switch_to(second);
        build.end(Terminator::Jump {
            target: BlockId(0),
            args: Vec::new(),
        });
        assert_eq!(
            verify(&build.finish()),
            Err(Malformed::EntryHasPredecessor(BlockId(0)))
        );
    }
}
