//! The operators that read a number as thirty-two bits, and the two that do
//! not fit anywhere else.
//!
//! # Why these are entry points when the operation is pure arithmetic
//!
//! For the same reason `-` is. `a & b` is
//! `ToInt32(ToNumber(a)) & ToInt32(ToNumber(b))`, and `ToNumber` of a string
//! reads that string's text out of the heap. The `&` itself is one instruction;
//! it is the conversion in front of it that decides membership.
//!
//! So a type pass that proved both operands are numbers should emit the
//! instruction rather than call these — the same sentence the arithmetic
//! operators carry, and true here for the same reason.
//!
//! # `ToInt32` is where the surprises live
//!
//! It is not a cast. Every JavaScript number is a double, and `ToInt32` is
//! specified as: discard the fractional part, take the result modulo 2^32, and
//! read the low thirty-two bits as signed. Non-finite values and zero answer
//! zero.
//!
//! That is why `2147483648 | 0` is `-2147483648` and `4294967296 | 0` is `0` —
//! results that look like bugs and are the specification. Writing this as `as
//! i32` in Rust would be a saturating cast, which answers `2147483647` for the
//! first and is wrong in the direction nobody checks.

use super::operators::operands;
use super::with_current;
use crate::value::Value;

/// How many bits the shift operators keep of their right operand.
///
/// Five, so a shift is always by 0..=31 — `1 << 32` is `1`, not zero, which is
/// a fact about JavaScript rather than about the machine underneath.
const SHIFT_MASK: u32 = 31;

/// `ToInt32`.
///
/// Written out rather than expressed as a cast, because Rust's `as i32`
/// saturates where the specification wraps: `2147483648 as i32` is
/// `2147483647` in Rust and `-2147483648` in JavaScript. The difference is
/// invisible until a program shifts a large number, and then it is silent.
fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

/// `ToUint32`.
///
/// The same conversion read as unsigned, which is what `>>>` answers with and
/// what every shift takes its count from.
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    // `rem_euclid` rather than `%`: the remainder has to be non-negative, and
    // `%` in Rust takes the sign of the dividend, so `-1.0 % 2^32` is `-1.0`
    // where the specification wants `4294967295`.
    let wrapped = value.trunc().rem_euclid(4_294_967_296.0);
    wrapped as u32
}

/// The shift count both operands agree on.
fn shift_by(value: f64) -> u32 {
    to_uint32(value) & SHIFT_MASK
}

/// `a & b`.
#[rtse::entry]
pub fn bit_and(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
        // A bigint has no thirty-two-bit truncation: `&` on two of them is
        // over the whole two's-complement value, which is what makes
        // `-1n & 3n` answer `3n` where `-1 & 3` also does and `(2n**40n) & 1n`
        // would not survive `ToInt32`.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::BitAnd, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(f64::from(to_int32(a) & to_int32(b))).bits()
    })
}

/// `a | b`.
#[rtse::entry]
pub fn bit_or(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::BitOr, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(f64::from(to_int32(a) | to_int32(b))).bits()
    })
}

/// `a ^ b`.
#[rtse::entry]
pub fn bit_xor(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::BitXor, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(f64::from(to_int32(a) ^ to_int32(b))).bits()
    })
}

/// `~a`.
///
/// Takes two operands and ignores one, so that it can share [`operands`] with
/// everything else here. The alternative — a second conversion helper for the
/// one unary case — is a second statement of `ToNumber`'s heap path.
#[rtse::entry]
pub fn bit_not(value: u64) -> u64 {
    with_current(|context| {
        // `~1n` is `-2n` over the whole value, not over thirty-two bits. Unary,
        // so it cannot go through `binary` — the pattern is the same and the
        // one-operand shape is not.
        if let Some(held) = super::bigints::digits_of(context, value).map(crate::bigint::BigInt::bit_not)
        {
            return context.bigint_value(held);
        }
        let (a, _) = operands(context, Value(value), Value(value));
        Value::from_f64(f64::from(!to_int32(a))).bits()
    })
}

/// `a << b`.
#[rtse::entry]
pub fn shift_left(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
        // A bigint shift has no width and no thirty-two-bit mask: `1n << 64n` is
        // the number, where `1 << 64` is `1`. Asked before the conversion for
        // the reason `&` states — reaching `ToInt32` at all is already the wrong
        // operation.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::Shl, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        // `wrapping_shl` because the count is already masked to 0..=31, so
        // nothing can overflow — and because a plain `<<` panics in a debug
        // build on a count it cannot receive, which is a crash for a case that
        // does not exist.
        Value::from_f64(f64::from(to_int32(a).wrapping_shl(shift_by(b)))).bits()
    })
}

/// `a >> b` — the sign-propagating shift.
#[rtse::entry]
pub fn shift_right(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::Shr, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(f64::from(to_int32(a).wrapping_shr(shift_by(b)))).bits()
    })
}

/// `a >>> b` — the one bitwise operator whose result outgrows a signed
/// thirty-two-bit value.
///
/// `-1 >>> 0` is `4294967295`, which does not fit an `i32` — so the operand is
/// read as unsigned and the result is a double large enough to hold it. That is
/// the whole reason this operator is separate from `>>` rather than a flag on
/// it.
#[rtse::entry]
pub fn shift_right_unsigned(left: u64, right: u64) -> u64 {
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(f64::from(to_uint32(a).wrapping_shr(shift_by(b)))).bits()
    })
}

/// `Number::exponentiate`, where it disagrees with Rust's `powf`.
///
/// # The two rows IEEE-754 and ECMAScript answer differently
///
/// - **`1 ** NaN` is `NaN`.** IEEE makes a base of one absorbing, so
///   `pow(1, NaN)` is `1`, and Rust follows it. The language propagates the
///   `NaN` instead. This is the kind of divergence nothing finds later: the
///   answer is a plausible number rather than a crash, and `x ** y` over
///   unknown data quietly answers 1 where every other NaN-touching operation
///   propagates.
/// - **`(±1) ** ±Infinity` is `NaN`**, for the same reason.
///
/// Every other row `powf` already agrees with — `0 ** -1` being `Infinity`, the
/// odd/even-integer sign rules on a negative base — so only the exceptions are
/// written, rather than the whole table restated in a form that could drift
/// from it.
///
/// # Why this is a function and not two copies
///
/// There are two callers: the `**` operator below and `Math.pow`. They were
/// two copies of a partial table, and they had already drifted — the operator
/// handled the infinite exponent and `Math.pow` handled neither. A rule written
/// twice is a rule that will be written differently, which here means `x ** 2`
/// and `Math.pow(x, 2)` answering differently for the same `x`.
pub(super) fn exponentiate(base: f64, power: f64) -> f64 {
    if power.is_nan() {
        return f64::NAN;
    }
    if power.is_infinite() && base.abs() == 1.0 {
        return f64::NAN;
    }
    base.powf(power)
}

/// `a ** b`.
#[rtse::entry]
pub fn exponent(left: u64, right: u64) -> u64 {
    // Asked in a borrow of its own: a refused count becomes a `RangeError`,
    // and building that error borrows the context again, so `settled` has to
    // run after this borrow has ended.
    if let Some(outcome) = with_current(|context| {
        super::bigint_class::binary(context, super::bigint_class::Op::Pow, left, right)
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (base, power) = operands(context, Value(left), Value(right));
        Value::from_f64(exponentiate(base, power)).bits()
    })
}
