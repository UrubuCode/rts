//! `toFixed` over the double's own bits, exactly, with no big arithmetic.
//!
//! `format::fixed` asks the standard formatter for the exact decimal expansion
//! sixty digits past the places wanted and rounds the STRING — correct, and
//! about 900 ns for `(1.23).toFixed(2)` (`bench/analytic.ts`, release,
//! 2026-09-27), because the formatter reaches for big-integer arithmetic to
//! spell sixty-two digits that are then thrown away.
//!
//! A double is `M × 2^E` with `M < 2^53`, and the specification's `toFixed`
//! is the integer `n` closest to `x × 10^f`, the larger of two when both are
//! equally close. For `f ≤ 22`, `10^f < 2^74`, so `M × 10^f` is below `2^127`
//! and fits a `u128` exactly: the answer is that product shifted right by
//! `-E`, rounded up when the dropped bits are at least a half. Nothing is
//! approximated — the multiplication is exact in integers, the shift is
//! exact, and the tie test reads the dropped bits themselves — so this
//! answers what the sixty-digit expansion answers, and the unit test says so
//! over a sweep of both.
//!
//! An `x` that is already an integer (`E ≥ 0`) is spelled as one with `f`
//! zeros appended, which is what its exact expansion is. `f > 22` keeps the
//! expansion, which is where the product would overflow; nobody writes it.

/// `magnitude.toFixed(places)` for a finite `magnitude ≥ 0` below `1e21`, or
/// `None` where the exact integer form does not apply.
pub(super) fn fixed_exact(magnitude: f64, places: usize) -> Option<String> {
    if places > 22 || !magnitude.is_finite() || magnitude < 0.0 || magnitude >= 1e21 {
        return None;
    }
    let bits = magnitude.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, shift) = match exponent {
        0 => (fraction, -1074),
        _ => (fraction | (1u64 << 52), exponent - 1075),
    };
    let scaled: u128 = if mantissa == 0 {
        0
    } else if shift >= 0 {
        // An integer below 1e21: its digits, then `places` zeros.
        let whole = (mantissa as u128) << shift;
        whole * 10u128.pow(places as u32)
    } else {
        let product = (mantissa as u128) * 10u128.pow(places as u32);
        let dropped = (-shift) as u32;
        if dropped >= 128 {
            // Below a half: `product < 2^127`, and `2^127 / 2^128` is the bound.
            0
        } else {
            let quotient = product >> dropped;
            let remainder = product & ((1u128 << dropped) - 1);
            let half = 1u128 << (dropped - 1);
            quotient + u128::from(remainder >= half)
        }
    };
    let mut digits = scaled.to_string();
    if places == 0 {
        return Some(digits);
    }
    while digits.len() <= places {
        digits.insert(0, '0');
    }
    digits.insert(digits.len() - places, '.');
    Some(digits)
}

#[cfg(test)]
mod tests {
    use super::fixed_exact;
    use super::super::format::fixed_slow;

    /// The values a scale-then-round gets wrong, and the ties the specification
    /// decides upward where the formatter decides to even.
    #[test]
    fn agrees_with_the_expansion_on_the_known_hard_cases() {
        for (value, places) in [
            (2.55, 1),
            (1.005, 2),
            (0.5, 0),
            (1.5, 0),
            (2.5, 0),
            (1.25, 1),
            (1.35, 1),
            (8.345, 2),
            (1e20, 2),
            (999999999999999900000.0, 0),
            (5e-324, 22),
            (1e-7, 3),
            (0.1 + 0.2, 17),
            (123456789.987654321, 5),
            (0.0, 3),
        ] {
            assert_eq!(fixed_exact(value, places), Some(fixed_slow(value, places)), "{value} to {places}");
        }
    }

    /// A deterministic sweep over the bit patterns, every place count the fast
    /// form takes, against the expansion.
    #[test]
    fn agrees_with_the_expansion_over_a_sweep() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..40_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let value = f64::from_bits(state & 0x7fff_ffff_ffff_ffff);
            if !value.is_finite() || value >= 1e21 {
                continue;
            }
            let places = (state >> 60) as usize % 23;
            assert_eq!(fixed_exact(value, places), Some(fixed_slow(value, places)), "{value:e} to {places}");
        }
    }

    #[test]
    fn declines_what_it_cannot_do_exactly() {
        assert_eq!(fixed_exact(1.0, 23), None);
        assert_eq!(fixed_exact(1e21, 2), None);
        assert_eq!(fixed_exact(f64::NAN, 2), None);
        assert_eq!(fixed_exact(-1.0, 2), None);
    }
}
