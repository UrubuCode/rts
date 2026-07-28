//! String allocation, identity, and cheap-shape-probe ABI (`gc.string_*`,
//! `gc.is_*`, `gc.handle_len`).

use crate::heap::handles::{Entry, alloc_entry, free_handle, rtse_class_of, with_entry, with_two_entries};

/// Allocates a new string by copying `len` bytes from `ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    alloc_entry(Entry::String(slice.to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64 {
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FREE(handle: u64) -> i64 {
    if free_handle(handle) { 1 } else { 0 }
}

/// Generic length dispatcher — backs `.size`/`.length` in codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_HANDLE_LEN(handle: u64) -> i64 {
    // (#1023) StringBox unwrap before dispatch — recurse once.
    let unwrap: Option<u64> = with_entry(handle, |entry| match entry {
        Some(Entry::StringBox(h)) => Some(*h),
        _ => None,
    });
    if let Some(inner) = unwrap {
        return __RTS_FN_NS_GC_HANDLE_LEN(inner);
    }
    with_entry(handle, |entry| match entry {
        // JS spec: String.length = number of UTF-16 code units, not bytes.
        Some(Entry::String(b)) => {
            // FAST PATH ASCII: byte count == code-unit count. `encode_utf16()
            // .count()` walked the WHOLE string on every `.length` read, which
            // in a `while (i < s.length)` loop costs O(n^2).
            if b.is_ascii() {
                return b.len() as i64;
            }
            match std::str::from_utf8(b) {
                Ok(s) => s.encode_utf16().count() as i64,
                Err(_) => b.len() as i64,
            }
        }
        Some(Entry::Map(m)) => {
            // Array-like maps (e.g. regex match results) store a "length" key.
            if let Some(&v) = m.get("length") {
                v
            } else {
                m.len() as i64
            }
        }
        Some(Entry::Vec(v)) => v.len() as i64,
        Some(Entry::Buffer(b)) => b.len() as i64,
        Some(Entry::Env(s)) => s.len() as i64,
        _ => -1,
    })
}

/// (#208 / Array.isArray) Returns 1 if the handle points at a Vec, 0 otherwise.
/// Backs `Array.isArray(x)` in codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_VEC(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(_)) => 1,
        _ => 0,
    })
}

/// `instanceof Date` — the handle points at an `Entry::Rtse` of class "Date"
/// (the `#[rtse::class("Date")]` struct in rts-shared).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_DATE(handle: u64) -> i64 {
    if rtse_class_of(handle) == Some("Date") { 1 } else { 0 }
}

/// `instanceof RegExp` — the handle points at `Entry::Regex`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_REGEX(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(_)) => 1,
        _ => 0,
    })
}

/// `instanceof Map`/`Object` — the handle points at `Entry::Map` (Object
/// accepts any Map). WeakMap/WeakSet are `.ts` classes (arrays of
/// PolyValue), not Entry variants — not map-like by handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_MAP_LIKE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(_)) => 1,
        _ => 0,
    })
}

/// `instanceof Promise` — the handle points at `Entry::PromiseAsync`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_PROMISE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(_)) => 1,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64(value: i64) -> u64 {
    // Raw i64 -> decimal string. This fn is the primitive exposed via
    // `gc.string_from_i64` (e.g. num.checked_* returning i64::MIN on
    // overflow needs to become "-9223372036854775808"). JS sentinels
    // (MIN..MIN+4) are handled in coerce_to_handle/template via
    // TPL_COERCE_AUTO (and via STRING_FROM_I64_TPL below).
    alloc_entry(Entry::String(value.to_string().into_bytes()))
}

/// (cross-runtime) Variant of STRING_FROM_I64 used in template
/// literal/coerce_to_handle: filters JS sentinels before formatting so
/// `${undefined}` -> "undefined" instead of the raw i64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64_TPL(value: i64) -> u64 {
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 { return alloc_entry(Entry::String(b"null".to_vec())); }
    alloc_entry(Entry::String(value.to_string().into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_F64(value: f64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_EQ(a: u64, b: u64) -> i64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CMP(a: u64, b: u64) -> i64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64 {
    use super::snapshot::{EntrySnap, snapshot_entry, snapshot_to_bytes};
    // Handle 0 = JS null — displayed as "null" in string context (JS spec: null + "" = "null").
    let snap_a = if a == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(a) };
    let snap_b = if b == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(b) };
    let mut out = snapshot_to_bytes(&snap_a);
    out.extend_from_slice(&snapshot_to_bytes(&snap_b));
    alloc_entry(Entry::String(out))
}
