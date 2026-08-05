//! What a JavaScript value is, at run time.
//!
//! # This module owns no bits
//!
//! The encoding belongs to [`rts_cranelift::tags`] and is not restated here.
//! Every construction and every inspection below **calls** it — `encode_double`,
//! `is_encoded`, `tag_of`, `payload_of` — rather than re-deriving the same
//! arithmetic from the same constants.
//!
//! That is not tidiness. A rule written in two places is a rule that will be
//! written differently: this module's first draft canonicalised `NaN` as
//! `f64::NAN.to_bits()` while the machine used `CANONICAL_NAN`. They agree on
//! this target and there is no reason they must, and a value encoded by one and
//! read by the other is the kind of disagreement that shows up as a wrong
//! object rather than as a crash.
//!
//! So what is this module for, if it owns no bits? For what the machine
//! deliberately has no opinion about: **what the values mean**.
//!
//! # Three equalities, and they differ in two cells
//!
//! JavaScript has `===`, `SameValue` and `SameValueZero`, and they disagree only
//! about `NaN` and about `+0` versus `-0`:
//!
//! | | `NaN` vs `NaN` | `+0` vs `-0` |
//! |---|---|---|
//! | `===` ([`strict_equals`]) | false | **true** |
//! | `SameValue` ([`same_value`]) | **true** | false |
//! | `SameValueZero` ([`same_value_zero`]) | **true** | **true** |
//!
//! Each is used where the others would be wrong. `Object.is` is `SameValue`.
//! `Map` and `Set` key on `SameValueZero`, which is why `NaN` works as a key.
//! `Array.prototype.indexOf` uses `===`, which is why it cannot find one.
//!
//! Getting these wrong does not crash. `map.set(NaN, 1)` twice quietly produces
//! two entries, and nothing points at the equality that did it.
//!
//! A NaN-boxed value is a bit pattern and comparing bit patterns is one
//! instruction — and wrong for both cells, since `+0` and `-0` have different
//! bits and are `===`, while two `NaN`s can have identical bits and are not.

use rts_cranelift::tags::{
    BOOL_FALSE, BOOL_TRUE, TAG_BOOL, TAG_INT32, TAG_REFERENCE, TAG_SINGLETON, decode_double,
    encode, encode_double, is_encoded, payload_of, tag_of,
};

mod convert;
mod equality;

pub use convert::{Kinds, Singletons, to_boolean, to_int32, to_number, to_uint32};
pub use equality::{same_value, same_value_zero, strict_equals};

/// One JavaScript value, as the program holds it.
///
/// A transparent wrapper over the machine's word. Transparent so it crosses the
/// ABI as one register with no conversion; a wrapper so the meaning below has
/// somewhere to live and so a `u64` from elsewhere cannot be mistaken for one.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Value(pub u64);

/// What kind of thing a value is.
///
/// Not `typeof`. `typeof` is a language question with a language answer —
/// including `"object"` for `null`, a mistake from 1995 that this layer declines
/// to reproduce. This is the representation question, and the two are
/// deliberately separate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A double that is genuinely a double.
    Float,
    /// A small integer, held inline.
    Int,
    /// A boolean.
    Bool,
    /// One of the values there is exactly one of, identified by its number.
    Singleton(u32),
    /// Something on the heap, identified by its slot.
    Reference(u64),
    /// A kind the **language** declared, carrying whatever payload it chose.
    ///
    /// # Why this is not four more variants
    ///
    /// Because this layer does not know what they are. The machine reserves
    /// four tags for what it defines itself and hands the rest out through
    /// `TagRegistry::declare_kind`; which language put what there is the
    /// language's business, and naming `Symbol` here would be this crate
    /// learning a fact rule 4 says it must be told instead.
    ///
    /// # Why the catch-all it replaces was a bug waiting
    ///
    /// `kind` used to answer `Reference` for every tag it did not recognise. So
    /// the first client kind ever declared would have read back as a heap slot,
    /// and `as_slot` would have handed its payload to the region — a wrong
    /// answer that runs, found by adding the first one rather than by reading.
    Client {
        /// Which tag the registry issued.
        tag: u8,
        /// What the language put in it.
        payload: u64,
    },
}

impl Value {
    /// The bits.
    pub fn bits(self) -> u64 {
        self.0
    }

    /// A double.
    ///
    /// `NaN` canonicalisation happens in the machine's `encode_double`, and the
    /// reason it must happen somewhere is worth knowing here: an arithmetic
    /// `NaN` carries whatever payload the hardware chose, which can land in the
    /// encoded quadrant — at which point an ordinary arithmetic result reads
    /// back as a reference and names whatever heap slot those bits spell.
    pub fn from_f64(value: f64) -> Self {
        Value(encode_double(value))
    }

    /// A small integer.
    pub fn from_i32(value: i32) -> Self {
        Value(encode(TAG_INT32, u64::from(value as u32)))
    }

    /// A boolean.
    pub fn from_bool(value: bool) -> Self {
        Value(encode(TAG_BOOL, if value { BOOL_TRUE } else { BOOL_FALSE }))
    }

    /// A singleton, by the number the machine's registry gave it.
    pub fn from_singleton(number: u32) -> Self {
        Value(encode(TAG_SINGLETON, u64::from(number)))
    }

    /// A reference to a heap slot.
    pub fn from_slot(slot: u32) -> Self {
        Value(encode(TAG_REFERENCE, u64::from(slot)))
    }

    /// Whether this is an encoded pattern rather than a genuine double.
    pub fn is_boxed(self) -> bool {
        is_encoded(self.0)
    }

    /// What kind of thing this is.
    pub fn kind(self) -> Kind {
        if !is_encoded(self.0) {
            return Kind::Float;
        }
        let payload = payload_of(self.0);
        match tag_of(self.0) {
            TAG_INT32 => Kind::Int,
            TAG_BOOL => Kind::Bool,
            TAG_SINGLETON => Kind::Singleton(payload as u32),
            TAG_REFERENCE => Kind::Reference(payload),
            tag => Kind::Client { tag, payload },
        }
    }

    /// A value of a kind the language declared.
    pub fn from_client(tag: u8, payload: u64) -> Self {
        Value(encode(tag, payload))
    }

    /// The payload this carries, if it is of the given declared kind.
    ///
    /// The tag is passed in rather than compared against a constant, which is
    /// the whole of rule 4 applied to the encoding: this crate is told which
    /// number means which language kind and never decides.
    pub fn as_client(self, tag: u8) -> Option<u64> {
        match self.kind() {
            Kind::Client { tag: held, payload } if held == tag => Some(payload),
            _ => None,
        }
    }

    /// The double this holds, if it is one.
    pub fn as_f64(self) -> Option<f64> {
        (!is_encoded(self.0)).then(|| decode_double(self.0))
    }

    /// The integer this holds, if it is one.
    pub fn as_i32(self) -> Option<i32> {
        matches!(self.kind(), Kind::Int).then(|| payload_of(self.0) as u32 as i32)
    }

    /// The boolean this holds, if it is one.
    pub fn as_bool(self) -> Option<bool> {
        matches!(self.kind(), Kind::Bool).then(|| payload_of(self.0) == BOOL_TRUE)
    }

    /// The heap slot this names, if it names one.
    pub fn as_slot(self) -> Option<u32> {
        match self.kind() {
            Kind::Reference(slot) => Some(slot as u32),
            _ => None,
        }
    }

    /// The number this holds, whether an integer or a double.
    ///
    /// Not a conversion — this answers only for values that already *are*
    /// numbers, and says nothing for a string that looks like one. That is
    /// [`to_number`], and keeping them apart is what stops a coercion happening
    /// where nobody meant one.
    pub fn numeric(self) -> Option<f64> {
        match self.kind() {
            Kind::Float => Some(decode_double(self.0)),
            Kind::Int => Some(f64::from(payload_of(self.0) as u32 as i32)),
            _ => None,
        }
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.kind() {
            Kind::Float => write!(f, "Value({})", decode_double(self.0)),
            Kind::Int => write!(f, "Value({}i32)", payload_of(self.0) as u32 as i32),
            Kind::Bool => write!(f, "Value({})", payload_of(self.0) == BOOL_TRUE),
            Kind::Singleton(id) => write!(f, "Value(singleton {id})"),
            Kind::Reference(slot) => write!(f, "Value(ref {slot})"),
            Kind::Client { tag, payload } => write!(f, "Value(kind {tag} of {payload})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arithmetic_nan_never_reads_back_as_a_reference() {
        // A NaN from arithmetic can carry any payload the hardware chose,
        // including one landing in the encoded quadrant.
        let hostile = decode_double_unchecked(encode(TAG_REFERENCE, 42));
        assert!(hostile.is_nan(), "the bit pattern is a NaN");

        let value = Value::from_f64(hostile);
        assert_eq!(
            value.kind(),
            Kind::Float,
            "canonicalising is what stops an ordinary arithmetic result from \
             naming heap slot 42"
        );
        assert!(value.as_f64().unwrap().is_nan());
    }

    /// The machine's `decode_double` asserts the word is not encoded, which is
    /// exactly what this test needs to violate on purpose.
    fn decode_double_unchecked(bits: u64) -> f64 {
        f64::from_bits(bits)
    }

    #[test]
    fn every_kind_round_trips() {
        assert_eq!(Value::from_i32(-7).as_i32(), Some(-7));
        assert_eq!(Value::from_i32(i32::MIN).as_i32(), Some(i32::MIN));
        assert_eq!(Value::from_bool(true).as_bool(), Some(true));
        assert_eq!(Value::from_bool(false).as_bool(), Some(false));
        assert_eq!(Value::from_f64(1.5).as_f64(), Some(1.5));
        assert_eq!(Value::from_f64(-0.0).as_f64(), Some(-0.0));
        assert_eq!(Value::from_slot(9).as_slot(), Some(9));
        assert_eq!(Value::from_singleton(3).kind(), Kind::Singleton(3));
    }

    #[test]
    fn a_kind_accessor_answers_for_its_own_kind_and_no_other() {
        let integer = Value::from_i32(1);
        assert!(
            integer.as_f64().is_none(),
            "an int is not stored as a double"
        );
        assert!(integer.as_bool().is_none());
        assert!(integer.as_slot().is_none());

        assert!(
            Value::from_bool(true).as_i32().is_none(),
            "true is not the integer 1"
        );
    }

    #[test]
    fn numeric_reads_a_number_and_refuses_to_coerce() {
        assert_eq!(Value::from_i32(3).numeric(), Some(3.0));
        assert_eq!(Value::from_f64(3.5).numeric(), Some(3.5));
        assert_eq!(
            Value::from_bool(true).numeric(),
            None,
            "`true` becomes 1 only where the language asked for a coercion"
        );
    }
}
