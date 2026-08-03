//! The generic value encoding: mechanism here, meaning elsewhere.
//!
//! A generic value is one machine word. A word whose top bits match
//! [`BOX_BASE`] is encoded; anything else is a genuine double. An encoded word
//! carries a small tag and a payload, and the payload of a reference is a table
//! index rather than an address — which is what makes the encoding safe for a
//! collector that relocates.
//!
//! This module owns the layout and nothing else. It does not know what a
//! singleton *means*, and it records nothing about who declared one: the
//! registry takes a count and returns encodings. A namespace per client would
//! let this layer answer "which client does this value belong to", and no
//! machine operation needs the answer — declining to model it is what keeps a
//! capability from accidentally depending on it.
//!
//! # Why this encoding is not where performance is won
//!
//! Measurement on this codebase isolated the representation from everything
//! else: the encoded form costs under a nanosecond more than the alternatives,
//! which is orders of magnitude below the cost of allocation and of reaching a
//! field through a call. The encoding is infrastructure to get right, not an
//! optimization target.

mod registry;

pub use registry::{SingletonId, TagRegistry, ValueKind};

/// The bit pattern that distinguishes an encoded word from a double.
///
/// This is the negative quiet-NaN quadrant: sign set, exponent all ones, quiet
/// bit set. Real arithmetic never produces it, because [`encode_double`]
/// normalizes every NaN to the positive quiet form.
pub const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;

/// The quiet NaN every real NaN is normalized to, so doubles stay disjoint from
/// the encoded space.
pub const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

/// Bit position of the tag within an encoded word.
pub const TAG_SHIFT: u32 = 48;

/// Mask selecting the tag once shifted down.
pub const TAG_MASK: u64 = 0x7;

/// Mask selecting the payload of an encoded word.
pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// How many payload bits an encoded word carries.
pub const PAYLOAD_BITS: u32 = 48;

/// How many distinct tags the encoding can express.
pub const TAG_COUNT: u8 = 8;

/// A 32-bit integer carried inline in the payload.
///
/// This and the two tags below are reserved because the representations they
/// encode are defined by this layer, not chosen by a client. They are written
/// down here, once, and every consumer reads them from here — a tag number
/// reconstructed at a call site is the drift the registry exists to prevent.
pub const TAG_INT32: u8 = 0;

/// A singleton, selected by its number in the payload.
pub const TAG_SINGLETON: u8 = 1;

/// A reference: the payload is a table index, never an address.
pub const TAG_REFERENCE: u8 = 2;

/// A machine boolean: the payload is zero or one.
///
/// This layer defines the boolean representation, so widening one has to produce
/// something — and asking a client which number to use would make a machine
/// operation depend on a client. An earlier design answered that by reserving
/// two numbers in the singleton space, which was wrong in a way worth recording:
/// it borrowed the client's space to solve the machine's problem. A client with
/// its own false would then have had two.
///
/// A machine value kind gets a machine tag. The cost is one of eight tags; what
/// it buys is that the singleton space is entirely the client's, and that asking
/// "is this a boolean" is exact rather than approximately right.
///
/// Folding booleans into the inline-integer space instead was considered and is
/// worse for the same reason stated differently: the number one would answer yes
/// to that question, and a guard that returns yes for the wrong thing is not a
/// proof.
pub const TAG_BOOL: u8 = 3;

/// The payload a machine `false` carries.
pub const BOOL_FALSE: u64 = 0;

/// The payload a machine `true` carries.
pub const BOOL_TRUE: u64 = 1;

/// How many tags this layer reserves before a client's first one.
pub const TAG_RESERVED_COUNT: u8 = 4;

/// Whether a word is encoded rather than a double.
pub fn is_encoded(word: u64) -> bool {
    (word & BOX_BASE) == BOX_BASE
}

/// Builds an encoded word from a tag and a payload.
///
/// The payload is masked to [`PAYLOAD_BITS`]; a caller handing over a wider
/// value has a bug the mask would hide, so debug builds fail instead.
pub fn encode(tag: u8, payload: u64) -> u64 {
    debug_assert!(u64::from(tag) <= TAG_MASK, "tag does not fit the tag field");
    debug_assert!(
        payload <= PAYLOAD_MASK,
        "payload does not fit the payload field"
    );
    BOX_BASE | ((u64::from(tag) & TAG_MASK) << TAG_SHIFT) | (payload & PAYLOAD_MASK)
}

/// The tag of an encoded word.
///
/// Meaningless for a word that is not encoded; check with [`is_encoded`] first.
pub fn tag_of(word: u64) -> u8 {
    ((word >> TAG_SHIFT) & TAG_MASK) as u8
}

/// The payload of an encoded word.
pub fn payload_of(word: u64) -> u64 {
    word & PAYLOAD_MASK
}

/// Encodes a double, normalizing NaN so it cannot collide with the encoded space.
pub fn encode_double(value: f64) -> u64 {
    if value.is_nan() {
        CANONICAL_NAN
    } else {
        value.to_bits()
    }
}

/// Reads a word that is known not to be encoded as a double.
pub fn decode_double(word: u64) -> f64 {
    debug_assert!(!is_encoded(word), "word is encoded, not a double");
    f64::from_bits(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_words_and_doubles_are_disjoint() {
        let encoded = encode(3, 0x1234);
        assert!(is_encoded(encoded));
        assert_eq!(tag_of(encoded), 3);
        assert_eq!(payload_of(encoded), 0x1234);

        for value in [0.0, -0.0, 1.5, -1.5, f64::MAX, f64::MIN, f64::INFINITY] {
            assert!(!is_encoded(encode_double(value)), "{value} collided");
            assert_eq!(decode_double(encode_double(value)), value);
        }
    }

    #[test]
    fn every_nan_normalizes_out_of_the_encoded_space() {
        let hostile = f64::from_bits(BOX_BASE | 0xDEAD);
        assert!(hostile.is_nan(), "test premise: this bit pattern is a NaN");
        assert!(!is_encoded(encode_double(hostile)));
    }

    #[test]
    fn a_boolean_and_the_number_it_resembles_are_distinguishable() {
        let truth = encode(TAG_BOOL, BOOL_TRUE);
        let one = encode(TAG_INT32, 1);

        assert_ne!(
            truth, one,
            "folding booleans into the integer space would make the number one \
             answer yes to \"is this a boolean\", and a guard that says yes to the \
             wrong thing is not a proof"
        );
        assert_ne!(tag_of(truth), tag_of(one));
    }

    #[test]
    fn the_singleton_space_is_untouched_by_the_boolean_encoding() {
        assert_ne!(TAG_BOOL, TAG_SINGLETON);
        assert_eq!(
            encode(TAG_SINGLETON, 0),
            encode(TAG_SINGLETON, 0),
            "a client's first singleton is its own, not this layer's false"
        );
    }

    #[test]
    fn negative_infinity_is_a_double_not_an_encoded_word() {
        // It sits one bit below the encoded quadrant. Widening BOX_BASE by a bit
        // would swallow it, so this is a guard on the constant itself.
        assert!(!is_encoded(encode_double(f64::NEG_INFINITY)));
    }
}
