//! String-producing ABI for the GC namespace.
//!
//! Strings are stored as UTF-8 byte vectors behind a handle. Callers read
//! back the bytes via `ptr` + `len` accessors that expose the underlying
//! buffer. The pointer remains valid as long as the handle is live; callers
//! must copy before freeing.

use super::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

/// Reads a string handle into an owned Rust `String`.
/// Returns `None` for invalid/non-string handles.
pub fn read_string_handle(handle: u64) -> Option<String> {
    let t = shard_for_handle(handle).lock().expect("handle table poisoned");
    match t.get(handle) {
        Some(Entry::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

/// Allocates a new string by copying `len` bytes from `ptr`.
/// Returns a handle, or `0` on invalid input.
///
/// # Safety
/// `ptr` must be valid for `len` bytes. Contents are treated as opaque
/// bytes; callers are responsible for ensuring UTF-8 when that matters.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    // SAFETY: caller contract guarantees `ptr` covers `len` live bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    alloc_entry(Entry::String(slice.to_vec()))
}

/// Returns the byte length of the string behind `handle`, or `-1` if the
/// handle is invalid or has been freed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64 {
    let t = shard_for_handle(handle).lock().expect("handle table poisoned");
    match t.get(handle) {
        Some(Entry::String(bytes)) => bytes.len() as i64,
        _ => -1,
    }
}

/// Returns a pointer to the first byte of the string's buffer, or null on
/// invalid handle. The pointer is valid until the handle is freed.
///
/// # Safety
/// Readers must not exceed `LEN` bytes from the returned pointer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8 {
    let t = shard_for_handle(handle).lock().expect("handle table poisoned");
    match t.get(handle) {
        Some(Entry::String(bytes)) => bytes.as_ptr(),
        _ => std::ptr::null(),
    }
}

/// Frees the handle, returning the slot to the pool. Returns `1` on
/// success, `0` if the handle was already invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FREE(handle: u64) -> i64 {
    if free_handle(handle) { 1 } else { 0 }
}

/// Converts an `i64` to its decimal string representation and returns a handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64(value: i64) -> u64 {
    let s = value.to_string();
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Converts an `f64` to its decimal string representation and returns a handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_F64(value: f64) -> u64 {
    let s = if value.fract() == 0.0 && value.is_finite() {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    };
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Concatenates two string handles and returns a new handle.
/// Handles invalidos sao tratados como string vazia — match ergonomia
/// JS de `${undefined}` (que vira "undefined") e evita propagar handle
/// 0 que silenciaria o resto do template.
///
/// `a` e `b` podem viver em shards diferentes — clonamos cada um sob
/// seu lock proprio antes de alocar o resultado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64 {
    let mut bytes = {
        let t = shard_for_handle(a).lock().expect("handle table poisoned");
        match t.get(a) {
            Some(Entry::String(s)) => s.clone(),
            _ => Vec::new(),
        }
    };
    {
        let t = shard_for_handle(b).lock().expect("handle table poisoned");
        if let Some(Entry::String(s)) = t.get(b) {
            bytes.extend_from_slice(s);
        }
    }
    alloc_entry(Entry::String(bytes))
}

/// Promotes a static string slice (ptr, len) to a GC handle.
/// Equivalent to `string_new` but named distinctly for codegen clarity.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_STATIC(ptr: *const u8, len: i64) -> u64 {
    __RTS_FN_NS_GC_STRING_NEW(ptr, len)
}

/// Compares dois string handles por conteudo (memcmp). Retorna 1 se
/// os bytes forem iguais, 0 caso contrario. Handles invalidos so sao
/// iguais entre si quando ambos forem 0.
///
/// Como `a` e `b` podem viver em shards diferentes, cada lookup ocorre
/// sob seu shard proprio. Clonamos `sa` pra liberar o primeiro lock
/// antes de pegar o segundo (evita deadlock se mesma thread tentar dois
/// locks nos diferentes shards na ordem oposta em outra chamada).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_EQ(a: u64, b: u64) -> i64 {
    if a == b {
        return 1;
    }
    let sa: Vec<u8> = {
        let t = shard_for_handle(a).lock().expect("handle table poisoned");
        match t.get(a) {
            Some(Entry::String(s)) => s.clone(),
            _ => return 0,
        }
    };
    let t = shard_for_handle(b).lock().expect("handle table poisoned");
    let sb = match t.get(b) {
        Some(Entry::String(s)) => s,
        _ => return 0,
    };
    if sa == *sb { 1 } else { 0 }
}
