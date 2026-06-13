//! Fase 2b: mais fns N-API implementáveis com o engine atual — strings
//! latin1/utf16, error/version helpers, instance-data, wrap/unwrap (via chave
//! reservada no Map), property keys, symbol_for. Ver
//! docs/specs/napi-implementation.md.

use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

use crate::env::{handle_from_value, value_from_handle, RtsNapiEnv};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

// ── strings latin1 / utf16 ───────────────────────────────────────────────────

/// latin1 → UTF-8 (cada byte é um code point U+0000..U+00FF).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_latin1(
    _env: napi_env,
    str_: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let bytes = if str_.is_null() {
        Vec::new()
    } else {
        let len = if length == usize::MAX {
            unsafe {
                let mut n = 0;
                while *str_.add(n) != 0 {
                    n += 1;
                }
                n
            }
        } else {
            length
        };
        let latin1 = unsafe { std::slice::from_raw_parts(str_ as *const u8, len) };
        // cada byte latin1 → char → UTF-8
        latin1.iter().map(|&b| b as char).collect::<String>().into_bytes()
    };
    let h = alloc_entry(Entry::String(bytes));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

/// utf16-le → UTF-8. `length` em code units (u16), `NAPI_AUTO_LENGTH` = até NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf16(
    _env: napi_env,
    str_: *const u16,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let bytes = if str_.is_null() {
        Vec::new()
    } else {
        let len = if length == usize::MAX {
            unsafe {
                let mut n = 0;
                while *str_.add(n) != 0 {
                    n += 1;
                }
                n
            }
        } else {
            length
        };
        let units = unsafe { std::slice::from_raw_parts(str_, len) };
        String::from_utf16_lossy(units).into_bytes()
    };
    let h = alloc_entry(Entry::String(bytes));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

/// utf16: protocolo de 2 passagens (length em code units, sem o NUL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf16(
    _env: napi_env,
    value: napi_value,
    buf: *mut u16,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let s = match with_entry(handle_from_value(value), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    }) {
        Some(s) => s,
        None => return napi_status::napi_string_expected,
    };
    let units: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() {
        if !result.is_null() {
            unsafe { *result = units.len() };
        }
        return napi_ok;
    }
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return napi_ok;
    }
    let n = (bufsize - 1).min(units.len());
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), buf, n);
        *buf.add(n) = 0;
    }
    if !result.is_null() {
        unsafe { *result = n };
    }
    napi_ok
}

/// latin1: 2 passagens (cada char ≤ U+00FF vira 1 byte; demais → '?').
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_latin1(
    _env: napi_env,
    value: napi_value,
    buf: *mut c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let s = match with_entry(handle_from_value(value), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    }) {
        Some(s) => s,
        None => return napi_status::napi_string_expected,
    };
    let latin1: Vec<u8> = s
        .chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect();
    if buf.is_null() {
        if !result.is_null() {
            unsafe { *result = latin1.len() };
        }
        return napi_ok;
    }
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return napi_ok;
    }
    let n = (bufsize - 1).min(latin1.len());
    unsafe {
        std::ptr::copy_nonoverlapping(latin1.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    if !result.is_null() {
        unsafe { *result = n };
    }
    napi_ok
}

/// property keys são strings — reusam create_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_utf8(
    env: napi_env,
    str_: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { crate::strings::napi_create_string_utf8(env, str_, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_latin1(
    env: napi_env,
    str_: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_string_latin1(env, str_, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_utf16(
    env: napi_env,
    str_: *const u16,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_string_utf16(env, str_, length, result) }
}

// ── version / error info ─────────────────────────────────────────────────────

#[repr(C)]
pub struct napi_node_version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub release: *const c_char,
}

/// Wrapper que afirma `Sync` para um struct C com ponteiros imutáveis a strings
/// estáticas (`'static`, read-only) — seguro de compartilhar entre threads.
struct StaticSync<T>(T);
// SAFETY: os ponteiros internos apontam só para literais `c"..."` estáticos,
// nunca mutados.
unsafe impl<T> Sync for StaticSync<T> {}

/// Versão de Node anunciada (fingimos Node 22 LTS, coerente com a N-API 8+).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_node_version(
    _env: napi_env,
    result: *mut *const napi_node_version,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    static VERSION: StaticSync<napi_node_version> = StaticSync(napi_node_version {
        major: 22,
        minor: 0,
        patch: 0,
        release: c"rts".as_ptr(),
    });
    unsafe { *result = &VERSION.0 as *const _ };
    napi_ok
}

#[repr(C)]
pub struct napi_extended_error_info {
    pub error_message: *const c_char,
    pub engine_reserved: *mut c_void,
    pub engine_error_code: u32,
    pub error_code: napi_status,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_last_error_info(
    _env: napi_env,
    result: *mut *const napi_extended_error_info,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Fase 2: sempre reporta "ok / sem erro detalhado". Suficiente para addons
    // que só logam quando uma chamada falha.
    static INFO: StaticSync<napi_extended_error_info> = StaticSync(napi_extended_error_info {
        error_message: c"no extended error info".as_ptr(),
        engine_reserved: std::ptr::null_mut(),
        engine_error_code: 0,
        error_code: napi_status::napi_ok,
    });
    unsafe { *result = &INFO.0 as *const _ };
    napi_ok
}

// ── instance data (por env) ──────────────────────────────────────────────────
// O addon guarda um ponteiro global por instância. Como o RtsNapiEnv vive pelo
// processo (loader leak), guardamos num slot global simples.

static INSTANCE_DATA: Mutex<usize> = Mutex::new(0);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_instance_data(
    _env: napi_env,
    data: *mut c_void,
    _finalize_cb: *mut c_void,
    _finalize_hint: *mut c_void,
) -> napi_status {
    *INSTANCE_DATA.lock().unwrap_or_else(|e| e.into_inner()) = data as usize;
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_instance_data(
    _env: napi_env,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let d = *INSTANCE_DATA.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { *result = d as *mut c_void };
    napi_ok
}

// ── wrap / unwrap (via chave reservada no Map) ───────────────────────────────
// Associa um ponteiro nativo a um objeto JS. Guardamos o ptr como i64 numa
// chave reservada que não colide com props normais. Limitação: o finalizer
// não dispara automaticamente (precisa do hook de engine — follow-up Drysius);
// o ptr vive enquanto o Map viver.

const WRAP_KEY: &str = "__napi_wrap_ptr__";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_wrap(
    _env: napi_env,
    js_object: napi_value,
    native_object: *mut c_void,
    _finalize_cb: *mut c_void,
    _finalize_hint: *mut c_void,
    _result: *mut c_void,
) -> napi_status {
    let ok = with_entry_mut(handle_from_value(js_object), |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(WRAP_KEY.to_string(), native_object as i64);
            true
        }
        _ => false,
    });
    if ok { napi_ok } else { napi_status::napi_object_expected }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unwrap(
    _env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let ptr = with_entry(handle_from_value(js_object), |e| match e {
        Some(Entry::Map(m)) => m.get(WRAP_KEY).map(|&v| v as *mut c_void),
        _ => None,
    });
    match ptr {
        Some(p) => {
            unsafe { *result = p };
            napi_ok
        }
        None => napi_invalid_arg,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_wrap(
    _env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    let ptr = with_entry_mut(handle_from_value(js_object), |e| match e {
        Some(Entry::Map(m)) => m.shift_remove(WRAP_KEY).map(|v| v as *mut c_void),
        _ => None,
    });
    if !result.is_null() {
        unsafe { *result = ptr.unwrap_or(std::ptr::null_mut()) };
    }
    napi_ok
}

// ── get_new_target (Fase 2: sempre null — sem construtor real ainda) ─────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_new_target(
    _env: napi_env,
    _cbinfo: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Sem suporte a `new` em fns N-API ainda → new.target = undefined.
    unsafe { *result = value_from_handle((i64::MIN + 2) as u64) };
    napi_ok
}

/// symbol_for: registry global de symbols por chave (string C).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_symbol_for(
    _env: napi_env,
    utf8description: *const c_char,
    _length: usize,
    result: *mut napi_value,
) -> napi_status {
    use std::collections::HashMap;
    static REGISTRY: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);
    if result.is_null() {
        return napi_invalid_arg;
    }
    let key = if utf8description.is_null() {
        String::new()
    } else {
        unsafe {
            let mut len = 0;
            while *utf8description.add(len) != 0 {
                len += 1;
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(
                utf8description as *const u8,
                len,
            ))
            .into_owned()
        }
    };
    let mut guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let h = *map.entry(key.clone()).or_insert_with(|| {
        alloc_entry(Entry::Symbol {
            description: Some(key),
        })
    });
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

// `RtsNapiEnv` referenciado para coerência (env opaco). Suprime unused.
#[allow(unused_imports)]
use RtsNapiEnv as _Env;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn latin1_roundtrip() {
        let s = b"caf\xe9"; // "café" em latin1 (é = 0xE9)
        let mut v = napi_value(ptr::null_mut());
        unsafe {
            napi_create_string_latin1(env(), s.as_ptr() as *const c_char, s.len(), &mut v)
        };
        // lê de volta como utf8: deve ser "café" (5 bytes UTF-8)
        let mut len = 0usize;
        unsafe {
            crate::strings::napi_get_value_string_utf8(env(), v, ptr::null_mut(), 0, &mut len)
        };
        assert_eq!(len, 5);
    }

    #[test]
    fn utf16_roundtrip() {
        let units: Vec<u16> = "hi".encode_utf16().collect();
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_string_utf16(env(), units.as_ptr(), units.len(), &mut v) };
        let mut buf = [0u16; 8];
        let mut len = 0usize;
        unsafe { napi_get_value_string_utf16(env(), v, buf.as_mut_ptr(), 8, &mut len) };
        assert_eq!(len, 2);
        assert_eq!(String::from_utf16_lossy(&buf[..len]), "hi");
    }

    #[test]
    fn wrap_unwrap() {
        let obj = value_from_handle(alloc_entry(Entry::Map(Box::new(
            indexmap::IndexMap::new(),
        ))));
        let native = 0xCAFE_usize as *mut c_void;
        assert_eq!(
            unsafe {
                napi_wrap(env(), obj, native, ptr::null_mut(), ptr::null_mut(), ptr::null_mut())
            },
            napi_ok
        );
        let mut out: *mut c_void = ptr::null_mut();
        unsafe { napi_unwrap(env(), obj, &mut out) };
        assert_eq!(out, native);
        unsafe { napi_remove_wrap(env(), obj, &mut out) };
        assert_eq!(out, native);
        // após remove, unwrap falha
        assert_eq!(
            unsafe { napi_unwrap(env(), obj, &mut out) },
            napi_invalid_arg
        );
    }

    #[test]
    fn instance_data_roundtrip() {
        let d = 0x1234_usize as *mut c_void;
        unsafe { napi_set_instance_data(env(), d, ptr::null_mut(), ptr::null_mut()) };
        let mut out: *mut c_void = ptr::null_mut();
        unsafe { napi_get_instance_data(env(), &mut out) };
        assert_eq!(out, d);
    }

    #[test]
    fn node_version() {
        let mut v: *const napi_node_version = ptr::null();
        unsafe { napi_get_node_version(env(), &mut v) };
        assert!(!v.is_null());
        assert_eq!(unsafe { (*v).major }, 22);
    }
}
