//! What the verifier reports.
//!
//! Every variant names the program point it concerns. A diagnostic that says a
//! rule was broken without saying where costs more to act on than the rule saves.

use crate::ir::{BlockId, InstId, ValueId};
use crate::repr::Repr;
use crate::types::TypeId;
use crate::unwind::RegionId;

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
}
