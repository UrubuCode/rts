//! `fmt` namespace — parse and format primitives (string <-> number).
//!
//! `parse_*` carry an `on_null` sentinel matching their error convention
//! (`i64::MIN` / `NaN` / `-1`); `fmt_*` return GC string handles (`ts string`).
//!
//! `__RTS_FN_NS_FMT_PARSE_INT_RADIX` is NOT a namespace member — it backs the
//! `parseInt(s, radix)` builtin in codegen — so it stays a plain extern below.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use rts_abi::ty::{F64, Handle, I32, I64};
use rts_macro::rts_namespace;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Parse and format primitives (string <-> number).
#[rts_namespace(fmt)]
impl FmtNs {
    /// Parses an integer. Returns i64::MIN on error.
    #[rts_fn(pure, on_null = i64::MIN)]
    pub fn parse_i64(s: Str) -> I64 {
        s.trim().parse::<i64>().unwrap_or(i64::MIN)
    }

    /// Parses a float. Returns NaN on error.
    #[rts_fn(pure, on_null = f64::NAN)]
    pub fn parse_f64(s: Str) -> F64 {
        let trimmed = s.trim_start();
        // Parse direto primeiro (numero puro, caso mais comum/rapido).
        if let Ok(v) = trimmed.trim_end().parse::<f64>() {
            return v;
        }
        // (cross-runtime) JS parseFloat: parseia o MAIOR prefixo numerico
        // valido e ignora o resto (`"3.14abc"` -> 3.14, `"42px"` -> 42).
        let bytes = trimmed.as_bytes();
        let mut i = 0usize;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let rest = &trimmed[i..];
        if rest.starts_with("Infinity") {
            let v = f64::INFINITY;
            return if trimmed.as_bytes().first() == Some(&b'-') {
                -v
            } else {
                v
            };
        }
        let mut seen_digit = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                seen_digit = true;
            }
        }
        if !seen_digit {
            return f64::NAN;
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            let mut exp_digit = false;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
                exp_digit = true;
            }
            if exp_digit {
                i = j;
            }
        }
        trimmed[..i].parse::<f64>().unwrap_or(f64::NAN)
    }

    /// Parses 'true'/'false'/'1'/'0' (case-insensitive). Returns -1 on error.
    #[rts_fn(pure, on_null = -1)]
    pub fn parse_bool(s: Str) -> I64 {
        match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => 1,
            "false" | "0" => 0,
            _ => -1,
        }
    }

    /// Decimal string of an integer.
    #[rts_fn(pure, ts = "fmt_i64(value: number): string")]
    pub fn fmt_i64(value: I64) -> Handle {
        intern(&value.to_string())
    }

    /// Shortest round-trippable decimal of a float.
    #[rts_fn(pure, ts = "fmt_f64(value: number): string")]
    pub fn fmt_f64(value: F64) -> Handle {
        intern(&value.to_string())
    }

    /// 'true' when value is non-zero, 'false' otherwise.
    #[rts_fn(pure, ts = "fmt_bool(value: number): string")]
    pub fn fmt_bool(value: I64) -> Handle {
        intern(if value != 0 { "true" } else { "false" })
    }

    /// Lowercase hex with `0x` prefix (bits as u64).
    #[rts_fn(pure, ts = "fmt_hex(value: number): string")]
    pub fn fmt_hex(value: I64) -> Handle {
        intern(&format!("0x{:x}", value as u64))
    }

    /// Binary with `0b` prefix.
    #[rts_fn(pure, ts = "fmt_bin(value: number): string")]
    pub fn fmt_bin(value: I64) -> Handle {
        intern(&format!("0b{:b}", value as u64))
    }

    /// Octal with `0o` prefix.
    #[rts_fn(pure, ts = "fmt_oct(value: number): string")]
    pub fn fmt_oct(value: I64) -> Handle {
        intern(&format!("0o{:o}", value as u64))
    }

    /// Float formatted with a fixed number of decimal places.
    #[rts_fn(pure, ts = "fmt_f64_prec(value: number, precision: number): string")]
    pub fn fmt_f64_prec(value: F64, precision: I32) -> Handle {
        let prec = precision.max(0) as usize;
        intern(&format!("{value:.prec$}"))
    }
}

/// `parseInt(s, radix?)` — JS spec. NOT a namespace member; backs the codegen
/// `parseInt` builtin directly. Returns i64::MIN sentinel on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_INT_RADIX(ptr: *const u8, len: i64, radix: i64) -> i64 {
    let Some(s) = (unsafe { rts_abi::str_abi::from_abi(ptr, len) }) else {
        return i64::MIN;
    };
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return i64::MIN;
    }
    let negative = match bytes[i] {
        b'+' => {
            i += 1;
            false
        }
        b'-' => {
            i += 1;
            true
        }
        _ => false,
    };
    if i >= bytes.len() {
        return i64::MIN;
    }
    let mut effective_radix: u32 = if radix == 0 { 10 } else { radix as u32 };
    if (radix == 0 || radix == 16)
        && i + 1 < bytes.len()
        && bytes[i] == b'0'
        && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
    {
        effective_radix = 16;
        i += 2;
    }
    if !(2..=36).contains(&effective_radix) {
        return i64::MIN;
    }
    let mut acc: i64 = 0;
    let mut any_digit = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let Some(d) = c.to_digit(effective_radix) else {
            break;
        };
        let Some(next) = acc
            .checked_mul(effective_radix as i64)
            .and_then(|v| v.checked_add(d as i64))
        else {
            return i64::MIN;
        };
        acc = next;
        any_digit = true;
        i += 1;
    }
    if !any_digit {
        return i64::MIN;
    }
    if negative { -acc } else { acc }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str, radix: i64) -> i64 {
        __RTS_FN_NS_FMT_PARSE_INT_RADIX(s.as_ptr(), s.len() as i64, radix)
    }

    #[test]
    fn radix_10_default() {
        assert_eq!(p("100", 0), 100);
        assert_eq!(p("100", 10), 100);
        assert_eq!(p("-42", 0), -42);
        assert_eq!(p("+42", 0), 42);
    }

    #[test]
    fn radix_16_hex() {
        assert_eq!(p("FF", 16), 255);
        assert_eq!(p("ff", 16), 255);
        assert_eq!(p("0xFF", 16), 255);
        assert_eq!(p("0xff", 0), 255);
    }

    #[test]
    fn radix_2_binary() {
        assert_eq!(p("101", 2), 5);
        assert_eq!(p("1111", 2), 15);
    }

    #[test]
    fn tolerant_trailing_chars() {
        assert_eq!(p("42abc", 10), 42);
        assert_eq!(p("100xyz", 0), 100);
    }

    #[test]
    fn whitespace_stripped() {
        assert_eq!(p("  42", 0), 42);
        assert_eq!(p("\t-7", 10), -7);
    }

    #[test]
    fn empty_or_invalid_returns_sentinel() {
        assert_eq!(p("", 0), i64::MIN);
        assert_eq!(p("xyz", 10), i64::MIN);
        assert_eq!(p("   ", 10), i64::MIN);
    }
}
