//! String allocation, identity, and cheap-shape-probe ABI (`gc.string_*`,
//! `gc.is_*`).

use crate::heap::handles::{Entry, alloc_entry, rtse_class_of, with_entry, with_two_entries};

/// Allocates a new string by copying `len` bytes from `ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    alloc_entry(Entry::String(slice.to_vec()))
}

#[rtse::abi("__RTS_FN_NS_GC_STRING_LEN")]
pub fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => bytes.len() as i64,
        _ => -1,
    })
}

/// Returns a pointer to the string's buffer. Valid until the handle is freed.
///
/// # Safety
/// Caller must not read past `LEN` bytes or access after `free`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8 {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => bytes.as_ptr(),
        _ => std::ptr::null(),
    })
}

/// `instanceof Date` — the handle points at an `Entry::Rtse` of class "Date"
/// (the `#[rtse::class("Date")]` struct in rts-shared).
#[rtse::abi("__RTS_FN_NS_GC_IS_DATE")]
pub fn __RTS_FN_NS_GC_IS_DATE(handle: u64) -> i64 {
    if rtse_class_of(handle) == Some("Date") { 1 } else { 0 }
}

/// `instanceof RegExp` — the handle points at `Entry::Regex`.
#[rtse::abi("__RTS_FN_NS_GC_IS_REGEX")]
pub fn __RTS_FN_NS_GC_IS_REGEX(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(_)) => 1,
        _ => 0,
    })
}

#[rtse::abi(native, value = "string_from_f64")]
pub fn string_from_f64(value: f64) -> u64 {
    let s = crate::numfmt::format_js_number(value);
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Intern a STATIC string literal (a `.rodata` DATA object emitted by the AOT
/// lowering — see `expr.rs::HirLit::Str`). Unlike the JIT, which interns each
/// literal ONCE at lowering time and bakes the handle as a code immediate, the
/// AOT path calls this at the binary's runtime — and re-evaluating the literal
/// (e.g. in a loop) would otherwise allocate a fresh handle every time.
///
/// We cache by the literal's DATA address (`ptr`): the same literal has a stable
/// `ptr`, so the first call allocates + PINS the handle (a compile-time constant
/// lives for the whole program — the correct lifetime), and every later call
/// returns the same handle. This (a) matches the JIT's one-handle-per-literal
/// behavior, (b) keeps the constant alive under GC (it's a code constant, on no
/// scanned stack), and (c) bounds the cache to the number of distinct literals.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_STATIC(ptr: *const u8, len: i64) -> u64 {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<(usize, i64), u64>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    // Key on (ptr, len), NOT ptr alone: an EMPTY string literal (`""`) emits a
    // ZERO-LENGTH `.rodata` data object, and a zero-length symbol is placed at the
    // SAME address as the NEXT data symbol by the linker. Keying on `ptr` alone then
    // cache-HITS the empty handle for the next distinct literal -> it is silently
    // corrupted to `""` (AOT-only: the JIT bakes distinct immediate handles, no ptr).
    // (ptr, len) makes `""` = (P, 0) and the colliding literal = (P, len>0) distinct;
    // reading `len` bytes at P still yields the next object's real content.
    let key = (ptr as usize, len);
    let mut g = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&h) = g.get(&key) {
        return h;
    }
    let handle = __RTS_FN_NS_GC_STRING_NEW(ptr, len);
    crate::heap::handles::__RTS_FN_NS_GC_PIN_HANDLE(handle);
    g.insert(key, handle);
    handle
}

/// Compares two string handles by content. Returns 1 if equal, 0 otherwise.
#[rtse::abi("__RTS_FN_NS_GC_STRING_EQ")]
pub fn __RTS_FN_NS_GC_STRING_EQ(a: u64, b: u64) -> i64 {
    if a == b {
        return 1;
    }
    with_two_entries(a, b, |ea, eb| match (ea, eb) {
        (Some(Entry::String(sa)), Some(Entry::String(sb))) => {
            if sa == sb { 1 } else { 0 }
        }
        _ => 0,
    })
}

/// Lexicographic comparison of two string handles by content (memcmp + length).
/// Returns -1 if a < b, 0 if equal, 1 if a > b. Matches JS string ordering.
#[rtse::abi("__RTS_FN_NS_GC_STRING_CMP")]
pub fn __RTS_FN_NS_GC_STRING_CMP(a: u64, b: u64) -> i64 {
    if a == b {
        return 0;
    }
    with_two_entries(a, b, |ea, eb| match (ea, eb) {
        (Some(Entry::String(sa)), Some(Entry::String(sb))) => {
            match sa.as_slice().cmp(sb.as_slice()) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }
        }
        _ => 0,
    })
}

/// Concatenates two string handles and returns a new handle.
///
/// For handles that are NOT String (Vec, Map, etc.), applies JS-style
/// coercion identical to `TPL_COERCE_AUTO`:
/// - Vec<i64> -> "1,2,3"
/// - Map -> "[object Object]"
/// - Others -> "[object <Kind>]"
///
/// Invalid handles become the empty string (JS template-literal semantics).
#[rtse::abi("__RTS_FN_NS_GC_STRING_CONCAT")]
pub fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64 {
    use super::snapshot::{EntrySnap, snapshot_entry, snapshot_to_bytes};
    // Handle 0 = JS null — displayed as "null" in string context (JS spec: null + "" = "null").
    let snap_a = if a == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(a) };
    let snap_b = if b == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(b) };
    let mut out = snapshot_to_bytes(&snap_a);
    out.extend_from_slice(&snapshot_to_bytes(&snap_b));
    alloc_entry(Entry::String(out))
}
