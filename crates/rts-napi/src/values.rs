//! Making a value, and reading one back.
//!
//! The half of the ABI that needs nothing from the engine beyond what
//! `rts-core` already exports: numbers, booleans, the two singletons, UTF-8
//! text, and what a value IS.
//!
//! Every function here follows the same three steps, and the shape is the
//! point rather than boilerplate: check the out-parameter, do the one thing,
//! answer a status. No panic crosses back into the addon (rule 3), so a null
//! pointer is `napi_invalid_arg` rather than a fault.

use crate::abi::{NAPI_AUTO_LENGTH, napi_env, napi_status, napi_value, napi_valuetype};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_ok, napi_string_expected};

/// Puts an engine word in a handle of `env`'s innermost scope.
///
/// # Safety
///
/// `env` must be live and `out` writable; see the callers.
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

/// `napi_create_double` — a number from an `f64`.
///
/// # Safety
///
/// The ABI's: `env` live, `result` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_double(
    env: napi_env,
    value: f64,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded from this function's own contract.
    unsafe { produce(env, result, rts_core::entry::make_number(value)) }
}

/// `napi_create_int32`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int32(
    env: napi_env,
    value: i32,
    result: *mut napi_value,
) -> napi_status {
    // Through `f64` because that is what a JavaScript number IS — the engine
    // has an integer representation and it is an optimisation of the same type,
    // not a second one an addon can ask for.
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_number(value as f64)) }
}

/// `napi_get_boolean`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_boolean(
    env: napi_env,
    value: bool,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::boolean_value(value)) }
}

/// `napi_get_undefined`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_undefined(
    env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::undefined_value()) }
}

/// `napi_get_null`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_null(env: napi_env, result: *mut napi_value) -> napi_status {
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::null_value()) }
}

/// `napi_create_string_utf8`.
///
/// `length` is `NAPI_AUTO_LENGTH` for a NUL-terminated string, which is what
/// most addons pass, or a byte count for one that is not terminated.
///
/// # Safety
///
/// `str` must point at `length` readable bytes, or at a NUL-terminated string
/// when `length` is [`NAPI_AUTO_LENGTH`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf8(
    env: napi_env,
    str: *const core::ffi::c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if str.is_null() {
        return napi_invalid_arg;
    }
    let bytes: &[u8] = match length {
        NAPI_AUTO_LENGTH => {
            // SAFETY: the caller's contract — NUL-terminated when auto.
            unsafe { core::ffi::CStr::from_ptr(str) }.to_bytes()
        }
        // SAFETY: the caller's contract — `length` readable bytes.
        length => unsafe { core::slice::from_raw_parts(str.cast::<u8>(), length) },
    };
    // Refused rather than replaced. An addon that passes bytes it called UTF-8
    // and are not has a bug, and a string of replacement characters would carry
    // that bug into the program as data.
    let Ok(text) = core::str::from_utf8(bytes) else {
        return napi_string_expected;
    };
    let word = rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, text));
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_value_double`.
///
/// # Safety
///
/// The ABI's, plus `value` being a handle from an open scope.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_double(
    _env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some(number) = rts_core::entry::number_of(word) else {
        return napi_status::napi_number_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract — `result` writable.
    unsafe { *result = number };
    napi_ok
}

/// `napi_get_value_bool`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bool(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // `napi_get_value_bool` is NOT `ToBoolean`: the ABI says the value must
    // already be a boolean, and coercing here would answer `true` for the
    // string "false".
    match kind_of(word) {
        napi_valuetype::napi_boolean => {
            // SAFETY: the caller's contract.
            unsafe { *result = rts_core::entry::to_boolean(word) };
            napi_ok
        }
        _ => napi_status::napi_boolean_expected,
    }
}

/// `napi_typeof`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_typeof(
    _env: napi_env,
    value: napi_value,
    result: *mut napi_valuetype,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = kind_of(word) };
    napi_ok
}

/// What the ABI calls a value's type, from what the engine calls it.
///
/// Through `entry::type_of` — the `typeof` operator — rather than by reading
/// the tag here, because "what type is this" is a language question and rule 5
/// says this crate does not answer those. The two places the ABI is finer than
/// `typeof` are `null` (which `typeof` calls an object) and `external` (which
/// the language has no word for at all), and both are handled by asking a
/// second question rather than by a second tag table.
fn kind_of(word: u64) -> napi_valuetype {
    if word == rts_core::entry::null_value() {
        return napi_valuetype::napi_null;
    }
    // The ABI has a word for this and the language does not — see `crate::wrap`.
    if crate::wrap::is_external(word) {
        return napi_valuetype::napi_external;
    }
    let named = rts_core::entry::type_of(word);
    match rts_core::entry::text_of(named).as_deref() {
        Some("undefined") => napi_valuetype::napi_undefined,
        Some("boolean") => napi_valuetype::napi_boolean,
        Some("number") => napi_valuetype::napi_number,
        Some("string") => napi_valuetype::napi_string,
        Some("symbol") => napi_valuetype::napi_symbol,
        Some("function") => napi_valuetype::napi_function,
        Some("bigint") => napi_valuetype::napi_bigint,
        // Everything else the language calls an object. `napi_external` is not
        // reachable until P5 makes one.
        _ => napi_valuetype::napi_object,
    }
}
