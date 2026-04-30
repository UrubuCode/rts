//! String-producing ABI for the GC namespace.

use super::handles::{Entry, alloc_entry, free_handle, with_entry, with_two_entries};

/// Reads a string handle into an owned Rust `String`.
pub fn read_string_handle(handle: u64) -> Option<String> {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    })
}

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
    with_entry(handle, |entry| match entry {
        Some(Entry::String(b)) => b.len() as i64,
        Some(Entry::Map(m)) => m.len() as i64,
        Some(Entry::Vec(v)) => v.len() as i64,
        Some(Entry::Buffer(b)) => b.len() as i64,
        Some(Entry::Env(s)) => s.len() as i64,
        _ => -1,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64(value: i64) -> u64 {
    alloc_entry(Entry::String(value.to_string().into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_F64(value: f64) -> u64 {
    let s = if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    };
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Concatenates two string handles and returns a new handle.
/// Invalid handles are treated as empty strings (JS template literal semantics).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64 {
    // Read both entries under a single arena lock via with_two_entries,
    // clone the bytes, then allocate the result (which acquires the lock again).
    let bytes = with_two_entries(a, b, |ea, eb| {
        let mut out = match ea {
            Some(Entry::String(s)) => s.clone(),
            _ => Vec::new(),
        };
        if let Some(Entry::String(s)) = eb {
            out.extend_from_slice(s);
        }
        out
    });
    alloc_entry(Entry::String(bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_STATIC(ptr: *const u8, len: i64) -> u64 {
    __RTS_FN_NS_GC_STRING_NEW(ptr, len)
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
