//! What the verifier reports.
//!
//! Every variant names the program point it concerns. A diagnostic that says a
//! rule was broken without saying where costs more to act on than the rule saves.

use crate::ir::{BlockId, InstId, ValueId};
use crate::repr::Repr;
use crate::types::TypeId;
use crate::unwind::RegionId;

/// Where a call is.
///
/// An ordinary call is an instruction; a tail call is a terminator, because the
/// frame is gone before control transfers. One type so that a diagnostic about
/// calls does not come in two shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallSite {
    /// An ordinary call.
    Inst(InstId),
    /// A tail call, named by the block it ends.
    Tail(BlockId),
}

/// A structural violation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VerifyError {
    /// A block has no terminator.
    UnterminatedBlock(BlockId),

    /// A branch names a block that does not exist in this function.
    UnknownBlock {
        /// Where the branch is.
        from: BlockId,
        /// What it named.
        target: BlockId,
    },

    /// A branch passes the wrong number of arguments.
    ArgumentCount {
        /// Where the branch is.
        from: BlockId,
        /// The block being entered.
        target: BlockId,
        /// How many parameters it declares.
        expected: usize,
        /// How many arguments were passed.
        found: usize,
    },

    /// A branch argument disagrees with the parameter it binds.
    ArgumentRepr {
        /// Where the branch is.
        from: BlockId,
        /// The block being entered.
        target: BlockId,
        /// Which argument.
        position: usize,
        /// What the parameter declares.
        expected: Repr,
        /// What was passed.
        found: Repr,
    },

    /// An operation that requires proven operands received a generic one.
    GenericOperand {
        /// The instruction.
        inst: InstId,
    },

    /// An operation received operands that disagree about representation.
    MixedOperands {
        /// The instruction.
        inst: InstId,
        /// The left operand's representation.
        left: Repr,
        /// The right operand's representation.
        right: Repr,
    },

    /// An operation received a representation it does not apply to.
    WrongDomain {
        /// The instruction.
        inst: InstId,
        /// What it received.
        found: Repr,
    },

    /// A remainder cannot be answered totally as written.
    ///
    /// The float domain never admits one — IEEE remainder is a library call and
    /// the identity avoiding it is inexact past 2^53. The integer domain admits
    /// it only for a divisor settled to be neither `0` nor `-1`, because `srem`
    /// traps on both where the language requires a value.
    ///
    /// [`crate::ir::builder::BuildError::UnsafeRemainder`] refuses the same
    /// program where it is written. This exists for the representation that did
    /// not come from the builder — rule 7 wants both, and a trap is exactly the
    /// failure the honesty floor refuses to ship as a pass.
    UnsafeRemainder {
        /// The instruction.
        inst: InstId,
        /// What its operands were proven to be.
        found: Repr,
    },

    /// A branch condition is not a proven boolean.
    ConditionNotBool {
        /// Where the branch is.
        from: BlockId,
        /// What the condition's representation is.
        found: Repr,
    },

    /// A guard's success block does not begin with the narrowed value.
    GuardTargetMissingValue {
        /// Where the guard is.
        from: BlockId,
        /// The block named as the success path.
        target: BlockId,
        /// The representation the guard establishes.
        expect: Repr,
    },

    /// A guard tested something that is not in the generic form.
    ///
    /// A proven value has nothing to test, so the guard is either redundant or
    /// the representation it carries is wrong. Both are worth reporting.
    GuardOnProvenValue {
        /// Where the guard is.
        from: BlockId,
        /// What it tested.
        found: Repr,
    },

    /// A narrowing appears outside a guard's success path.
    UnguardedNarrowing {
        /// The instruction.
        inst: InstId,
    },

    /// A return does not match the function's signature.
    ReturnArity {
        /// Where the return is.
        from: BlockId,
        /// How many values the signature declares.
        expected: usize,
        /// How many were returned.
        found: usize,
    },

    /// A returned value disagrees with the signature.
    ReturnRepr {
        /// Where the return is.
        from: BlockId,
        /// Which value.
        position: usize,
        /// What the signature declares.
        expected: Repr,
        /// What was returned.
        found: Repr,
    },

    /// A field index does not exist in the aggregate.
    NoSuchField {
        /// The instruction.
        inst: InstId,
        /// The aggregate.
        ty: TypeId,
        /// The index that was asked for.
        field: u32,
    },

    /// A type identifier did not come from the registry being verified against.
    ForeignType {
        /// The instruction.
        inst: InstId,
        /// The identifier.
        ty: TypeId,
    },

    /// A value handle did not come from the function being verified.
    ForeignValue {
        /// The value.
        value: ValueId,
    },

    /// A thrown value is not in the generic form.
    ///
    /// This layer does not know what may be thrown, so what travels must be the
    /// uniform form. A proven representation here is a claim about a language.
    ThrownValueNotGeneric {
        /// Where the throw is.
        from: BlockId,
        /// What was thrown.
        found: Repr,
    },

    /// A handler does not receive the thrown value.
    ///
    /// A handler that did not receive it would have to find it somewhere else,
    /// and somewhere else outlives the frame it belongs to.
    HandlerMissingPayload {
        /// The region the handler is attached to.
        region: RegionId,
        /// The handler's block.
        target: BlockId,
    },

    /// A region names a block that does not exist in this function.
    UnknownRegionBlock {
        /// The region.
        region: RegionId,
        /// What it named.
        target: BlockId,
    },

    /// A block is placed in a region this function did not declare.
    UnknownRegion {
        /// The block.
        block: BlockId,
        /// The region it names.
        region: RegionId,
    },

    /// A call names a function or shape this registry did not issue.
    UnknownCallee {
        /// The instruction, or the terminating block for a tail call.
        at: CallSite,
    },

    /// A call passes the wrong number of arguments.
    CallArity {
        /// Where the call is.
        at: CallSite,
        /// How many the signature declares.
        expected: usize,
        /// How many were passed.
        found: usize,
    },

    /// A call argument disagrees with the parameter it fills.
    CallArgumentRepr {
        /// Where the call is.
        at: CallSite,
        /// Which argument.
        position: usize,
        /// What the signature declares.
        expected: Repr,
        /// What was passed.
        found: Repr,
    },

    /// A call binds a different number of values than the callee returns.
    CallResultArity {
        /// The instruction.
        inst: InstId,
        /// How many the signature declares.
        expected: usize,
        /// How many were bound.
        found: usize,
    },

    /// An indirect call reaches its callee through something that is not code.
    ///
    /// A generic value might be code, and a guard is how one finds out. Calling
    /// through it unguarded is the narrowing this layer refuses everywhere else.
    IndirectCalleeNotCallable {
        /// Where the call is.
        at: CallSite,
        /// What was named.
        found: Repr,
    },

    /// A tail call whose conventions or returns do not match.
    ///
    /// Both sides must permit tail calls and their return lists must match
    /// exactly, which makes a tail-recursive group compile as a unit or not at
    /// all.
    TailCallNotPermitted {
        /// Where the call is.
        from: BlockId,
    },

    /// A tail call inside a protected region.
    ///
    /// The frame is discarded before control transfers, so the handler installed
    /// around it no longer exists when it would be needed. Returning a call's
    /// result directly and catching that call's exception are mutually exclusive.
    TailCallInProtectedRegion {
        /// Where the call is.
        from: BlockId,
    },

    /// A type guard reads a type out of something that is not an object.
    ///
    /// The type is in the object, so reading it from a value that is not one
    /// reads whatever is at that address.
    GuardTypeOnNonReference {
        /// Where the guard is.
        from: BlockId,
        /// What it was given.
        found: Repr,
    },

    /// A cached read names somewhere to remember that this function has not got.
    UnknownCache {
        /// Where the read is.
        from: BlockId,
        /// What it named.
        cache: crate::ir::CacheId,
    },

    /// A block of a cleanup ends in a way a cleanup may not.
    ///
    /// A cleanup is copied into each path that needs it, which is only sound if
    /// it has one exit. Within the piece a jump or a branch is fine -- it stays
    /// inside, so it is not an exit at all. A return, a throw or a tail call is
    /// a second exit, and so is a piece with no `CleanupDone` anywhere, which
    /// has none.
    CleanupDoesNotEnd {
        /// The region it belongs to.
        region: RegionId,
        /// The block.
        block: BlockId,
    },

    /// A block ends as a cleanup without being one.
    CleanupEndOutsideCleanup {
        /// The block.
        block: BlockId,
    },

    /// The entry of a cleanup declares parameters.
    ///
    /// It is entered from every path that unwinds through it, and those paths
    /// have nothing in common to pass. An interior block may take them: it is
    /// entered from within the piece, where there is something to pass -- which
    /// is what lets a merge inside a cleanup exist at all.
    CleanupTakesParameters {
        /// The block.
        block: BlockId,
    },

    /// A cleanup reads a value defined outside itself.
    ///
    /// Copying it into a path where that value was never computed would read
    /// something that does not exist there. A cleanup reaches what it needs
    /// through the objects it is releasing, which is how cleanup works anyway.
    CleanupReadsOutsideItself {
        /// The block.
        block: BlockId,
        /// The value it read.
        value: ValueId,
    },

    /// A function parks its frame without declaring that it may.
    ///
    /// Whether a function suspends is part of what it is, and a caller decides
    /// what to do about a call by reading it. A function that suspends while
    /// claiming not to makes every caller's decision wrong.
    UndeclaredSuspension {
        /// The instruction that parks the frame.
        inst: InstId,
    },
}
