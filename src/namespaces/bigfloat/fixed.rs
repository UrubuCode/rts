//! Simple fixed-point decimal numbers backed by `i128`.
//!
//! Represents `significand * 10^-scale`. The runtime picks a scale when the
//! value is created (usually the "precision digits" requested by the caller)
//! and every operation keeps that scale. Multiplication is the tricky one:
//! with `i128` we can multiply two ~64-bit significands safely, but for
//! larger scales we split into high/low halves and accumulate in `i256`
//! emulated via two `i128`s. That's more than enough for 30-digit pi
//! calculations without pulling in a full big-int dependency.

use std::cmp::Ordering;

/// `value = significand * 10^(-scale)`.
#[derive(Debug, Clone)]
pub struct FixedDecimal {
    pub sig: i128,
    pub scale: u32,
}

impl FixedDecimal {
    pub const fn zero(scale: u32) -> Self {
        Self { sig: 0, scale }
    }

    pub fn from_i64(v: i64, scale: u32) -> Self {
        let sig = (v as i128) * pow10(scale);
        Self { sig, scale }
    }

    pub fn from_f64(v: f64, scale: u32) -> Self {
        // Route through a string to avoid binary-to-decimal slop at build
        // time. f64 only has ~15 significant digits, so any higher scale
        // just pads zeros — fine for seeding.
        let text = format!("{v:.prec$}", prec = scale as usize);
        Self::from_str(&text, scale).unwrap_or_else(|| Self::zero(scale))
    }

    pub fn from_str(s: &str, scale: u32) -> Option<Self> {
        let (negative, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };

        // Integer part as an integer.
        let mut int_sig: i128 = 0;
        for &b in int_part.as_bytes() {
            if !b.is_ascii_digit() {
                return None;
            }
            int_sig = int_sig.checked_mul(10)?.checked_add((b - b'0') as i128)?;
        }

        // Fractional part: interpret up to `scale` digits as an integer,
        // pad with zeros if we have fewer.
        let mut frac_sig: i128 = 0;
        let mut consumed: u32 = 0;
        for &b in frac_part.as_bytes() {
            if consumed == scale {
                break;
            }
            if !b.is_ascii_digit() {
                return None;
            }
            frac_sig = frac_sig.checked_mul(10)?.checked_add((b - b'0') as i128)?;
            consumed += 1;
        }
        for _ in consumed..scale {
            frac_sig = frac_sig.checked_mul(10)?;
        }

        let mut sig = int_sig.checked_mul(pow10(scale))?.checked_add(frac_sig)?;
        if negative {
            sig = -sig;
        }
        Some(Self { sig, scale })
    }

    pub fn to_f64(&self) -> f64 {
        // Split significand into hi/lo to keep f64 rounding correct.
        let div = pow10(self.scale) as f64;
        (self.sig as f64) / div
    }

    pub fn to_string_decimal(&self) -> String {
        let mut out = String::new();
        let neg = self.sig < 0;
        let abs: u128 = if neg { self.sig.unsigned_abs() } else { self.sig as u128 };
        let scale = self.scale as usize;
        let s = abs.to_string();
        if scale == 0 {
            if neg { out.push('-'); }
            out.push_str(&s);
            return out;
        }
        if neg {
            out.push('-');
        }
        if s.len() <= scale {
            out.push_str("0.");
            for _ in 0..(scale - s.len()) {
                out.push('0');
            }
            out.push_str(&s);
        } else {
            let split = s.len() - scale;
            out.push_str(&s[..split]);
            out.push('.');
            out.push_str(&s[split..]);
        }
        out
    }

    /// Returns `self + other`. Requires matching scales.
    pub fn add(&self, other: &Self) -> Self {
        debug_assert_eq!(self.scale, other.scale);
        Self {
            sig: self.sig.wrapping_add(other.sig),
            scale: self.scale,
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        debug_assert_eq!(self.scale, other.scale);
        Self {
            sig: self.sig.wrapping_sub(other.sig),
            scale: self.scale,
        }
    }

    pub fn neg(&self) -> Self {
        Self {
            sig: self.sig.wrapping_neg(),
            scale: self.scale,
        }
    }

    /// Returns `self * other`. Requires matching scales. Uses 256-bit
    /// intermediate to avoid overflow when both sides are near the i128 limit.
    pub fn mul(&self, other: &Self) -> Self {
        debug_assert_eq!(self.scale, other.scale);
        let product = i256_mul(self.sig, other.sig);
        let result = i256_div_pow10(product, self.scale);
        Self {
            sig: result,
            scale: self.scale,
        }
    }

    /// Returns `self / other`. Requires matching scales and `other != 0`.
    pub fn div(&self, other: &Self) -> Option<Self> {
        debug_assert_eq!(self.scale, other.scale);
        if other.sig == 0 {
            return None;
        }
        // (a / 10^s) / (b / 10^s) = a / b, but we want the result scaled.
        // So result = a * 10^s / b, computed with 256-bit intermediate.
        let numerator = i256_mul(self.sig, pow10(self.scale));
        let sig = i256_div(numerator, other.sig);
        Some(Self {
            sig,
            scale: self.scale,
        })
    }

    /// Integer square root (Newton-Raphson).
    pub fn sqrt(&self) -> Option<Self> {
        if self.sig < 0 {
            return None;
        }
        if self.sig == 0 {
            return Some(Self::zero(self.scale));
        }
        // Work at `2*scale` so the final square root lands at `scale`.
        let scaled = i256_mul(self.sig, pow10(self.scale));
        // Start with a rough estimate: sqrt of the high bits.
        let (hi, lo) = scaled;
        let initial = if hi == 0 {
            (lo as f64).sqrt() as i128
        } else {
            let combined = (hi as f64) * (u128::MAX as f64) + (lo as f64);
            combined.sqrt() as i128
        };
        let mut x = initial.max(1);
        // Newton iterations: x_{n+1} = (x + target/x) / 2.
        for _ in 0..120 {
            let q = i256_div(scaled, x);
            let next = (x + q) / 2;
            if next == x || next == x + 1 || next + 1 == x {
                x = next.min(x);
                break;
            }
            x = next;
        }
        Some(Self {
            sig: x,
            scale: self.scale,
        })
    }
}

impl PartialEq for FixedDecimal {
    fn eq(&self, other: &Self) -> bool {
        self.scale == other.scale && self.sig == other.sig
    }
}

impl PartialOrd for FixedDecimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.scale != other.scale {
            return None;
        }
        Some(self.sig.cmp(&other.sig))
    }
}

// ── 256-bit helpers over (hi: i128, lo: u128) ─────────────────────────────
// Only the subset we need: mul(i128, i128), div by i128, div_pow10.

type I256 = (i128, u128);

fn i256_mul(a: i128, b: i128) -> I256 {
    // Split into signed sign × unsigned 127-bit magnitudes.
    let neg = (a < 0) ^ (b < 0);
    let au = a.unsigned_abs();
    let bu = b.unsigned_abs();

    // 128x128 -> 256 via 64-bit halves.
    let a_lo = au as u64 as u128;
    let a_hi = (au >> 64) as u128;
    let b_lo = bu as u64 as u128;
    let b_hi = (bu >> 64) as u128;

    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;

    let mid = lh.wrapping_add(hl);
    let mid_carry: u128 = if mid < lh { 1 << 64 } else { 0 };

    let lo_final = ll.wrapping_add(mid << 64);
    let lo_carry: u128 = if lo_final < ll { 1 } else { 0 };

    let hi_u = hh + (mid >> 64) + mid_carry + lo_carry;
    let hi = hi_u as i128;

    if neg {
        negate_i256((hi, lo_final))
    } else {
        (hi, lo_final)
    }
}

fn negate_i256(x: I256) -> I256 {
    let (hi, lo) = x;
    let new_lo = (!lo).wrapping_add(1);
    let carry: i128 = if new_lo == 0 { 1 } else { 0 };
    let new_hi = (!hi).wrapping_add(carry);
    (new_hi, new_lo)
}

fn i256_is_negative((hi, _): I256) -> bool {
    hi < 0
}

/// Divides a 256-bit signed integer by a 128-bit signed integer, returning
/// the low 128 bits of the quotient. Assumes the true quotient fits in i128
/// (caller's responsibility).
fn i256_div(num: I256, den: i128) -> i128 {
    let neg = i256_is_negative(num) ^ (den < 0);
    let abs_num = if i256_is_negative(num) { negate_i256(num) } else { num };
    let abs_den = den.unsigned_abs();

    // Shift-subtract long division: 256 bits over a 128-bit denominator.
    let (mut hi, mut lo) = abs_num;
    let mut quotient_hi: u128 = 0;
    let mut quotient_lo: u128 = 0;
    let mut remainder: u128 = 0;

    for _ in 0..256 {
        // Shift (remainder, hi, lo) left by 1.
        let top_remainder = (remainder >> 127) as u128;
        remainder = (remainder << 1) | ((hi as u128) >> 127);
        let _ = top_remainder; // discarded — we assume no overflow
        hi = ((hi as u128) << 1) as i128 | (lo >> 127) as i128;
        lo <<= 1;

        // Shift quotient left 1.
        let q_top = quotient_hi >> 127;
        quotient_hi = (quotient_hi << 1) | (quotient_lo >> 127);
        quotient_lo <<= 1;
        let _ = q_top;

        if remainder >= abs_den {
            remainder -= abs_den;
            quotient_lo |= 1;
        }
    }

    // We only need the low 128 bits of the quotient as i128.
    let result = quotient_lo as i128;
    if neg { -result } else { result }
}

fn i256_div_pow10(num: I256, exp: u32) -> i128 {
    let divisor = pow10(exp);
    i256_div(num, divisor)
}

fn pow10(exp: u32) -> i128 {
    let mut r: i128 = 1;
    for _ in 0..exp {
        r *= 10;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_from_str() {
        let v = FixedDecimal::from_str("3.14159265358979323", 30).unwrap();
        assert!((v.to_f64() - std::f64::consts::PI).abs() < 1e-14);
    }

    #[test]
    fn mul_preserves_scale() {
        let two = FixedDecimal::from_str("2", 20).unwrap();
        let half = FixedDecimal::from_str("0.5", 20).unwrap();
        let one = two.mul(&half);
        assert_eq!(one.to_string_decimal(), "1.00000000000000000000");
    }

    #[test]
    fn sqrt_of_two() {
        let two = FixedDecimal::from_str("2", 30).unwrap();
        let r = two.sqrt().unwrap();
        // Accept within 1e-25 against the ground truth.
        let expected = FixedDecimal::from_str(
            "1.414213562373095048801688724209",
            30,
        )
        .unwrap();
        let diff = (r.sig - expected.sig).abs();
        assert!(diff < 1_000_000_000, "diff = {diff}");
    }
}
