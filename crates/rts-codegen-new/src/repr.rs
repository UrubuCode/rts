//! Representation lattice — the soundness core.
//!
//! Every IR value has exactly ONE `Repr`. This is what replaces the old engine's
//! unsound side-tables (`var_member_call_values` et al.): instead of tracking
//! "this `i64` is secretly not-a-pure-int" in a `HashSet` that a forgetful call
//! site can desync, the representation is a property OF the value, decided by the
//! front-end and carried through lowering.
//!
//! Rule: a value is kept UNBOXED (`Repr::Int32`/`Float64`/`Bool`/`Ref`) only
//! where the front-end PROVES it monomorphic (from TS annotations validated at
//! untrusted boundaries, literals, and flow). Anywhere two arms disagree
//! (if/ternary/phi joins, container element load, `any`, untrusted input) the
//! representation joins to [`Repr::Tagged`] — the uniform [`crate::value::PolyValue`].
//! Coercions between `Repr`s are EXPLICIT box/unbox IR nodes (see [`crate::value`]),
//! never implicit reinterpretation.

/// The machine representation a value is proven to have at a given program point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repr {
    /// Unboxed 32-bit integer in an `i64` register (JS small-int fast path).
    Int32,
    /// Unboxed IEEE-754 double in an `f64` register (the winning numeric path).
    Float64,
    /// Unboxed boolean (0/1) in an `i64` register.
    Bool,
    /// A GC handle of a statically-known heap kind. Still a slot index, not a ptr.
    Ref(RefKind),
    /// Unknown / union / `any`: the uniform NaN-boxed [`crate::value::PolyValue`].
    Tagged,
}

/// The heap kind behind a `Repr::Ref`, when statically known.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefKind {
    Str,
    Object,
    Array,
    Function,
    /// Registry-resolved non-primordial (Map/Set/Date/RegExp/...). The engine
    /// never names these; it only knows "some handle of a registered class".
    Registered,
}

impl Repr {
    /// Join two representations at a control-flow merge. Disagreement widens to
    /// `Tagged` — the single, total rule that makes unboxing decidable.
    pub fn join(self, other: Repr) -> Repr {
        if self == other { self } else { Repr::Tagged }
    }

    /// Whether this representation lives unboxed in a register.
    pub fn is_unboxed(self) -> bool {
        !matches!(self, Repr::Tagged)
    }
}
