//! Writing a number in the shapes the shortest round-tripping decimal is not:
//! a fixed number of places, an exponent, a count of significant digits, and a
//! base that is not ten.
//!
//! # Why these are here rather than as `format!` widths
//!
//! Rust rounds half to **even** and the specification rounds half **away from
//! zero**: `format!("{:.0}", 2.5)` is `"2"` and `(2.5).toFixed(0)` is `"3"`. The
//! same gap [`fixed`] records, and the reason the digits are produced by
//! scaling and rounding rather than by handing the number to a formatter.
//!
//! # Why the exponent is recomputed after rounding
//!
//! Rounding can carry: `(9.99).toExponential(1)` is `"1.0e+1"`, not `"10.0e+0"`.
//! An implementation that took the exponent from the input and then rounded the
//! mantissa produces the second, which is a number written in a form no engine
//! answers.

/// `x.toExponential(digits)`, with `None` for "as many as it takes".
pub(super) fn exponential(number: f64, digits: Option<usize>) -> String {
    if !number.is_finite() {
        return crate::coerce::number_to_string(number).to_rust().unwrap_or_default();
    }
    let sign = if number < 0.0 { "-" } else { "" };
    let magnitude = number.abs();
    if magnitude == 0.0 {
        let mantissa = with_places(0.0, digits.unwrap_or(0));
        return format!("{sign}{mantissa}e+0");
    }

    let (mantissa, exponent) = match digits {
        Some(places) => carried(magnitude, places),
        // Shortest form: Rust's own `{:e}` already answers the fewest digits
        // that round-trip, which is the same question `number_to_string` asks
        // and not one worth asking a second way.
        None => {
            let written = format!("{magnitude:e}");
            let (head, tail) = written.split_once('e').unwrap_or((written.as_str(), "0"));
            (head.to_string(), tail.parse::<i32>().unwrap_or(0))
        }
    };
    let marker = if exponent < 0 { "-" } else { "+" };
    format!("{sign}{mantissa}e{marker}{}", exponent.abs())
}

/// `x.toPrecision(digits)` — significant digits, in whichever shape fits.
///
/// The specification's own rule for which: exponential when the exponent is
/// below -6 or has reached the digit count, and fixed otherwise. Written as that
/// comparison rather than as a length test on the result, because the two
/// disagree exactly where a program would notice.
pub(super) fn precision(number: f64, digits: usize) -> String {
    if !number.is_finite() {
        return crate::coerce::number_to_string(number).to_rust().unwrap_or_default();
    }
    if number == 0.0 {
        // Unsigned, for the reason `super::fixed` records: the specification
        // tests `x < 0`, which negative zero is not.
        return with_places(0.0, digits.saturating_sub(1));
    }
    let (_, exponent) = carried(number.abs(), digits.saturating_sub(1));
    if exponent < -6 || exponent >= digits as i32 {
        return exponential(number, Some(digits.saturating_sub(1)));
    }
    // Below the decimal point, the digits left of it are already spent — so the
    // count that remains is the precision minus what the exponent used.
    let places = (digits as i32 - 1 - exponent).max(0) as usize;
    fixed(number, places)
}

/// A number written in a base other than ten.
///
/// The integer part by repeated division and the fraction by repeated
/// multiplication, to twenty places — which is where a base-2 expansion of a
/// double stops carrying information rather than an arbitrary cut. The
/// specification leaves the exact digit count implementation-defined here, and
/// saying so is better than implying a precision that is not there.
pub(super) fn in_radix(number: f64, base: u32) -> String {
    if !number.is_finite() {
        return crate::coerce::number_to_string(number)
            .to_rust()
            .unwrap_or_default();
    }
    let negative = number < 0.0;
    let number = number.abs();
    let mut whole = number.trunc();
    let mut fraction = number.fract();

    let digit = |value: u32| char::from_digit(value, base).unwrap_or('0');
    let mut left = String::new();
    if whole == 0.0 {
        left.push('0');
    }
    while whole >= 1.0 {
        let rest = whole % f64::from(base);
        left.push(digit(rest as u32));
        whole = (whole / f64::from(base)).trunc();
    }
    let mut out: String = left.chars().rev().collect();

    if fraction > 0.0 {
        out.push('.');
        for _ in 0..20 {
            if fraction == 0.0 {
                break;
            }
            fraction *= f64::from(base);
            out.push(digit(fraction.trunc() as u32));
            fraction = fraction.fract();
        }
    }
    match negative {
        true => format!("-{out}"),
        false => out,
    }
}

/// A number to a fixed number of decimal places, rounding half away from zero.
///
/// Written over the *exact* decimal expansion rather than a scale-then-round
/// (`number * 10^places`, then `f64::round`), which is lossy in the
/// multiplication itself: `2.55_f64` is `2.549999999999999822…`, but
/// `2.55 * 10.0` rounds *up* to exactly `25.5` in f64, so a scaled round sees a
/// tie that the real value never had and answers `"2.6"` where the spec (and
/// Node) answer `"2.5"`. `format!("{:.N}")` on an `f64` is exact for any `N` —
/// the standard library's fixed-precision float formatter computes the true
/// decimal digits via big-integer arithmetic — so asking for far more digits
/// than `places` and rounding the resulting *string* half-away-from-zero
/// (ties round up, matching the spec's "pick the larger n") never sees a
/// multiplication-introduced tie.
pub(super) fn fixed(number: f64, places: usize) -> String {
    // `x < 0`, and NOT the sign bit: the specification's step is "if x < 0, set
    // s to `-`", which leaves negative zero unsigned. `(-0).toFixed(2)` is
    // `"0.00"` in every engine, and reading the bit answered `"-0.00"`.
    let negative = number < 0.0;
    let magnitude = number.abs();
    // f64's exact decimal expansion terminates within a few dozen digits for
    // any value with a non-degenerate binary fraction; 60 digits of margin
    // beyond `places` is enough to see whether the first dropped digit is a
    // genuine tie or just close to one.
    let exact = format!("{:.*}", places + 60, magnitude);
    let rounded = round_decimal_string(&exact, places);
    match negative {
        true => format!("-{rounded}"),
        false => rounded,
    }
}

/// Rounds a `"123.456789…"` string to `places` fractional digits,
/// half-away-from-zero (i.e. ties round up), on the digits themselves — so it
/// never re-introduces the floating-point rounding [`fixed`] exists to avoid.
fn round_decimal_string(digits: &str, places: usize) -> String {
    let (int_part, frac_part) = digits.split_once('.').unwrap_or((digits, ""));
    let mut int_digits: Vec<u8> = int_part.bytes().collect();
    let frac_bytes = frac_part.as_bytes();
    let round_up = frac_bytes.get(places).is_some_and(|&d| d >= b'5');
    let mut frac_digits: Vec<u8> = frac_bytes[..places.min(frac_bytes.len())].to_vec();
    frac_digits.resize(places, b'0');
    if round_up {
        let mut carry = true;
        for d in frac_digits.iter_mut().rev() {
            if !carry {
                break;
            }
            match *d {
                b'9' => *d = b'0',
                _ => {
                    *d += 1;
                    carry = false;
                }
            }
        }
        for d in int_digits.iter_mut().rev() {
            if !carry {
                break;
            }
            match *d {
                b'9' => *d = b'0',
                _ => {
                    *d += 1;
                    carry = false;
                }
            }
        }
        if carry {
            int_digits.insert(0, b'1');
        }
    }
    let int_str = String::from_utf8(int_digits).unwrap();
    match places {
        0 => int_str,
        _ => format!("{int_str}.{}", String::from_utf8(frac_digits).unwrap()),
    }
}

/// A magnitude as a mantissa and the exponent it ended up with.
///
/// Both together, because rounding decides the second: see the module
/// documentation for the carry that makes taking the exponent first wrong.
///
/// Built over the exact decimal expansion rather than `magnitude * 10^scale`
/// then `f64::round` — the same lossy-multiplication trap [`fixed`]
/// documents: `9.95_f64` is `9.94999999999999928…`, but scaling by 10 and
/// rounding can land exactly on a tie the real value never had, answering
/// `"10"` where the spec (and Node) answer `"9.9"`. `format!("{:.N}")` is
/// exact for any `N`, so the significant digits are read off that string and
/// rounded half-away-from-zero on the digits themselves.
fn carried(magnitude: f64, places: usize) -> (String, i32) {
    let significant_count = places + 1;
    // 80 digits after the point is far more than any f64's exact decimal
    // expansion needs to disambiguate a rounding decision at a realistic
    // `places` — see `fixed`'s equivalent margin.
    let exact = format!("{:.80}", magnitude);
    let (int_part, frac_part) = exact.split_once('.').unwrap_or((exact.as_str(), ""));
    let int_len = int_part.len() as i32;
    let mut digits: Vec<u8> = int_part.bytes().chain(frac_part.bytes()).collect();
    let first_nonzero = digits.iter().position(|&d| d != b'0').unwrap_or(0);
    let mut exponent = int_len - 1 - first_nonzero as i32;

    let take = significant_count + 1;
    let mut sig: Vec<u8> = digits.split_off(first_nonzero).into_iter().take(take).collect();
    sig.resize(take, b'0');
    let round_up = sig[significant_count] >= b'5';
    sig.truncate(significant_count);
    if round_up {
        let mut carry = true;
        for d in sig.iter_mut().rev() {
            if !carry {
                break;
            }
            match *d {
                b'9' => *d = b'0',
                _ => {
                    *d += 1;
                    carry = false;
                }
            }
        }
        if carry {
            // Every digit was `9`: the carry pushed a new leading `1` in,
            // which shifts every kept digit one place right and raises the
            // exponent — `9.99` rounded to one place is `10.0`, i.e. `1.0e+1`.
            sig.insert(0, b'1');
            sig.pop();
            exponent += 1;
        }
    }
    let digit_str = String::from_utf8(sig).unwrap();
    let mantissa = match places {
        0 => digit_str,
        _ => format!("{}.{}", &digit_str[..1], &digit_str[1..]),
    };
    (mantissa, exponent)
}

/// A mantissa written with exactly this many places after the point.
fn with_places(value: f64, places: usize) -> String {
    match places {
        0 => format!("{}", value.trunc() as i64),
        _ => format!("{value:.places$}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_carries_into_the_exponent() {
        // The case an implementation that fixes the exponent first gets wrong.
        assert_eq!(exponential(9.99, Some(1)), "1.0e+1");
        assert_eq!(exponential(1234.0, Some(2)), "1.23e+3");
        assert_eq!(exponential(0.0001, Some(1)), "1.0e-4");
    }

    #[test]
    fn a_half_rounds_away_from_zero_rather_than_to_even() {
        // Rust's formatter answers "1.2e+0" for both of these.
        assert_eq!(exponential(1.25, Some(1)), "1.3e+0");
        assert_eq!(exponential(1.35, Some(1)), "1.4e+0");
    }

    #[test]
    fn precision_chooses_the_shape_by_the_exponent() {
        assert_eq!(precision(123.456, 5), "123.46");
        // The exponent has reached the digit count, so the fixed form would
        // spend digits the caller did not ask for.
        assert_eq!(precision(123.456, 2), "1.2e+2");
        assert_eq!(precision(0.000001, 2), "0.0000010");
        assert_eq!(precision(0.0000001, 2), "1.0e-7");
    }

    #[test]
    fn zero_keeps_its_places() {
        assert_eq!(exponential(0.0, Some(2)), "0.00e+0");
        assert_eq!(precision(0.0, 3), "0.00");
    }

    #[test]
    fn a_fraction_terminates_in_a_base_that_divides_it() {
        // A power of two written in base 2, 8 or 16 has a FINITE expansion, so
        // there is one right answer and every engine gives it. The twenty-digit
        // cut must not show up in any of these, which is what a loop that
        // stops when the fraction reaches zero buys over one that always runs
        // its full count.
        assert_eq!(in_radix(0.5, 2), "0.1");
        assert_eq!(in_radix(0.0625, 2), "0.0001");
        assert_eq!(in_radix(10.625, 2), "1010.101");
        assert_eq!(in_radix(255.5, 8), "377.4");
        assert_eq!(in_radix(4095.9375, 16), "fff.f");
        assert_eq!(in_radix(0.03125, 32), "0.1");
        // The sign is written once, in front of the whole thing.
        assert_eq!(in_radix(-2.5, 2), "-10.1");
    }

    #[test]
    fn an_integer_keeps_every_digit_it_has() {
        assert_eq!(in_radix(9_007_199_254_740_991.0, 36), "2gosa7pa2gv");
        assert_eq!(in_radix(0.0, 2), "0");
        // Negative zero is not negative here: `-0 < 0` is false, and every
        // engine writes `"0"`.
        assert_eq!(in_radix(-0.0, 2), "0");
    }
}
