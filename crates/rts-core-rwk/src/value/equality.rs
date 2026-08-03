//! The three equalities.
//!
//! Every one of these could have been `a.bits() == b.bits()`, and every one of
//! them would have been wrong. The bits differ for `+0` and `-0`, which two of
//! the three call equal; the bits can match for two `NaN`s, which two of the
//! three call unequal. A bit compare gets both cells wrong and neither loudly.

use super::Value;

/// `===`.
///
/// Two cells are not a bit compare:
///
/// - `+0 === -0` is **true**, and their bit patterns differ.
/// - `NaN === NaN` is **false**, even for two NaNs with identical bits — and
///   *especially* then, because that is the case a bit compare gets wrong while
///   looking right.
/// - **Two strings are equal when their TEXT is**, however they were allocated.
///   Objects compare by identity and strings do not, and a comparison that
///   treated a reference as a reference would make `"a" === "a"` false whenever
///   the two were built separately.
///
/// The last one is why `same_text` is a parameter: deciding it needs the heap,
/// which this module does not have. A caller with no strings passes something
/// that always answers false.
pub fn strict_equals(left: Value, right: Value, same_text: impl Fn(u32, u32) -> bool) -> bool {
    match (left.numeric(), right.numeric()) {
        // Numbers compare as numbers, whichever representation holds them, so
        // `1 === 1.0` is true across an int and a double.
        (Some(a), Some(b)) => a == b,
        // A number is never strictly equal to a non-number.
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => identical(left, right, &same_text),
    }
}

/// Whether two non-numbers are the same value.
///
/// The same bits settle it for a singleton, a boolean, and two references to one
/// slot. What the bits cannot settle is two DIFFERENT slots holding equal text,
/// which `===` calls equal and identity does not.
fn identical(left: Value, right: Value, same_text: &impl Fn(u32, u32) -> bool) -> bool {
    if left.bits() == right.bits() {
        return true;
    }
    match (left.as_slot(), right.as_slot()) {
        (Some(a), Some(b)) => same_text(a, b),
        _ => false,
    }
}

/// `Object.is`, and what the specification calls `SameValue`.
///
/// Differs from `===` in exactly the two cells: `NaN` is the same value as
/// `NaN`, and `+0` is not the same value as `-0`.
///
/// Used wherever the language needs an equality that can *distinguish* zeros —
/// `Object.defineProperty` deciding whether a write changes anything, and the
/// proxy invariant checks.
pub fn same_value(left: Value, right: Value, same_text: impl Fn(u32, u32) -> bool) -> bool {
    match (left.numeric(), right.numeric()) {
        (Some(a), Some(b)) => {
            if a.is_nan() && b.is_nan() {
                return true;
            }
            // `to_bits` separates the zeros, and is safe to reach for only
            // because NaN was handled above — an uncanonicalised NaN would
            // otherwise make two NaNs unequal here.
            a.to_bits() == b.to_bits()
        }
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => identical(left, right, &same_text),
    }
}

/// What `Map`, `Set` and `Array.prototype.includes` key on.
///
/// `NaN` is equal to itself — which is the entire reason `map.set(NaN, 1)`
/// works — and `+0` equals `-0`, so a map does not grow two entries for two
/// spellings of zero.
///
/// The difference from `===` is one cell and it is the one that matters:
/// hashing a map on `===` silently produces a second entry every time `NaN` is
/// used as a key, and nothing anywhere reports it.
pub fn same_value_zero(left: Value, right: Value, same_text: impl Fn(u32, u32) -> bool) -> bool {
    match (left.numeric(), right.numeric()) {
        (Some(a), Some(b)) => {
            if a.is_nan() && b.is_nan() {
                return true;
            }
            a == b
        }
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => identical(left, right, &same_text),
    }
}

#[cfg(test)]
mod tests {
    use rts_cranelift::tags::{TAG_REFERENCE, encode};

    use super::*;

    /// A heap with no strings in it. Most of these tests are about numbers and
    /// singletons, where the answer does not depend on text.
    fn no_text(_: u32, _: u32) -> bool {
        false
    }

    fn nan() -> Value {
        Value::from_f64(f64::NAN)
    }

    #[test]
    fn the_three_equalities_differ_in_exactly_two_cells() {
        let positive = Value::from_f64(0.0);
        let negative = Value::from_f64(-0.0);

        // NaN vs NaN
        assert!(!strict_equals(nan(), nan(), no_text), "=== says no");
        assert!(same_value(nan(), nan(), no_text), "Object.is says yes");
        assert!(same_value_zero(nan(), nan(), no_text), "a Map key says yes");

        // +0 vs -0
        assert!(strict_equals(positive, negative, no_text), "=== says yes");
        assert!(
            !same_value(positive, negative, no_text),
            "Object.is says no"
        );
        assert!(
            same_value_zero(positive, negative, no_text),
            "a Map key says yes"
        );
    }

    #[test]
    fn none_of_them_is_a_bit_compare() {
        let positive = Value::from_f64(0.0);
        let negative = Value::from_f64(-0.0);
        assert_ne!(
            positive.bits(),
            negative.bits(),
            "the bits differ, and two of the three still call them equal"
        );
        assert_eq!(
            nan().bits(),
            nan().bits(),
            "the bits match, and === still calls them unequal"
        );
        assert!(!strict_equals(nan(), nan(), no_text));
    }

    #[test]
    fn a_number_is_a_number_whichever_representation_holds_it() {
        assert!(strict_equals(
            Value::from_i32(1),
            Value::from_f64(1.0),
            no_text
        ));
        assert!(same_value(
            Value::from_i32(1),
            Value::from_f64(1.0),
            no_text
        ));
    }

    #[test]
    fn a_boolean_is_not_the_number_it_would_coerce_to() {
        assert!(!strict_equals(
            Value::from_bool(true),
            Value::from_i32(1),
            no_text
        ));
        assert!(!same_value(
            Value::from_bool(true),
            Value::from_i32(1),
            no_text
        ));
    }

    #[test]
    fn a_reference_is_equal_only_to_itself() {
        let one = Value(encode(TAG_REFERENCE, 7));
        let same = Value(encode(TAG_REFERENCE, 7));
        let other = Value(encode(TAG_REFERENCE, 8));

        assert!(
            strict_equals(one, same, no_text),
            "the same slot is the same object"
        );
        assert!(!strict_equals(one, other, no_text));
    }
}
