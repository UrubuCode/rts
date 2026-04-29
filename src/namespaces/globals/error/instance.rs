//! Error global class family — constructor and instance method implementations.
//!
//! Each Error subtype (`TypeError`, `RangeError`, etc.) stores `Entry::ErrorObj`
//! with `name` set to the appropriate class name. All instance methods are
//! shared (same symbol `__RTS_FN_GL_ERROR_*`).

use crate::namespaces::gc::handles::{Entry, alloc_entry, shard_for_handle};

// ── Helper ────────────────────────────────────────────────────────────────────

unsafe fn str_from_raw(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

fn alloc_error(name: &str, message: String) -> u64 {
    alloc_entry(Entry::ErrorObj {
        message,
        name: name.to_owned(),
    })
}

fn get_field(handle: u64, field: &str) -> String {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::ErrorObj { message, name }) => match field {
            "message" => message.clone(),
            "name" => name.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn alloc_str(s: String) -> u64 {
    alloc_entry(Entry::String(s.into_bytes()))
}

// ── Constructors ──────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("Error", msg)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TYPE_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("TypeError", msg)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_RANGE_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("RangeError", msg)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REF_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("ReferenceError", msg)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYNTAX_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("SyntaxError", msg)
}

// ── Instance methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_MESSAGE(handle: u64) -> u64 {
    alloc_str(get_field(handle, "message"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_NAME(handle: u64) -> u64 {
    alloc_str(get_field(handle, "name"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_TO_STRING(handle: u64) -> u64 {
    let name = get_field(handle, "name");
    let msg = get_field(handle, "message");
    let s = if msg.is_empty() {
        name
    } else {
        format!("{name}: {msg}")
    };
    alloc_str(s)
}
