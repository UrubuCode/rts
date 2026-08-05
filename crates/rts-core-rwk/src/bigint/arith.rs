//! The arithmetic, and the magnitude primitives everything else is built from.
//!
//! Two layers on purpose. The `mag_*` functions know nothing about signs and
//! operate on `&[u32]`; the [`BigInt`] methods know nothing about carries and
//! only decide signs. Mixing them is what makes a subtraction that is correct
//! for positives and wrong for one negative operand, because the sign decision
//! ends up made twice — once in the borrow loop and once at the top.

use super::BigInt;
use core::cmp::Ordering;

impl BigInt {
    /// The sum.
    pub fn add(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            return Self::from_parts(self.negative, mag_add(&self.digits, &other.digits));
        }
        // Different signs, so this is a subtraction of magnitudes and the answer
        // takes the sign of whichever magnitude wins.
        match mag_cmp(&self.digits, &other.digits) {
            Ordering::Less => Self::from_parts(
                other.negative,
                mag_sub(&other.digits, &self.digits),
            ),
            _ => Self::from_parts(self.negative, mag_sub(&self.digits, &other.digits)),
        }
    }

    /// The difference.
    ///
    /// Written as `self + (-other)` rather than as its own borrow loop: one
    /// place decides what a mixed-sign result's sign is, so the two operations
    /// cannot come to disagree about it.
    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    /// The product.
    pub fn mul(&self, other: &Self) -> Self {
        Self::from_parts(
            self.negative != other.negative,
            mag_mul(&self.digits, &other.digits),
        )
    }

    /// The quotient, **truncated toward zero**, or `None` when dividing by zero.
    ///
    /// Truncation is worth spelling out because it is only visible with a
    /// negative operand and the alternative is widespread: `-7n / 2n` is `-3n`
    /// here and in JavaScript, and `-4` in Python and in several bignum
    /// libraries that floor. Rust's own `/` truncates, so the machine agrees
    /// with the language for once.
    ///
    /// Division by zero answers `None` rather than throwing: the language raises
    /// a `RangeError`, and this module holds no language knowledge it can be
    /// told instead.
    pub fn div(&self, other: &Self) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        let (quotient, _) = mag_divrem(&self.digits, &other.digits);
        Some(Self::from_parts(
            self.negative != other.negative,
            quotient,
        ))
    }

    /// The remainder, or `None` when dividing by zero.
    ///
    /// Takes the sign of the **dividend**, which is what truncating division
    /// forces: `-7n % 2n` is `-1n`, not `1n`. A flooring division would give the
    /// sign of the divisor instead, and the identity `(a/b)*b + a%b === a` is
    /// what holds either way — it is not what tells the two apart.
    pub fn rem(&self, other: &Self) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        let (_, remainder) = mag_divrem(&self.digits, &other.digits);
        Some(Self::from_parts(self.negative, remainder))
    }

    /// This value raised to a non-negative power.
    ///
    /// Square-and-multiply rather than a loop of multiplications: the operand
    /// grows with every step, so a linear loop multiplies a number of increasing
    /// size by the base `exponent` times, where this does it `log2(exponent)`
    /// times. `10n ** 400n` is the difference between one test and a hang.
    ///
    /// A negative exponent is not representable in the parameter, which is the
    /// point: `2n ** -1n` is a `RangeError` in the language, and refusing it in
    /// the type means there is no case here to answer wrongly.
    pub fn pow(&self, exponent: u32) -> Self {
        let mut result = BigInt::from_i64(1);
        let mut base = self.clone();
        let mut remaining = exponent;
        while remaining > 0 {
            if remaining & 1 == 1 {
                result = result.mul(&base);
            }
            remaining >>= 1;
            if remaining > 0 {
                base = base.mul(&base);
            }
        }
        result
    }
}

/// Compares two magnitudes.
///
/// Length first, which is only valid because both are normalised — with a
/// leading zero digit allowed, `[1, 0]` would compare greater than `[2]`.
pub(super) fn mag_cmp(left: &[u32], right: &[u32]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        other => other,
    }
}

/// The sum of two magnitudes.
pub(super) fn mag_add(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u64;
    for index in 0..left.len().max(right.len()) {
        let sum = carry + digit(left, index) as u64 + digit(right, index) as u64;
        out.push(sum as u32);
        carry = sum >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

/// The difference of two magnitudes, where `left >= right`.
///
/// The precondition is the caller's: every call site has already compared, and
/// re-comparing here would be the same decision made in two places. A violation
/// wraps rather than panicking, which is why it is asserted in debug.
pub(super) fn mag_sub(left: &[u32], right: &[u32]) -> Vec<u32> {
    debug_assert!(mag_cmp(left, right) != Ordering::Less);
    let mut out = Vec::with_capacity(left.len());
    let mut borrow = 0i64;
    for index in 0..left.len() {
        let difference = digit(left, index) as i64 - digit(right, index) as i64 - borrow;
        if difference < 0 {
            out.push((difference + (1i64 << 32)) as u32);
            borrow = 1;
        } else {
            out.push(difference as u32);
            borrow = 0;
        }
    }
    trim(&mut out);
    out
}

/// The product of two magnitudes.
///
/// Schoolbook. Karatsuba and the FFT methods win, but only past operand sizes a
/// JavaScript program reaches by trying to — and each one is a second algorithm
/// that has to agree with this one for every input. The crossover is where it
/// gets added, with the measurement that found it.
pub(super) fn mag_mul(left: &[u32], right: &[u32]) -> Vec<u32> {
    if left.is_empty() || right.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u32; left.len() + right.len()];
    for (i, &a) in left.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &b) in right.iter().enumerate() {
            // Cannot overflow a u64: (2^32-1)^2 + 2*(2^32-1) is 2^64-1 exactly,
            // which is why the accumulator and the carry both fit here and why
            // the digit is 32 bits and not 64.
            let product = a as u64 * b as u64 + out[i + j] as u64 + carry;
            out[i + j] = product as u32;
            carry = product >> 32;
        }
        out[i + right.len()] = carry as u32;
    }
    trim(&mut out);
    out
}

/// The quotient and remainder of two magnitudes, where the divisor is non-zero.
///
/// Binary long division, one bit at a time. Knuth's algorithm D is the faster
/// answer and it is rejected here for now: it needs a normalisation shift, a
/// two-digit trial quotient and a correction step that fires on inputs a test
/// does not reach by accident, and a division that is wrong once in ten million
/// inputs is worse than one that is slower on all of them. It is the place to
/// look when division shows up in a profile, and `to_radix` already avoids this
/// path entirely by dividing by a single digit.
pub(super) fn mag_divrem(dividend: &[u32], divisor: &[u32]) -> (Vec<u32>, Vec<u32>) {
    debug_assert!(!divisor.is_empty());
    if mag_cmp(dividend, divisor) == Ordering::Less {
        return (Vec::new(), dividend.to_vec());
    }

    let length = mag_bit_len(dividend);
    let mut quotient = vec![0u32; dividend.len()];
    let mut remainder: Vec<u32> = Vec::new();
    for index in (0..length).rev() {
        mag_shl1_in_place(&mut remainder, mag_bit(dividend, index));
        if mag_cmp(&remainder, divisor) != Ordering::Less {
            remainder = mag_sub(&remainder, divisor);
            quotient[index / 32] |= 1 << (index % 32);
        }
    }
    trim(&mut quotient);
    trim(&mut remainder);
    (quotient, remainder)
}

/// Divides a magnitude by a single digit, returning the quotient and remainder.
///
/// Exists because `to_radix` and `parse` only ever need this shape, and this is
/// a linear pass where the general division is quadratic in bits.
pub(super) fn mag_divrem_small(digits: &[u32], divisor: u32) -> (Vec<u32>, u32) {
    debug_assert!(divisor != 0);
    let mut out = vec![0u32; digits.len()];
    let mut remainder = 0u64;
    for index in (0..digits.len()).rev() {
        let current = (remainder << 32) | digits[index] as u64;
        out[index] = (current / divisor as u64) as u32;
        remainder = current % divisor as u64;
    }
    trim(&mut out);
    (out, remainder as u32)
}

/// Multiplies a magnitude by a digit and adds another, in one pass.
///
/// One function rather than two because `parse` always does both together, and
/// splitting them would walk the digits twice per input character.
pub(super) fn mag_mul_add_small(digits: &mut Vec<u32>, factor: u32, addend: u32) {
    let mut carry = addend as u64;
    for digit in digits.iter_mut() {
        let product = *digit as u64 * factor as u64 + carry;
        *digit = product as u32;
        carry = product >> 32;
    }
    while carry != 0 {
        digits.push(carry as u32);
        carry >>= 32;
    }
    trim(digits);
}

/// A magnitude shifted left, growing without limit.
pub(super) fn mag_shl(digits: &[u32], amount: u32) -> Vec<u32> {
    if digits.is_empty() {
        return Vec::new();
    }
    let words = (amount / 32) as usize;
    let bits = amount % 32;
    let mut out = vec![0u32; words];
    if bits == 0 {
        // A shift by 32 is undefined for a u32 in Rust as much as in C, so the
        // aligned case is separated rather than clamped.
        out.extend_from_slice(digits);
    } else {
        let mut carry = 0u32;
        for &digit in digits {
            out.push((digit << bits) | carry);
            carry = digit >> (32 - bits);
        }
        out.push(carry);
    }
    trim(&mut out);
    out
}

/// A magnitude shifted right, discarding the bits that fall off.
///
/// Discarding is a magnitude operation and not the language's `>>`; the sign
/// correction that makes `-1n >> 1n` answer `-1n` lives in `bits.rs`, where the
/// sign is known.
pub(super) fn mag_shr(digits: &[u32], amount: u32) -> Vec<u32> {
    let words = (amount / 32) as usize;
    if words >= digits.len() {
        return Vec::new();
    }
    let bits = amount % 32;
    let kept = &digits[words..];
    let mut out = Vec::with_capacity(kept.len());
    if bits == 0 {
        out.extend_from_slice(kept);
    } else {
        for index in 0..kept.len() {
            let low = kept[index] >> bits;
            let high = digit(kept, index + 1) << (32 - bits);
            out.push(low | high);
        }
    }
    trim(&mut out);
    out
}

/// How many bits the magnitude occupies; zero for an empty one.
pub(super) fn mag_bit_len(digits: &[u32]) -> usize {
    match digits.last() {
        None => 0,
        Some(top) => digits.len() * 32 - top.leading_zeros() as usize,
    }
}

/// Whether a given bit of the magnitude is set; false past the top.
pub(super) fn mag_bit(digits: &[u32], index: usize) -> bool {
    digit(digits, index / 32) >> (index % 32) & 1 == 1
}

/// Doubles a magnitude and sets the low bit, which is the inner step of the
/// long division above and has no other caller.
fn mag_shl1_in_place(digits: &mut Vec<u32>, low_bit: bool) {
    let mut carry = u32::from(low_bit);
    for digit in digits.iter_mut() {
        let next = *digit >> 31;
        *digit = (*digit << 1) | carry;
        carry = next;
    }
    if carry != 0 {
        digits.push(carry);
    }
    trim(digits);
}

/// A digit, or zero past the end — the sign extension a magnitude has.
fn digit(digits: &[u32], index: usize) -> u32 {
    digits.get(index).copied().unwrap_or(0)
}

/// Drops leading zero digits.
pub(super) fn trim(digits: &mut Vec<u32>) {
    while digits.last() == Some(&0) {
        digits.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> BigInt {
        BigInt::parse(text).expect("test literal parses")
    }

    #[test]
    fn a_product_that_crosses_the_digit_boundary() {
        // (2^32 - 1)^2 is the worst case the inner loop has: it is the largest
        // product two digits can make, and it plus the accumulator plus the
        // carry is exactly u64::MAX. A base-2^64 digit would overflow here,
        // which is the measurement behind the base choice.
        let max = big("4294967295");
        assert_eq!(max.mul(&max).to_decimal(), "18446744065119617025");

        // And one digit past, where a two-digit operand needs the carry to
        // propagate out of the second column.
        let past = big("4294967296");
        assert_eq!(past.mul(&past).to_decimal(), "18446744073709551616");

        // A long product checked by a law rather than by a literal: the digit
        // sum of a base-ten number is congruent to it modulo 9, so casting out
        // nines catches any carry that landed in the wrong column. A literal
        // here would only be as trustworthy as whoever typed it.
        let a = big("123456789012345678901234567890");
        let b = big("987654321098765432109876543210");
        let product = a.mul(&b);
        let nine = big("9");
        assert_eq!(
            product.rem(&nine).unwrap(),
            a.rem(&nine).unwrap().mul(&b.rem(&nine).unwrap()).rem(&nine).unwrap()
        );
        assert_eq!(product.div(&a).unwrap(), b, "and it divides back exactly");
        assert_eq!(product.to_decimal().len(), 60);
    }

    #[test]
    fn division_truncates_toward_zero_rather_than_flooring() {
        // The whole difference is here, and only with a negative operand: a
        // flooring implementation answers -4 and 1 for the first pair.
        assert_eq!(big("-7").div(&big("2")).unwrap().to_decimal(), "-3");
        assert_eq!(big("-7").rem(&big("2")).unwrap().to_decimal(), "-1");
        assert_eq!(big("7").div(&big("-2")).unwrap().to_decimal(), "-3");
        assert_eq!(big("7").rem(&big("-2")).unwrap().to_decimal(), "1");
        assert_eq!(big("-7").div(&big("-2")).unwrap().to_decimal(), "3");
        assert_eq!(big("-7").rem(&big("-2")).unwrap().to_decimal(), "-1");
    }

    #[test]
    fn the_remainder_takes_the_sign_of_the_dividend_and_the_identity_holds() {
        for (a, b) in [("-7", "2"), ("7", "-2"), ("-100", "-7"), ("0", "5")] {
            let (a, b) = (big(a), big(b));
            let q = a.div(&b).unwrap();
            let r = a.rem(&b).unwrap();
            assert_eq!(q.mul(&b).add(&r), a, "(a/b)*b + a%b == a");
            if !r.is_zero() {
                assert_eq!(r.is_negative(), a.is_negative(), "sign of the dividend");
            }
        }
    }

    #[test]
    fn dividing_by_zero_is_refused_rather_than_answered() {
        // `None` and not a panic and not zero: the language throws a RangeError
        // and this module is not the one that knows that.
        assert_eq!(big("1").div(&BigInt::zero()), None);
        assert_eq!(big("1").rem(&BigInt::zero()), None);
        assert_eq!(BigInt::zero().div(&BigInt::zero()), None);
    }

    #[test]
    fn division_by_a_multi_digit_divisor_far_past_a_u64() {
        let dividend = big("123456789012345678901234567890123456789012345678901234567890");
        let divisor = big("98765432109876543210987654321");
        let quotient = dividend.div(&divisor).unwrap();
        let remainder = dividend.rem(&divisor).unwrap();
        assert_eq!(quotient.mul(&divisor).add(&remainder), dividend);
        assert!(remainder.cmp(&divisor) == Ordering::Less, "reduced");
        assert!(!remainder.is_negative());
        // The quotient has as many digits as the two lengths differ by, give or
        // take one — enough to catch a bit index off by 32, which is the way
        // the long division fails, and it fails by a factor of a billion.
        assert_eq!(quotient.to_decimal().len(), 60 - 29);
    }

    #[test]
    fn addition_across_a_carry_that_grows_the_magnitude() {
        assert_eq!(
            big("18446744073709551615").add(&big("1")).to_decimal(),
            "18446744073709551616"
        );
        // And the borrow that shrinks it back, leaving no zero digit behind —
        // which is what the derived equality depends on.
        assert_eq!(big("18446744073709551616").sub(&big("1")), big("18446744073709551615"));
        assert_eq!(big("4294967296").sub(&big("4294967296")), BigInt::zero());
    }

    #[test]
    fn mixed_sign_addition_takes_the_sign_of_the_larger_magnitude() {
        assert_eq!(big("5").add(&big("-3")).to_decimal(), "2");
        assert_eq!(big("3").add(&big("-5")).to_decimal(), "-2");
        assert_eq!(big("-5").add(&big("3")).to_decimal(), "-2");
        assert_eq!(big("-3").add(&big("5")).to_decimal(), "2");
    }

    #[test]
    fn pow_reaches_sizes_a_linear_loop_would_not() {
        assert_eq!(big("2").pow(0).to_decimal(), "1");
        assert_eq!(BigInt::zero().pow(0).to_decimal(), "1", "0n ** 0n is 1n");
        assert_eq!(big("2").pow(64).to_decimal(), "18446744073709551616");
        assert_eq!(big("-2").pow(3).to_decimal(), "-8");
        assert_eq!(big("-2").pow(4).to_decimal(), "16");
        assert_eq!(big("10").pow(400).to_decimal().len(), 401);
    }
}
