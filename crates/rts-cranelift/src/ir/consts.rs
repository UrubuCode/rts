//! Constants.
//!
//! Five kinds, kept apart because they differ in lifetime, in how they are
//! materialized, and in whether the collector must know about them. Collapsing
//! them into one leaves nowhere to say "this constant is a reference", which
//! then becomes a special case at every use.
//!
//! Whether a constant becomes an immediate or a load from a data section is not
//! a client's decision. It follows from the kind and from the target's
//! instruction encodings, and is settled during lowering.

use crate::repr::Repr;
use crate::types::TypeId;

/// A scalar constant's bits, interpreted according to its representation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ScalarBits(pub u64);

/// A declared constant.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum ConstDecl {
    /// A value that fits in a register.
    Scalar {
        /// How it exists on the machine.
        repr: Repr,
        /// Its bits.
        bits: ScalarBits,
    },

    /// Immutable bytes in a data section.
    ///
    /// Deduplicated by content across the whole compilation, not per function:
    /// this is where identical literals stop costing once each.
    Bytes {
        /// The content.
        bytes: Vec<u8>,
        /// Required alignment.
        align: u32,
    },

    /// Immutable text, held apart from arbitrary bytes so that string handling
    /// can intern it without the byte path needing to know what text is.
    Text(String),

    /// The address of a symbol.
    Symbol(SymbolRef),

    /// A reference to immutable, statically placed storage.
    ///
    /// Its storage never moves and is never reclaimed, so reading it needs no
    /// barrier. It is still a reference in every other respect: it takes part in
    /// representation merges, it carries a barrier when written into something
    /// mutable, and it is reported as a root when live across a safepoint.
    /// Declaring that once, here, is what keeps every consumer of a reference
    /// uniform instead of special-casing this one.
    StaticRef {
        /// The content the reference designates.
        content: Box<ConstDecl>,
        /// The aggregate layout that content follows.
        ty: TypeId,
    },
}

/// Names a symbol resolved when the compilation is linked or installed.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct SymbolRef(pub String);

impl ConstDecl {
    /// The representation a value materialized from this constant has.
    pub fn repr(&self) -> Repr {
        match self {
            ConstDecl::Scalar { repr, .. } => *repr,
            ConstDecl::Bytes { .. } | ConstDecl::Text(_) => Repr::Ref(crate::repr::RefKind::Bytes),
            ConstDecl::Symbol(_) => Repr::I64,
            ConstDecl::StaticRef { ty, .. } => Repr::Ref(crate::repr::RefKind::Aggregate(*ty)),
        }
    }

    /// Whether the collector must be able to find a value materialized from this
    /// constant.
    pub fn is_gc_relevant(&self) -> bool {
        self.repr().is_gc_relevant()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::RefKind;

    #[test]
    fn a_symbol_address_is_an_integer_not_a_reference() {
        let c = ConstDecl::Symbol(SymbolRef("entry".into()));
        assert_eq!(c.repr(), Repr::I64);
        assert!(!c.is_gc_relevant(), "the collector does not trace code addresses");
    }

    #[test]
    fn a_static_reference_is_traced_like_any_other() {
        let c = ConstDecl::StaticRef {
            content: Box::new(ConstDecl::Text("literal".into())),
            ty: TypeId(0),
        };
        assert_eq!(c.repr(), Repr::Ref(RefKind::Aggregate(TypeId(0))));
        assert!(c.is_gc_relevant());
    }
}
