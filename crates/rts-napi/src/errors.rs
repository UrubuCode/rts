//! Erros e exceções N-API. Mapeiam a um slot de exceção pendente no
//! `RtsNapiEnv` (per-instância, síncrono — Fase 1). Ver
//! docs/specs/napi-implementation.md (Etapa 10).
//!
//! Distinção de assinatura importante:
//! - `napi_throw_*` recebem `msg: *const c_char` (C string crua) e SETAM o slot.
//! - `napi_create_*` recebem `msg: napi_value` (handle String) e NÃO setam o
//!   slot (só constroem o objeto Error e o devolvem).

use std::ffi::c_char;

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use crate::env::{handle_from_value, value_from_handle, RtsNapiEnv};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(p as *const u8, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

fn string_of(value: napi_value) -> Option<String> {
    with_entry(handle_from_value(value), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    })
}

/// Aloca um `Entry::ErrorObj` e devolve o handle.
fn make_error(name: &str, message: String) -> u64 {
    alloc_entry(Entry::ErrorObj {
        message,
        name: name.to_string(),
        cause: 0,
    })
}

// ── throw (setam o slot pendente) ────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw(env: napi_env, error: napi_value) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    e.pending_exception = handle_from_value(error);
    napi_ok
}

unsafe fn throw_named(env: napi_env, name: &str, _code: *const c_char, msg: *const c_char) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let h = make_error(name, unsafe { cstr(msg) });
    e.pending_exception = h;
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_named(env, "Error", code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_type_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_named(env, "TypeError", code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_range_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_named(env, "RangeError", code, msg) }
}

// ── create (NÃO setam o slot; só constroem o Error) ──────────────────────────

unsafe fn create_named(
    name: &str,
    _code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let message = string_of(msg).unwrap_or_default();
    let h = make_error(name, message);
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_error(
    _env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_named("Error", code, msg, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_type_error(
    _env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_named("TypeError", code, msg, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_range_error(
    _env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_named("RangeError", code, msg, result) }
}

// ── consulta / limpeza do slot pendente ──────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_exception_pending(
    env: napi_env,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    unsafe { *result = e.pending_exception != 0 };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_and_clear_last_exception(
    env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let h = e.pending_exception;
    e.pending_exception = 0;
    // Sem exceção → undefined.
    let out = if h != 0 { h } else { (i64::MIN + 2) as u64 };
    unsafe { *result = value_from_handle(out) };
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn make_env() -> napi_env {
        Box::new(RtsNapiEnv::new(8)).into_raw()
    }

    #[test]
    fn throw_then_pending_then_clear() {
        let env = make_env();
        let code = b"ERR_X\0";
        let msg = b"algo deu errado\0";
        assert_eq!(
            unsafe {
                napi_throw_type_error(
                    env,
                    code.as_ptr() as *const c_char,
                    msg.as_ptr() as *const c_char,
                )
            },
            napi_ok
        );
        // pending = true
        let mut pending = false;
        unsafe { napi_is_exception_pending(env, &mut pending) };
        assert!(pending);
        // get_and_clear devolve o ErrorObj com name TypeError
        let mut err = napi_value(ptr::null_mut());
        unsafe { napi_get_and_clear_last_exception(env, &mut err) };
        with_entry(handle_from_value(err), |e| {
            assert!(matches!(e, Some(Entry::ErrorObj { name, message, .. })
                if name == "TypeError" && message == "algo deu errado"));
        });
        // depois de limpar, não há mais pendência
        unsafe { napi_is_exception_pending(env, &mut pending) };
        assert!(!pending);
    }

    #[test]
    fn create_does_not_set_pending() {
        let env = make_env();
        // cria uma String para a mensagem
        let mut msg = napi_value(ptr::null_mut());
        let s = b"oops";
        unsafe {
            crate::strings::napi_create_string_utf8(
                env,
                s.as_ptr() as *const c_char,
                s.len(),
                &mut msg,
            )
        };
        let mut err = napi_value(ptr::null_mut());
        let code = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_create_range_error(env, code, msg, &mut err) },
            napi_ok
        );
        // create NÃO seta o slot pendente
        let mut pending = true;
        unsafe { napi_is_exception_pending(env, &mut pending) };
        assert!(!pending);
        with_entry(handle_from_value(err), |e| {
            assert!(matches!(e, Some(Entry::ErrorObj { name, .. }) if name == "RangeError"));
        });
    }

    #[test]
    fn clear_with_no_exception_returns_undefined() {
        let env = make_env();
        let mut err = napi_value(ptr::null_mut());
        unsafe { napi_get_and_clear_last_exception(env, &mut err) };
        assert_eq!(handle_from_value(err), (i64::MIN + 2) as u64);
    }
}
