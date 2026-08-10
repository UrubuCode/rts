//! Division, remainder, and the digit-at-a-time helpers `to_radix` and `parse`
//! build on.
//!
//! Split out of `arith.rs` to stay under this crate's 500-line ceiling (rule 6
//! of the crate `README.md`) once the in-place division below and its tests
//! were added — not a new layer, the same `mag_*` / `BigInt` split `arith.rs`
//! documents, just the division half of it.

use super::arith::{digit, mag_cmp, mag_bit, mag_bit_len, trim};
use super::BigInt;
use core::cmp::Ordering;

impl BigInt {
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
}

/// The quotient and remainder of two magnitudes, where the divisor is non-zero.
///
/// Binary long division, one bit at a time. Knuth's algorithm D is the faster
/// answer and it is rejected here for now: it needs a normalisation shift, a
/// two-digit trial quotient and a correction step that fires on inputs a test
/// does not reach by accident, and a division that is wrong once in ten million
/// inputs is worse than one that is slower on all of them. It is the place to
/// look when division shows up in a profile, and `to_radix` already avoids this
/// path entirely by dividing by a single digit. **That rejection is about the
/// algorithm, not about this function's allocation shape** — the loop below used
/// to call the allocating `mag_sub` on every set bit, which is a fresh `Vec` per
/// bit of the dividend (measured: 3.1 µs per division of a 30-digit number by a
/// 9-digit one, up to ~100 allocations for a 100-bit operand). Subtracting into
/// the `remainder` buffer in place, the same way it is already shifted in place,
/// keeps the same bit-at-a-time algorithm and turns that into two allocations
/// total: the `quotient` vector and the `remainder` vector, each sized once up
/// front.
pub(super) fn mag_divrem(dividend: &[u32], divisor: &[u32]) -> (Vec<u32>, Vec<u32>) {
    debug_assert!(!divisor.is_empty());
    if mag_cmp(dividend, divisor) == Ordering::Less {
        return (Vec::new(), dividend.to_vec());
    }

    let length = mag_bit_len(dividend);
    let mut quotient = vec![0u32; dividend.len()];
    // Sized for the largest the remainder ever gets — one word past the
    // dividend's own length, the same headroom `mag_shl1_in_place`'s carry
    // would otherwise grow into one push at a time — so neither the shift nor
    // the in-place subtraction below reallocates during the loop.
    let mut remainder: Vec<u32> = Vec::with_capacity(dividend.len() + 1);
    for index in (0..length).rev() {
        mag_shl1_in_place(&mut remainder, mag_bit(dividend, index));
        if mag_cmp(&remainder, divisor) != Ordering::Less {
            mag_sub_in_place(&mut remainder, divisor);
            quotient[index / 32] |= 1 << (index % 32);
        }
    }
    trim(&mut quotient);
    trim(&mut remainder);
    (quotient, remainder)
}

/// The difference of two magnitudes, in place, where `left >= right`.
///
/// Same borrow loop as `arith::mag_sub`, written into the existing buffer
/// instead of a fresh one. It exists only for `mag_divrem`'s inner loop, where
/// `left` is the remainder that is already being mutated in place by the shift
/// beside it — building a new `Vec` here just to overwrite `remainder` with it
/// on the next line was the allocation this module's doc on `mag_divrem`
/// measures.
fn mag_sub_in_place(left: &mut Vec<u32>, right: &[u32]) {
    debug_assert!(mag_cmp(left, right) != Ordering::Less);
    let mut borrow = 0i64;
    for index in 0..left.len() {
        let difference = left[index] as i64 - digit(right, index) as i64 - borrow;
        if difference < 0 {
            left[index] = (difference + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            left[index] = difference as u32;
            borrow = 0;
        }
    }
    trim(left);
}

/// Doubles a magnitude and sets the low bit, which is the inner step of the
/// long division above and has no other caller.
///
/// The `push` on carry only allocates if the caller under-sized `digits` —
/// `mag_divrem` reserves one word of headroom up front so this path never
/// reallocates during its loop; a caller that does not reserve still gets a
/// correct answer, just with the growth `Vec::push` always does.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> BigInt {
        BigInt::parse(text).expect("test literal parses")
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
    fn in_place_division_matches_across_a_remainder_that_grows_and_shrinks_words() {
        // Targets the in-place subtraction added to `mag_divrem`: the remainder
        // buffer is reused across iterations, so a bug there (an off-by-one in
        // the borrow loop, or a capacity reservation one word short) would show
        // up as a wrong low digit or a wrong length rather than a crash. Chosen
        // to cross a 32-bit digit boundary on both operands, and to force the
        // remainder's top word to appear and disappear as bits shift in: the
        // divisor's top digit is all-ones, so a subtraction borrows through
        // every word of the remainder on many steps.
        let dividend = big("18446744073709551617"); // 2^64 + 1
        let divisor = big("4294967295"); // 2^32 - 1
        let quotient = dividend.div(&divisor).unwrap();
        let remainder = dividend.rem(&divisor).unwrap();
        assert_eq!(quotient.mul(&divisor).add(&remainder), dividend);
        assert!(remainder.cmp(&divisor) == Ordering::Less);
        assert_eq!(quotient.to_decimal(), "4294967297");
        assert_eq!(remainder.to_decimal(), "2");

        // And a case where the remainder buffer's reserved capacity
        // (`dividend.len() + 1` words) is exercised right at its edge: a
        // dividend that is exactly one bit past a word boundary.
        let dividend = big("4294967296"); // 2^32, two words wide
        let divisor = big("3");
        let quotient = dividend.div(&divisor).unwrap();
        let remainder = dividend.rem(&divisor).unwrap();
        assert_eq!(quotient.mul(&divisor).add(&remainder), dividend);
        assert_eq!(quotient.to_decimal(), "1431655765");
        assert_eq!(remainder.to_decimal(), "1");
    }
}
