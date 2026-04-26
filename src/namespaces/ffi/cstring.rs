//! CString — buffer nul-terminado proprio, gerenciado via HandleTable.

use std::ffi::CString;

use super::super::gc::handles::{Entry, table};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Constroi CString a partir de string TS. 0 se contiver nul interior
/// ou input invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_NEW(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    match CString::new(s) {
        Ok(c) => table()
            .lock()
            .unwrap()
            .alloc(Entry::CString(Box::new(c))),
        Err(_) => 0,
    }
}

/// Ponteiro raw para os bytes da CString (terminados em \0).
/// 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_PTR(handle: u64) -> u64 {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::CString(c)) => c.as_ptr() as u64,
        _ => 0,
    }
}

/// Libera o handle (no-op se invalido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_FREE(handle: u64) {
    table().lock().unwrap().free(handle);
}
