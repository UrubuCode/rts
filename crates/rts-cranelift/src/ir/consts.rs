//! Constants.
//!
//! One kind: a value that fits in a register. Whether it becomes an immediate or
//! a load is not a client's decision — it follows from the representation and
//! from the target's instruction encodings, and is settled during lowering.
//!
//! # There were five, and four had no producer
//!
//! `Bytes`, `Text`, `Symbol` and `StaticRef` were all in this enum and all
//! unreachable: nothing in the workspace built one, and `lower_const` answered
//! `NotYetLowered { needs: Capability::Memory }` for every one of them. Rule 6
//! says a structure with no producer is a gap rather than a feature, and this
//! crate's README records that it has shipped three such structures before and
//! found each of them by looking rather than by the build.
//!
//! Three of the four carried documentation describing behaviour the code did not
//! have — `Bytes` said it was "deduplicated by content across the whole
//! compilation", `StaticRef` said it "is reported as a root when live across a
//! safepoint" — which is rule 4's own failure mode: documentation that is not
//! enforced degrades into documentation that is wrong.
//!
//! **And each of the four was refused by its own would-be client, in writing.**
//! `Text` is the one a language layer would want for a string literal, and
//! `rts-codegen`'s `emit/mod.rs` states the decision against it: putting the
//! bytes in the compiled image "would make the text part of the code", so a
//! literal is referred to by an index into a table the host seeds instead —
//! the same shape as the two agreements that already exist. `Symbol` addresses a
//! function by string where the registry already numbers it, which is what
//! `Inst::FuncAddr` exists to do and what its documentation calls "the wrong
//! mechanism anyway".
//!
//! So the four are gone rather than implemented. What that buys beyond a smaller
//! enum is that **lowering a constant can no longer fail**: `lower_const` is
//! total, and a whole `NotYetLowered` path stops existing.
//!
//! What brings one back is a client that needs it, arriving with the lowering
//! that makes it mean something — not the declaration on its own.

use crate::repr::Repr;

/// A scalar constant's bits, interpreted according to its representation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ScalarBits(pub u64);

/// A declared constant.
///
/// # Why an enum with one variant
///
/// Because the question it answers has more than one possible answer, and the
/// day a second kind arrives it arrives here. A struct would be the same data
/// with the shape of the decision erased, and every construction site would then
/// have to change back. Sixty sites spell `ConstDecl::Scalar { .. }` today; the
/// name is what says which kind of constant they mean.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum ConstDecl {
    /// A value that fits in a register.
    Scalar {
        /// How it exists on the machine.
        repr: Repr,
        /// Its bits.
        bits: ScalarBits,
    },
}

impl ConstDecl {
    /// The representation a value materialized from this constant has.
    pub fn repr(&self) -> Repr {
        match self {
            ConstDecl::Scalar { repr, .. } => *repr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_declarations_are_the_same_constant() {
        // What `Function::push_const` keys its pool on, and therefore the
        // property that makes deduplication mean anything. Stated here because
        // it is a property of this type rather than of the pool.
        let one = ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(7),
        };
        let same = ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(7),
        };
        assert_eq!(one, same);
    }

    #[test]
    fn the_same_bits_under_two_representations_are_two_constants() {
        // The bits alone do not identify a constant, and merging on them would
        // hand a site an integer where it declared a double — the same eight
        // bytes meaning two different things.
        let integer = ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(0),
        };
        let floating = ConstDecl::Scalar {
            repr: Repr::F64,
            bits: ScalarBits(0),
        };
        assert_ne!(integer, floating);
        assert_eq!(integer.repr(), Repr::I64);
        assert_eq!(floating.repr(), Repr::F64);
    }
}
