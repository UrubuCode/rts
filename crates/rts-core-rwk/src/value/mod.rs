//! What a JavaScript value is, at run time.
//!
//! The language layer decides what a value *means*; the machine layer decides
//! how sixty-four bits are arranged. This is the third thing: the operations a
//! program performs on values, which belong to neither and are called by both.
//!
//! # Why the encoding is restated here rather than shared
//!
//! The machine layer's [`tags`] module owns the bit layout, and this module has
//! to agree with it exactly. It agrees by *reading the same constants*, not by
//! copying them: `BOX_BASE` and the tag numbers come from `rts-cranelift`, so a
//! change there is a compile error here rather than a value silently read as
//! the wrong kind.
//!
//! [`tags`]: rts_cranelift::tags
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
//! Each is used somewhere the others would be wrong. `Object.is` is
//! `SameValue`. `Map` and `Set` key on `SameValueZero`, which is why `NaN` works
//! as a key at all. `Array.prototype.indexOf` uses `===`, which is why it cannot
//! find one.
//!
//! Getting these wrong does not crash. `map.set(NaN, 1)` twice quietly produces
//! two entries, and nothing points at the equality that did it.
//!
//! # The trap this module exists to keep out of the lowering
//!
//! A NaN-boxed value is a bit pattern, and comparing bit patterns is one
//! instruction. It is also wrong for both of the cells above: `+0` and `-0` have
//! different bits and are `===`, and two `NaN`s can have identical bits and are
//! not. Every function here that could have been a bit compare and is not says
//! so at its definition.

use rts_cranelift::tags::{BOX_BASE, TAG_BOOL, TAG_INT32, TAG_REFERENCE, TAG_SINGLETON, tag_of};

mod convert;
mod equality;

pub use convert::{to_boolean, to_int32, to_number, to_uint32};
pub use equality::{same_value, same_value_zero, strict_equals};

/// One JavaScript value, as the program holds it.
///
/// A transparent wrapper over the machine's sixty-four bits. Transparent so it
/// crosses the ABI as one word with no conversion; a wrapper so that the
/// operations below have somewhere to live and so that a raw `u64` from
/// somewhere else cannot be mistaken for one.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Value(pub u64);

/// What kind of thing a value is.
///
/// Not `typeof`. `typeof` is a language question with a language answer —
/// including `"object"` for `null`, which is a mistake from 1995 that this layer
/// declines to reproduce. This is the representation question, and the two are
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
}

impl Value {
    /// The bits.
    pub fn bits(self) -> u64 {
        self.0
    }

    /// A double.
    ///
    /// Canonicalises `NaN`. Every arithmetic operation on this platform can
    /// produce a `NaN`, and an uncanonicalised one may land anywhere in the
    /// quadrant the boxed space uses — at which point a perfectly ordinary
    /// arithmetic result reads back as a reference, and the slot it names is
    /// whatever happened to be at those bits. One `f64::NAN` here is the whole
    /// defence.
    pub fn from_f64(value: f64) -> Self {
        Value(if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }

    /// A small integer.
    pub fn from_i32(value: i32) -> Self {
        Value(BOX_BASE | (u64::from(TAG_INT32) << 48) | u64::from(value as u32))
    }

    /// A boolean.
    pub fn from_bool(value: bool) -> Self {
        Value(BOX_BASE | (u64::from(TAG_BOOL) << 48) | u64::from(value))
    }

    /// Whether this is a boxed pattern rather than a genuine double.
    pub fn is_boxed(self) -> bool {
        (self.0 & BOX_BASE) == BOX_BASE
    }

    /// What kind of thing this is.
    pub fn kind(self) -> Kind {
        if !self.is_boxed() {
            return Kind::Float;
        }
        let payload = self.0 & 0x0000_FFFF_FFFF_FFFF;
        match tag_of(self.0) {
            TAG_INT32 => Kind::Int,
            TAG_BOOL => Kind::Bool,
            TAG_SINGLETON => Kind::Singleton(payload as u32),
            TAG_REFERENCE => Kind::Reference(payload),
            _ => Kind::Reference(payload),
        }
    }

    /// The double this holds, if it is one.
    pub fn as_f64(self) -> Option<f64> {
        (!self.is_boxed()).then(|| f64::from_bits(self.0))
    }

    /// The integer this holds, if it is one.
    pub fn as_i32(self) -> Option<i32> {
        matches!(self.kind(), Kind::Int).then(|| self.0 as u32 as i32)
    }

    /// The boolean this holds, if it is one.
    pub fn as_bool(self) -> Option<bool> {
        matches!(self.kind(), Kind::Bool).then(|| (self.0 & 1) != 0)
    }

    /// The heap slot this names, if it names one.
    pub fn as_reference(self) -> Option<u64> {
        match self.kind() {
            Kind::Reference(slot) => Some(slot),
            _ => None,
        }
    }

    /// The number this holds, whether it is stored as an integer or a double.
    ///
    /// Not a conversion — this answers only for values that already *are*
    /// numbers, and says nothing for a string that looks like one. That is
    /// [`to_number`], and keeping them apart is what stops a coercion happening
    /// somewhere nobody meant one to.
    pub fn numeric(self) -> Option<f64> {
        match self.kind() {
            Kind::Float => Some(f64::from_bits(self.0)),
            Kind::Int => Some(f64::from(self.0 as u32 as i32)),
            _ => None,
        }
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.kind() {
            Kind::Float => write!(f, "Value({})", f64::from_bits(self.0)),
            Kind::Int => write!(f, "Value({}i32)", self.0 as u32 as i32),
            Kind::Bool => write!(f, "Value({})", (self.0 & 1) != 0),
            Kind::Singleton(id) => write!(f, "Value(singleton {id})"),
            Kind::Reference(slot) => write!(f, "Value(ref {slot})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arithmetic_nan_never_reads_back_as_a_reference() {
        // A NaN produced by arithmetic can carry any payload the hardware
        // chose, including one that lands in the boxed quadrant.
        let hostile = f64::from_bits(BOX_BASE | (u64::from(TAG_REFERENCE) << 48) | 42);
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

    #[test]
    fn every_kind_round_trips() {
        assert_eq!(Value::from_i32(-7).as_i32(), Some(-7));
        assert_eq!(Value::from_i32(i32::MIN).as_i32(), Some(i32::MIN));
        assert_eq!(Value::from_bool(true).as_bool(), Some(true));
        assert_eq!(Value::from_bool(false).as_bool(), Some(false));
        assert_eq!(Value::from_f64(1.5).as_f64(), Some(1.5));
        assert_eq!(Value::from_f64(-0.0).as_f64(), Some(-0.0));
    }

    #[test]
    fn a_kind_accessor_answers_for_its_own_kind_and_no_other() {
        let integer = Value::from_i32(1);
        assert!(
            integer.as_f64().is_none(),
            "an int is not stored as a double"
        );
        assert!(integer.as_bool().is_none());
        assert!(integer.as_reference().is_none());

        let boolean = Value::from_bool(true);
        assert!(boolean.as_i32().is_none(), "true is not the integer 1");
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
