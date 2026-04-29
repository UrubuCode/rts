//! `RegExp` global class — constructor and instance method implementations.
//!
//! Constructors delegate to `__RTS_FN_NS_REGEX_COMPILE` (which accepts flags).
//! Instance methods delegate to the existing `regex` namespace ops.

use crate::namespaces::gc::handles::{Entry, alloc_entry, shard_for_handle};

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Constructors ──────────────────────────────────────────────────────────────

/// `new RegExp(pattern)` — no flags.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_NEW(pat_ptr: i64, pat_len: i64) -> u64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_COMPILE(
        pat_ptr as *const u8,
        pat_len,
        std::ptr::null(),
        0,
    )
}

/// `new RegExp(pattern, flags)` — with flags like "gi", "im", "s".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_NEW_WITH_FLAGS(
    pat_ptr: i64,
    pat_len: i64,
    flag_ptr: i64,
    flag_len: i64,
) -> u64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_COMPILE(
        pat_ptr as *const u8,
        pat_len,
        flag_ptr as *const u8,
        flag_len,
    )
}

// ── Instance methods ──────────────────────────────────────────────────────────

/// `re.test(str)` — returns 1 if match, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_TEST(handle: u64, ptr: i64, len: i64) -> i64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_TEST(
        handle,
        ptr as *const u8,
        len,
    )
}

/// `re.exec(str)` — returns string handle of first match, or 0 if none.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_EXEC(handle: u64, ptr: i64, len: i64) -> u64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_FIND(
        handle,
        ptr as *const u8,
        len,
    )
}

/// `re.source` — returns pattern string as a handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_SOURCE(handle: u64) -> u64 {
    let guard = shard_for_handle(handle).lock().unwrap();
    if let Some(Entry::Regex(rx)) = guard.get(handle) {
        let source = rx.as_str().to_owned();
        drop(guard);
        alloc_entry(Entry::String(source.into_bytes()))
    } else {
        0
    }
}
