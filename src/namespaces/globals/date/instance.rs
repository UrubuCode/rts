//! `Date` global class — constructor and instance method implementations.
//!
//! Each `Date` instance is stored as `Entry::DateMs(i64)` in the HandleTable,
//! where the i64 is milliseconds since Unix epoch (UTC).

use crate::namespaces::gc::handles::{Entry, alloc_entry, shard_for_handle};

// ── Helper ────────────────────────────────────────────────────────────────────

fn get_ms(handle: u64) -> i64 {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::DateMs(ms)) => *ms,
        _ => 0,
    }
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// `new Date()` — current Unix timestamp in milliseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_NOW() -> u64 {
    let ms = crate::namespaces::date::ops::__RTS_FN_NS_DATE_NOW_MS();
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(ms)` — from explicit milliseconds since epoch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_MS(ms: i64) -> u64 {
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(iso_str)` — from ISO 8601 string (`ptr`/`len` ABI).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_ISO(ptr: i64, len: i64) -> u64 {
    let ms = crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_ISO(ptr as u64, len);
    alloc_entry(Entry::DateMs(ms))
}

// ── Instance methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_TIME(handle: u64) -> i64 {
    get_ms(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_VALUE_OF(handle: u64) -> i64 {
    get_ms(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_FULL_YEAR(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_YEAR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MONTH(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MONTH(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DATE(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_DAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_HOURS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_HOUR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MINUTES(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MINUTE(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_SECONDS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_SECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MILLISECONDS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MILLISECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DAY(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_WEEKDAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_ISO_STRING(handle: u64) -> u64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_TO_ISO(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_STRING(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}
