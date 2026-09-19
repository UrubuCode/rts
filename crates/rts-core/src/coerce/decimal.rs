//! A number's decimal text, written where the caller wants it.
//!
//! # Why this is beside `number.rs` and not inside it
//!
//! Because most callers of a number's text do not want a STRING, they want its
//! characters somewhere else: `JSON.stringify` copies them into its output and
//! throws the string away. Building a `Str` first is a heap allocation per
//! number for text whose length is bounded by [`LONGEST`]. Measured 2026-09-19,
//! `target/release/rts.exe`: eight doubles cost 3 036 ns to serialise — 380 ns
//! each — against 95 ns for an integer, which already took a stack path.
//!
//! There is still ONE statement of `Number::toString`: `number_to_string` is
//! this, copied into a `Str`.

use super::number::{INFINITY, NAN};

/// The longest text [`decimal_of`] produces, with room to spare.
///
/// The worst case is the fixed form just above the exponential threshold: a
/// sign, `0.`, five zeros and seventeen digits — twenty-five bytes.
const LONGEST: usize = 40;

/// Decimal text on the stack. ASCII by construction.
pub struct Decimal {
    held: [u8; LONGEST],
    used: usize,
}

impl Decimal {
    fn new() -> Self {
        Decimal { held: [0; LONGEST], used: 0 }
    }

    /// The text written so far.
    pub fn bytes(&self) -> &[u8] {
        &self.held[..self.used]
    }

    /// Truncating rather than panicking: this runs under `extern "C"` frames,
    /// and [`LONGEST`] is past anything the rules below can produce.
    fn push(&mut self, bytes: &[u8]) {
        let taken = bytes.len().min(LONGEST - self.used);
        self.held[self.used..self.used + taken].copy_from_slice(&bytes[..taken]);
        self.used += taken;
    }

    fn zeros(&mut self, count: usize) {
        for _ in 0..count {
            self.push(b"0");
        }
    }
}

impl std::fmt::Write for Decimal {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.push(text.as_bytes());
        Ok(())
    }
}

/// `Number::toString`, radix 10. See `number_to_string` for the rules.
pub fn decimal_of(value: f64) -> Decimal {
    let mut out = Decimal::new();
    write(value, &mut out);
    out
}

fn write(value: f64, out: &mut Decimal) {
    if value.is_nan() {
        return out.push(NAN.as_bytes());
    }
    if value == 0.0 {
        // Both zeros print "0". The sign is a real distinction everywhere else
        // — `Object.is(-0, 0)` is false, `1 / -0` is `-Infinity` — and printing
        // is the one place it is dropped.
        return out.push(b"0");
    }
    if value < 0.0 {
        out.push(b"-");
        return write(-value, out);
    }
    if value.is_infinite() {
        return out.push(INFINITY.as_bytes());
    }

    // An INTEGER, by division.
    //
    // Bounded by 2^53 and not by 1e21, and the difference was a wrong answer:
    // above 2^53 a double no longer holds every integer, and `as u64` on
    // 1.2345678901234568e20 saturates — the first version printed
    // 18446744073709551615 for it. Below 2^53 the conversion is exact by
    // construction, and everything above falls to the general path, which was
    // already correct there.
    if value.fract() == 0.0 && value < 9_007_199_254_740_992.0 {
        let mut digits = [0u8; 21];
        let mut at = digits.len();
        let mut left = value as u64;
        while left > 0 {
            at -= 1;
            digits[at] = b'0' + (left % 10) as u8;
            left /= 10;
        }
        return out.push(&digits[at..]);
    }

    let mut held = [0u8; LONGEST];
    let (count, exponent) = shortest(value, &mut held);
    let digits = &held[..count];
    // The specification's `n`: the position of the decimal point relative to
    // the digit string. `k` is how many digits there are.
    let n = exponent + 1;
    let k = count as i32;

    if k <= n && n <= 21 {
        // Digits, then the zeros needed to reach the point.
        out.push(digits);
        out.zeros((n - k) as usize);
    } else if 0 < n && n <= 21 {
        // A point inside the digits.
        out.push(&digits[..n as usize]);
        out.push(b".");
        out.push(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        // Leading zeros, then every digit after the point.
        out.push(b"0.");
        out.zeros((-n) as usize);
        out.push(digits);
    } else {
        // Exponential, and the sign of the exponent is always written.
        out.push(&digits[..1]);
        if k > 1 {
            out.push(b".");
            out.push(&digits[1..]);
        }
        out.push(if n >= 1 { b"e+" } else { b"e-" });
        let _ = std::fmt::Write::write_fmt(out, format_args!("{}", (n - 1).abs()));
    }
}

/// The shortest digits that read back as `value`, and the power of ten the
/// FIRST of them sits at. Finite, positive and non-zero by the time it is asked.
///
/// # Why `ryu` and not `{:e}`
///
/// Both answer the fewest digits that round-trip, and `the_digits_round_trip_and_
/// are_as_few_as_core_fmt_gives` below holds `ryu` to that over two hundred
/// thousand doubles.
///
/// They DIFFER on a tie, and the test written to prove them equal is what said
/// so: 1658206780088562.25 is a double, its two seventeen-digit candidates are
/// equally far from it, and the language says to take the EVEN one — `…562.2`,
/// which is what node and bun print and what `ryu` answers. `{:e}` answers
/// `…562.3`. So the path this replaced printed a digit the language does not,
/// rarely, and nothing had asked.
///
/// The other reason is the clock. `{:e}` reaches its digits through
/// `core::fmt`, which is a formatter, a `Write` and a parse of the exponent
/// back out of text: measured 2026-09-19, about 100 ns of the 134 a double cost
/// to serialise.
///
/// Its TEXT is not used as text. `ryu` prints `1e21` where the language says
/// `1e+21` and `1.0` where it says `1`, so only the digits and the exponent are
/// taken out of it and the rules above place the point.
fn shortest(value: f64, held: &mut [u8; LONGEST]) -> (usize, i32) {
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value).as_bytes();
    let split = text.iter().position(|byte| *byte == b'e').unwrap_or(text.len());
    let written: i32 = std::str::from_utf8(&text[split.min(text.len() - 1) + 1..])
        .ok()
        .filter(|_| split < text.len())
        .and_then(|exponent| exponent.parse().ok())
        .unwrap_or(0);
    let mantissa = &text[..split];
    let point = mantissa.iter().position(|byte| *byte == b'.').unwrap_or(mantissa.len()) as i32;
    // Every digit, without the point; then the zeros on either side go, and the
    // leading ones move the exponent as they do.
    let mut count = 0usize;
    let mut leading = 0i32;
    for byte in mantissa.iter().copied().filter(|byte| *byte != b'.') {
        if count == 0 && byte == b'0' {
            leading += 1;
            continue;
        }
        held[count] = byte;
        count += 1;
    }
    while count > 1 && held[count - 1] == b'0' {
        count -= 1;
    }
    (count, point - 1 - leading + written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `shortest` replaced, kept as the ruler for HOW MANY digits.
    fn by_core_fmt(value: f64) -> (String, i32) {
        let scientific = format!("{value:e}");
        let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
        (mantissa.replace('.', ""), exponent.parse().expect("an integer"))
    }

    #[test]
    fn the_digits_round_trip_and_are_as_few_as_core_fmt_gives() {
        // A fixed generator, so a failure names a double anyone can retry.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut checked = 0;
        let edges = [5e-324, 1.7976931348623157e308, 0.1, 0.3, 1e21, 1e-7, 123.456, 2.5e-5, 1e22, 4.35];
        let mut ask = |value: f64| {
            let mut held = [0u8; LONGEST];
            let (count, exponent) = shortest(value, &mut held);
            let digits = std::str::from_utf8(&held[..count]).expect("ascii");
            let (expected, at) = by_core_fmt(value);
            assert_eq!((count, exponent), (expected.len(), at), "as few digits, at the same power, for {value:e}");
            let read: f64 = format!("{}e{}", digits, exponent - (count as i32 - 1)).parse().expect("a number");
            assert_eq!(read.to_bits(), value.to_bits(), "{digits} reads back as {value:e}");
        };
        for value in edges {
            ask(value);
            checked += 1;
        }
        while checked < 200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let value = f64::from_bits(state).abs();
            if value.is_finite() && value != 0.0 {
                ask(value);
                checked += 1;
            }
        }
    }

    #[test]
    fn a_tie_between_two_shortest_candidates_takes_the_even_digit() {
        // Exactly 1658206780088562.25. Node and bun print `.2`; `{:e}` says `.3`.
        let text = String::from_utf8(decimal_of(1.6582067800885623e15).bytes().to_vec()).expect("ascii");
        assert_eq!(text, "1658206780088562.2");
    }

    #[test]
    fn the_point_lands_where_the_language_puts_it() {
        let text = |value: f64| String::from_utf8(decimal_of(value).bytes().to_vec()).expect("ascii");
        assert_eq!(text(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(text(1e21), "1e+21");
        assert_eq!(text(1e20), "100000000000000000000");
        assert_eq!(text(1.2345678901234568e20), "123456789012345680000");
        assert_eq!(text(1e-7), "1e-7");
        assert_eq!(text(1.5e-10), "1.5e-10");
        assert_eq!(text(0.000001234), "0.000001234");
        assert_eq!(text(-2.25), "-2.25");
        assert_eq!(text(5e-324), "5e-324");
        assert_eq!(text(1.7976931348623157e308), "1.7976931348623157e+308");
    }
}
