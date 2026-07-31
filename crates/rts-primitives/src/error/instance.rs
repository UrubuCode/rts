//! Error global class family — constructor implementations.
//!
//! Each Error subtype (`TypeError`, `RangeError`, etc.) stores `Entry::ErrorObj`
//! with `name` set to the appropriate class name.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

// ── Helper ────────────────────────────────────────────────────────────────────

unsafe fn str_from_raw(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

fn alloc_error_with_cause(name: &str, message: String, cause: u64) -> u64 {
    alloc_entry(Entry::ErrorObj {
        message,
        name: name.to_owned(),
        cause,
    })
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// (cross-runtime #277) Extrai handle de `options.cause` se options for
/// um Map com chave "cause". Retorna 0 se ausente.
fn extract_cause(options: u64) -> u64 {
    if options == 0 {
        return 0;
    }
    with_entry(options, |e| match e {
        Some(Entry::Map(m)) => m.get("cause").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[rtse::abi("__RTS_FN_GL_TYPE_ERROR_NEW")]
pub fn __RTS_FN_GL_TYPE_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("TypeError", msg, extract_cause(options))
}

#[rtse::abi("__RTS_FN_GL_RANGE_ERROR_NEW")]
pub fn __RTS_FN_GL_RANGE_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("RangeError", msg, extract_cause(options))
}
