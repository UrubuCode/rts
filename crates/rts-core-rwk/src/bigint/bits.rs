//! Shifts, the bitwise operators, and `BigInt.asIntN` / `asUintN`.
//!
//! # Why this file is the awkward one
//!
//! The value is stored as a sign and a magnitude, and `&`, `|`, `^` and `~` are
//! *defined* on the two's-complement interpretation of an arbitrary-precision
//! integer. `-1n & 3n` is `3n`, because `-1n` is conceptually an infinite string
//! of one-bits. A magnitude-wise `and` answers `1n` — the magnitude of `-1n` is
//! `1` — and it is wrong in a way that looks plausible in a small test.
//!
//! So each of the four converts into two's complement over a fixed number of
//! words, operates, and converts back. The width is `max(len_a, len_b) + 1`
//! words, and the extra word is not slack: it holds the sign extension, so the
//! top bit of the result is the sign the infinite version would have. Without
//! it, `-1n & -1n` over one word gives `0xFFFFFFFF` with no bit left to say the
//! answer is negative, and the result reads back as `4294967295n`.
//!
//! The same conversion is what `asIntN`/`asUintN` need, which is why they live
//! here rather than beside the arithmetic.

use super::arith::{mag_bit, mag_bit_len, mag_shl, mag_shr, trim};
use super::BigInt;

impl BigInt {
    /// This value shifted left, which is a multiplication by a power of two.
    ///
    /// The sign is untouched, and that is right for both signs: shifting left
    /// scales, and scaling does not cross zero. No truncation — the language's
    /// `<<` on a bigint has no width to overflow.
    pub fn shl(&self, amount: u32) -> Self {
        Self::from_parts(self.negative, mag_shl(&self.digits, amount))
    }

    /// This value shifted right **arithmetically**, so the sign is preserved.
    ///
    /// `-1n >> 1n` is `-1n`, not `0n`. That is the operation JavaScript defines,
    /// and it is the one a sign-magnitude representation gets wrong when written
    /// as a magnitude shift: `>>` is a floor division by a power of two, and
    /// flooring a negative rounds *away* from zero. So a magnitude shift that
    /// dropped any set bit has rounded the wrong way, and the magnitude is
    /// corrected by one to compensate.
    pub fn shr(&self, amount: u32) -> Self {
        let shifted = mag_shr(&self.digits, amount);
        if !self.negative {
            return Self::from_parts(false, shifted);
        }
        // Scanning to `amount` would be right and would also be a four-billion
        // step loop for a count the caller is allowed to pass: `-1n >> 2n**40n`
        // is `-1n`, and every bit at or above the width is already zero, so the
        // scan stops at the width rather than at the count.
        let scan = (amount as usize).min(mag_bit_len(&self.digits));
        let lost = (0..scan).any(|index| mag_bit(&self.digits, index));
        if lost {
            Self::from_parts(true, shifted).sub(&BigInt::from_i64(1))
        } else {
            Self::from_parts(true, shifted)
        }
    }

    /// Bitwise `and`, on the two's-complement interpretation.
    pub fn bit_and(&self, other: &Self) -> Self {
        self.bitwise(other, |a, b| a & b)
    }

    /// Bitwise `or`, on the two's-complement interpretation.
    pub fn bit_or(&self, other: &Self) -> Self {
        self.bitwise(other, |a, b| a | b)
    }

    /// Bitwise `exclusive or`, on the two's-complement interpretation.
    pub fn bit_xor(&self, other: &Self) -> Self {
        self.bitwise(other, |a, b| a ^ b)
    }

    /// Bitwise `not`, which is `-self - 1`.
    ///
    /// Written as the identity rather than as a word-wise complement, because
    /// the word-wise version has to choose a width for a value with no width and
    /// then get the sign extension right; the identity is exact at every size
    /// and is what the two's-complement definition says `~x` means.
    pub fn bit_not(&self) -> Self {
        self.neg().sub(&BigInt::from_i64(1))
    }

    /// `BigInt.asIntN` and `BigInt.asUintN`: the low `bits` bits, read signed or
    /// unsigned.
    ///
    /// `asUintN(8, -1n)` is `255n` and `asIntN(8, 255n)` is `-1n` — the same
    /// eight bits, two readings, which is the whole content of the operation.
    ///
    /// The caller bounds `bits`: the language allows any index up to 2^53 and
    /// this allocates a word per 32 of them, so a value the caller has not
    /// sanity-checked can ask for a terabyte. Values that already fit take no
    /// allocation at all.
    pub fn wrap_to_bits(&self, bits: u32, signed: bool) -> Self {
        if bits == 0 {
            return BigInt::zero();
        }
        // The early-out is not only an optimisation — it is what keeps
        // `asIntN(64, small)` from touching memory proportional to `bits`.
        let occupied = mag_bit_len(&self.digits) as u32;
        if signed && occupied < bits {
            return self.clone();
        }
        if !signed && !self.negative && occupied <= bits {
            return self.clone();
        }

        let words = bits.div_ceil(32) as usize;
        let mut twos = self.to_twos(words);
        let top_bits = bits % 32;
        if top_bits != 0 {
            twos[words - 1] &= (1u32 << top_bits) - 1;
        }

        if !signed {
            trim(&mut twos);
            return Self::from_parts(false, twos);
        }

        // Signed: bit `bits - 1` is the sign, so when it is set the value is
        // negative and the bits above it inside the top word have to be filled
        // in before `from_twos` can read the word as a two's-complement number.
        let sign_index = (bits - 1) as usize;
        if twos[sign_index / 32] >> (sign_index % 32) & 1 == 1 && top_bits != 0 {
            twos[words - 1] |= !((1u32 << top_bits) - 1);
        }
        Self::from_twos(twos)
    }

    /// Applies a word-wise operation over the two's-complement forms.
    fn bitwise(&self, other: &Self, op: fn(u32, u32) -> u32) -> Self {
        let words = self.digits.len().max(other.digits.len()) + 1;
        let left = self.to_twos(words);
        let right = other.to_twos(words);
        let out = left
            .into_iter()
            .zip(right)
            .map(|(a, b)| op(a, b))
            .collect::<Vec<u32>>();
        Self::from_twos(out)
    }

    /// This value as `words` two's-complement words, truncating above them.
    ///
    /// Truncation is deliberate and `wrap_to_bits` depends on it: taking the low
    /// `n` words of the two's-complement form *is* reducing modulo 2^(32n), for
    /// a negative value as much as a positive one, because negating after
    /// truncating and truncating after negating agree modulo that power.
    fn to_twos(&self, words: usize) -> Vec<u32> {
        let mut out = vec![0u32; words];
        for (slot, digit) in out.iter_mut().zip(self.digits.iter()) {
            *slot = *digit;
        }
        if self.negative {
            negate(&mut out);
        }
        out
    }

    /// Reads two's-complement words back, with the top bit as the sign.
    fn from_twos(mut words: Vec<u32>) -> Self {
        let negative = words.last().is_some_and(|top| top >> 31 == 1);
        if negative {
            negate(&mut words);
        }
        trim(&mut words);
        Self::from_parts(negative, words)
    }
}

/// Negates a fixed-width two's-complement number in place: complement, then add
/// one. Wrapping at the top is correct rather than tolerated — that is what
/// modular arithmetic at this width means.
fn negate(words: &mut [u32]) {
    let mut carry = 1u64;
    for word in words.iter_mut() {
        let sum = (!*word) as u64 + carry;
        *word = sum as u32;
        carry = sum >> 32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> BigInt {
        BigInt::parse(text).expect("test literal parses")
    }

    #[test]
    fn shifting_right_is_arithmetic_and_keeps_the_sign() {
        // The case a magnitude shift gets wrong: the magnitude of -1n is 1, and
        // 1 >> 1 is 0, so the naive answer is 0n. JavaScript says -1n.
        assert_eq!(big("-1").shr(1), big("-1"));
        assert_eq!(big("-1").shr(1000), big("-1"), "and it stays there");
        assert_eq!(big("1").shr(1), BigInt::zero(), "the positive does reach 0");

        // `>>` floors, so a negative rounds away from zero: -5 >> 1 is -3, not
        // -2, which is what truncation would give.
        assert_eq!(big("-5").shr(1), big("-3"));
        assert_eq!(big("-8").shr(1), big("-4"), "and exact division does not round");
        assert_eq!(big("-8").shr(2), big("-2"));
        assert_eq!(big("-9").shr(2), big("-3"));
    }

    #[test]
    fn shifting_right_by_an_enormous_count_finishes() {
        // The count is a bigint, so a program may pass one that no loop can
        // walk. Answering `-1n` here takes a step per bit of the OPERAND, not
        // per unit of the count — without that, this test does not fail, it
        // never returns.
        assert_eq!(big("-1").shr(u32::MAX), big("-1"));
        assert_eq!(big("-12345678901234567890").shr(u32::MAX), big("-1"));
        assert_eq!(big("12345678901234567890").shr(u32::MAX), BigInt::zero());
    }

    #[test]
    fn shifting_left_scales_and_never_overflows() {
        assert_eq!(big("1").shl(64).to_decimal(), "18446744073709551616");
        assert_eq!(big("-1").shl(64).to_decimal(), "-18446744073709551616");
        assert_eq!(big("3").shl(1), big("6"));
        assert_eq!(BigInt::zero().shl(1000), BigInt::zero());
        // Round-trip across a word boundary, where the split shift is written.
        assert_eq!(big("123456789012345678901").shl(37).shr(37), big("123456789012345678901"));
    }

    #[test]
    fn the_bitwise_operators_read_the_twos_complement_and_not_the_magnitude() {
        // The headline case. A magnitude-wise `and` answers 1n here, because the
        // magnitude of -1n is 1 — plausible, and wrong.
        assert_eq!(big("-1").bit_and(&big("3")), big("3"));
        assert_eq!(big("-1").bit_or(&big("3")), big("-1"));
        assert_eq!(big("-1").bit_xor(&big("3")), big("-4"));
        assert_eq!(big("-2").bit_and(&big("3")), big("2"));
        assert_eq!(big("12").bit_and(&big("10")), big("8"));
        assert_eq!(big("12").bit_or(&big("10")), big("14"));
        assert_eq!(big("12").bit_xor(&big("10")), big("6"));
    }

    #[test]
    fn a_negative_result_needs_the_extra_sign_word() {
        // Over exactly max(len) words this answers 4294967295n: the result fills
        // the word and there is no bit left to carry the sign.
        assert_eq!(big("-1").bit_and(&big("-1")), big("-1"));
        assert_eq!(big("-4294967296").bit_and(&big("-1")), big("-4294967296"));
        assert_eq!(
            big("-18446744073709551616").bit_or(&big("-1")),
            big("-1")
        );
    }

    #[test]
    fn not_is_the_identity_and_agrees_with_the_other_three() {
        assert_eq!(big("0").bit_not(), big("-1"));
        assert_eq!(big("-1").bit_not(), BigInt::zero());
        assert_eq!(big("5").bit_not(), big("-6"));
        // De Morgan, which fails for any width the implementation picked wrong.
        for (a, b) in [("-1", "3"), ("12", "10"), ("-7", "-13"), ("0", "-1")] {
            let (a, b) = (big(a), big(b));
            assert_eq!(
                a.bit_and(&b).bit_not(),
                a.bit_not().bit_or(&b.bit_not()),
                "~(a & b) == ~a | ~b for {a:?} {b:?}"
            );
        }
    }

    #[test]
    fn as_int_n_and_as_uint_n_are_the_same_bits_read_two_ways() {
        assert_eq!(big("-1").wrap_to_bits(8, false), big("255"));
        assert_eq!(big("255").wrap_to_bits(8, true), big("-1"));
        assert_eq!(big("127").wrap_to_bits(8, true), big("127"));
        assert_eq!(big("128").wrap_to_bits(8, true), big("-128"));
        assert_eq!(big("-1").wrap_to_bits(64, false), big("18446744073709551615"));
        assert_eq!(
            big("18446744073709551615").wrap_to_bits(64, true),
            big("-1"),
            "the top bit of a full word is the sign, with no partial mask to set"
        );
        assert_eq!(big("5").wrap_to_bits(0, true), BigInt::zero(), "no bits, no value");
        assert_eq!(big("5").wrap_to_bits(0, false), BigInt::zero());
    }

    #[test]
    fn a_value_that_already_fits_is_returned_unchanged() {
        // Same answer as the general path would give — asserted rather than
        // assumed, because the early-out is what keeps a large `bits` cheap and
        // an early-out that disagrees is worse than none.
        for bits in [8u32, 32, 64, 200] {
            for text in ["0", "1", "-1", "127", "-128", "1000000"] {
                let value = big(text);
                let wrapped = value.wrap_to_bits(bits, true);
                let forced = {
                    let words = bits.div_ceil(32) as usize;
                    let mut twos = value.to_twos(words);
                    let top = bits % 32;
                    if top != 0 {
                        twos[words - 1] &= (1u32 << top) - 1;
                        let sign = (bits - 1) as usize;
                        if twos[sign / 32] >> (sign % 32) & 1 == 1 {
                            twos[words - 1] |= !((1u32 << top) - 1);
                        }
                    }
                    BigInt::from_twos(twos)
                };
                assert_eq!(wrapped, forced, "asIntN({bits}, {text}n)");
            }
        }
    }
}
