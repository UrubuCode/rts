//! How a block ends.
//!
//! Apart from the rest of the graph because `cfg.rs` passed this crate's 500-line
//! ceiling — rule 11 — and this is the seam the file already had: the vocabulary of an
//! INSTRUCTION is one question and the vocabulary of an EXIT is another. Two of the six
//! terminators name no successor at all, which is what makes them a subject rather than
//! a tail of the other list.

use super::{BlockId, ValueId};
use crate::guard::PointId;
/// How a block ends. Exactly one per block, and rule 9's verifier says so.
#[derive(Clone, PartialEq, Debug)]
pub enum Terminator {
    /// To one block, with arguments for its parameters.
    Jump { target: BlockId, args: Vec<ValueId> },
    /// To one of two, on a truth value.
    Branch {
        /// The condition. A language's own notion of truth is a [`Prim`] that
        /// produced this, never something this crate decides.
        condition: ValueId,
        /// Taken when true.
        then_block: BlockId,
        /// Arguments for it.
        then_args: Vec<ValueId>,
        /// Taken when false.
        else_block: BlockId,
        /// Arguments for it.
        else_args: Vec<ValueId>,
    },
    /// Leaving the function.
    Return(Option<ValueId>),
    /// Leaving this tier for the other one, at a paired point.
    ///
    /// Not a call and not an unwind: the generic body of the same function has a
    /// resume label at this `PointId`, in the same binary. README rule 8.
    Fall(PointId),
    /// Ends a cleanup, handing control back to whatever brought us into it.
    ///
    /// # Why a cleanup has one exit and no continuation parameter
    ///
    /// Because it is COPIED into each path that needs it rather than jumped to, and
    /// `rts_cranelift::ir::Terminator::CleanupDone` -- which this is the neutral form
    /// of -- says why the alternative lost: a parameter naming where to continue
    /// *"would make every cleanup able to reach every continuation, which is an edge
    /// in the graph for every pair and no useful analysis afterwards"*, and the
    /// representation has no indirect branch to lower it to anyway.
    ///
    /// A cleanup is a PIECE and not a block. It may branch and merge inside itself,
    /// and more than one of its blocks may end this way: several are still one exit,
    /// because they all leave to the same place.
    ///
    /// This is what makes "one entry, one exit" structural instead of hoped for, and
    /// `verify` refuses it outside a cleanup piece for that reason.
    CleanupDone,
    /// Raising: control leaves along the enclosing region's exception edge.
    ///
    /// # Why it is a terminator and has no successor
    ///
    /// Because an exception edge is not a jump, which is the same thing
    /// [`crate::region`] already says about a handler: nothing jumps to one, so
    /// nothing carries arguments to one, so a handler's predecessors are empty. A
    /// `Raise` naming its handler as a successor would make that false, and every
    /// pass reading the graph as a CFG would then expect an argument list nothing
    /// can supply.
    ///
    /// Where it lands is [`Func::region_of`] over the block it sits in, and out
    /// along [`crate::region::Region::parent`] from there — the search
    /// `rts_cranelift::unwind::plan_unwind` computes, which is why the region tree
    /// is what this carries instead of a target.
    ///
    /// # Why no tag
    ///
    /// A tag says which handlers match, and *what may be thrown* is the one thing
    /// the machine's own header refuses to decide. So the value travels and the
    /// language declares the tag; until it does, the machine refuses this by name
    /// with [`crate::lower::Unlowerable::NeedsHandlerTag`] — the same refusal a
    /// protected region already gets, because it is the same missing declaration.
    Raise(ValueId),
    /// Control does not reach here. A verifier error if it does.
    ///
    /// NOT a raise. This is a trap: the machine's way of saying a point is
    /// unreachable. A language that lowered `throw` to it would get an abort where
    /// the program expects a catchable value.
    Unreachable,
}

impl Terminator {
    /// Every block this one may reach.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump { target, .. } => vec![*target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            // A RAISE HAS NONE, and that is the claim rather than an omission: see its
            // own doc for why an exception edge is not an edge here.
            Terminator::Return(_)
            | Terminator::Fall(_)
            | Terminator::Raise(_)
            | Terminator::CleanupDone
            | Terminator::Unreachable => Vec::new(),
        }
    }

    /// Every value it reads.
    pub fn reads(&self) -> Vec<ValueId> {
        match self {
            Terminator::Jump { args, .. } => args.clone(),
            Terminator::Branch {
                condition,
                then_args,
                else_args,
                ..
            } => {
                let mut all = vec![*condition];
                all.extend(then_args.iter().copied());
                all.extend(else_args.iter().copied());
                all
            }
            Terminator::Return(Some(value)) => vec![*value],
            Terminator::Raise(value) => vec![*value],
            Terminator::Return(None)
            | Terminator::Fall(_)
            | Terminator::CleanupDone
            | Terminator::Unreachable => Vec::new(),
        }
    }
}
