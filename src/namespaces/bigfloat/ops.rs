//! Arbitrary-precision fixed-point decimal operations.
//!
//! All values flow through the shared GC handle table (`gc::handles`) as
//! `Entry::BigFixed`. Algorithms like Machin's π live in user-space TS
//! and compose these primitives.

use super::fixed::FixedDecimal;
// `super::super` resolves to the crate root in the standalone runtime
// staticlib build (`rt_all.rs`) and to `namespaces` in the main `rts`
// crate; both expose the `gc` submodule at that level.
use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

// Forward-declared extern so we can intern the decimal string without
// hard-coding the `gc::string_pool::...` path (which differs between
// build contexts).
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn alloc(value: FixedDecimal) -> u64 {
    alloc_entry(Entry::BigFixed(Box::new(value)))
}

fn clone_of(handle: u64) -> Option<FixedDecimal> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::BigFixed(b)) => Some(b.as_ref().clone()),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_ZERO(precision_digits: i64) -> u64 {
    let scale = precision_digits.max(1).min(36) as u32;
    alloc(FixedDecimal::zero(scale))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_F64(x: f64, precision_digits: i64) -> u64 {
    let scale = precision_digits.max(1).min(36) as u32;
    alloc(FixedDecimal::from_f64(x, scale))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_STR(
    ptr: *const u8,
    len: i64,
    precision_digits: i64,
) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    // SAFETY: caller contract — valid UTF-8, covers `len` bytes, stays live.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let Ok(s) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let scale = precision_digits.max(1).min(36) as u32;
    match FixedDecimal::from_str(s, scale) {
        Some(v) => alloc(v),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FROM_I64(x: i64, precision_digits: i64) -> u64 {
    let scale = precision_digits.max(1).min(36) as u32;
    alloc(FixedDecimal::from_i64(x, scale))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_TO_F64(handle: u64) -> f64 {
    clone_of(handle).map(|v| v.to_f64()).unwrap_or(f64::NAN)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_TO_STRING(handle: u64) -> u64 {
    let s = clone_of(handle)
        .map(|v| v.to_string_decimal())
        .unwrap_or_else(|| "NaN".to_string());
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_ADD(a: u64, b: u64) -> u64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.add(&r))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_SUB(a: u64, b: u64) -> u64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.sub(&r))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_MUL(a: u64, b: u64) -> u64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    alloc(l.mul(&r))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_DIV(a: u64, b: u64) -> u64 {
    let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
        return 0;
    };
    match l.div(&r) {
        Some(v) => alloc(v),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_NEG(a: u64) -> u64 {
    let Some(v) = clone_of(a) else { return 0 };
    alloc(v.neg())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_SQRT(a: u64) -> u64 {
    let Some(v) = clone_of(a) else { return 0 };
    match v.sqrt() {
        Some(r) => alloc(r),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BIGFLOAT_FREE(handle: u64) {
    free_handle(handle);
}
