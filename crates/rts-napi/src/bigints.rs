//! Arbitrary-precision integers, across the boundary.
//!
//! # Why sixty-four-bit words and not the engine's digits
//!
//! The ABI's spelling of a bigint is a sign plus an array of `uint64_t`, least
//! significant first. The engine stores base-2^32 digits, for reasons that are
//! its own (`rts_core::bigint`). Neither side converts to the other's shape
//! here: `entry::bigint_from_words` and `entry::bigint_words` are the engine's
//! own translation, added for this crate, because the digit layout is a fact
//! about the representation and rule 5 keeps those inside `rts-core`.
//!
//! # What "lossless" means, and why a refusal would be wrong
//!
//! `napi_get_value_bigint_int64` answers a status AND a `lossless` flag. A
//! value that does not fit is still converted — truncated to the low
//! sixty-four bits, the same wrap a C cast performs — and reported as lossy.
//! Refusing instead would leave the addon with no number at all for a case the
//! header says it gets one for.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_bigint_expected, napi_invalid_arg, napi_ok};

/// Puts an engine word in a handle of `env`'s innermost scope.
///
/// # Safety
///
/// `env` live, `out` writable.
unsafe fn produce(env: napi_env, out: *mut napi_value, word: u64) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(word);
    // SAFETY: the caller's contract.
    match unsafe { write_out(out, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_create_bigint_int64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_int64(
    env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    let word = rts_core::entry::bigint_from_words(value < 0, &[value.unsigned_abs()]);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_create_bigint_uint64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_uint64(
    env: napi_env,
    value: u64,
    result: *mut napi_value,
) -> napi_status {
    let word = rts_core::entry::bigint_from_words(false, &[value]);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_create_bigint_words` — a sign and `word_count` sixty-four-bit words.
///
/// # Safety
///
/// The ABI's, and `words` must point at `word_count` readable `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_words(
    env: napi_env,
    sign_bit: i32,
    word_count: usize,
    words: *const u64,
    result: *mut napi_value,
) -> napi_status {
    // Zero words is a legal way to spell zero, and the pointer is then allowed
    // to be null — so the emptiness is checked before the pointer is.
    let digits: &[u64] = match word_count {
        0 => &[],
        _ if words.is_null() => return napi_invalid_arg,
        // SAFETY: the caller's contract.
        count => unsafe { core::slice::from_raw_parts(words, count) },
    };
    let word = rts_core::entry::bigint_from_words(sign_bit != 0, digits);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_value_bigint_int64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_int64(
    _env: napi_env,
    value: napi_value,
    result: *mut i64,
    lossless: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some((number, lossy)) = rts_core::entry::bigint_i64(word) else {
        return napi_bigint_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = number };
    if !lossless.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *lossless = !lossy };
    }
    napi_ok
}

/// `napi_get_value_bigint_uint64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_uint64(
    _env: napi_env,
    value: napi_value,
    result: *mut u64,
    lossless: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some((number, lossy)) = rts_core::entry::bigint_u64(word) else {
        return napi_bigint_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = number };
    if !lossless.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *lossless = !lossy };
    }
    napi_ok
}

/// `napi_get_value_bigint_words` — the sign and the words.
///
/// Called twice by an addon that does not know the size: once with a null
/// `words` to learn `word_count`, then again with a buffer. So `word_count` is
/// read as a capacity AND written as a length, which is the one place in this
/// crate an out-parameter is also an in-parameter.
///
/// # Safety
///
/// The ABI's, and `words` must point at `*word_count` writable `u64` when it is
/// not null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_words(
    _env: napi_env,
    value: napi_value,
    sign_bit: *mut i32,
    word_count: *mut usize,
    words: *mut u64,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some((negative, digits)) = rts_core::entry::bigint_words(word) else {
        return napi_bigint_expected;
    };
    if word_count.is_null() {
        return napi_invalid_arg;
    }
    if !sign_bit.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *sign_bit = i32::from(negative) };
    }
    if words.is_null() {
        // The sizing call. `word_count` is answered and nothing is copied.
        // SAFETY: the caller's contract.
        unsafe { *word_count = digits.len() };
        return napi_ok;
    }
    // SAFETY: the caller's contract — a readable capacity.
    let capacity = unsafe { *word_count };
    let count = capacity.min(digits.len());
    // SAFETY: the caller's contract — `capacity` writable words, and `count` is
    // no larger.
    unsafe { core::ptr::copy_nonoverlapping(digits.as_ptr(), words, count) };
    // What was WRITTEN, not what was wanted: an addon that undersized its
    // buffer must be able to tell, and Node answers the same way.
    // SAFETY: the caller's contract.
    unsafe { *word_count = count };
    napi_ok
}
