//! Arbitrary-precision integers — JavaScript's `BigInt`.
//!
//! # The decision this module is built around
//!
//! **A `BigInt` is a primitive, so it is equal by value and never by identity.**
//! `1n === 1n` is `true`, and it is true for two bigints computed by different
//! paths a megabyte apart in the heap. That single fact is what separates this
//! from an object wrapper, and it is why every representation question below is
//! answered by "make one value have exactly one spelling": the normalisation in
//! [`BigInt::normalise`] is not tidiness, it is what makes a derived `PartialEq`
//! the *correct* equality rather than an approximation of it.
//!
//! The digits live on the heap because the precision is arbitrary. That is a
//! storage fact and not a semantic one, and it is the whole reason this module
//! exists apart from the value representation: **nothing here knows about heaps,
//! tags, slots or values.** It owns digits. Whoever owns values decides how a
//! `BigInt` is reached, and what a division by zero throws.
//!
//! # Why sign-magnitude, and not two's complement
//!
//! Two's complement is what a machine word does, and the reason it wins there is
//! that the width is fixed: addition and subtraction become one operation, and
//! the sign costs nothing because it is a bit that was already there. Neither
//! reason survives arbitrary precision. There is no width, so a negative number
//! has no "all the remaining bits are ones" to be stored in — the sign extension
//! is infinite and has to be *implied* anyway, which is a sign field wearing a
//! disguise.
//!
//! What sign-magnitude then makes easy is exactly the set JavaScript asks for:
//! multiply, divide and remainder are magnitude operations with a sign computed
//! separately; `to_decimal` reads the magnitude directly; comparison is a
//! magnitude comparison and a sign test. Under two's complement each of those
//! starts by recovering the magnitude.
//!
//! It also removes the asymmetric minimum. `i32::MIN` has no positive
//! counterpart, so `-x` is the one unary operation that can overflow, and every
//! fixed-width bignum built on two's complement inherits a version of that
//! special case. Here [`BigInt::neg`] flips a `bool`.
//!
//! The price is paid in one place, and it is real: the bitwise operators are
//! *defined* on the two's-complement interpretation, so `-1n & 3n` is `3n`. They
//! are implemented in `bits.rs` by converting into two's complement, operating,
//! and converting back — see that file for the reasoning. Paying the cost in the
//! four operators that need it beats paying it in the twenty that do not.
//!
//! # Why base 2^32, and not 2^64
//!
//! Multiply needs the full product of two digits and divide needs a double-width
//! partial remainder, so a digit must be half of the widest integer the
//! arithmetic can rely on. `u64` is that width: it is a machine word on every
//! target this workspace builds for except wasm32, and on wasm32 it is a native
//! i64 operation.
//!
//! `u128` was considered and rejected, and not on portability grounds — it
//! exists on every Rust target. It is rejected because on the 32-bit targets in
//! range (wasm32, and armv7 for a mobile host) a `u128` multiply is not an
//! instruction; it is a call into compiler-rt. Base 2^64 would halve the digit
//! count and then spend the saving, plus a call, in the inner loop. Base 2^32
//! keeps the inner loop at one `u64` multiply, which every target has.
//!
//! # What is deliberately not here
//!
//! Underscore separators. `1_000n` is a legal *literal* and `BigInt("1_000")` is
//! a `SyntaxError`, so the two callers disagree — and [`BigInt::parse`] answers
//! for the string form, which refuses them. The literal path strips separators
//! while lexing, which it must do for `1_000` as well, so there is nothing for a
//! flag here to buy.

mod arith;
mod bits;
mod div;
mod text;

use core::cmp::Ordering;

/// An arbitrary-precision integer: a sign and magnitude digits.
///
/// Equality is derived, and that is load-bearing rather than convenient: the
/// representation is normalised so a value has exactly one spelling, which makes
/// the derived comparison the value equality `1n === 1n` requires.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BigInt {
    /// True when the value is strictly below zero.
    ///
    /// Zero is always `false`. There is no negative zero here — the language has
    /// one for `Number` and not for `BigInt`, and allowing the representation
    /// would put two spellings on one value, which is the one thing the derived
    /// equality cannot survive.
    negative: bool,
    /// Base-2^32 digits, least significant first, with no leading zero digit.
    ///
    /// Zero is the empty vector rather than `[0]`, for the same one-spelling
    /// reason, and because "is it zero" then costs a length check.
    digits: Vec<u32>,
}

impl BigInt {
    /// Zero.
    pub fn zero() -> Self {
        BigInt {
            negative: false,
            digits: Vec::new(),
        }
    }

    /// The value of an `i64`.
    pub fn from_i64(value: i64) -> Self {
        // `unsigned_abs` rather than `-value`, because `i64::MIN` has no
        // positive counterpart and negating it is the one case that panics.
        Self::from_parts(value < 0, digits_of_u64(value.unsigned_abs()))
    }

    /// An unsigned sixty-four-bit integer, exactly.
    ///
    /// Beside [`Self::from_i64`] rather than reached through it, because the
    /// top half of the range has no signed counterpart: `from_i64(word as i64)`
    /// answers a negative value for anything at or above `2^63`, which is
    /// precisely the range `BigUint64Array` exists to hold.
    pub fn from_u64(value: u64) -> Self {
        Self::from_parts(false, digits_of_u64(value))
    }

    /// The exact value of a double, or `None` when it does not name an integer.
    ///
    /// `None` covers a fractional value and a non-finite one alike. `BigInt(1.5)`
    /// and `BigInt(Infinity)` are both a `RangeError` in the language, but this
    /// module must not decide that — a caller wanting a saturating or truncating
    /// conversion is served by the same answer, and a caller wanting the throw
    /// knows which throw it wants.
    pub fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() || value.fract() != 0.0 {
            return None;
        }
        if value == 0.0 {
            // Catches both zeros, and keeps the subnormal exponent case below
            // from having to mean anything: a subnormal is smaller than one, so
            // an integral one is zero.
            return Some(Self::zero());
        }

        // Taking the double apart rather than looping on division: the bits are
        // the exact value, and any arithmetic route rounds somewhere.
        let bits = value.abs().to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as i32;
        let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
        // The stored exponent is biased by 1023 and the mantissa carries 52
        // fractional bits, so the value is `mantissa * 2^(exponent - 1075)`.
        let scale = exponent - 1075;

        let magnitude = if scale >= 0 {
            arith::mag_shl(&digits_of_u64(mantissa), scale as u32)
        } else {
            // The value is integral, so every bit shifted out here is already
            // zero — this is exact, not truncating.
            arith::mag_shr(&digits_of_u64(mantissa), (-scale) as u32)
        };
        Some(Self::from_parts(value < 0.0, magnitude))
    }

    /// The nearest double, which is what `Number(big)` produces.
    ///
    /// Rounds to nearest, ties to even — the same rule the hardware uses, so a
    /// value that fits gives the identical answer whichever route it took. Too
    /// large answers an infinity rather than a `None`: `Number(10n ** 400n)` is
    /// `Infinity` in the language, and it is not an error there.
    pub fn to_f64(&self) -> f64 {
        if self.digits.is_empty() {
            return 0.0;
        }
        let length = arith::mag_bit_len(&self.digits);

        let magnitude = if length <= 64 {
            // A `u64` holds the value exactly, so the cast performs the rounding
            // and performs it correctly. Reimplementing it below 64 bits would
            // only be a second chance to get the tie rule wrong.
            let mut value = 0u64;
            for (index, digit) in self.digits.iter().enumerate() {
                value |= (*digit as u64) << (32 * index);
            }
            value as f64
        } else if length > 1024 {
            // The largest finite double is below 2^1024, so nothing this long
            // rounds to one.
            f64::INFINITY
        } else {
            let start = length - 64;
            let mut top = 0u64;
            for offset in 0..64 {
                if arith::mag_bit(&self.digits, start + offset) {
                    top |= 1 << offset;
                }
            }
            // Everything below the window only matters as "was anything set" —
            // that is what breaks a tie, and no bit of it can do more.
            let sticky = (0..start).any(|index| arith::mag_bit(&self.digits, index));

            let mut significand = top >> 11;
            let dropped = top & 0x7ff;
            let half = 0x400;
            if dropped > half || (dropped == half && (sticky || significand & 1 == 1)) {
                significand += 1;
            }
            // Exact: a power of two times a value that fits in 53 bits (or 54,
            // when the rounding carried, in which case the low bit is zero).
            significand as f64 * two_pow(length as i32 - 53)
        };

        if self.negative {
            -magnitude
        } else {
            magnitude
        }
    }

    /// Whether this is zero.
    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Whether this is strictly below zero.
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// How many bits the magnitude occupies; zero for zero.
    ///
    /// Exposed for the callers that have to REFUSE an operation before running
    /// it: `1n << 2n**40n` and `2n ** 2n**40n` are each one line of arithmetic
    /// and a terabyte of digits, and a size that has to be checked after the
    /// allocation is a size nothing checks.
    pub fn bit_len(&self) -> usize {
        arith::mag_bit_len(&self.digits)
    }

    /// Numeric order.
    ///
    /// Delegates to [`Ord`] rather than the other way round, so the two can
    /// never answer differently — an inherent method shadows the trait one at
    /// every call site, and a reader who assumes otherwise gets the wrong sort.
    pub fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(self, other)
    }

    /// This value with its sign flipped.
    ///
    /// Total, unlike the fixed-width version: there is no minimum without a
    /// counterpart, so this is a `bool` flip and never overflows.
    pub fn neg(&self) -> Self {
        Self::from_parts(!self.negative, self.digits.clone())
    }

    /// The value as an `i64`, or `None` when it does not fit.
    pub fn as_i64(&self) -> Option<i64> {
        let magnitude = self.magnitude_u64()?;
        if self.negative {
            // `-(i64::MIN as u64)` is not expressible, so the boundary is tested
            // on the unsigned side where it is.
            if magnitude > (i64::MAX as u64) + 1 {
                None
            } else {
                Some((magnitude as i64).wrapping_neg())
            }
        } else {
            i64::try_from(magnitude).ok()
        }
    }

    /// The value as a `u64`, or `None` when it is negative or does not fit.
    pub fn as_u64(&self) -> Option<u64> {
        if self.negative {
            return None;
        }
        self.magnitude_u64()
    }

    /// The magnitude as a `u64`, ignoring the sign entirely.
    fn magnitude_u64(&self) -> Option<u64> {
        if self.digits.len() > 2 {
            return None;
        }
        let mut value = 0u64;
        for (index, digit) in self.digits.iter().enumerate() {
            value |= (*digit as u64) << (32 * index);
        }
        Some(value)
    }

    /// Builds a value from a sign and magnitude digits, normalising both.
    ///
    /// Every construction in this module goes through here. A caller that
    /// assembled the fields directly could leave a leading zero digit or a
    /// negative zero behind, and the failure would not be a wrong answer from
    /// arithmetic — it would be `a == b` answering `false` for two values that
    /// print the same, which is far harder to find.
    fn from_parts(negative: bool, digits: Vec<u32>) -> Self {
        let mut value = BigInt { negative, digits };
        value.normalise();
        value
    }

    /// Drops leading zero digits, and the sign if nothing is left.
    fn normalise(&mut self) {
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.negative = false;
        }
    }
}

impl Ord for BigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            // Among negatives the larger magnitude is the smaller number, which
            // is the one place sign-magnitude needs a reversal and the one place
            // a comparison is written wrong.
            (true, true) => arith::mag_cmp(&other.digits, &self.digits),
            (false, false) => arith::mag_cmp(&self.digits, &other.digits),
        }
    }
}

impl PartialOrd for BigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(self, other))
    }
}

/// The magnitude digits of a `u64`.
fn digits_of_u64(value: u64) -> Vec<u32> {
    vec![value as u32, (value >> 32) as u32]
}

/// Two raised to an exponent, built from the bits rather than computed.
///
/// `powi` would be a loop of multiplications with a rounding step each; a power
/// of two is one exponent field. Only called with an exponent a normal double
/// can hold, which the caller has already bounded by the 1024-bit check.
fn two_pow(exponent: i32) -> f64 {
    f64::from_bits(((exponent + 1023) as u64) << 52)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_has_exactly_one_spelling() {
        // Not a tautology: these are built by routes that produce different
        // intermediate digit vectors. If normalisation were skipped anywhere,
        // equality — which is what `1n === 1n` compiles to — would answer false
        // for values that print the same.
        assert_eq!(BigInt::from_i64(0), BigInt::zero());
        assert_eq!(BigInt::from_f64(0.0).unwrap(), BigInt::zero());
        assert_eq!(BigInt::from_f64(-0.0).unwrap(), BigInt::zero());
        assert_eq!(
            BigInt::from_i64(5).sub(&BigInt::from_i64(5)),
            BigInt::zero()
        );
        assert!(!BigInt::from_i64(-5).add(&BigInt::from_i64(5)).is_negative());
    }

    #[test]
    fn negation_is_total_at_the_boundary_a_machine_word_cannot_negate() {
        let min = BigInt::from_i64(i64::MIN);
        assert_eq!(min.neg().to_decimal(), "9223372036854775808");
        assert_eq!(min.neg().as_i64(), None, "one past what an i64 holds");
        assert_eq!(min.as_i64(), Some(i64::MIN), "and it reads back");
        assert_eq!(BigInt::zero().neg(), BigInt::zero(), "no negative zero");
    }

    #[test]
    fn from_f64_refuses_what_is_not_an_integer() {
        assert_eq!(BigInt::from_f64(1.5), None);
        assert_eq!(BigInt::from_f64(-0.5), None);
        assert_eq!(BigInt::from_f64(f64::NAN), None);
        assert_eq!(BigInt::from_f64(f64::INFINITY), None);
        assert_eq!(BigInt::from_f64(1e300).unwrap().to_f64(), 1e300);
        assert_eq!(BigInt::from_f64(-42.0).unwrap(), BigInt::from_i64(-42));
    }

    #[test]
    fn from_f64_is_exact_past_where_a_double_stops_being_dense() {
        // 2^70 is representable exactly, and is far past a u64. A conversion
        // that went through decimal text or repeated division would land near
        // it rather than on it.
        let value = BigInt::from_f64(f64::powi(2.0, 70)).unwrap();
        assert_eq!(value.to_decimal(), "1180591620717411303424");
        assert_eq!(value.to_f64(), f64::powi(2.0, 70));
    }

    #[test]
    fn to_f64_overflows_to_infinity_rather_than_failing() {
        let huge = BigInt::from_i64(10).pow(400);
        assert_eq!(huge.to_f64(), f64::INFINITY);
        assert_eq!(huge.neg().to_f64(), f64::NEG_INFINITY);
        // The largest finite double still answers finitely, which is what says
        // the threshold is at the right place and not merely somewhere.
        let max = BigInt::from_f64(f64::MAX).unwrap();
        assert_eq!(max.to_f64(), f64::MAX);
    }

    #[test]
    fn to_f64_rounds_to_nearest_even_like_the_hardware() {
        // 2^53 + 1 is the first integer a double cannot hold; it ties, and the
        // even neighbour is 2^53. A truncating implementation gives the same
        // answer here, which is why the next case matters: 2^53 + 3 ties toward
        // 2^53 + 4, where truncation gives 2^53 + 2.
        let base = BigInt::from_i64(1i64 << 53);
        let one = BigInt::from_i64(1);
        assert_eq!(base.add(&one).to_f64(), 9007199254740992.0);
        assert_eq!(base.add(&BigInt::from_i64(3)).to_f64(), 9007199254740996.0);
    }

    #[test]
    fn order_reverses_among_negatives() {
        let values = [
            BigInt::from_i64(-100),
            BigInt::from_i64(-1),
            BigInt::zero(),
            BigInt::from_i64(1),
            BigInt::from_i64(100),
        ];
        for window in values.windows(2) {
            assert_eq!(window[0].cmp(&window[1]), Ordering::Less, "{window:?}");
        }
        // Bigger magnitude, smaller number — the reversal a sign-magnitude
        // comparison written without thinking gets backwards.
        assert!(BigInt::from_i64(-100) < BigInt::from_i64(-1));
    }

    #[test]
    fn as_i64_and_as_u64_disagree_exactly_about_the_sign_and_the_top_bit() {
        let big = BigInt::parse("18446744073709551615").unwrap();
        assert_eq!(big.as_u64(), Some(u64::MAX));
        assert_eq!(big.as_i64(), None, "too large signed");
        assert_eq!(BigInt::from_i64(-1).as_u64(), None, "negative");
        assert_eq!(BigInt::from_i64(-1).as_i64(), Some(-1));
        assert_eq!(
            BigInt::parse("18446744073709551616").unwrap().as_u64(),
            None,
            "one past a u64 needs a third digit"
        );
    }
}
