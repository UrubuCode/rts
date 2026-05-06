//! Operacoes sobre CStr — visao raw de string nul-terminada vinda de FFI.
//!
//! Recebe ponteiro u64 que o caller obteve via `cstring.ptr()` ou retorno
//! de funcao C externa. Le ate o primeiro `\0`. Como nao e possivel
//! validar memoria do caller, o contrato e: `ptr` deve ser nul-terminado
//! e dentro de regiao valida.

use std::ffi::CStr;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Le CStr em `ptr` e devolve string handle UTF-8 (lossy — bytes invalidos
/// viram U+FFFD). 0 se `ptr` nulo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_FROM_PTR(ptr: u64) -> u64 {
    if ptr == 0 {
        return 0;
    }
    // SAFETY: caller contract — ptr aponta para uma regiao nul-terminada
    // valida.
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    let cow = cstr.to_string_lossy();
    let bytes = cow.as_bytes();
    unsafe { __RTS_FN_NS_GC_STRING_NEW(bytes.as_ptr(), bytes.len() as i64) }
}

/// Bytes ate o nul terminator. -1 se ptr nulo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_LEN(ptr: u64) -> i64 {
    if ptr == 0 {
        return -1;
    }
    // SAFETY: caller contract.
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    cstr.to_bytes().len() as i64
}

/// Tenta validar como UTF-8 estrito; retorna handle de string ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FFI_CSTR_TO_STR(ptr: u64) -> u64 {
    if ptr == 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    match cstr.to_str() {
        Ok(s) => unsafe {
            __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
        },
        Err(_) => 0,
    }
}
