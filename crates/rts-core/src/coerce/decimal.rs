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

    // The shortest round-tripping digits, and the exponent they sit at.
    //
    // Rust's `{:e}` produces exactly the shortest form; this only takes it
    // apart. Reimplementing the digit generation would be reimplementing Ryū,
    // and getting it subtly wrong is how a program prints a number that reads
    // back as a different one. Formatted INTO a stack buffer: it was a `String`
    // from `{:e}`, a second from `replace`, a third from `format!` and the
    // `Str` itself — four heap allocations for one number.
    let mut scientific = Decimal::new();
    let _ = std::fmt::Write::write_fmt(&mut scientific, format_args!("{value:e}"));
    let written = scientific.bytes();
    let split = written.iter().position(|byte| *byte == b'e').unwrap_or(written.len());
    let mut held = [0u8; LONGEST];
    let mut count = 0usize;
    for byte in &written[..split] {
        if *byte != b'.' {
            held[count] = *byte;
            count += 1;
        }
    }
    let digits = &held[..count];
    let exponent: i32 = std::str::from_utf8(written.get(split + 1..).unwrap_or(b"0"))
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
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
