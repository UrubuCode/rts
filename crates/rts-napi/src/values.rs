//! Marshalling escalar `napi_value` ↔ handle/sentinela + `napi_typeof`.
//!
//! Invariantes (ver docs/specs/napi-implementation.md):
//! - `napi_value` é SEMPRE um handle vivo da `HandleTable` OU uma das 5
//!   sentinelas JS (`i64::MIN..=MIN+4`). Nunca um i64 escalar cru de valor.
//! - Todo número é **sempre boxed** em `Entry::FloatPrim(f64)` — nunca inline —
//!   para ter identidade estável e ser GC-rastreável dentro do frame nativo
//!   opaco do addon.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value, napi_valuetype};

use napi_status::{napi_invalid_arg, napi_number_expected, napi_ok};

// ── Sentinelas JS (espelham o runtime: string_pool.rs) ───────────────────────
/// `false`.
pub const SENTINEL_FALSE: u64 = i64::MIN as u64;
/// `true`.
pub const SENTINEL_TRUE: u64 = (i64::MIN + 1) as u64;
/// `undefined`.
pub const SENTINEL_UNDEFINED: u64 = (i64::MIN + 2) as u64;
/// `null`.
pub const SENTINEL_NULL: u64 = (i64::MIN + 3) as u64;

/// `true` se o handle é uma das sentinelas JS (não decodifica para slot).
#[inline]
pub fn is_sentinel(h: u64) -> bool {
    h == SENTINEL_FALSE || h == SENTINEL_TRUE || h == SENTINEL_UNDEFINED || h == SENTINEL_NULL
}

/// Boxa um f64 num `Entry::FloatPrim` e devolve o `napi_value`.
#[inline]
fn box_number(value: f64) -> napi_value {
    value_from_handle(alloc_entry(Entry::FloatPrim(value)))
}

/// Escreve `value` em `*result` (helper comum de out-param), tratando ptr nulo.
#[inline]
unsafe fn write_result(result: *mut napi_value, value: napi_value) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = value };
    napi_ok
}

// ── criação de valores ───────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_double(
    _env: napi_env,
    value: f64,
    result: *mut napi_value,
) -> napi_status {
    unsafe { write_result(result, box_number(value)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int32(
    _env: napi_env,
    value: i32,
    result: *mut napi_value,
) -> napi_status {
    unsafe { write_result(result, box_number(value as f64)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_uint32(
    _env: napi_env,
    value: u32,
    result: *mut napi_value,
) -> napi_status {
    unsafe { write_result(result, box_number(value as f64)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int64(
    _env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    // JS number é f64; int64 fora de ±2^53 perde precisão (comportamento N-API).
    unsafe { write_result(result, box_number(value as f64)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_boolean(
    _env: napi_env,
    value: bool,
    result: *mut napi_value,
) -> napi_status {
    let h = if value { SENTINEL_TRUE } else { SENTINEL_FALSE };
    unsafe { write_result(result, value_from_handle(h)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_undefined(
    _env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    unsafe { write_result(result, value_from_handle(SENTINEL_UNDEFINED)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_null(_env: napi_env, result: *mut napi_value) -> napi_status {
    unsafe { write_result(result, value_from_handle(SENTINEL_NULL)) }
}

// ── extração de valores ──────────────────────────────────────────────────────

/// Lê o f64 de um `napi_value`. Aceita `FloatPrim` (número boxed) e os ints
/// inline (raro num napi_value, mas defensivo). Sentinela bool → 0/1.
fn read_number(value: napi_value) -> Option<f64> {
    let h = handle_from_value(value);
    if h == SENTINEL_TRUE {
        return Some(1.0);
    }
    if h == SENTINEL_FALSE {
        return Some(0.0);
    }
    if is_sentinel(h) {
        return None;
    }
    with_entry(h, |e| match e {
        Some(Entry::FloatPrim(f)) => Some(*f),
        Some(Entry::NumberBox(f)) => Some(*f),
        _ => None,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_double(
    _env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    let Some(n) = read_number(value) else {
        return napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = n };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int32(
    _env: napi_env,
    value: napi_value,
    result: *mut i32,
) -> napi_status {
    let Some(n) = read_number(value) else {
        return napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // ToInt32 do JS: trunca para i64 e pega os 32 bits baixos.
    unsafe { *result = js_to_int32(n) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_uint32(
    _env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    let Some(n) = read_number(value) else {
        return napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = js_to_int32(n) as u32 };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int64(
    _env: napi_env,
    value: napi_value,
    result: *mut i64,
) -> napi_status {
    let Some(n) = read_number(value) else {
        return napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = if n.is_finite() { n as i64 } else { 0 } };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bool(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let h = handle_from_value(value);
    let b = match h {
        SENTINEL_TRUE => true,
        SENTINEL_FALSE => false,
        _ => return napi_status::napi_boolean_expected,
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = b };
    napi_ok
}

/// ToInt32 do ECMAScript (truncamento + wrap em 2^32).
fn js_to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let t = n.trunc();
    // Reduz módulo 2^32 e reinterpreta como i32.
    let m = (t.rem_euclid(4294967296.0)) as u32;
    m as i32
}

// ── typeof ───────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_typeof(
    _env: napi_env,
    value: napi_value,
    result: *mut napi_valuetype,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = handle_from_value(value);
    let ty = classify(h);
    unsafe { *result = ty };
    napi_ok
}

/// Classifica um handle/sentinela em `napi_valuetype`.
fn classify(h: u64) -> napi_valuetype {
    use napi_valuetype::*;
    match h {
        SENTINEL_TRUE | SENTINEL_FALSE => return napi_boolean,
        SENTINEL_NULL => return napi_null,
        SENTINEL_UNDEFINED => return napi_undefined,
        0 => return napi_undefined, // null-ish handle inválido
        _ => {}
    }
    with_entry(h, |e| match e {
        Some(Entry::FloatPrim(_)) | Some(Entry::NumberBox(_)) => napi_number,
        Some(Entry::String(_)) | Some(Entry::StringBox(_)) => napi_string,
        Some(Entry::Function(_)) => napi_function,
        Some(Entry::Symbol { .. }) => napi_symbol,
        Some(Entry::NapiExternal(_)) => napi_external,
        Some(_) => napi_object, // Map/Vec/etc → object
        None => napi_undefined,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn double_roundtrip() {
        let mut v = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_create_double(env(), 3.14, &mut v) },
            napi_ok
        );
        let mut out = 0.0;
        assert_eq!(
            unsafe { napi_get_value_double(env(), v, &mut out) },
            napi_ok
        );
        assert_eq!(out, 3.14);
        // typeof → number
        let mut t = napi_valuetype::napi_undefined;
        assert_eq!(unsafe { napi_typeof(env(), v, &mut t) }, napi_ok);
        assert_eq!(t, napi_valuetype::napi_number);
    }

    #[test]
    fn int32_truncation() {
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_double(env(), 4294967298.5, &mut v) }; // 2^32 + 2.5
        let mut out = 0i32;
        unsafe { napi_get_value_int32(env(), v, &mut out) };
        assert_eq!(out, 2); // ToInt32(2^32 + 2.5) = 2
    }

    #[test]
    fn bool_and_null_undefined() {
        let mut t = napi_value(ptr::null_mut());
        unsafe { napi_get_boolean(env(), true, &mut t) };
        let mut b = false;
        assert_eq!(unsafe { napi_get_value_bool(env(), t, &mut b) }, napi_ok);
        assert!(b);

        let mut u = napi_value(ptr::null_mut());
        unsafe { napi_get_undefined(env(), &mut u) };
        let mut ty = napi_valuetype::napi_number;
        unsafe { napi_typeof(env(), u, &mut ty) };
        assert_eq!(ty, napi_valuetype::napi_undefined);

        let mut n = napi_value(ptr::null_mut());
        unsafe { napi_get_null(env(), &mut n) };
        unsafe { napi_typeof(env(), n, &mut ty) };
        assert_eq!(ty, napi_valuetype::napi_null);
    }

    #[test]
    fn wrong_type_extraction_fails() {
        let mut b = napi_value(ptr::null_mut());
        unsafe { napi_get_boolean(env(), true, &mut b) };
        let mut d = 0.0;
        // bool → double dá 1.0 (coerção definida), mas string→double falha.
        assert_eq!(unsafe { napi_get_value_double(env(), b, &mut d) }, napi_ok);
        assert_eq!(d, 1.0);
    }
}
