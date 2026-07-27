//! `ffi` namespace — C-string and OS-string interop via std::ffi
//! (CStr/CString/OsStr/OsString).
//!
//! Permite TS lidar com strings C-terminadas (\0) e plataforma-OS, comuns em
//! interop com APIs nativas via `extern "C"`. Handles de string/CString/OsString
//! viajam como u64; o ts dessas funcoes e `number` (handle), nao `string`.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::ffi::{CStr, CString, OsString};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_FROM_PTR(ptr: U64) -> Handle {
    if ptr == 0 {
        return 0;
    }
    // SAFETY: caller contract — ptr aponta para regiao nul-terminada valida.
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    let cow = cstr.to_string_lossy();
    intern(&cow)
}

/// Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_LEN(ptr: U64) -> I64 {
    if ptr == 0 {
        return -1;
    }
    // SAFETY: caller contract.
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    cstr.to_bytes().len() as i64
}

/// Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_TO_STR(ptr: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_NEW(s_ptr: *const u8, s_len: i64) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    match CString::new(s) {
        Ok(c) => alloc_entry(Entry::CString(Box::new(c))),
        Err(_) => 0,
    }
}

/// Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_PTR(handle: U64) -> U64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::CString(c)) => c.as_ptr() as u64,
        _ => 0,
    })
}

/// Releases the CString handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTRING_FREE(handle: U64) {
    free_handle(handle);
}

/// Builds an OsString from a UTF-8 source and returns a handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_FROM_STR(s_ptr: *const u8, s_len: i64) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let os = OsString::from(s);
    alloc_entry(Entry::OsString(Box::new(os)))
}

/// Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_TO_STR(handle: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_OSSTR_FREE(handle: U64) {
    free_handle(handle);
}

/// Função `ffi.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `ffi` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("ffi")
        .doc("C-string and OS-string interop via std::ffi (CStr/CString/OsStr/OsString).")
        .member(func(
            "cstr_from_ptr",
            "__RTS_FN_NS_FFI_CSTR_FROM_PTR",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "cstr_from_ptr(ptr: number): string",
            "Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).",
            __RTS_FN_NS_FFI_CSTR_FROM_PTR as *const u8,
        ))
        .member(func(
            "cstr_len",
            "__RTS_FN_NS_FFI_CSTR_LEN",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "cstr_len(ptr: number): number",
            "Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.",
            __RTS_FN_NS_FFI_CSTR_LEN as *const u8,
        ))
        .member(func(
            "cstr_to_str",
            "__RTS_FN_NS_FFI_CSTR_TO_STR",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "cstr_to_str(ptr: number): string",
            "Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.",
            __RTS_FN_NS_FFI_CSTR_TO_STR as *const u8,
        ))
        .member(func(
            "cstring_new",
            "__RTS_FN_NS_FFI_CSTRING_NEW",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "cstring_new(s: string): number",
            "Builds a nul-terminated CString from `s` and returns a handle. 0 if `s` contains an interior nul.",
            __RTS_FN_NS_FFI_CSTRING_NEW as *const u8,
        ))
        .member(func(
            "cstring_ptr",
            "__RTS_FN_NS_FFI_CSTRING_PTR",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "cstring_ptr(handle: number): number",
            "Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.",
            __RTS_FN_NS_FFI_CSTRING_PTR as *const u8,
        ))
        .member(func(
            "cstring_free",
            "__RTS_FN_NS_FFI_CSTRING_FREE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "cstring_free(handle: number): void",
            "Releases the CString handle.",
            __RTS_FN_NS_FFI_CSTRING_FREE as *const u8,
        ))
        .member(func(
            "osstr_from_str",
            "__RTS_FN_NS_FFI_OSSTR_FROM_STR",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "osstr_from_str(s: string): number",
            "Builds an OsString from a UTF-8 source and returns a handle.",
            __RTS_FN_NS_FFI_OSSTR_FROM_STR as *const u8,
        ))
        .member(func(
            "osstr_to_str",
            "__RTS_FN_NS_FFI_OSSTR_TO_STR",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "osstr_to_str(handle: number): string",
            "Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.",
            __RTS_FN_NS_FFI_OSSTR_TO_STR as *const u8,
        ))
        .member(func(
            "osstr_free",
            "__RTS_FN_NS_FFI_OSSTR_FREE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "osstr_free(handle: number): void",
            "Releases the OsString handle.",
            __RTS_FN_NS_FFI_OSSTR_FREE as *const u8,
        ))
        .done();
}
