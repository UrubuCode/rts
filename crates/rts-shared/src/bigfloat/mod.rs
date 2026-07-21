//! `bigfloat` namespace — i128 decimal fixed-point (scale ≤ 36).
//!
//! Handle-based API backed by `FixedDecimal` (`fixed` submodule). Values live in
//! the shared GC handle table as `Entry::BigFixed`; callers allocate with
//! zero/from_*, operate via named methods, and eventually `free` the handle.
//! Algorithms like Machin's π compose these primitives in user-space TS.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

/// `FixedDecimal` migrou pro `rts-engine` (heap GC no motor). Facade pra
/// `bigfloat::fixed::FixedDecimal` seguir resolvendo nos consumidores.
pub mod fixed {
    pub use rts_engine::heap::fixed::*;
}

use rts_engine::abi::ty::{F64, Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use fixed::FixedDecimal;

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn alloc(value: FixedDecimal) -> u64 {
    alloc_entry(Entry::BigFixed(Box::new(value)))
}

fn clone_of(handle: u64) -> Option<FixedDecimal> {
    with_entry(handle, |entry| match entry {
        Some(Entry::BigFixed(b)) => Some(b.as_ref().clone()),
        _ => None,
    })
}

/// Zero with `precision` decimal digits (clamped 1..=36). Handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_ZERO(precision: I64) -> U64 {
    let scale = precision.max(1).min(36) as u32;
    alloc(FixedDecimal::zero(scale))
}

/// From an f64 with `precision` decimal digits. Handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_F64(x: F64, precision: I64) -> U64 {
    let scale = precision.max(1).min(36) as u32;
    alloc(FixedDecimal::from_f64(x, scale))
}

/// Parse a decimal string with `precision` digits. Handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_STR(
    s_ptr: *const u8,
    s_len: i64,
    precision: I64,
) -> U64 {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let scale = precision.max(1).min(36) as u32;
    match FixedDecimal::from_str(s, scale) {
        Some(v) => alloc(v),
        None => 0,
    }
}

/// From an i64 with `precision` decimal digits. Handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_I64(x: I64, precision: I64) -> U64 {
    let scale = precision.max(1).min(36) as u32;
    alloc(FixedDecimal::from_i64(x, scale))
}

/// Convert to f64 (NaN if handle invalid).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_TO_F64(h: U64) -> F64 {
    clone_of(h).map(|v| v.to_f64()).unwrap_or(f64::NAN)
}

/// Decimal string (string handle; "NaN" if handle invalid).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_TO_STRING(h: U64) -> Handle {
    let s = clone_of(h)
        .map(|v| v.to_string_decimal())
        .unwrap_or_else(|| "NaN".to_string());
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// a + b. New handle, 0 if either invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_ADD(a: U64, b: U64) -> U64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.add(&r))
}

/// a - b. New handle, 0 if either invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_SUB(a: U64, b: U64) -> U64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.sub(&r))
}

/// a * b. New handle, 0 if either invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_MUL(a: U64, b: U64) -> U64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.mul(&r))
}

/// a / b. New handle, 0 if either invalid or division fails.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_DIV(a: U64, b: U64) -> U64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    match l.div(&r) {
        Some(v) => alloc(v),
        None => 0,
    }
}

/// -a. New handle, 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_NEG(a: U64) -> U64 {
    let Some(v) = clone_of(a) else { return 0 };
    alloc(v.neg())
}

/// sqrt(a). New handle, 0 if invalid or negative.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_SQRT(a: U64) -> U64 {
    let Some(v) = clone_of(a) else { return 0 };
    match v.sqrt() {
        Some(r) => alloc(r),
        None => 0,
    }
}

/// Releases the handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FREE(h: U64) {
    free_handle(h);
}

/// Função `bigfloat.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `bigfloat` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("bigfloat")
        .doc("i128 decimal fixed-point arithmetic (scale ≤ 36), handle-based.")
        .member(func(
            "zero",
            "__RTS_FN_NS_BIGFLOAT_ZERO",
            Sig::new(vec![AbiType::I64], AbiType::U64),
            "zero(precision: number): number",
            "Zero with `precision` decimal digits (clamped 1..=36). Handle.",
            __RTS_FN_NS_BIGFLOAT_ZERO as *const u8,
        ))
        .member(func(
            "from_f64",
            "__RTS_FN_NS_BIGFLOAT_FROM_F64",
            Sig::new(vec![AbiType::F64, AbiType::I64], AbiType::U64),
            "from_f64(x: number, precision: number): number",
            "From an f64 with `precision` decimal digits. Handle.",
            __RTS_FN_NS_BIGFLOAT_FROM_F64 as *const u8,
        ))
        .member(func(
            "from_str",
            "__RTS_FN_NS_BIGFLOAT_FROM_STR",
            Sig::new(vec![AbiType::StrPtr, AbiType::I64], AbiType::U64),
            "from_str(s: string, precision: number): number",
            "Parse a decimal string with `precision` digits. Handle, 0 on error.",
            __RTS_FN_NS_BIGFLOAT_FROM_STR as *const u8,
        ))
        .member(func(
            "from_i64",
            "__RTS_FN_NS_BIGFLOAT_FROM_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::U64),
            "from_i64(x: number, precision: number): number",
            "From an i64 with `precision` decimal digits. Handle.",
            __RTS_FN_NS_BIGFLOAT_FROM_I64 as *const u8,
        ))
        .member(func(
            "to_f64",
            "__RTS_FN_NS_BIGFLOAT_TO_F64",
            Sig::new(vec![AbiType::U64], AbiType::F64),
            "to_f64(h: number): number",
            "Convert to f64 (NaN if handle invalid).",
            __RTS_FN_NS_BIGFLOAT_TO_F64 as *const u8,
        ))
        .member(func(
            "to_string",
            "__RTS_FN_NS_BIGFLOAT_TO_STRING",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "to_string(h: number): string",
            "Decimal string (string handle; \"NaN\" if handle invalid).",
            __RTS_FN_NS_BIGFLOAT_TO_STRING as *const u8,
        ))
        .member(func(
            "add",
            "__RTS_FN_NS_BIGFLOAT_ADD",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "add(a: number, b: number): number",
            "a + b. New handle, 0 if either invalid.",
            __RTS_FN_NS_BIGFLOAT_ADD as *const u8,
        ))
        .member(func(
            "sub",
            "__RTS_FN_NS_BIGFLOAT_SUB",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "sub(a: number, b: number): number",
            "a - b. New handle, 0 if either invalid.",
            __RTS_FN_NS_BIGFLOAT_SUB as *const u8,
        ))
        .member(func(
            "mul",
            "__RTS_FN_NS_BIGFLOAT_MUL",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "mul(a: number, b: number): number",
            "a * b. New handle, 0 if either invalid.",
            __RTS_FN_NS_BIGFLOAT_MUL as *const u8,
        ))
        .member(func(
            "div",
            "__RTS_FN_NS_BIGFLOAT_DIV",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "div(a: number, b: number): number",
            "a / b. New handle, 0 if either invalid or division fails.",
            __RTS_FN_NS_BIGFLOAT_DIV as *const u8,
        ))
        .member(func(
            "neg",
            "__RTS_FN_NS_BIGFLOAT_NEG",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "neg(a: number): number",
            "-a. New handle, 0 if invalid.",
            __RTS_FN_NS_BIGFLOAT_NEG as *const u8,
        ))
        .member(func(
            "sqrt",
            "__RTS_FN_NS_BIGFLOAT_SQRT",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "sqrt(a: number): number",
            "sqrt(a). New handle, 0 if invalid or negative.",
            __RTS_FN_NS_BIGFLOAT_SQRT as *const u8,
        ))
        .member(func(
            "free",
            "__RTS_FN_NS_BIGFLOAT_FREE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "free(h: number): void",
            "Releases the handle.",
            __RTS_FN_NS_BIGFLOAT_FREE as *const u8,
        ))
        .done();
}
