//! The application binary interface.
//!
//! What a function accepts and returns, expressed so that a client can describe
//! its own convention without the machine layer ever learning whose it is.
//!
//! # Why this is rebuilt rather than extended
//!
//! The interface this replaces is entirely scalar: no aggregate, no structure,
//! a return position holding zero or one machine slot, and a string that cannot
//! be returned at all. That is workable for one client whose values are already
//! uniform. It is not a foundation: a client with genuine multiple returns
//! cannot express its convention in it, and a client with value types pays an
//! allocation at every boundary crossing — which measurement identifies as the
//! largest single cost in the system.
//!
//! # What the compiler underneath does and does not do
//!
//! It performs *no* classification of an aggregate into register-sized pieces.
//! It sees a flat list of scalars per slot and nothing else. So this module
//! either classifies, or declines to and pays that allocation everywhere.
//! Declining is not an option, so the classifier lives here — written once,
//! parameterized by target, driven by the shared type registry.
//!
//! Returns are assigned to registers until the budget runs out, after which
//! compilation fails with an explicit instruction to use an out-pointer. There
//! is a flag that performs that rewrite silently; this module does not set it. A
//! flag that changes an emitted signature underneath the abstraction is exactly
//! the inferred effect this layer exists to prevent, so the rewrite happens
//! here, visibly, in one place.
//!
//! # A client's convention is not a calling convention
//!
//! A leading receiver, a trailing rest-argument slice, returns adjusted to the
//! caller's arity — these are protocols expressed in the parameter and return
//! lists, riding on top of an internal convention. They are data a client
//! declares, never a convention this layer would have to name, which it could
//! not do without naming the client.

mod classify;
mod target;

pub use classify::{LoweredSignature, ParamClass, ReturnClass, SlotSpec, lower_signature};
pub use target::{AggregatePolicy, TargetAbi};

use crate::repr::Repr;
use crate::types::TypeId;

/// A type at a call boundary.
///
/// Distinct from [`Repr`], which says how a value exists in a register. An ABI
/// type additionally knows how it occupies slots, which is only a question at a
/// boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AbiType {
    /// A value that fits in one register.
    Scalar(Repr),
    /// An instance of a registered aggregate, passed by value.
    Aggregate(TypeId),
    /// A pointer and a length, as one logical argument.
    Slice,
}

impl AbiType {
    /// The representation a scalar carries, if this is one.
    pub fn scalar_repr(self) -> Option<Repr> {
        match self {
            AbiType::Scalar(repr) => Some(repr),
            _ => None,
        }
    }
}

/// Which register and stack discipline a call follows.
///
/// The compiler underneath offers two categories and neither is extensible:
/// conventions stable across a library boundary, one per target, and internal
/// conventions explicitly not stable — including the one that permits tail
/// calls. Everything else is a protocol on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Convention {
    /// Between functions this compilation owns. Not stable across a boundary.
    Internal,
    /// Internal, and permitting tail calls.
    ///
    /// The callee's convention must match the caller's exactly and the return
    /// types must match exactly, which makes a tail-recursive group a unit: the
    /// whole group compiles under this convention or none of it does. That
    /// decision belongs to the machine layer, not to a call site.
    InternalTail,
    /// The target's stable convention, for anything crossing a library boundary.
    Foreign,
}

impl Convention {
    /// Whether a tail call may target a function with this convention.
    pub fn permits_tail_calls(self) -> bool {
        matches!(self, Convention::InternalTail)
    }

    /// Whether this convention is stable across a library boundary.
    pub fn is_stable(self) -> bool {
        matches!(self, Convention::Foreign)
    }
}

/// What a function accepts and returns.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Signature {
    /// Parameters, in order.
    pub params: Vec<AbiType>,
    /// Returns, in order. More than one is expressible from the start.
    pub returns: Vec<AbiType>,
    /// The register and stack discipline.
    pub convention: Convention,
}

impl Signature {
    /// A signature with the given types under the internal convention.
    pub fn internal(params: Vec<AbiType>, returns: Vec<AbiType>) -> Self {
        Self {
            params,
            returns,
            convention: Convention::Internal,
        }
    }

    /// A signature crossing a library boundary.
    pub fn foreign(params: Vec<AbiType>, returns: Vec<AbiType>) -> Self {
        Self {
            params,
            returns,
            convention: Convention::Foreign,
        }
    }

    /// The same signature, permitting tail calls.
    pub fn with_tail_calls(mut self) -> Self {
        self.convention = Convention::InternalTail;
        self
    }

    /// Whether a tail call from this function to `callee` is legal.
    ///
    /// Both conventions must permit tail calls and the return types must match
    /// exactly. Checking it here means a call site cannot construct an edge that
    /// would be rejected further down, where the cause is no longer visible.
    pub fn permits_tail_call_to(&self, callee: &Signature) -> bool {
        self.convention.permits_tail_calls()
            && callee.convention.permits_tail_calls()
            && self.returns == callee.returns
    }
}
