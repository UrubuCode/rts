//! `Number.parseInt`/`Number.parseFloat` (and the identical global
//! `parseInt`/`parseFloat`) string→number parsers — the SINGLE correct JS
//! implementation, moved down from `rts-runtime` (owner directive: a
//! primordial's implementation belongs in `rts-primitives`, the bottom of the
//! chain any consumer can reach). `rts-runtime::adapters::value::globalops` and
//! `rts-shared::fmt` both call the `#[rtse::statical]` externs this module's
//! `mod.rs` generates (`__rtsm_global_number_parse_int`/`_parse_float`)
//! instead of carrying their own copy.

/// JS `parseInt(s, radix)` returning the parsed `f64` (`NaN` on failure).
/// `radix` `0` (or absent) = auto (16 with a `0x` prefix, else 10); `2..=36`
/// fixes the base. Leading whitespace + an optional sign are consumed; the
/// longest valid-digit run is parsed; trailing garbage is ignored.
pub(crate) fn parse_int_str(s: &str, radix: i64) -> f64 {
    let t = s.trim_start();
    let (neg, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut base = radix;
    let mut digits = rest;
    if base == 16 || base == 0 {
        if let Some(r) = digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
        {
            digits = r;
            base = 16;
        }
    }
    if base == 0 {
        base = 10;
    }
    if !(2..=36).contains(&base) {
        return f64::NAN;
    }
    let radix_u = base as u32;
    let valid: String = digits
        .chars()
        .take_while(|c| c.to_digit(radix_u).is_some())
        .collect();
    if valid.is_empty() {
        return f64::NAN;
    }
    // Accumulate in f64 to keep large magnitudes (JS parseInt returns a double).
    let mut acc = 0.0f64;
    for c in valid.chars() {
        acc = acc * base as f64 + c.to_digit(radix_u).unwrap() as f64;
    }
    if neg { -acc } else { acc }
}

/// JS `parseFloat(s)` returning the parsed `f64` (`NaN` on no leading float).
/// The longest leading run matching a JS float literal is parsed; trailing
/// garbage is ignored. `Infinity`/`+Infinity`/`-Infinity` are recognized.
pub(crate) fn parse_float_str(s: &str) -> f64 {
    let t = s.trim_start();
    for spell in ["Infinity", "+Infinity"] {
        if t.starts_with(spell) {
            return f64::INFINITY;
        }
    }
    if t.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = t.as_bytes();
    let mut end = 0usize;
    // optional sign
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut saw_digit = false;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        saw_digit = true;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            saw_digit = true;
        }
    }
    if !saw_digit {
        return f64::NAN;
    }
    // optional exponent
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut e = end + 1;
        if e < bytes.len() && (bytes[e] == b'+' || bytes[e] == b'-') {
            e += 1;
        }
        let mut saw_exp = false;
        while e < bytes.len() && bytes[e].is_ascii_digit() {
            e += 1;
            saw_exp = true;
        }
        if saw_exp {
            end = e;
        }
    }
    t[..end].parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_basics() {
        assert_eq!(parse_int_str("42abc", 0), 42.0);
        assert_eq!(parse_int_str("  7 ", 0), 7.0);
        assert_eq!(parse_int_str("0x1f", 0), 31.0);
        assert!(parse_int_str("abc", 0).is_nan());
        assert_eq!(parse_int_str("3.9", 0), 3.0);
        assert_eq!(parse_int_str("FF", 16), 255.0);
    }

    #[test]
    fn parse_float_basics() {
        assert_eq!(parse_float_str("3.5xyz"), 3.5);
        assert!(parse_float_str("abc").is_nan());
        assert_eq!(parse_float_str(".5"), 0.5);
    }
}
