//! Conversions between representations.
//!
//! The ones here need no heap: a value that is already a number, a boolean or a
//! singleton converts by arithmetic. Anything reaching a string or calling an
//! object's `valueOf` needs the heap and belongs with the code that has one —
//! kept out of here so that this module compiles and is testable on a target
//! with no allocator at all.

use super::{Kind, Value};

/// Which singletons exist, as far as conversion is concerned.
///
/// The language layer decides what its singletons *mean* and numbers them; this
/// layer only needs to know which are falsy and which convert to what. Passed
/// in rather than assumed, because a second language on this machine would
/// number its own differently — and hardcoding JavaScript's numbering here is
/// exactly the kind of knowledge this crate is not supposed to hold.
#[derive(Clone, Copy, Debug)]
pub struct Singletons {
    /// The one that reads as `NaN` when converted to a number.
    pub undefined: u32,
    /// The one that reads as `+0`.
    pub null: u32,
}

/// The falsy set.
///
/// `undefined`, `null`, `false`, `+0`, `-0`, `NaN`, and the empty string. Every
/// other value is truthy — *every* object, including an empty one, an empty
/// array, and `new Boolean(false)`. Nothing is unwrapped, and no `valueOf` runs,
/// which is why this needs no heap.
///
/// The empty string is the one case this cannot answer alone, so a caller that
/// may hold a string passes `string_is_empty`. Making it a parameter rather than
/// reaching for the heap keeps the whole falsy rule in one place while leaving
/// the one heap question with the code that owns the heap.
pub fn to_boolean(
    value: Value,
    singletons: Singletons,
    string_is_empty: impl Fn(u64) -> bool,
) -> bool {
    match value.kind() {
        Kind::Float => {
            let number = f64::from_bits(value.bits());
            // Catches +0, -0 and NaN in one condition: NaN fails every
            // comparison, and both zeros fail `!= 0.0`.
            !(number == 0.0 || number.is_nan())
        }
        Kind::Int => value.as_i32() != Some(0),
        Kind::Bool => value.as_bool() == Some(true),
        Kind::Singleton(id) => id != singletons.undefined && id != singletons.null,
        Kind::Reference(slot) => !string_is_empty(slot),
    }
}

/// `ToNumber`, for the values that need no heap.
///
/// Returns `None` for a reference, which needs `ToPrimitive` and therefore a
/// heap and possibly a call into user code. An absent answer is the honest one:
/// guessing zero here is how a conversion that should have run a `valueOf` turns
/// into a wrong number nobody traces back.
pub fn to_number(value: Value, singletons: Singletons) -> Option<f64> {
    match value.kind() {
        Kind::Float | Kind::Int => value.numeric(),
        // `true` is 1 and `false` is +0.
        Kind::Bool => Some(if value.as_bool() == Some(true) {
            1.0
        } else {
            0.0
        }),
        Kind::Singleton(id) if id == singletons.undefined => Some(f64::NAN),
        Kind::Singleton(id) if id == singletons.null => Some(0.0),
        Kind::Singleton(_) | Kind::Reference(_) => None,
    }
}

/// `ToInt32`: what every bitwise operator does to its operands.
///
/// Modular, not saturating and not a `as` cast: `2**32` becomes `0`, and
/// `2**31` becomes `i32::MIN`. Anything non-finite becomes zero, which is why
/// `NaN | 0` is `0` rather than an error.
pub fn to_int32(number: f64) -> i32 {
    if !number.is_finite() {
        return 0;
    }
    // Truncate toward zero, then reduce modulo 2^32 and reinterpret the low
    // half as signed. `as i32` alone would saturate, which is the wrong
    // arithmetic: it makes `2**31 | 0` produce i32::MAX instead of i32::MIN.
    let truncated = number.trunc();
    let wrapped = truncated.rem_euclid(4_294_967_296.0);
    wrapped as u32 as i32
}

/// `ToUint32`: what `>>>` does to its left operand.
///
/// The reason `>>>` is the one bitwise operator whose result does not fit a
/// signed 32-bit value — it ranges to 2³²−1, so `(-1) >>> 0` is `4294967295`.
pub fn to_uint32(number: f64) -> u32 {
    to_int32(number) as u32
}

#[cfg(test)]
mod tests {
    use rts_cranelift::tags::{TAG_REFERENCE, TAG_SINGLETON, encode};

    use super::*;

    const S: Singletons = Singletons {
        undefined: 0,
        null: 1,
    };

    fn never_empty(_: u64) -> bool {
        false
    }

    #[test]
    fn the_falsy_set_is_exactly_seven_things() {
        assert!(!to_boolean(Value::from_f64(0.0), S, never_empty));
        assert!(!to_boolean(Value::from_f64(-0.0), S, never_empty));
        assert!(!to_boolean(Value::from_f64(f64::NAN), S, never_empty));
        assert!(!to_boolean(Value::from_i32(0), S, never_empty));
        assert!(!to_boolean(Value::from_bool(false), S, never_empty));

        assert!(to_boolean(Value::from_f64(0.1), S, never_empty));
        assert!(to_boolean(Value::from_i32(-1), S, never_empty));
        assert!(to_boolean(Value::from_bool(true), S, never_empty));
        assert!(to_boolean(Value::from_f64(f64::INFINITY), S, never_empty));
    }

    #[test]
    fn an_object_is_truthy_however_empty_it_is() {
        let object = Value(encode(TAG_REFERENCE, 5));
        assert!(
            to_boolean(object, S, never_empty),
            "no unwrapping happens: new Boolean(false) is truthy"
        );
    }

    #[test]
    fn undefined_becomes_nan_and_null_becomes_zero() {
        let undefined = Value(encode(TAG_SINGLETON, 0));
        let null = Value(encode(TAG_SINGLETON, 1));

        assert!(to_number(undefined, S).unwrap().is_nan());
        assert_eq!(to_number(null, S), Some(0.0));
        assert!(!to_boolean(undefined, S, never_empty));
        assert!(!to_boolean(null, S, never_empty));
    }

    #[test]
    fn to_number_refuses_a_reference_rather_than_guessing() {
        let object = Value(encode(TAG_REFERENCE, 5));
        assert_eq!(
            to_number(object, S),
            None,
            "it needs ToPrimitive, which may run user code; zero would be a \
             wrong number nobody traces back"
        );
    }

    #[test]
    fn to_int32_wraps_where_a_cast_would_saturate() {
        assert_eq!(to_int32(4_294_967_296.0), 0, "2**32 | 0 is 0");
        assert_eq!(
            to_int32(2_147_483_648.0),
            i32::MIN,
            "2**31 | 0 is i32::MIN — `as i32` would saturate to i32::MAX"
        );
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(1.9), 1, "truncates toward zero");
        assert_eq!(to_int32(-1.9), -1);
    }

    #[test]
    fn non_finite_becomes_zero_rather_than_failing() {
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(f64::NEG_INFINITY), 0);
    }

    #[test]
    fn unsigned_shift_is_the_one_that_outgrows_a_signed_result() {
        assert_eq!(to_uint32(-1.0), 4_294_967_295, "(-1) >>> 0");
        assert_eq!(to_int32(-1.0), -1, "and (-1) >> 0 is still -1");
    }
}
