//! Get/set/remove de variaveis de ambiente via std::env.
//!
//! Retornos de `get_var` sao handles de string (via `gc::string_pool`)
//! para coerencia com o resto do ABI — 0 sinaliza "variavel nao existe".
//! Strings dinamicas precisam ser freed via `gc.string_free`.

use std::env;

// Bridge para string_pool do gc sem depender do path cross-crate.
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract — UTF-8 valido cobrindo `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Retorna handle de string com o valor da variavel, ou 0 se nao existe.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_GET_VAR(name_ptr: *const u8, name_len: i64) -> u64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    match env::var(name) {
        Ok(value) => unsafe { __RTS_FN_NS_GC_STRING_NEW(value.as_ptr(), value.len() as i64) },
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_SET_VAR(
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return;
    };
    let Some(value) = str_from_abi(value_ptr, value_len) else {
        return;
    };
    // SAFETY: std::env::set_var e marcada unsafe no Rust 2024 porque
    // modifica estado global; o RTS e single-threaded por construcao
    // no run path.
    unsafe { env::set_var(name, value) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_REMOVE_VAR(name_ptr: *const u8, name_len: i64) {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return;
    };
    // SAFETY: mesma justificativa de set_var.
    unsafe { env::remove_var(name) };
}
