//! BigInt N-API (#219) — usa `Entry::BigInt` do engine (negative + words
//! little-endian). Fiel a `napi_create/get_value_bigint_int64/uint64/words`.
//! Ver docs/specs/napi-implementation.md.

use std::ffi::c_void;

use rts_engine::heap::handles::{
    alloc_bigint_i64, alloc_bigint_u64, alloc_bigint_words, bigint_to_i64, bigint_to_u64,
    bigint_words, is_bigint,
};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_bigint_expected, napi_invalid_arg, napi_ok};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_int64(
    _env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = value_from_handle(alloc_bigint_i64(value)) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_uint64(
    _env: napi_env,
    value: u64,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = value_from_handle(alloc_bigint_u64(value)) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_words(
    _env: napi_env,
    sign_bit: i32,
    word_count: usize,
    words: *const u64,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let slice = if words.is_null() || word_count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(words, word_count) }
    };
    let h = alloc_bigint_words(sign_bit != 0, slice);
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_int64(
    _env: napi_env,
    value: napi_value,
    result: *mut i64,
    lossless: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some((v, ll)) = bigint_to_i64(handle_from_value(value)) else {
        return napi_bigint_expected;
    };
    unsafe { *result = v };
    if !lossless.is_null() {
        unsafe { *lossless = ll };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_uint64(
    _env: napi_env,
    value: napi_value,
    result: *mut u64,
    lossless: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some((v, ll)) = bigint_to_u64(handle_from_value(value)) else {
        return napi_bigint_expected;
    };
    unsafe { *result = v };
    if !lossless.is_null() {
        unsafe { *lossless = ll };
    }
    napi_ok
}

/// Protocolo de 2 passagens: `words=NULL` mede `*word_count`; senão copia.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_words(
    _env: napi_env,
    value: napi_value,
    sign_bit: *mut i32,
    word_count: *mut usize,
    words: *mut u64,
) -> napi_status {
    let Some((negative, w)) = bigint_words(handle_from_value(value)) else {
        return napi_bigint_expected;
    };
    if !sign_bit.is_null() {
        unsafe { *sign_bit = if negative { 1 } else { 0 } };
    }
    // Medição: words NULL → reporta a contagem.
    if words.is_null() {
        if !word_count.is_null() {
            unsafe { *word_count = w.len() };
        }
        return napi_ok;
    }
    // Cópia: até a capacidade fornecida.
    let cap = if word_count.is_null() {
        w.len()
    } else {
        unsafe { *word_count }
    };
    let n = cap.min(w.len());
    unsafe {
        std::ptr::copy_nonoverlapping(w.as_ptr(), words, n);
    }
    if !word_count.is_null() {
        unsafe { *word_count = w.len() };
    }
    let _ = is_bigint; // mantém o import (usado em testes)
    napi_ok
}

// suprime unused em alguns paths
#[allow(unused_imports)]
use c_void as _cv;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn int64_roundtrip() {
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_bigint_int64(env(), -1234567890123, &mut v) };
        assert!(is_bigint(handle_from_value(v)));
        let mut out = 0i64;
        let mut ll = false;
        unsafe { napi_get_value_bigint_int64(env(), v, &mut out, &mut ll) };
        assert_eq!(out, -1234567890123);
        assert!(ll);
    }

    #[test]
    fn uint64_max() {
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_bigint_uint64(env(), u64::MAX, &mut v) };
        let mut out = 0u64;
        let mut ll = false;
        unsafe { napi_get_value_bigint_uint64(env(), v, &mut out, &mut ll) };
        assert_eq!(out, u64::MAX);
        assert!(ll);
        // u64::MAX como int64 NÃO é lossless.
        let mut i = 0i64;
        unsafe { napi_get_value_bigint_int64(env(), v, &mut i, &mut ll) };
        assert!(!ll);
    }

    #[test]
    fn words_roundtrip() {
        // BigInt de 2 words: 0x0000000000000002_FFFFFFFFFFFFFFFF
        let input = [0xFFFFFFFFFFFFFFFFu64, 0x2u64];
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_bigint_words(env(), 0, 2, input.as_ptr(), &mut v) };
        // mede
        let mut count = 0usize;
        let mut sign = 9i32;
        unsafe { napi_get_value_bigint_words(env(), v, &mut sign, &mut count, ptr::null_mut()) };
        assert_eq!(count, 2);
        assert_eq!(sign, 0);
        // copia
        let mut out = [0u64; 4];
        let mut cap = 4usize;
        unsafe { napi_get_value_bigint_words(env(), v, &mut sign, &mut cap, out.as_mut_ptr()) };
        assert_eq!(&out[..2], &input);
        // uint64 não-lossless (2 words)
        let mut u = 0u64;
        let mut ll = true;
        unsafe { napi_get_value_bigint_uint64(env(), v, &mut u, &mut ll) };
        assert_eq!(u, 0xFFFFFFFFFFFFFFFF);
        assert!(!ll);
    }

    #[test]
    fn get_on_non_bigint_fails() {
        let mut out = 0i64;
        let bogus = napi_value(0x1 as *mut std::ffi::c_void);
        assert_eq!(
            unsafe { napi_get_value_bigint_int64(env(), bogus, &mut out, ptr::null_mut()) },
            napi_bigint_expected
        );
    }
}
