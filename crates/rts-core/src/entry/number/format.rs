//! Writing a number in the two shapes `toString` does not produce.
//!
//! # Why these are here rather than as `format!` widths
//!
//! Rust rounds half to **even** and the specification rounds half **away from
//! zero**: `format!("{:.0}", 2.5)` is `"2"` and `(2.5).toFixed(0)` is `"3"`. The
//! same gap [`super::fixed`] records, and the reason the digits are produced by
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
        let sign = if number.is_sign_negative() { "-" } else { "" };
        return format!("{sign}{}", with_places(0.0, digits.saturating_sub(1)));
    }
    let (_, exponent) = carried(number.abs(), digits.saturating_sub(1));
    if exponent < -6 || exponent >= digits as i32 {
        return exponential(number, Some(digits.saturating_sub(1)));
    }
    // Below the decimal point, the digits left of it are already spent — so the
    // count that remains is the precision minus what the exponent used.
    let places = (digits as i32 - 1 - exponent).max(0) as usize;
    super::fixed(number, places)
}

/// A magnitude as a mantissa and the exponent it ended up with.
///
/// Both together, because rounding decides the second: see the module
/// documentation for the carry that makes taking the exponent first wrong.
///
/// Built over the exact decimal expansion rather than `magnitude * 10^scale`
/// then `f64::round` — the same lossy-multiplication trap `super::fixed`
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
}
