//! Reading a value as a number, and turning one thing into another.
//!
//! The rest of the value surface: the integer widths an addon asks for, the
//! four coercions, strict equality, and the global object.
//!
//! # Every coercion here is the language's, called
//!
//! `napi_coerce_to_number` is `Number(x)` and `napi_get_value_int32` is
//! `ToInt32`, which is `x | 0`. Neither is reimplemented: this crate decides no
//! semantics (rule 5), and the interesting cases are exactly the ones a
//! hand-rolled version gets wrong — `Number("0x10")` is 16, `ToInt32(2^31)` is
//! negative, and `NaN | 0` is 0.
//!
//! So each of these is one call into `rts-core`'s own operator, which is the
//! same code a compiled `x | 0` runs.
//!
//! # Why `int64` is not `as i64`
//!
//! `napi_get_value_int64`'s contract is not "truncate": the ABI says a value
//! outside the range is clamped, and `NaN`/infinity answer zero. A cast in Rust
//! does the first differently and the second by saturating, so both are written
//! out rather than left to `as`.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_number_expected, napi_ok};

/// Hands a word back as a handle in the innermost scope.
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

/// `napi_create_uint32`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_uint32(
    env: napi_env,
    value: u32,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_number(value as f64)) }
}

/// `napi_create_int64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int64(
    env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    // A JavaScript number is an `f64`, so an `i64` past 2^53 arrives rounded.
    // That is the language's answer rather than this crate's, and it is what
    // `napi_create_int64` does in Node too — an addon needing exactness reaches
    // for BigInt.
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_number(value as f64)) }
}

/// `napi_get_value_int32` — `ToInt32`, which is `x | 0`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int32(
    _env: napi_env,
    value: napi_value,
    result: *mut i32,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if rts_core::entry::number_of(word).is_none() {
        return napi_number_expected;
    }
    if result.is_null() {
        return napi_invalid_arg;
    }
    // The engine's own `|`, so `2^31` is negative here exactly as it is in a
    // program, and `NaN | 0` is zero.
    let zero = rts_core::entry::make_number(0.0);
    let truncated = rts_core::entry::bit_or(word, zero);
    let Some(number) = rts_core::entry::number_of(truncated) else {
        return napi_number_expected;
    };
    // SAFETY: the caller's contract.
    unsafe { *result = number as i32 };
    napi_ok
}

/// `napi_get_value_uint32` — `ToUint32`, which is `x >>> 0`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_uint32(
    _env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if rts_core::entry::number_of(word).is_none() {
        return napi_number_expected;
    }
    if result.is_null() {
        return napi_invalid_arg;
    }
    let zero = rts_core::entry::make_number(0.0);
    let widened = rts_core::entry::shift_right_unsigned(word, zero);
    let Some(number) = rts_core::entry::number_of(widened) else {
        return napi_number_expected;
    };
    // SAFETY: the caller's contract.
    unsafe { *result = number as u32 };
    napi_ok
}

/// `napi_get_value_int64`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int64(
    _env: napi_env,
    value: napi_value,
    result: *mut i64,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some(number) = rts_core::entry::number_of(word) else {
        return napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // The ABI's three cases, written out because `as i64` answers two of them
    // differently: `NaN` and the infinities are zero, anything outside the
    // range clamps, and the rest truncates toward zero.
    let answer = match number {
        n if n.is_nan() || n.is_infinite() => 0,
        n if n >= i64::MAX as f64 => i64::MAX,
        n if n <= i64::MIN as f64 => i64::MIN,
        n => n.trunc() as i64,
    };
    // SAFETY: the caller's contract.
    unsafe { *result = answer };
    napi_ok
}

/// `napi_coerce_to_bool` — `Boolean(x)`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_bool(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let answer = rts_core::entry::boolean_value(rts_core::entry::to_boolean(word));
    // SAFETY: forwarded.
    unsafe { produce(env, result, answer) }
}

/// `napi_coerce_to_number` — `Number(x)`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_number(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    // `x - 0`, which is the language's `ToNumber` reached through an operator
    // rather than reimplemented here: `Number("0x10")` is 16 and `Number("")`
    // is 0, and neither is obvious enough to write twice.
    let zero = rts_core::entry::make_number(0.0);
    let answer = rts_core::entry::subtract(word, zero);
    // SAFETY: forwarded.
    unsafe { produce(env, result, answer) }
}

/// `napi_coerce_to_string` — `String(x)`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_string(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    // `"" + x`, the same way. An object with a `toString` runs it, which is
    // user code — and `add` is an entry point that holds no borrow across the
    // call, which is why this is a call rather than a read.
    let empty = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, "")
    });
    let answer = rts_core::entry::add(empty, word);
    // SAFETY: forwarded.
    unsafe { produce(env, result, answer) }
}

/// `napi_strict_equals` — `===`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_strict_equals(
    _env: napi_env,
    lhs: napi_value,
    rhs: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(left), Some(right)) = (unsafe { value_of(lhs) }, unsafe { value_of(rhs) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::strict_equals(left, right) };
    napi_ok
}

/// `napi_get_global` — `globalThis`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_global(
    env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    // Through the same door a program uses, and in three steps for the reason
    // `errors.rs` states at length: `key_number` and `global_get` reach the
    // thread's context themselves, so calling either inside a `with_runtime` is
    // a re-entrant borrow, which aborts.
    let name = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, "globalThis")
    });
    let key = rts_core::entry::key_number(name);
    let global = rts_core::entry::global_get(key);
    // SAFETY: forwarded.
    unsafe { produce(env, result, global) }
}
