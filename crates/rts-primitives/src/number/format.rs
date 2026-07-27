//! Number FORMATTING — irreducible, kept in Rust as the one source of truth for
//! `toString(radix)`/`toFixed`/`toPrecision`/`toExponential`. Split out of
//! `mod.rs` (the value-class ctor/methods) purely for the file-size ceiling;
//! these are plain private fns, no `#[rtse::*]` surface of their own — EXCEPT
//! [`__RTS_FN_GL_NUMBER_TO_STRING_RADIX`], a real extern kept for a second
//! caller: `rts-adapters`'s `__rtsadp_dyn_to_string_radix` (the DYNAMIC/
//! unproven-receiver `x.toString(radix)` path) calls it directly through the
//! `rts-std` `engine` bridge, so it must stay reachable outside the value-class
//! method dispatch this file otherwise only serves.

use rts_engine::heap::handles::{Entry, alloc_entry};

/// `n.toString(radix)` with an arbitrary radix (2..36; 10 is plain decimal).
pub(super) fn to_string_radix(v: f64, radix: i64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return (if v > 0.0 { "Infinity" } else { "-Infinity" }).to_string();
    }
    let r = radix.clamp(2, 36) as u32;
    if r == 10 {
        if v.fract() == 0.0 && v.is_finite() {
            return format!("{}", v as i64);
        }
        return format!("{v}");
    }
    if v.fract() == 0.0 && v.is_finite() {
        let n = v as i64;
        if n == 0 {
            return "0".to_string();
        }
        let negative = n < 0;
        let mut n = n.unsigned_abs();
        let mut digits = Vec::new();
        while n > 0 {
            let d = (n % r as u64) as u8;
            digits.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
            n /= r as u64;
        }
        if negative {
            digits.push(b'-');
        }
        digits.reverse();
        String::from_utf8(digits).unwrap_or_else(|_| "0".to_string())
    } else {
        match r {
            16 => format!("{:x}", v.to_bits()),
            _ => format!("{v}"),
        }
    }
}

/// `n.toString(radix)` as a real extern — the DYNAMIC/unproven-receiver path
/// (`rts-adapters::value::dyndispatch::__rtsadp_dyn_to_string_radix`, reached via
/// the `rts-std` `engine` bridge re-export) calls this directly. LOAD-BEARING.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64 {
    alloc_entry(Entry::String(to_string_radix(v, radix).into_bytes()))
}

/// `n.toFixed(digits)` — fixed-point string.
pub(super) fn to_fixed_str(v: f64, digits: i64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return (if v > 0.0 { "Infinity" } else { "-Infinity" }).to_string();
    }
    let d = digits.clamp(0, 100) as usize;
    let neg = v.is_sign_negative() && v != 0.0;
    let abs = v.abs();
    if abs < 1e21 {
        let high = format!("{abs:.25}");
        let out = round_decimal_str(&high, d);
        return if neg { format!("-{out}") } else { out };
    }
    // |v| ≥ 1e21: the spec falls back to plain `ToString(v)` (exponential form —
    // `(1e21).toFixed(2)` is `"1e+21"`, never a fixed-point expansion).
    let e = format!("{v:e}");
    match e.split_once('e') {
        Some((m, exp)) if !exp.starts_with('-') => format!("{m}e+{exp}"),
        _ => e,
    }
}

/// `n.toPrecision(digits)` — significant digits string.
pub(super) fn to_precision_str(v: f64, digits: i64) -> String {
    if digits <= 0 || !v.is_finite() {
        return format!("{v}");
    }
    let sig = digits.clamp(1, 100) as usize;
    let mag = if v == 0.0 { 0i32 } else { v.abs().log10().floor() as i32 };
    if mag >= -6 && mag < sig as i32 {
        let frac = (sig as i32 - 1 - mag).max(0) as usize;
        let neg = v.is_sign_negative() && v != 0.0;
        let high = format!("{:.25}", v.abs());
        let mut body = round_decimal_str(&high, frac);
        let int_part = body.split('.').next().unwrap_or("");
        let int_digits = int_part.trim_start_matches('0').len();
        if int_digits > sig {
            let s = round_exp_str(v, sig - 1);
            return js_exp_notation(&s);
        }
        if int_digits >= sig && body.contains('.') {
            body = int_part.to_string();
        }
        if neg { format!("-{body}") } else { body }
    } else {
        let s = round_exp_str(v, sig - 1);
        js_exp_notation(&s)
    }
}

/// `n.toExponential(digits)` — exponential notation string.
pub(super) fn to_exponential_str(v: f64, digits: i64) -> String {
    if digits < 0 {
        let formatted = format!("{v:e}");
        let with_js_notation = js_exp_notation(&formatted);
        remove_trailing_zeros_exp(&with_js_notation)
    } else {
        let d = digits.clamp(0, 100) as usize;
        let formatted = round_exp_str(v, d);
        js_exp_notation(&formatted)
    }
}

/// Rounds the decimal string `s` ("int.frac", no sign) to `d` places,
/// half-away-from-zero. `s` has enough precision (>= d+1 places).
fn round_decimal_str(s: &str, d: usize) -> String {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (s.to_string(), String::new()),
    };
    let frac_bytes: Vec<u8> = frac_part.bytes().collect();
    let round_up = frac_bytes.get(d).map(|&b| b >= b'5').unwrap_or(false);
    let mut digits: Vec<u8> = Vec::new();
    for b in int_part.bytes() {
        digits.push(b);
    }
    for i in 0..d {
        digits.push(*frac_bytes.get(i).unwrap_or(&b'0'));
    }
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let new_int_len = digits.len() - d;
    let int_s: String = String::from_utf8_lossy(&digits[..new_int_len]).into_owned();
    let int_s = if int_s.is_empty() { "0".to_string() } else { int_s };
    if d == 0 {
        int_s
    } else {
        let frac_s: String = String::from_utf8_lossy(&digits[new_int_len..]).into_owned();
        format!("{int_s}.{frac_s}")
    }
}

/// Formats `v` in scientific notation with `prec` mantissa places,
/// half-away-from-zero over the real value. Shape of `format!("{:e}")`.
fn round_exp_str(v: f64, prec: usize) -> String {
    if v == 0.0 {
        let mant = if prec == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(prec)) };
        return format!("{mant}e0");
    }
    let neg = v < 0.0;
    let abs = v.abs();
    let high = format!("{abs:.30e}");
    let (mant_part, exp_part) = high.split_once('e').unwrap_or((high.as_str(), "0"));
    let mut exp: i32 = exp_part.parse().unwrap_or(0);
    let mant_digits: Vec<u8> = mant_part.bytes().filter(|b| b.is_ascii_digit()).collect();
    let keep = prec + 1;
    let mut digits: Vec<u8> = mant_digits.iter().take(keep).copied().collect();
    while digits.len() < keep {
        digits.push(b'0');
    }
    let round_up = mant_digits.get(keep).map(|&b| b >= b'5').unwrap_or(false);
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                exp += 1;
                digits.pop();
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let first = digits[0] as char;
    let rest: String = digits[1..].iter().map(|&b| b as char).collect();
    let mantissa = if prec == 0 { first.to_string() } else { format!("{first}.{rest}") };
    let sign = if neg { "-" } else { "" };
    format!("{sign}{mantissa}e{exp}")
}

/// Removes trailing zeros from the mantissa in exponential notation.
fn remove_trailing_zeros_exp(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let trimmed = mantissa.trim_end_matches('0');
        let trimmed = if trimmed.ends_with('.') { &trimmed[..trimmed.len() - 1] } else { trimmed };
        format!("{trimmed}{exp_part}")
    } else {
        s.to_string()
    }
}

/// Converts `1.5e10` / `1.5e-3` (Rust) to `1.5e+10` / `1.5e-3` (JS).
fn js_exp_notation(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let exp_str = &exp_part[1..];
        if exp_str.starts_with('-') { format!("{mantissa}e{exp_str}") } else { format!("{mantissa}e+{exp_str}") }
    } else {
        s.to_string()
    }
}
