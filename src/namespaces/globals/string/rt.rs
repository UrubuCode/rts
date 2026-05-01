//! Instance + static methods da GlobalClassSpec de String.
//! Todos recebem/retornam handles u64. Simbolos: __RTS_FN_GL_STRING_*.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8;
    fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64;
    fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64);
}

fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn handle_to_str<'a>(h: u64) -> Option<&'a str> {
    let ptr = unsafe { __RTS_FN_NS_GC_STRING_PTR(h) };
    let len = unsafe { __RTS_FN_NS_GC_STRING_LEN(h) };
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

// ── Constructors ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NEW_FROM(handle: u64) -> u64 {
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NEW_EMPTY() -> u64 {
    alloc_str("")
}

// ── Static methods ─────────────────────────────────────────────────────────────

/// String.fromCharCode(code) — char from UTF-16 code unit (BMP only).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CHAR_CODE(code: i64) -> u64 {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}

/// String.fromCodePoint(codePoint) — char from full Unicode code point.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CODE_POINT(code: i64) -> u64 {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}

// ── Search methods ─────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INDEX_OF(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.find(n).map(|i| i as i64).unwrap_or(-1),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LAST_INDEX_OF(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.rfind(n).map(|i| i as i64).unwrap_or(-1),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INCLUDES(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.contains(n) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_STARTS_WITH(recv: u64, prefix: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(prefix)) {
        (Some(s), Some(p)) => s.starts_with(p) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_ENDS_WITH(recv: u64, suffix: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(suffix)) {
        (Some(s), Some(p)) => s.ends_with(p) as i64,
        _ => 0,
    }
}

// ── Indexing methods ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CHAR_AT(recv: u64, idx: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else { return alloc_str("") };
    if idx < 0 { return alloc_str(""); }
    match s.chars().nth(idx as usize) {
        Some(ch) => { let mut buf = [0u8; 4]; alloc_str(ch.encode_utf8(&mut buf)) }
        None => alloc_str(""),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CHAR_CODE_AT(recv: u64, idx: i64) -> i64 {
    let Some(s) = handle_to_str(recv) else { return -1 };
    if idx < 0 { return -1; }
    s.chars().nth(idx as usize).map(|c| c as i64).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CODE_POINT_AT(recv: u64, idx: i64) -> i64 {
    let Some(s) = handle_to_str(recv) else { return -1 };
    if idx < 0 { return -1; }
    s.chars().nth(idx as usize).map(|c| c as u32 as i64).unwrap_or(-1)
}

/// str.at(idx) — supports negative index (counts from end).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_AT(recv: u64, idx: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else { return alloc_str("") };
    let count = s.chars().count() as i64;
    let i = if idx < 0 { count + idx } else { idx };
    if i < 0 || i >= count { return alloc_str(""); }
    match s.chars().nth(i as usize) {
        Some(ch) => { let mut buf = [0u8; 4]; alloc_str(ch.encode_utf8(&mut buf)) }
        None => alloc_str(""),
    }
}

// ── Slicing methods ────────────────────────────────────────────────────────────

/// str.slice(start, end) — negative indices count from end.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SLICE(recv: u64, start: i64, end: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else { return alloc_str("") };
    let count = s.chars().count() as i64;
    let norm = |i: i64| -> usize {
        let n = if i < 0 { count + i } else { i };
        n.clamp(0, count) as usize
    };
    let si = norm(start);
    let ei = norm(end);
    if si >= ei { return alloc_str(""); }
    let result: String = s.chars().skip(si).take(ei - si).collect();
    alloc_str(&result)
}

/// str.substring(start, end) — like slice but negatives clamp to 0, swaps if start>end.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SUBSTRING(recv: u64, start: i64, end: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else { return alloc_str("") };
    let count = s.chars().count() as i64;
    let clamp = |i: i64| i.clamp(0, count) as usize;
    let (si, ei) = {
        let a = clamp(start);
        let b = clamp(end);
        if a <= b { (a, b) } else { (b, a) }
    };
    if si >= ei { return alloc_str(""); }
    let result: String = s.chars().skip(si).take(ei - si).collect();
    alloc_str(&result)
}

/// str.substr(start, length) — deprecated start+count form.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SUBSTR(recv: u64, start: i64, length: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else { return alloc_str("") };
    let count = s.chars().count() as i64;
    let si = (if start < 0 { count + start } else { start }).clamp(0, count) as usize;
    let take = if length < 0 { 0 } else { length as usize };
    let result: String = s.chars().skip(si).take(take).collect();
    alloc_str(&result)
}

// ── Transform methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_UPPER_CASE(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(&s.to_uppercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_LOWER_CASE(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(&s.to_lowercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM_START(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim_start())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM_END(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim_end())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPEAT(recv: u64, n: i64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    if n <= 0 { return alloc_str(""); }
    alloc_str(&s.repeat(n as usize))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE(recv: u64, from: u64, to: u64) -> u64 {
    match (handle_to_str(recv), handle_to_str(from), handle_to_str(to)) {
        (Some(s), Some(f), Some(t)) => alloc_str(&s.replacen(f, t, 1)),
        _ => recv,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE_ALL(recv: u64, from: u64, to: u64) -> u64 {
    match (handle_to_str(recv), handle_to_str(from), handle_to_str(to)) {
        (Some(s), Some(f), Some(t)) => alloc_str(&s.replace(f, t)),
        _ => recv,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CONCAT(recv: u64, other: u64) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_CONCAT(recv, other) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_START(recv: u64, target_len: i64, pad: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let fill = handle_to_str(pad).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || fill.is_empty() { return recv; }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = fill.chars().collect();
    let prefix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{prefix}{s}"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_END(recv: u64, target_len: i64, pad: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let fill = handle_to_str(pad).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || fill.is_empty() { return recv; }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = fill.chars().collect();
    let suffix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{s}{suffix}"))
}

/// str.split(sep) — retorna Vec handle com string handles como elementos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT(recv: u64, sep: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let delim = handle_to_str(sep).unwrap_or("");
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    for part in s.split(delim) {
        let h = alloc_str(part) as i64;
        unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
    }
    vec_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LOCALE_COMPARE(recv: u64, other: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(other)) {
        (Some(a), Some(b)) => a.cmp(b) as i64 - 1, // Less=-1, Equal=0, Greater=1 via (ord as i64 - 1)
        _ => 0,
    }
}

// ── Identity / ES2024 ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_STRING(handle: u64) -> u64 {
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_IS_WELL_FORMED(_handle: u64) -> i64 {
    1 // RTS strings are always valid UTF-8, so always well-formed
}
