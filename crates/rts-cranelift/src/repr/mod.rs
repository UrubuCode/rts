//! Representations: the one property every IR value carries.
//!
//! A representation says how a value exists on the machine — in an integer
//! register, in a floating-point register, as a reference the collector
//! understands, or in the uniform generic form. It is deliberately *carried as
//! data* rather than encoded in the Rust type of a value handle, for three
//! structural reasons:
//!
//! 1. Lowering frequently does not know a representation until a merge is
//!    computed, so a translation function would have no expressible return type.
//! 2. Argument lists, block parameters and field lists are heterogeneous by
//!    nature, so a typed handle forces an enum back into existence at every
//!    collection.
//! 3. The code generator underneath uses an untyped value handle with a typed
//!    lookup, so a typed handle fights the layer it must eventually become.
//!
//! The property that matters is preserved by a stronger mechanism than types:
//! no operation accepts both a proven and a generic operand. See
//! [`crate::ir::builder`].

use crate::types::TypeId;

/// What a reference points at, as far as the machine is concerned.
///
/// This is a *machine* classification, not a language one: it distinguishes
/// cases the collector and the layout rules must treat differently, and nothing
/// finer. A client that needs more distinctions expresses them in its own value
/// kinds (see [`crate::tags`]), not here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RefKind {
    /// An instance of a registered aggregate with a static layout.
    Aggregate(TypeId),
    /// A byte-addressed payload of dynamic length.
    Bytes,
    /// Code with an entry point and a signature.
    Callable,
    /// A reference whose layout is not statically known.
    ///
    /// Reachable and collectable like any other reference, but its fields
    /// cannot be addressed at a constant offset.
    Opaque,
}

/// How a value exists on the machine.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Repr {
    /// Signed 8-bit integer in a register.
    I8,
    /// Signed 16-bit integer in a register.
    I16,
    /// Signed 32-bit integer in a register.
    I32,
    /// Signed 64-bit integer in a register.
    I64,
    /// 32-bit floating point in a register.
    F32,
    /// 64-bit floating point in a register.
    F64,
    /// A truth value, zero or one, in a register.
    Bool,
    /// A reference the collector understands.
    Ref(RefKind),
    /// The uniform generic form: representation not proven at this point.
    Tagged,
}

impl Repr {
    /// The merge rule, and the only one.
    ///
    /// Total and decidable: agreement preserves the representation,
    /// disagreement widens to [`Repr::Tagged`]. There is no case where a merge
    /// picks one side, because picking would make a value's representation
    /// depend on which edge control arrived from — which is exactly the class
    /// of unsoundness a single carried representation exists to prevent.
    pub fn join(self, other: Repr) -> Repr {
        if self == other { self } else { Repr::Tagged }
    }

    /// Whether integer arithmetic applies.
    pub fn is_integer(self) -> bool {
        matches!(self, Repr::I8 | Repr::I16 | Repr::I32 | Repr::I64)
    }

    /// Whether floating-point arithmetic applies.
    pub fn is_float(self) -> bool {
        matches!(self, Repr::F32 | Repr::F64)
    }

    /// Whether the collector must be able to find this value.
    ///
    /// This is the predicate that decides root reporting and write-barrier
    /// insertion. A generic value is included: it may hold a reference, and the
    /// machine layer cannot prove otherwise at the point the question is asked.
    pub fn is_gc_relevant(self) -> bool {
        matches!(self, Repr::Ref(_) | Repr::Tagged)
    }

    /// Width in bits for the representations that have one.
    ///
    /// References and generic values report the machine word, because that is
    /// what a slot holding one occupies.
    pub fn bit_width(self) -> u32 {
        match self {
            Repr::I8 => 8,
            Repr::I16 => 16,
            Repr::I32 | Repr::F32 => 32,
            Repr::Bool => 8,
            Repr::I64 | Repr::F64 | Repr::Ref(_) | Repr::Tagged => 64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_agrees_or_widens() {
        assert_eq!(Repr::I32.join(Repr::I32), Repr::I32);
        assert_eq!(Repr::I32.join(Repr::F64), Repr::Tagged);
        assert_eq!(Repr::Tagged.join(Repr::Tagged), Repr::Tagged);
    }

    #[test]
    fn distinct_reference_kinds_widen_rather_than_pick() {
        let a = Repr::Ref(RefKind::Bytes);
        let b = Repr::Ref(RefKind::Callable);
        assert_eq!(a.join(b), Repr::Tagged);
        assert_eq!(a.join(a), a);
    }

    #[test]
    fn generic_values_are_gc_relevant() {
        assert!(Repr::Tagged.is_gc_relevant());
        assert!(Repr::Ref(RefKind::Opaque).is_gc_relevant());
        assert!(!Repr::F64.is_gc_relevant());
    }
}
