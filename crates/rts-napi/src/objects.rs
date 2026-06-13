//! Objetos, arrays e propriedades N-API. Opera direto sobre `Entry::Map`
//! (objeto: chave string → `napi_value` como i64) e `Entry::Vec` (array de
//! `napi_value` como i64). Ver docs/specs/napi-implementation.md (Etapa 7).
//!
//! `napi_value` é um `u64` handle; cabe num `i64` slot do Map/Vec por
//! reinterpretação de bits (não conversão numérica).

use std::ffi::c_char;

use indexmap::IndexMap;
use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_array_expected, napi_invalid_arg, napi_object_expected, napi_ok};

#[inline]
fn val_to_slot(v: napi_value) -> i64 {
    handle_from_value(v) as i64
}

#[inline]
fn slot_to_val(s: i64) -> napi_value {
    value_from_handle(s as u64)
}

unsafe fn cstr_to_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(p as *const u8, len);
        std::str::from_utf8(slice).ok().map(|s| s.to_string())
    }
}

/// Lê uma chave string de um `napi_value` String, ou `None`.
fn key_from_value(v: napi_value) -> Option<String> {
    with_entry(handle_from_value(v), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    })
}

// ── criação ──────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_object(
    _env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array(
    _env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array_with_length(
    _env: napi_env,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Preenche com a sentinela `undefined` (i64::MIN+2) — holes JS.
    let hole = (i64::MIN + 2) as i64;
    let h = alloc_entry(Entry::Vec(Box::new(vec![hole; length])));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

// ── propriedades nomeadas (string C) ─────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_named_property(
    _env: napi_env,
    object: napi_value,
    utf8name: *const c_char,
    value: napi_value,
) -> napi_status {
    let Some(key) = (unsafe { cstr_to_string(utf8name) }) else {
        return napi_invalid_arg;
    };
    let slot = val_to_slot(value);
    let ok = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(key, slot);
            true
        }
        _ => false,
    });
    if ok { napi_ok } else { napi_object_expected }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_named_property(
    _env: napi_env,
    object: napi_value,
    utf8name: *const c_char,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(key) = (unsafe { cstr_to_string(utf8name) }) else {
        return napi_invalid_arg;
    };
    let found = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => Some(m.get(&key).copied()),
        _ => None,
    });
    match found {
        Some(Some(slot)) => {
            unsafe { *result = slot_to_val(slot) };
            napi_ok
        }
        Some(None) => {
            // chave ausente → undefined
            unsafe { *result = value_from_handle((i64::MIN + 2) as u64) };
            napi_ok
        }
        None => napi_object_expected,
    }
}

// ── propriedades por chave napi_value (string) ───────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    value: napi_value,
) -> napi_status {
    let Some(k) = key_from_value(key) else {
        return napi_invalid_arg;
    };
    let slot = val_to_slot(value);
    let ok = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(k, slot);
            true
        }
        _ => false,
    });
    let _ = env;
    if ok { napi_ok } else { napi_object_expected }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(k) = key_from_value(key) else {
        return napi_invalid_arg;
    };
    let found = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => Some(m.get(&k).copied()),
        _ => None,
    });
    match found {
        Some(Some(slot)) => {
            unsafe { *result = slot_to_val(slot) };
            napi_ok
        }
        Some(None) => {
            unsafe { *result = value_from_handle((i64::MIN + 2) as u64) };
            napi_ok
        }
        None => napi_object_expected,
    }
}

// ── elementos de array ───────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    value: napi_value,
) -> napi_status {
    let slot = val_to_slot(value);
    let ok = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Vec(v)) => {
            let idx = index as usize;
            if idx >= v.len() {
                // Cresce preenchendo holes com undefined.
                v.resize(idx + 1, (i64::MIN + 2) as i64);
            }
            v[idx] = slot;
            true
        }
        _ => false,
    });
    if ok { napi_ok } else { napi_array_expected }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let found = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Vec(v)) => Some(v.get(index as usize).copied()),
        _ => None,
    });
    match found {
        Some(Some(slot)) => {
            unsafe { *result = slot_to_val(slot) };
            napi_ok
        }
        Some(None) => {
            unsafe { *result = value_from_handle((i64::MIN + 2) as u64) };
            napi_ok
        }
        None => napi_array_expected,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_array_length(
    _env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let len = with_entry(handle_from_value(value), |e| match e {
        Some(Entry::Vec(v)) => Some(v.len() as u32),
        _ => None,
    });
    match len {
        Some(l) => {
            unsafe { *result = l };
            napi_ok
        }
        None => napi_array_expected,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_array(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let is_arr = with_entry(handle_from_value(value), |e| matches!(e, Some(Entry::Vec(_))));
    unsafe { *result = is_arr };
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{napi_create_double, napi_get_value_double};
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    fn num(n: f64) -> napi_value {
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_double(env(), n, &mut v) };
        v
    }

    #[test]
    fn object_set_get_named() {
        let mut obj = napi_value(ptr::null_mut());
        unsafe { napi_create_object(env(), &mut obj) };
        let key = b"answer\0";
        assert_eq!(
            unsafe { napi_set_named_property(env(), obj, key.as_ptr() as *const c_char, num(42.0)) },
            napi_ok
        );
        let mut got = napi_value(ptr::null_mut());
        unsafe { napi_get_named_property(env(), obj, key.as_ptr() as *const c_char, &mut got) };
        let mut out = 0.0;
        unsafe { napi_get_value_double(env(), got, &mut out) };
        assert_eq!(out, 42.0);
    }

    #[test]
    fn array_set_get_length() {
        let mut arr = napi_value(ptr::null_mut());
        unsafe { napi_create_array(env(), &mut arr) };
        unsafe { napi_set_element(env(), arr, 0, num(10.0)) };
        unsafe { napi_set_element(env(), arr, 2, num(30.0)) }; // cria hole no 1
        let mut len = 0u32;
        unsafe { napi_get_array_length(env(), arr, &mut len) };
        assert_eq!(len, 3);
        let mut e2 = napi_value(ptr::null_mut());
        unsafe { napi_get_element(env(), arr, 2, &mut e2) };
        let mut out = 0.0;
        unsafe { napi_get_value_double(env(), e2, &mut out) };
        assert_eq!(out, 30.0);

        let mut is_arr = false;
        unsafe { napi_is_array(env(), arr, &mut is_arr) };
        assert!(is_arr);
    }

    #[test]
    fn named_property_missing_is_undefined() {
        let mut obj = napi_value(ptr::null_mut());
        unsafe { napi_create_object(env(), &mut obj) };
        let key = b"nope\0";
        let mut got = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_get_named_property(env(), obj, key.as_ptr() as *const c_char, &mut got) },
            napi_ok
        );
        assert_eq!(handle_from_value(got), (i64::MIN + 2) as u64);
    }

    #[test]
    fn set_named_on_non_object_fails() {
        let key = b"x\0";
        assert_eq!(
            unsafe { napi_set_named_property(env(), num(1.0), key.as_ptr() as *const c_char, num(2.0)) },
            napi_object_expected
        );
    }
}
