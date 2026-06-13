//! Fase 2d: últimas fns implementáveis sem engine novo — external strings
//! (como strings normais), is_sharedarraybuffer (false), make_callback
//! (síncrono via FUNCTION_CALL). Ver docs/specs/napi-implementation.md.

use std::ffi::{c_char, c_void};

use rts_engine::heap::handles::{alloc_entry, Entry};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

#[cfg(not(test))]
unsafe extern "C" {
    fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64;
}
#[cfg(test)]
unsafe fn __RTS_FN_GL_FUNCTION_CALL(_h: u64, _t: i64, _a: u64) -> i64 {
    0
}

/// External strings: o RTS não tem strings com backing externo (a String é
/// sempre copiada pro pool GC). Tratamos como `create_string_*` normal — o
/// `copied` (se houver) reporta que foi copiada. Semanticamente equivalente
/// para o addon (a string fica válida).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_external_string_latin1(
    env: napi_env,
    str_: *const c_char,
    length: usize,
    _finalize_cb: *mut c_void,
    _finalize_hint: *mut c_void,
    result: *mut napi_value,
    copied: *mut bool,
) -> napi_status {
    let st = unsafe { crate::phase2b::napi_create_string_latin1(env, str_, length, result) };
    if !copied.is_null() {
        unsafe { *copied = true };
    }
    st
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_external_string_utf16(
    env: napi_env,
    str_: *const u16,
    length: usize,
    _finalize_cb: *mut c_void,
    _finalize_hint: *mut c_void,
    result: *mut napi_value,
    copied: *mut bool,
) -> napi_status {
    let st = unsafe { crate::phase2b::napi_create_string_utf16(env, str_, length, result) };
    if !copied.is_null() {
        unsafe { *copied = true };
    }
    st
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_is_sharedarraybuffer(
    _env: napi_env,
    _value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = false };
    napi_ok
}

/// `make_callback`: invoca `func` com `recv` e `argv`, síncrono (sem async
/// context real — o RTS não tem o async context stack do Node). Suficiente para
/// addons que usam make_callback no caminho síncrono.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_make_callback(
    _env: napi_env,
    _async_context: *mut c_void,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    let func_h = handle_from_value(func);
    if func_h == 0 {
        return napi_status::napi_function_expected;
    }
    // Empacota argv num Entry::Vec.
    let mut items: Vec<i64> = Vec::with_capacity(argc);
    if !argv.is_null() {
        for i in 0..argc {
            items.push(handle_from_value(unsafe { *argv.add(i) }) as i64);
        }
    }
    let args_vec = alloc_entry(Entry::Vec(Box::new(items)));
    let ret = unsafe {
        __RTS_FN_GL_FUNCTION_CALL(func_h, handle_from_value(recv) as i64, args_vec)
    };
    if !result.is_null() {
        unsafe { *result = value_from_handle(ret as u64) };
    }
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn external_latin1_is_valid_string() {
        let s = b"hi";
        let mut v = napi_value(ptr::null_mut());
        let mut copied = false;
        unsafe {
            node_api_create_external_string_latin1(
                env(),
                s.as_ptr() as *const c_char,
                s.len(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut v,
                &mut copied,
            )
        };
        assert!(copied);
        // a string deve ser válida
        let mut len = 0usize;
        unsafe {
            crate::strings::napi_get_value_string_utf8(env(), v, ptr::null_mut(), 0, &mut len)
        };
        assert_eq!(len, 2);
    }

    #[test]
    fn shared_arraybuffer_false() {
        let mut b = true;
        let buf = value_from_handle(alloc_entry(Entry::Buffer(vec![1])));
        unsafe { node_api_is_sharedarraybuffer(env(), buf, &mut b) };
        assert!(!b);
    }
}
