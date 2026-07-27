//! `fmt` namespace — parse and format primitives (string <-> number).
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).
//! `parse_*` carregam um sentinela de erro (i64::MIN / NaN / -1) também emitido
//! em null/UTF-8 inválido; `fmt_*` retornam handles de string GC.

use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, sig};

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    // `parseFloat`'s real JS-correct body now lives in `rts-primitives`
    // (`number/parse.rs`, exposed via the `#[rtse::statical]`
    // `Number.parseFloat` member) — reached here the same way any cross-crate
    // `__rtsm_*`/`__rtsadp_*` symbol is, a forward decl resolved at final link
    // (rts-shared does not gain a Cargo dep on rts-primitives). `parseInt` is
    // NOT reused here — see `__RTS_FN_NS_FMT_PARSE_I64`'s doc for why its
    // contract genuinely differs and stays its own body.
    fn __rtsm_global_number_parse_float(ptr: i64, len: i64) -> f64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Parses an integer with `rts:fmt`'s OWN contract — a plain `i64` return with
/// sentinel `i64::MIN` on failure — NOT the JS `parseFloat`/`NaN` contract.
/// This is a distinct, documented behaviour from `Number.parseInt` (which
/// returns a possibly-fractional `f64` and truncates rather than rejecting
/// `"3.9"` as a parse failure): this entry point strictly parses `s` as a
/// base-10 (whitespace-trimmed) integer literal via `str::parse`, so `"3.9"`
/// or `"42abc"` are BOTH errors here even though `Number.parseInt` accepts
/// them. Kept as its own body (not delegated) — the two contracts genuinely
/// differ, so unifying them would silently change `rts:fmt`'s behaviour.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_I64(ptr: *const u8, len: i64) -> i64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return i64::MIN;
    };
    s.trim().parse::<i64>().unwrap_or(i64::MIN)
}

/// Parses a float using the real JS `parseFloat` grammar (leading run,
/// trailing garbage ignored) — same contract as `Number.parseFloat`, so this
/// delegates to the single shared implementation in `rts-primitives`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_F64(ptr: *const u8, len: i64) -> f64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return f64::NAN;
    };
    unsafe { __rtsm_global_number_parse_float(s.as_ptr() as i64, s.len() as i64) }
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
        ret_class: None,
        pure: true,
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
