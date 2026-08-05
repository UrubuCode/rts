//! Reading a bigint from text, and writing one back.
//!
//! Both directions work in **chunks** rather than one character at a time: the
//! largest power of the radix that fits in a `u32` is one multiply-and-add going
//! in and one single-digit division coming out, so a 1000-digit decimal costs a
//! ninth of the passes it would otherwise. The naive version is not wrong, only
//! quadratic with a constant nine times too large, and `to_decimal` is what a
//! program calls to print.

use super::arith::{mag_divrem_small, mag_mul_add_small};
use super::BigInt;

/// The digit characters, in the order their values run.
///
/// Lowercase, because `Number::toString(16)` and `BigInt::toString(16)` produce
/// lowercase and a program comparing the two strings must not find a difference.
const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

impl BigInt {
    /// Reads a bigint from text, or `None` when the text is not one.
    ///
    /// # What is accepted
    ///
    /// A decimal with an optional `+` or `-`, and the `0x`, `0o` and `0b` forms
    /// **without** a sign. The asymmetry is the grammar's, not an oversight:
    /// `-0x10n` is unary minus applied to the literal `0x10n`, so no signed
    /// prefixed form ever reaches a parser, and `BigInt("-0x10")` is a
    /// `SyntaxError`. A parser that stripped the sign and then dispatched on the
    /// prefix would accept all three — the same trap `string_to_number` names.
    ///
    /// The empty string is `0n`, matching `BigInt("")`. It is the one input
    /// where "not a number" and "zero" are the same answer.
    ///
    /// # What is refused
    ///
    /// Underscore separators. `1_000n` is a legal literal and `BigInt("1_000")`
    /// is a `SyntaxError`; this answers for the string form. The literal path
    /// removes separators while lexing — it must, for `1_000` as well — so a
    /// flag here would have no caller.
    ///
    /// Surrounding whitespace, which `BigInt(" 1 ")` does allow. It is refused
    /// **here** because trimming it needs the language's whitespace set, and
    /// that set is already written once, in `coerce::number`. A second copy is a
    /// rule that will be edited on one side only; the coercion layer trims and
    /// then calls this.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(BigInt::zero());
        }

        // The prefixed forms are tested before the sign is looked at, which is
        // what makes them unsigned.
        for (prefix, radix) in [
            ("0x", 16),
            ("0X", 16),
            ("0o", 8),
            ("0O", 8),
            ("0b", 2),
            ("0B", 2),
        ] {
            if let Some(rest) = text.strip_prefix(prefix) {
                return digits_from_text(rest, radix).map(|d| Self::from_parts(false, d));
            }
        }

        let (negative, body) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        digits_from_text(body, 10).map(|d| Self::from_parts(negative, d))
    }

    /// The value in base ten, which is what `String(big)` produces.
    ///
    /// No `n` suffix. The suffix belongs to the literal syntax and to
    /// `console.log`'s inspection, not to `ToString` — `` `${1n}` `` is `"1"`.
    pub fn to_decimal(&self) -> String {
        self.to_radix(10)
    }

    /// The value in a base from 2 to 36, which is `BigInt::toString(radix)`.
    ///
    /// # Panics
    ///
    /// If the radix is outside 2..=36, matching `u32::from_str_radix`. The
    /// language throws a `RangeError` there, and a caller that has not already
    /// checked has a bug rather than a value to convert.
    pub fn to_radix(&self, radix: u32) -> String {
        assert!(
            (2..=36).contains(&radix),
            "radix must be between 2 and 36, got {radix}"
        );
        if self.is_zero() {
            // The one value with no digits, and the one whose sign is not
            // written even though the loop below would happily write it.
            return "0".to_string();
        }

        let (chunk, per_chunk) = chunk_of(radix);
        let mut reversed = Vec::<u8>::new();
        let mut remaining = self.digits.clone();
        while !remaining.is_empty() {
            let (quotient, mut rest) = mag_divrem_small(&remaining, chunk);
            remaining = quotient;
            for _ in 0..per_chunk {
                reversed.push(DIGITS[(rest % radix) as usize]);
                rest /= radix;
                // A chunk that is not the last one is padded to its full width;
                // only the final one stops early, or `10n ** 9n` prints as "1".
                if remaining.is_empty() && rest == 0 {
                    break;
                }
            }
        }

        if self.negative {
            reversed.push(b'-');
        }
        reversed.reverse();
        String::from_utf8(reversed).expect("every byte written is ASCII")
    }
}

/// Reads unsigned digits in a radix, or `None` if any character is not one.
///
/// An empty string is `None` here, unlike in [`BigInt::parse`]: `BigInt("0x")`
/// is a `SyntaxError` where `BigInt("")` is `0n`, so the emptiness has to be
/// answered by whoever knows whether a prefix was consumed.
fn digits_from_text(text: &str, radix: u32) -> Option<Vec<u32>> {
    if text.is_empty() {
        return None;
    }
    let (_, per_chunk) = chunk_of(radix);
    let bytes = text.as_bytes();
    let mut digits: Vec<u32> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let end = (index + per_chunk as usize).min(bytes.len());
        let mut value = 0u32;
        let mut scale = 1u32;
        for &byte in &bytes[index..end] {
            // A non-ASCII byte maps to some char that is not a digit in any
            // radix, so this rejects it without a separate width check.
            let digit = (byte as char).to_digit(radix)?;
            value = value * radix + digit;
            scale *= radix;
        }
        // `scale` is at most `chunk`, which fits by construction — the partial
        // final group is smaller still.
        mag_mul_add_small(&mut digits, scale, value);
        index = end;
    }
    Some(digits)
}

/// The largest power of the radix that fits in a `u32`, and its exponent.
///
/// Computed rather than tabled: a table of 35 entries is 35 chances to mistype
/// one, and the loop runs at most 32 times.
fn chunk_of(radix: u32) -> (u32, u32) {
    let mut chunk = 1u32;
    let mut count = 0;
    while chunk as u64 * radix as u64 <= u32::MAX as u64 {
        chunk *= radix;
        count += 1;
    }
    (chunk, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> BigInt {
        BigInt::parse(text).expect("test literal parses")
    }

    #[test]
    fn parse_round_trips_through_decimal_far_past_a_u64() {
        // Well past 2^64, so this exercises the chunking on both sides rather
        // than a value that happens to fit in one machine word.
        for text in [
            "0",
            "1",
            "-1",
            "18446744073709551616",
            "123456789012345678901234567890123456789012345678901234567890",
            "-99999999999999999999999999999999999999999999999999",
            "1000000000000000000",
        ] {
            assert_eq!(big(text).to_decimal(), text, "{text} did not round-trip");
        }

        // And a value built by arithmetic rather than read, so the round-trip
        // is not merely the parser agreeing with itself.
        assert_eq!(
            big("2").pow(128).to_decimal(),
            "340282366920938463463374607431768211456"
        );
    }

    #[test]
    fn a_prefixed_form_takes_no_sign() {
        assert_eq!(big("0x1a").to_decimal(), "26");
        assert_eq!(big("0XFF").to_decimal(), "255");
        assert_eq!(big("0o17").to_decimal(), "15");
        assert_eq!(big("0b1011").to_decimal(), "11");
        assert_eq!(big("-26").to_decimal(), "-26", "a decimal does take one");
        assert_eq!(BigInt::parse("-0x1a"), None);
        assert_eq!(BigInt::parse("+0b1"), None);
        assert_eq!(BigInt::parse("0x"), None, "a prefix with no digits");
    }

    #[test]
    fn what_is_not_a_bigint() {
        assert_eq!(BigInt::parse("1_000"), None, "legal literal, illegal string");
        assert_eq!(BigInt::parse("1.5"), None);
        assert_eq!(BigInt::parse("1n"), None, "the suffix is syntax, not text");
        assert_eq!(BigInt::parse("1e3"), None, "no exponent form for a bigint");
        assert_eq!(BigInt::parse(" 1"), None, "the caller trims");
        assert_eq!(BigInt::parse("-"), None);
        assert_eq!(BigInt::parse("0b2"), None, "a digit outside the radix");
        assert_eq!(BigInt::parse(""), Some(BigInt::zero()), "and this one is 0n");
    }

    #[test]
    fn a_chunk_boundary_does_not_swallow_or_invent_a_zero() {
        // The failure this pins: a chunk that is not the last must be written to
        // its full width. 10^9 is exactly the decimal chunk, so a missing pad
        // prints it as "1" and an extra one as "10000000000".
        assert_eq!(big("1000000000").to_decimal(), "1000000000");
        assert_eq!(big("1000000001").to_decimal(), "1000000001");
        assert_eq!(big("1000000000000000000").to_decimal(), "1000000000000000000");
        assert_eq!(BigInt::from_i64(10).pow(9).to_decimal(), "1000000000");
        assert_eq!(BigInt::from_i64(10).pow(18).to_decimal(), "1000000000000000000");
    }

    #[test]
    fn radices_agree_with_each_other_and_are_lowercase() {
        assert_eq!(big("255").to_radix(16), "ff");
        assert_eq!(big("255").to_radix(2), "11111111");
        assert_eq!(big("-255").to_radix(16), "-ff");
        assert_eq!(big("35").to_radix(36), "z");
        assert_eq!(BigInt::zero().to_radix(36), "0", "and zero writes no sign");

        // Printed in a base and read back in the same one, for a value long
        // enough to cross several chunks in every base.
        let value = big("2").pow(200).sub(&big("1"));
        for radix in [2u32, 3, 8, 10, 16, 36] {
            let printed = value.to_radix(radix);
            let prefix = match radix {
                2 => "0b",
                8 => "0o",
                16 => "0x",
                _ => "",
            };
            if !prefix.is_empty() {
                assert_eq!(BigInt::parse(&format!("{prefix}{printed}")), Some(value.clone()));
            }
            assert!(!printed.starts_with('0'), "no leading zero in base {radix}");
        }
    }

    #[test]
    fn to_radix_refuses_a_base_it_has_no_digits_for() {
        assert!(std::panic::catch_unwind(|| big("1").to_radix(1)).is_err());
        assert!(std::panic::catch_unwind(|| big("1").to_radix(37)).is_err());
    }
}
