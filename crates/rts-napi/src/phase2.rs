//! Fase 2: fns N-API implementáveis com o engine atual (sem depender de novas
//! APIs do motor). Type checks, property checks, coerce, Date, Symbol, Buffer,
//! BigInt read. Substituem os stubs correspondentes de `surface.rs` (os stubs
//! são removidos da lista de symbols ao mover pra cá — o linker resolve a impl
//! real). Ver docs/specs/napi-implementation.md.

use std::ffi::{c_char, c_void};

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

const UNDEFINED: u64 = (i64::MIN + 2) as u64;

unsafe fn write_bool(result: *mut bool, v: bool) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = v };
    napi_ok
}

unsafe fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        std::str::from_utf8(std::slice::from_raw_parts(p as *const u8, len))
            .ok()
            .map(|s| s.to_string())
    }
}

// ── type checks ──────────────────────────────────────────────────────────────

macro_rules! is_variant {
    ($name:ident, $pat:pat) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            _env: napi_env,
            value: napi_value,
            result: *mut bool,
        ) -> napi_status {
            let is = with_entry(handle_from_value(value), |e| matches!(e, Some($pat)));
            unsafe { write_bool(result, is) }
        }
    };
}

is_variant!(napi_is_buffer, Entry::Buffer(_));
is_variant!(napi_is_date, Entry::DateMs(_));
is_variant!(napi_is_error, Entry::ErrorObj { .. });

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_promise(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let is = with_entry(handle_from_value(value), |e| {
        matches!(e, Some(Entry::Promise(_)) | Some(Entry::PromiseAsync(_)))
    });
    unsafe { write_bool(result, is) }
}

// ── property checks (sobre Entry::Map) ───────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_named_property(
    _env: napi_env,
    object: napi_value,
    utf8name: *const c_char,
    result: *mut bool,
) -> napi_status {
    let Some(key) = (unsafe { cstr(utf8name) }) else {
        return napi_invalid_arg;
    };
    let has = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => m.contains_key(&key),
        _ => false,
    });
    unsafe { write_bool(result, has) }
}

fn key_of(v: napi_value) -> Option<String> {
    with_entry(handle_from_value(v), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    let Some(k) = key_of(key) else {
        return napi_invalid_arg;
    };
    let has = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => m.contains_key(&k),
        _ => false,
    });
    unsafe { write_bool(result, has) }
}

/// `has_own_property` — para um Map plano, igual a `has_property` (sem cadeia de
/// protótipo modelada aqui).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_own_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    unsafe { napi_has_property(env, object, key, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    let Some(k) = key_of(key) else {
        return napi_invalid_arg;
    };
    let removed = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => m.shift_remove(&k).is_some(),
        _ => false,
    });
    if result.is_null() {
        return napi_ok;
    }
    unsafe { *result = removed };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    let has = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Vec(v)) => (index as usize) < v.len(),
        _ => false,
    });
    unsafe { write_bool(result, has) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    let removed = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Vec(v)) => {
            let i = index as usize;
            if i < v.len() {
                v[i] = UNDEFINED as i64; // JS delete deixa hole (undefined)
                true
            } else {
                false
            }
        }
        _ => false,
    });
    if result.is_null() {
        return napi_ok;
    }
    unsafe { *result = removed };
    napi_ok
}

/// Lista de chaves próprias do objeto, como um array de strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property_names(
    _env: napi_env,
    object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let keys: Option<Vec<String>> = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => Some(m.keys().cloned().collect()),
        _ => None,
    });
    let Some(keys) = keys else {
        return napi_status::napi_object_expected;
    };
    let items: Vec<i64> = keys
        .into_iter()
        .map(|k| alloc_entry(Entry::String(k.into_bytes())) as i64)
        .collect();
    let arr = alloc_entry(Entry::Vec(Box::new(items)));
    unsafe { *result = value_from_handle(arr) };
    napi_ok
}

/// `get_all_property_names` (versão com flags) — Fase 2 trata como
/// `get_property_names` (ignora os filtros key_mode/conversion).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_all_property_names(
    env: napi_env,
    object: napi_value,
    _key_mode: i32,
    _key_filter: i32,
    _key_conversion: i32,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_get_property_names(env, object, result) }
}

/// `strict_equals` (===): identidade de handle para objetos; valor para
/// primitivos boxados (FloatPrim) e sentinelas.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_strict_equals(
    _env: napi_env,
    lhs: napi_value,
    rhs: napi_value,
    result: *mut bool,
) -> napi_status {
    let a = handle_from_value(lhs);
    let b = handle_from_value(rhs);
    let eq = if a == b {
        true
    } else {
        // Números boxados: compara o f64.
        let fa = with_entry(a, |e| match e {
            Some(Entry::FloatPrim(f)) | Some(Entry::NumberBox(f)) => Some(*f),
            _ => None,
        });
        let fb = with_entry(b, |e| match e {
            Some(Entry::FloatPrim(f)) | Some(Entry::NumberBox(f)) => Some(*f),
            _ => None,
        });
        match (fa, fb) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    };
    unsafe { write_bool(result, eq) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_freeze(
    _env: napi_env,
    _object: napi_value,
) -> napi_status {
    // Fase 2: no-op (RTS não modela frozen no Entry::Map ainda). Aceita p/ não
    // quebrar addons que chamam por higiene.
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_seal(_env: napi_env, _object: napi_value) -> napi_status {
    napi_ok
}

// ── coerce ───────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_bool(
    _env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = handle_from_value(value);
    let truthy = coerce_truthy(h);
    let out = if truthy { i64::MIN + 1 } else { i64::MIN } as u64;
    unsafe { *result = value_from_handle(out) };
    napi_ok
}

fn coerce_truthy(h: u64) -> bool {
    match h {
        x if x == (i64::MIN) as u64 => false,       // false
        x if x == (i64::MIN + 1) as u64 => true,    // true
        x if x == (i64::MIN + 2) as u64 => false,   // undefined
        x if x == (i64::MIN + 3) as u64 => false,   // null
        0 => false,
        _ => with_entry(h, |e| match e {
            Some(Entry::FloatPrim(f)) | Some(Entry::NumberBox(f)) => *f != 0.0 && !f.is_nan(),
            Some(Entry::String(b)) => !b.is_empty(),
            Some(_) => true, // objetos/arrays/fns → truthy
            None => false,
        }),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_number(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = handle_from_value(value);
    let n = coerce_number(h);
    // Boxa o número resultante.
    unsafe {
        let mut v = napi_value(std::ptr::null_mut());
        crate::values::napi_create_double(env, n, &mut v);
        *result = v;
    }
    napi_ok
}

fn coerce_number(h: u64) -> f64 {
    match h {
        x if x == (i64::MIN) as u64 => 0.0,
        x if x == (i64::MIN + 1) as u64 => 1.0,
        x if x == (i64::MIN + 3) as u64 => 0.0, // null → 0
        _ => with_entry(h, |e| match e {
            Some(Entry::FloatPrim(f)) | Some(Entry::NumberBox(f)) => *f,
            Some(Entry::String(b)) => std::str::from_utf8(b)
                .ok()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(f64::NAN),
            _ => f64::NAN, // undefined/objeto → NaN
        }),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_object(
    _env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Se já é um objeto (Map/Vec), passa direto; senão Fase 2 devolve o próprio
    // valor (sem wrapper boxing completo).
    unsafe { *result = value };
    napi_ok
}

// ── Date (Entry::DateMs) ─────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_date(
    _env: napi_env,
    time: f64,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = alloc_entry(Entry::DateMs(time as i64));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_date_value(
    _env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let ms = with_entry(handle_from_value(value), |e| match e {
        Some(Entry::DateMs(ms)) => Some(*ms as f64),
        _ => None,
    });
    match ms {
        Some(m) => {
            unsafe { *result = m };
            napi_ok
        }
        None => napi_status::napi_date_expected,
    }
}

// ── Symbol (Entry::Symbol) ───────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_symbol(
    _env: napi_env,
    description: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let desc = with_entry(handle_from_value(description), |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    });
    let h = alloc_entry(Entry::Symbol { description: desc });
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

// ── Buffer (Entry::Buffer) ───────────────────────────────────────────────────

/// Cria um Buffer de `length` bytes (zerados) e devolve um ponteiro pros dados.
/// **Limitação Fase 2:** o ponteiro é estável só enquanto o handle não é movido;
/// o `Vec<u8>` interno não realoca após criado (sem push). Para
/// `external_buffer` com ponteiro garantido-estável, ver follow-up de engine
/// (Entry::ArrayBuffer com ptr estável).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer(
    _env: napi_env,
    length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = alloc_entry(Entry::Buffer(vec![0u8; length]));
    // Devolve o ponteiro pros bytes (estável enquanto o Vec não realocar).
    if !data.is_null() {
        let ptr = with_entry_mut(h, |e| match e {
            Some(Entry::Buffer(b)) => b.as_mut_ptr() as *mut c_void,
            _ => std::ptr::null_mut(),
        });
        unsafe { *data = ptr };
    }
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_buffer_info(
    _env: napi_env,
    value: napi_value,
    data: *mut *mut c_void,
    length: *mut usize,
) -> napi_status {
    let info = with_entry_mut(handle_from_value(value), |e| match e {
        Some(Entry::Buffer(b)) => Some((b.as_mut_ptr() as *mut c_void, b.len())),
        _ => None,
    });
    match info {
        Some((ptr, len)) => {
            if !data.is_null() {
                unsafe { *data = ptr };
            }
            if !length.is_null() {
                unsafe { *length = len };
            }
            napi_ok
        }
        None => napi_invalid_arg,
    }
}

/// `create_buffer_copy`: copia `length` bytes de `data` num novo Buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer_copy(
    _env: napi_env,
    length: usize,
    data: *const c_void,
    result_data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let bytes = if data.is_null() {
        vec![0u8; length]
    } else {
        unsafe { std::slice::from_raw_parts(data as *const u8, length).to_vec() }
    };
    let h = alloc_entry(Entry::Buffer(bytes));
    if !result_data.is_null() {
        let ptr = with_entry_mut(h, |e| match e {
            Some(Entry::Buffer(b)) => b.as_mut_ptr() as *mut c_void,
            _ => std::ptr::null_mut(),
        });
        unsafe { *result_data = ptr };
    }
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

// BigInt migrou para bigint.rs (Entry::BigInt real, #219).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::napi_create_double;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }
    fn dbl(n: f64) -> napi_value {
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_double(env(), n, &mut v) };
        v
    }

    #[test]
    fn type_checks() {
        let mut b = false;
        let buf = value_from_handle(alloc_entry(Entry::Buffer(vec![1, 2, 3])));
        unsafe { napi_is_buffer(env(), buf, &mut b) };
        assert!(b);
        unsafe { napi_is_buffer(env(), dbl(1.0), &mut b) };
        assert!(!b);

        let date = value_from_handle(alloc_entry(Entry::DateMs(1000)));
        unsafe { napi_is_date(env(), date, &mut b) };
        assert!(b);
    }

    #[test]
    fn buffer_roundtrip() {
        let mut data: *mut c_void = ptr::null_mut();
        let mut buf = napi_value(ptr::null_mut());
        unsafe { napi_create_buffer(env(), 4, &mut data, &mut buf) };
        assert!(!data.is_null());
        // Escreve nos bytes via o ponteiro.
        unsafe {
            let p = data as *mut u8;
            *p = 0xAB;
            *p.add(1) = 0xCD;
        }
        // Lê de volta via get_buffer_info.
        let mut d2: *mut c_void = ptr::null_mut();
        let mut len = 0usize;
        unsafe { napi_get_buffer_info(env(), buf, &mut d2, &mut len) };
        assert_eq!(len, 4);
        unsafe {
            assert_eq!(*(d2 as *const u8), 0xAB);
            assert_eq!(*(d2 as *const u8).add(1), 0xCD);
        }
    }

    #[test]
    fn coerce_number_and_bool() {
        let mut out = napi_value(ptr::null_mut());
        // string "42" → 42
        let s = value_from_handle(alloc_entry(Entry::String(b"42".to_vec())));
        unsafe { napi_coerce_to_number(env(), s, &mut out) };
        let mut n = 0.0;
        unsafe { crate::values::napi_get_value_double(env(), out, &mut n) };
        assert_eq!(n, 42.0);

        // 0 → false
        let mut bv = napi_value(ptr::null_mut());
        unsafe { napi_coerce_to_bool(env(), dbl(0.0), &mut bv) };
        assert_eq!(handle_from_value(bv), i64::MIN as u64);
    }

    #[test]
    fn property_has_delete() {
        let obj = value_from_handle(alloc_entry(Entry::Map(Box::new(
            indexmap::IndexMap::new(),
        ))));
        let key = value_from_handle(alloc_entry(Entry::String(b"k".to_vec())));
        // set via objects
        unsafe { crate::objects::napi_set_property(env(), obj, key, dbl(9.0)) };
        let mut has = false;
        unsafe { napi_has_property(env(), obj, key, &mut has) };
        assert!(has);
        let mut removed = false;
        unsafe { napi_delete_property(env(), obj, key, &mut removed) };
        assert!(removed);
        unsafe { napi_has_property(env(), obj, key, &mut has) };
        assert!(!has);
    }

    #[test]
    fn date_roundtrip() {
        let mut d = napi_value(ptr::null_mut());
        unsafe { napi_create_date(env(), 1234.0, &mut d) };
        let mut is_d = false;
        unsafe { napi_is_date(env(), d, &mut is_d) };
        assert!(is_d);
        let mut ms = 0.0;
        unsafe { napi_get_date_value(env(), d, &mut ms) };
        assert_eq!(ms, 1234.0);
    }
}
