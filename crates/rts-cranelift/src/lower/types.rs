//! Representations, in the code generator's vocabulary.
//!
//! One direction only. A representation says how a value exists on the machine
//! and the code generator's type says how it exists in a register, so the map
//! from the first to the second is total. The reverse is not: several
//! representations share a machine type, and reconstructing which one a value
//! had is exactly the guessing this layer exists to remove.

use cranelift_codegen::ir::{Type, types};

use crate::repr::Repr;

/// The machine type a representation occupies.
///
/// A boolean is a full machine word rather than a byte. Narrower than a word
/// invites a caller to assume the upper bits mean something, and at every
/// boundary this layer has — a call, a block parameter, a spill slot — a word is
/// what is actually moved.
///
/// References and generic values are also words: a reference's payload is a
/// table index, and a generic value is one encoded word by construction.
///
/// **One boundary disagrees, and it is not this one.** A boolean *returned* by a
/// foreign function is a byte, because that is what the callee defines — see
/// `super::abi_return_type`. That is a fact about somebody else's function
/// rather than about the representation, which is why it lives at the signature
/// and not here.
pub fn machine_type(repr: Repr) -> Type {
    match repr {
        Repr::I8 => types::I8,
        Repr::I16 => types::I16,
        Repr::I32 => types::I32,
        Repr::F32 => types::F32,
        Repr::F64 => types::F64,
        Repr::I64 | Repr::Bool | Repr::Ref(_) | Repr::Tagged => types::I64,
    }
}

/// Whether a representation occupies a machine word exactly.
///
/// Used where a value has to be moved as a word regardless of what it means —
/// encoding it, spilling it, or handing it to something that takes words.
pub fn is_word(repr: Repr) -> bool {
    machine_type(repr) == types::I64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::RefKind;

    #[test]
    fn a_boolean_is_a_word_not_a_byte() {
        assert_eq!(machine_type(Repr::Bool), types::I64);
    }

    #[test]
    fn every_reference_and_generic_value_is_one_word() {
        assert!(is_word(Repr::Ref(RefKind::Bytes)));
        assert!(is_word(Repr::Ref(RefKind::Callable)));
        assert!(is_word(Repr::Tagged));
    }

    #[test]
    fn narrow_integers_keep_their_width() {
        assert_eq!(machine_type(Repr::I8), types::I8);
        assert_eq!(machine_type(Repr::I16), types::I16);
        assert_eq!(machine_type(Repr::I32), types::I32);
    }
}
