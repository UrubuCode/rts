//! `ffi` namespace — C-string and OS-string interop via std::ffi
//! (CStr/CString/OsStr/OsString).
//!
//! Permite TS lidar com strings C-terminadas (\0) e plataforma-OS, comuns em
//! interop com APIs nativas via `extern "C"`. Handles de string/CString/OsString
//! viajam como u64; o ts dessas funcoes e `number` (handle), nao `string`.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::ffi::{CStr, CString, OsString};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// C-string and OS-string interop via std::ffi (CStr/CString/OsStr/OsString).
#[rts_namespace(ffi)]
impl FfiNs {
    /// Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).
    #[rts_fn]
    pub fn cstr_from_ptr(ptr: U64) -> Handle {
        if ptr == 0 {
            return 0;
        }
        // SAFETY: caller contract — ptr aponta para regiao nul-terminada valida.
        let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
        let cow = cstr.to_string_lossy();
        intern(&cow)
    }

    /// Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.
    #[rts_fn]
    pub fn cstr_len(ptr: U64) -> I64 {
        if ptr == 0 {
            return -1;
        }
        // SAFETY: caller contract.
        let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
        cstr.to_bytes().len() as i64
    }

    /// Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.
    #[rts_fn]
    pub fn cstr_to_str(ptr: U64) -> Handle {
        if ptr == 0 {
            return 0;
        }
        // SAFETY: caller contract.
        let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
        match cstr.to_str() {
            Ok(s) => intern(s),
            Err(_) => 0,
        }
    }

    /// Builds a nul-terminated CString from `s` and returns a handle. 0 if `s` contains an interior nul.
    #[rts_fn]
    pub fn cstring_new(s: Str) -> Handle {
        match CString::new(s) {
            Ok(c) => alloc_entry(Entry::CString(Box::new(c))),
            Err(_) => 0,
        }
    }

    /// Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.
    #[rts_fn]
    pub fn cstring_ptr(handle: U64) -> U64 {
        with_entry(handle, |entry| match entry {
            Some(Entry::CString(c)) => c.as_ptr() as u64,
            _ => 0,
        })
    }

    /// Releases the CString handle.
    #[rts_fn]
    pub fn cstring_free(handle: U64) {
        free_handle(handle);
    }

    /// Builds an OsString from a UTF-8 source and returns a handle.
    #[rts_fn]
    pub fn osstr_from_str(s: Str) -> Handle {
        let os = OsString::from(s);
        alloc_entry(Entry::OsString(Box::new(os)))
    }

    /// Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.
    #[rts_fn]
    pub fn osstr_to_str(handle: U64) -> Handle {
        // Clone bytes inside with_entry to release the lock before STRING_NEW.
        let bytes: Option<Vec<u8>> = with_entry(handle, |entry| match entry {
            Some(Entry::OsString(os)) => os.to_str().map(|s| s.as_bytes().to_vec()),
            _ => None,
        });
        match bytes {
            Some(b) => unsafe { __RTS_FN_NS_GC_STRING_NEW(b.as_ptr(), b.len() as i64) },
            None => 0,
        }
    }

    /// Releases the OsString handle.
    #[rts_fn]
    pub fn osstr_free(handle: U64) {
        free_handle(handle);
    }
}
