//! Error global class family — constructor and instance method implementations.
//!
//! Each Error subtype (`TypeError`, `RangeError`, etc.) stores `Entry::ErrorObj`
//! with `name` set to the appropriate class name. All instance methods are
//! shared (same symbol `__RTS_FN_GL_ERROR_*`).

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

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
    // Tenta primeiro Entry::ErrorObj (caso comum: new Error("msg"))
    let direct = with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { message, name }) => Some(match field {
            "message" => message.clone(),
            "name" => name.clone(),
            _ => String::new(),
        }),
        _ => None,
    });
    if let Some(s) = direct {
        return s;
    }
    // Fallback: Entry::Map com keys "message"/"name" — usado quando
    // user class extends Error e super(msg) armazena no Map.
    let slot_handle: u64 = with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => m.get(field).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if slot_handle == 0 {
        return String::new();
    }
    with_entry(slot_handle, |entry| match entry {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    })
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URI_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("URIError", msg)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVAL_ERROR_NEW(ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error("EvalError", msg)
}

// ── Instance methods ──────────────────────────────────────────────────────────

/// `instanceof Error` (any subtype) — handle aponta para Entry::ErrorObj.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_IS_ERROR(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { .. }) => 1,
        _ => 0,
    })
}

/// `instanceof TypeError` etc. — checa name field exato.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_IS_ERROR_NAMED(handle: u64, name_ptr: i64, name_len: i64) -> i64 {
    if name_ptr == 0 || name_len <= 0 { return 0; }
    let want = unsafe { std::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let want_s = match std::str::from_utf8(want) { Ok(s) => s, Err(_) => return 0 };
    with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { name, .. }) => if name == want_s { 1 } else { 0 },
        _ => 0,
    })
}

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
