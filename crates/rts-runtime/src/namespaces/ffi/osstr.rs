//! OsString — string plataforma-OS (WTF-8 no Windows, bytes no Unix).

use std::ffi::OsString;

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Constroi OsString a partir de string TS. 0 se input invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_FROM_STR(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    let os = OsString::from(s);
    alloc_entry(Entry::OsString(Box::new(os)))
}

/// Converte para string UTF-8 (handle gc::string). 0 se nao-UTF8 ou
/// handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_TO_STR(handle: u64) -> u64 {
    // Clone bytes inside with_entry to release lock before calling STRING_NEW.
    let bytes: Option<Vec<u8>> = with_entry(handle, |entry| match entry {
        Some(Entry::OsString(os)) => os.to_str().map(|s| s.as_bytes().to_vec()),
        _ => None,
    });
    match bytes {
        Some(b) => unsafe { __RTS_FN_NS_GC_STRING_NEW(b.as_ptr(), b.len() as i64) },
        None => 0,
    }
}

/// Libera o handle (no-op se invalido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_FREE(handle: u64) {
    free_handle(handle);
}
