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
fn carried(magnitude: f64, places: usize) -> (String, i32) {
    let mut exponent = magnitude.abs().log10().floor() as i32;
    let scale = 10f64.powi(places as i32 - exponent);
    // Half away from zero, which is `f64::round` — the rule the specification
    // states and the one Rust's formatter does not follow.
    let mut significant = (magnitude * scale).round();
    let ceiling = 10f64.powi(places as i32 + 1);
    if significant >= ceiling {
        significant /= 10.0;
        exponent += 1;
    }
    (with_places(significant / 10f64.powi(places as i32), places), exponent)
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
