//! `RegExp` global class — constructor and instance method implementations.
//!
//! Constructors delegate to `__RTS_FN_NS_REGEX_COMPILE` (which accepts flags).
//! Instance methods delegate to the existing `regex` namespace ops.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

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

/// (#781) `re.flags` — string canonica das flags (ex: "gi").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_FLAGS(handle: u64) -> u64 {
    let f: Option<String> = with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => Some(rx.flags.clone()),
        _ => None,
    });
    match f {
        Some(s) => alloc_entry(Entry::String(s.into_bytes())),
        None => 0,
    }
}

/// `re.global` — flag 'g' setada?
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_GLOBAL(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('g') { 1 } else { 0 },
        _ => 0,
    })
}

/// `re.ignoreCase` — flag 'i' setada?
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_IGNORE_CASE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('i') { 1 } else { 0 },
        _ => 0,
    })
}

/// `re.multiline` — flag 'm' setada?
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_MULTILINE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('m') { 1 } else { 0 },
        _ => 0,
    })
}

/// (#782) `re.lastIndex` — getter retorna posicao do proximo `exec`/`test`
/// em regex global (ou 0 em regex nao-global).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_LAST_INDEX_GET(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => rx.last_index as i64,
        _ => 0,
    })
}

/// (#782) `re.lastIndex = N` — setter direto. JS spec aceita qualquer
/// numero; clamps para >= 0 e armazena como usize.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_LAST_INDEX_SET(handle: u64, n: i64) {
    let v = if n < 0 { 0 } else { n as usize };
    with_entry_mut(handle, |entry| {
        if let Some(Entry::Regex(rx)) = entry {
            rx.last_index = v;
        }
    });
}

/// `re.source` — returns pattern string as a handle.
/// JS spec: empty pattern returns `"(?:)"` (RegExp.prototype.source default).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_SOURCE(handle: u64) -> u64 {
    let source: Option<String> = with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => Some(rx.regex.as_str().to_owned()),
        _ => None,
    });
    match source {
        Some(s) if s.is_empty() => alloc_entry(Entry::String(b"(?:)".to_vec())),
        Some(s) => alloc_entry(Entry::String(s.into_bytes())),
        None => 0,
    }
}
