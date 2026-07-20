//! `fmt` namespace — parse and format primitives (string <-> number).
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).
//! `parse_*` carregam um sentinela de erro (i64::MIN / NaN / -1) também emitido
//! em null/UTF-8 inválido; `fmt_*` retornam handles de string GC.
//!
//! `__RTS_FN_NS_FMT_PARSE_INT_RADIX` NÃO é membro — backa o `parseInt(s, radix)`
//! do codegen — então fica como extern simples.

use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, sig};

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Parses an integer. Returns i64::MIN on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_I64(ptr: *const u8, len: i64) -> i64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return i64::MIN;
    };
    s.trim().parse::<i64>().unwrap_or(i64::MIN)
}

/// Parses a float. Returns NaN on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_F64(ptr: *const u8, len: i64) -> f64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return f64::NAN;
    };
    let trimmed = s.trim_start();
    if let Ok(v) = trimmed.trim_end().parse::<f64>() {
        return v;
    }
    // JS parseFloat: maior prefixo numérico válido, ignora o resto.
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_BOOL(ptr: *const u8, len: i64) -> i64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return -1;
    };
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => 1,
        "false" | "0" => 0,
        _ => -1,
    }
}

/// Decimal string of an integer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_I64(value: i64) -> u64 {
    intern(&value.to_string())
}

/// Shortest round-trippable decimal of a float.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_F64(value: f64) -> u64 {
    intern(&value.to_string())
}

/// 'true' when value is non-zero, 'false' otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_BOOL(value: i64) -> u64 {
    intern(if value != 0 { "true" } else { "false" })
}

/// Lowercase hex with `0x` prefix (bits as u64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_HEX(value: i64) -> u64 {
    intern(&format!("0x{:x}", value as u64))
}

/// Binary with `0b` prefix.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_BIN(value: i64) -> u64 {
    intern(&format!("0b{:b}", value as u64))
}

/// Octal with `0o` prefix.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_OCT(value: i64) -> u64 {
    intern(&format!("0o{:o}", value as u64))
}

/// Float formatted with a fixed number of decimal places.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_F64_PREC(value: f64, precision: i32) -> u64 {
    let prec = precision.max(0) as usize;
    intern(&format!("{value:.prec$}"))
}

/// `parseInt(s, radix?)` — JS spec. NOT a namespace member; backs the codegen
/// `parseInt` builtin directly. Returns i64::MIN sentinel on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_INT_RADIX(ptr: *const u8, len: i64, radix: i64) -> i64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
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

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
        emit: None,
    }
}

/// Registra a namespace `fmt` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("fmt")
        .doc("Parse and format primitives (string <-> number).")
        .member(pure_func(
            "parse_i64",
            "__RTS_FN_NS_FMT_PARSE_I64",
            sig!(StrPtr => I64),
            "parse_i64(s: string): number",
            "Parses an integer. Returns i64::MIN on error.",
            __RTS_FN_NS_FMT_PARSE_I64 as *const u8,
        ))
        .member(pure_func(
            "parse_f64",
            "__RTS_FN_NS_FMT_PARSE_F64",
            sig!(StrPtr => F64),
            "parse_f64(s: string): number",
            "Parses a float. Returns NaN on error.",
            __RTS_FN_NS_FMT_PARSE_F64 as *const u8,
        ))
        .member(pure_func(
            "parse_bool",
            "__RTS_FN_NS_FMT_PARSE_BOOL",
            sig!(StrPtr => I64),
            "parse_bool(s: string): number",
            "Parses 'true'/'false'/'1'/'0' (case-insensitive). Returns -1 on error.",
            __RTS_FN_NS_FMT_PARSE_BOOL as *const u8,
        ))
        .member(pure_func(
            "fmt_i64",
            "__RTS_FN_NS_FMT_FMT_I64",
            sig!(I64 => Handle),
            "fmt_i64(value: number): string",
            "Decimal string of an integer.",
            __RTS_FN_NS_FMT_FMT_I64 as *const u8,
        ))
        .member(pure_func(
            "fmt_f64",
            "__RTS_FN_NS_FMT_FMT_F64",
            sig!(F64 => Handle),
            "fmt_f64(value: number): string",
            "Shortest round-trippable decimal of a float.",
            __RTS_FN_NS_FMT_FMT_F64 as *const u8,
        ))
        .member(pure_func(
            "fmt_bool",
            "__RTS_FN_NS_FMT_FMT_BOOL",
            sig!(I64 => Handle),
            "fmt_bool(value: number): string",
            "'true' when value is non-zero, 'false' otherwise.",
            __RTS_FN_NS_FMT_FMT_BOOL as *const u8,
        ))
        .member(pure_func(
            "fmt_hex",
            "__RTS_FN_NS_FMT_FMT_HEX",
            sig!(I64 => Handle),
            "fmt_hex(value: number): string",
            "Lowercase hex with `0x` prefix (bits as u64).",
            __RTS_FN_NS_FMT_FMT_HEX as *const u8,
        ))
        .member(pure_func(
            "fmt_bin",
            "__RTS_FN_NS_FMT_FMT_BIN",
            sig!(I64 => Handle),
            "fmt_bin(value: number): string",
            "Binary with `0b` prefix.",
            __RTS_FN_NS_FMT_FMT_BIN as *const u8,
        ))
        .member(pure_func(
            "fmt_oct",
            "__RTS_FN_NS_FMT_FMT_OCT",
            sig!(I64 => Handle),
            "fmt_oct(value: number): string",
            "Octal with `0o` prefix.",
            __RTS_FN_NS_FMT_FMT_OCT as *const u8,
        ))
        .member(pure_func(
            "fmt_f64_prec",
            "__RTS_FN_NS_FMT_FMT_F64_PREC",
            sig!(F64, I32 => Handle),
            "fmt_f64_prec(value: number, precision: number): string",
            "Float formatted with a fixed number of decimal places.",
            __RTS_FN_NS_FMT_FMT_F64_PREC as *const u8,
        ))
        .done();
}

/// node:util — a superfície node que o RTS mora em `fmt`. Os membros REUSAM os
/// externs nativos `__RTS_FN_NS_FMT_*` (mesmos símbolos, já harvestados/JIT) e só
/// dão os nomes node:util. `import { formatHex, parseInt, ... } from "node:util"`
/// resolve por DADO (`Member::matches_name`), sem o motor nomear `fmt` — espelha
/// o mapa de rts-node/src/util (#288 fase 1).
pub fn register_node_util(e: &mut Engine) {
    e.ns("util")
        .doc("node:util compat — formatHex/Bin/Oct + parseInt (alias de rts:fmt).")
        .member(pure_func(
            "formatHex",
            "__RTS_FN_NS_FMT_FMT_HEX",
            sig!(I64 => Handle),
            "formatHex(value: number): string",
            "Lowercase hex with `0x` prefix.",
            __RTS_FN_NS_FMT_FMT_HEX as *const u8,
        ))
        .member(pure_func(
            "formatBin",
            "__RTS_FN_NS_FMT_FMT_BIN",
            sig!(I64 => Handle),
            "formatBin(value: number): string",
            "Binary with `0b` prefix.",
            __RTS_FN_NS_FMT_FMT_BIN as *const u8,
        ))
        .member(pure_func(
            "formatOct",
            "__RTS_FN_NS_FMT_FMT_OCT",
            sig!(I64 => Handle),
            "formatOct(value: number): string",
            "Octal with `0o` prefix.",
            __RTS_FN_NS_FMT_FMT_OCT as *const u8,
        ))
        .member(pure_func(
            "parseInt",
            "__RTS_FN_NS_FMT_PARSE_I64",
            sig!(StrPtr => I64),
            "parseInt(s: string): number",
            "Parses an integer. Returns i64::MIN on error.",
            __RTS_FN_NS_FMT_PARSE_I64 as *const u8,
        ))
        .done();
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
}
