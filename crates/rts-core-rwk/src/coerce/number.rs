//! Numbers to strings and back.
//!
//! Both directions have a version that passes every obvious test and is wrong,
//! and they are wrong in opposite ways: printing is wrong about *precision*,
//! parsing is wrong about *what counts as a number*.

use crate::text::Str;

/// `Number::toString`, radix 10 — what `String(n)` and a template literal use.
///
/// # The shortest decimal that round-trips
///
/// Not a fixed precision. The specification asks for the fewest digits that read
/// back as the same double, so:
///
/// - `0.1` prints `"0.1"`, not `"0.1000000000000000055511151231257827"`.
/// - `0.1 + 0.2` prints `"0.30000000000000004"` — seventeen digits, because
///   sixteen read back as a different double.
///
/// No fixed choice is right for both, which is why `%.15g` and `%.17g` are each
/// wrong for one of them. Rust's own `f64` formatting already computes the
/// shortest round-tripping digits, so this reaches for that rather than
/// implementing Ryū again — what it adds is JavaScript's rules for *where the
/// decimal point goes*, which Rust does differently.
///
/// # Where the two disagree
///
/// | value | Rust `{}` | JavaScript |
/// |---|---|---|
/// | `1e21` | `1000000000000000000000` | `1e+21` |
/// | `1e-7` | `0.0000001` | `1e-7` |
/// | `-0.0` | `-0` | `0` |
///
/// The thresholds are exactly 21 significant positions and an exponent below
/// −6, and they are not arbitrary: they are what the specification says, so a
/// program printing an array of numbers matches other engines character for
/// character.
pub fn number_to_string(value: f64) -> Str {
    if value.is_nan() {
        return Str::from_str("NaN");
    }
    if value == 0.0 {
        // Both zeros print "0". The sign is a real distinction everywhere else
        // — `Object.is(-0, 0)` is false, `1 / -0` is `-Infinity` — and printing
        // is the one place it is dropped.
        return Str::from_str("0");
    }
    if value < 0.0 {
        return Str::from_str("-").concat(&number_to_string(-value));
    }
    if value.is_infinite() {
        return Str::from_str("Infinity");
    }

    let (digits, exponent) = shortest_digits(value);
    // The specification's `n`: the position of the decimal point relative to the
    // digit string. `k` is how many digits there are.
    let n = exponent + 1;
    let k = digits.len() as i32;

    let text = if k <= n && n <= 21 {
        // Digits, then the zeros needed to reach the point.
        let mut out = digits;
        out.push_str(&"0".repeat((n - k) as usize));
        out
    } else if 0 < n && n <= 21 {
        // A point inside the digits.
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        // Leading zeros, then every digit after the point.
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        // Exponential, and the sign of the exponent is always written.
        let sign = if n >= 1 { '+' } else { '-' };
        let magnitude = (n - 1).abs();
        if k == 1 {
            format!("{digits}e{sign}{magnitude}")
        } else {
            format!("{}.{}e{sign}{magnitude}", &digits[..1], &digits[1..])
        }
    };

    Str::from_str(&text)
}

/// The shortest round-tripping digits, and the exponent they sit at.
///
/// Rust's `{:e}` produces exactly the shortest form; this only takes it apart.
/// Reimplementing the digit generation would be reimplementing Ryū, and getting
/// it subtly wrong is how a program prints a number that reads back as a
/// different one.
fn shortest_digits(value: f64) -> (String, i32) {
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("`{:e}` always writes an exponent");
    let digits = mantissa.replace('.', "");
    let exponent: i32 = exponent.parse().expect("`{:e}` always writes an integer");
    (digits, exponent)
}

/// `ToNumber` applied to a string.
///
/// # What is not a mistake here
///
/// `Number("")` is `+0`, and so is `Number("   ")`. That is the specification,
/// and it is the opposite of `parseFloat("")`, which is `NaN` — the two are
/// different functions and the difference is easy to remember backwards.
///
/// # What a sign may precede
///
/// A decimal, `Infinity`, and nothing else. `Number("-0x1")` is `NaN` while
/// `Number("-1")` is `-1`, because the hex, octal and binary forms have no sign
/// in their grammar. A parser that stripped a leading sign and then dispatched
/// on the prefix would accept all three.
pub fn string_to_number(text: &Str) -> f64 {
    let Some(rust) = text.to_rust() else {
        // A lone surrogate is a legal string and not a legal number.
        return f64::NAN;
    };
    let trimmed = rust.trim_matches(is_string_whitespace);

    if trimmed.is_empty() {
        return 0.0;
    }

    // The prefixed forms, which take no sign.
    if let Some(rest) = trimmed.strip_prefix("0x").or(trimmed.strip_prefix("0X")) {
        return radix_value(rest, 16);
    }
    if let Some(rest) = trimmed.strip_prefix("0o").or(trimmed.strip_prefix("0O")) {
        return radix_value(rest, 8);
    }
    if let Some(rest) = trimmed.strip_prefix("0b").or(trimmed.strip_prefix("0B")) {
        return radix_value(rest, 2);
    }

    // `Infinity`, with an optional sign.
    let (sign, magnitude) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if magnitude == "Infinity" {
        return sign * f64::INFINITY;
    }

    // A decimal. Rust's parser accepts some spellings JavaScript does not —
    // `inf`, `NaN`, `1_0` — so they are refused before it is asked.
    if magnitude.is_empty()
        || magnitude
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return f64::NAN;
    }
    magnitude
        .parse::<f64>()
        .map(|value| sign * value)
        .unwrap_or(f64::NAN)
}

fn radix_value(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    match u128::from_str_radix(digits, radix) {
        Ok(value) => value as f64,
        // Too large for a `u128` is not an error — it is a number a double
        // rounds. Refusing it would make `Number("0x" + "f".repeat(40))` a NaN
        // where the language says `Infinity`-adjacent magnitude.
        Err(_) if digits.bytes().all(|b| (b as char).is_digit(radix)) => f64::INFINITY,
        Err(_) => f64::NAN,
    }
}

/// The code points `ToNumber` trims from a string.
///
/// Both whitespace and line terminators, which is why a string with a newline in
/// it still converts. `\u{FEFF}` is included — a byte-order mark at the front of
/// a number is trimmed rather than making it `NaN`.
fn is_string_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(value: f64) -> String {
        number_to_string(value).to_rust().unwrap()
    }

    fn parsed(text: &str) -> f64 {
        string_to_number(&Str::from_str(text))
    }

    #[test]
    fn printing_uses_the_shortest_decimal_that_reads_back() {
        assert_eq!(printed(0.1), "0.1", "not seventeen digits");
        assert_eq!(
            printed(0.1 + 0.2),
            "0.30000000000000004",
            "and here seventeen, because sixteen read back as a different double"
        );
        assert_eq!(printed(1.0), "1");
        assert_eq!(printed(1.5), "1.5");
        assert_eq!(printed(123.456), "123.456");
    }

    #[test]
    fn both_zeros_print_the_same_and_are_otherwise_different() {
        assert_eq!(printed(0.0), "0");
        assert_eq!(printed(-0.0), "0", "the one place the sign is dropped");
        assert_ne!(
            (0.0f64).to_bits(),
            (-0.0f64).to_bits(),
            "and they are distinguishable everywhere else"
        );
    }

    #[test]
    fn the_exponential_thresholds_are_the_specifications_and_not_rusts() {
        assert_eq!(printed(1e20), "100000000000000000000");
        assert_eq!(printed(1e21), "1e+21", "21 is where it turns over");
        assert_eq!(printed(1e-6), "0.000001");
        assert_eq!(printed(1e-7), "1e-7", "and −6 is the other end");
        assert_eq!(printed(1.5e22), "1.5e+22");
    }

    #[test]
    fn the_special_values_print_as_words() {
        assert_eq!(printed(f64::NAN), "NaN");
        assert_eq!(printed(f64::INFINITY), "Infinity");
        assert_eq!(printed(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn an_empty_string_is_zero_and_not_nan() {
        assert_eq!(parsed(""), 0.0);
        assert_eq!(parsed("   "), 0.0);
        assert_eq!(parsed("\n\t"), 0.0);
        assert_eq!(
            parsed("\u{FEFF}1"),
            1.0,
            "a byte-order mark is trimmed, not a syntax error"
        );
    }

    #[test]
    fn a_prefixed_form_takes_no_sign() {
        assert_eq!(parsed("0x1A"), 26.0);
        assert_eq!(parsed("0b101"), 5.0);
        assert_eq!(parsed("0o17"), 15.0);
        assert_eq!(parsed("-1"), -1.0, "a decimal does take one");
        assert!(
            parsed("-0x1").is_nan(),
            "and stripping the sign before dispatching on the prefix would \
             accept this"
        );
        assert!(parsed("+0b1").is_nan());
    }

    #[test]
    fn infinity_is_spelled_exactly_one_way() {
        assert_eq!(parsed("Infinity"), f64::INFINITY);
        assert_eq!(parsed("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(parsed("  Infinity  "), f64::INFINITY);
        assert!(parsed("infinity").is_nan(), "case matters");
        assert!(parsed("inf").is_nan(), "which Rust's parser would accept");
    }

    #[test]
    fn rust_spellings_that_javascript_does_not_have_are_refused() {
        assert!(parsed("NaN").is_nan(), "as a number, not as the word");
        assert!(parsed("1_0").is_nan(), "no separators in a string");
        assert!(parsed("0x").is_nan(), "a prefix with no digits");
        assert!(parsed("1 2").is_nan());
    }

    #[test]
    fn printing_and_parsing_are_inverses_where_they_can_be() {
        for value in [0.1, 1.0, 1.5, 1e21, 1e-7, 123.456, 0.1 + 0.2, -42.0] {
            let printed = number_to_string(value);
            assert_eq!(
                string_to_number(&printed),
                value,
                "{value} printed as {:?} and did not read back",
                printed.to_rust()
            );
        }
    }
}
