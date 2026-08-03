//! What lowering can refuse, and why.
//!
//! Every variant here is a refusal rather than a bug. Lowering does not
//! approximate: where it cannot emit something correctly, it says so and names
//! what it could not do, because a wrong instruction is discovered much later
//! than a refused one.

use crate::ir::{BlockId, InstId};
use crate::repr::Repr;

/// Why a function could not be lowered.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LowerError {
    /// A value cannot be widened into the generic form.
    ///
    /// A 64-bit integer does not fit the payload. Widening it correctly needs a
    /// heap box, which is a memory operation; truncating it or reinterpreting it
    /// as a double would lose values silently.
    CannotWiden {
        /// The representation that could not be widened.
        from: Repr,
    },

    /// A generic value cannot be narrowed to this representation.
    CannotNarrow {
        /// The representation asked for.
        to: Repr,
    },

    /// A guard asked to establish which kind of reference a value holds.
    ///
    /// The encoding says a value is a reference and nothing more. Proving the
    /// kind means reading the object, which is a memory operation. A guard can
    /// establish that a value is a reference; it cannot establish which.
    CannotProveReferenceKind {
        /// What was asked for.
        expect: Repr,
    },

    /// An instruction needs a capability this module does not have yet.
    ///
    /// Memory, calls and suspension all reach outside the function being
    /// lowered, which needs the module and symbol machinery that does not exist
    /// yet. Named rather than skipped so that a program relying on them fails
    /// here, visibly, instead of producing code that is quietly incomplete.
    NotYetLowered {
        /// The instruction.
        inst: InstId,
        /// What it needs.
        needs: Capability,
    },

    /// A terminator needs a capability this module does not have yet.
    TerminatorNotYetLowered {
        /// The block it ends.
        block: BlockId,
        /// What it needs.
        needs: Capability,
    },

    /// A call names a function the module has not declared.
    ///
    /// Declaring is what produces the identifier a call refers to, so this means
    /// a site is naming something that does not exist yet — usually an ordering
    /// mistake rather than a wrong name.
    UndeclaredCallee {
        /// The call.
        inst: InstId,
    },

    /// A tail call names a function the module has not declared.
    UndeclaredTailCallee {
        /// The block it ends.
        block: BlockId,
    },

    /// A field access names a field the aggregate does not have.
    ///
    /// The verifier rejects this too. Lowering reports it again because it has
    /// no offset to emit, and inventing one would write somewhere real.
    NoSuchField {
        /// The access.
        inst: InstId,
    },

    /// A block was reached with no terminator.
    ///
    /// The verifier rejects this too. Lowering checks it again because it cannot
    /// produce anything sensible for a block that does not end, and reaching
    /// here means the program was not verified first.
    UnterminatedBlock {
        /// The block.
        block: BlockId,
    },
}

/// A capability lowering does not have yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Capability {
    /// Allocation and field access, which need the heap.
    Memory,
    /// Calls, which need a module to declare a callee in.
    Calls,
    /// Parking and resuming a frame, which needs the frame transformation.
    Suspension,
    /// Promise operations, which need the scheduler at run time.
    Scheduling,
    /// Throwing, which needs the unwinder at run time.
    Unwinding,
}
