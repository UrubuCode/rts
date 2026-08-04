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
    /// A pointer and a length, as one logical argument, over elements of a
    /// stated representation.
    ///
    /// # Why the element is named rather than assumed
    ///
    /// It was `Slice` with nothing inside, and that was a hole. A pointer and a
    /// length describe UTF-8 bytes, UTF-16 code units and a buffer of doubles
    /// equally well, so two functions with genuinely different boundaries had
    /// the same descriptor and nothing could tell them apart.
    ///
    /// That matters here more than it would elsewhere, because **a JavaScript
    /// string is a sequence of UTF-16 code units** and Rust\x27s `&str` is UTF-8.
    /// A caller holding one and a callee expecting the other must re-encode, and
    /// a descriptor that cannot express the difference cannot tell anyone the
    /// cost is there: the call compiles, runs, and copies on every crossing.
    ///
    /// The element carries no charset and does not need one. `I8` and `I16` say
    /// how wide a unit is; what the units *mean* is the contract between the two
    /// sides, stated where the entry point is declared.
    ///
    /// The two slots this occupies are unchanged. The element decides nothing
    /// about the crossing itself. It decides whether the two sides agree about
    /// what crossed.
    Slice(Repr),
}

/// Which register and stack discipline a call follows.
///
/// The compiler underneath offers two categories and neither is extensible:
/// conventions stable across a library boundary, one per target, and internal
/// conventions explicitly not stable — including the one that permits tail
/// calls. Everything else is a protocol on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Convention {
    /// Between functions this compilation owns. Not stable across a boundary.
    ///
    /// The default: a function is internal until something says otherwise, and
    /// the two that are not internal both cost something a caller should have to
    /// ask for.
    #[default]
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

/// A function a linker resolves by name, described where it is defined.
///
/// `Signature` holds `Vec`s, so it cannot be built in a `const`. A declaration
/// has to be one: it is emitted beside the definition it describes, and the
/// point of emitting it there is that the two cannot drift. So this is the same
/// information over slices, and [`EntryDesc::signature`] builds the owned form
/// when a caller wants one.
///
/// Deliberately says nothing about *which* entry point this is. Numbering is a
/// property of the whole set, and a declaration cannot see its neighbours —
/// which is why the set is assembled somewhere that can.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntryDesc {
    /// The linker name.
    ///
    /// Needed in two places, and neither is a call site: an object file resolves
    /// an undefined symbol against an archive by name, and a backtrace naming
    /// one is readable where a backtrace naming an index is not.
    pub symbol: &'static str,
    /// Parameters, in order.
    pub params: &'static [AbiType],
    /// Returns, in order.
    pub returns: &'static [AbiType],
    /// The register and stack discipline.
    pub convention: Convention,
}

impl EntryDesc {
    /// The owned signature this describes.
    pub fn signature(&self) -> Signature {
        Signature {
            params: self.params.to_vec(),
            returns: self.returns.to_vec(),
            convention: self.convention,
        }
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
