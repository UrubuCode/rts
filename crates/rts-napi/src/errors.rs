//! Throwing, and finding out that something threw.
//!
//! # A throw is a value, not a status
//!
//! `napi_status` says whether the CALL worked. A JavaScript exception is a
//! different fact, and the ABI keeps them apart: a function that raised answers
//! `napi_pending_exception`, and the value itself is read with
//! `napi_get_and_clear_last_exception`. An addon that treats a status as the
//! exception loses the value; one that treats the exception as a status
//! reports a failure to the wrong layer.
//!
//! # Where the throw actually lives
//!
//! In the runtime's one slot — `rts_core::entry::throw`, the same slot a
//! compiled `throw` writes and a compiled call site checks. Nothing is
//! duplicated here: an exception an addon raises is caught by a `try` in the
//! program, and one the program raised is visible to the addon, because there is
//! one place either can look.
//!
//! That slot is also why the tag is not this crate's business.
//! `rts_core::entry::throw_value` takes the value and supplies the tag itself —
//! which number a `catch` matches is an agreement between the runtime and the
//! compiler, and a third opinion here would be the kind of duplicate this
//! repository's rules keep naming.
//!
//! # `code` is a property, not a second channel
//!
//! Node's `napi_throw_error(env, code, msg)` puts `code` on the error object as
//! an ordinary property, which is what `err.code === "ENOENT"` reads. So that is
//! what happens here, rather than a field of our own that nothing in the
//! language could see.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_ok};

/// Builds one of the language's error classes and, when asked, puts `code` on
/// it.
///
/// `None` for a class this engine does not provide — see
/// `rts_core::entry::make_named_error` for why that is an absence rather than a
/// plain object wearing the name.
fn build(class: &str, code: Option<&str>, message: &str) -> Option<u64> {
    let error = rts_core::entry::make_named_error(class, message)?;
    if let Some(code) = code {
        rts_core::entry::with_runtime(|context| {
            let text = rts_core::entry::make_string(context, code);
            rts_core::entry::put_member(context, error, "code", text);
        });
    }
    Some(error)
}

/// The text a C string holds, or `None` when it is null or not UTF-8.
///
/// # Safety
///
/// `text` must be null or NUL-terminated.
unsafe fn text_of<'a>(text: *const core::ffi::c_char) -> Option<&'a str> {
    match text.is_null() {
        true => None,
        // SAFETY: the caller's contract.
        false => unsafe { core::ffi::CStr::from_ptr(text) }.to_str().ok(),
    }
}

/// `napi_throw` — raise whatever the addon built.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw(_env: napi_env, error: napi_value) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(error) }) else {
        return napi_invalid_arg;
    };
    // Any value, not just an error object: JavaScript is emphatic that a string
    // and a number are throwable, and refusing them here would be this crate
    // deciding a language question (rule 5).
    rts_core::entry::throw_value(word);
    napi_ok
}

/// Builds and raises one of the language's error classes.
///
/// # Safety
///
/// `code` and `msg` must be null or NUL-terminated.
unsafe fn throw_named(
    class: &str,
    code: *const core::ffi::c_char,
    msg: *const core::ffi::c_char,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(message) = (unsafe { text_of(msg) }) else {
        return napi_invalid_arg;
    };
    // SAFETY: the caller's contract.
    let code = unsafe { text_of(code) };
    match build(class, code, message) {
        Some(error) => {
            rts_core::entry::throw_value(error);
            napi_ok
        }
        None => napi_status::napi_generic_failure,
    }
}

/// `napi_throw_error`.
///
/// # Safety
///
/// `code` and `msg` must be null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_error(
    _env: napi_env,
    code: *const core::ffi::c_char,
    msg: *const core::ffi::c_char,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { throw_named("Error", code, msg) }
}

/// `napi_throw_type_error`.
///
/// # Safety
///
/// As [`napi_throw_error`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_type_error(
    _env: napi_env,
    code: *const core::ffi::c_char,
    msg: *const core::ffi::c_char,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { throw_named("TypeError", code, msg) }
}

/// `napi_throw_range_error`.
///
/// # Safety
///
/// As [`napi_throw_error`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_range_error(
    _env: napi_env,
    code: *const core::ffi::c_char,
    msg: *const core::ffi::c_char,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { throw_named("RangeError", code, msg) }
}

/// Builds an error without raising it.
///
/// # Safety
///
/// `code` must be null or a handle; `msg` must be a string handle.
unsafe fn create_named(
    class: &str,
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(message) = (unsafe { value_of(msg) }).and_then(rts_core::entry::text_of) else {
        return napi_status::napi_string_expected;
    };
    // SAFETY: the caller's contract.
    let code = unsafe { value_of(code) }.and_then(rts_core::entry::text_of);
    let Some(error) = build(class, code.as_deref(), &message) else {
        return napi_status::napi_generic_failure;
    };
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(error);
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_create_error`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { create_named("Error", env, code, msg, result) }
}

/// `napi_create_type_error`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_type_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { create_named("TypeError", env, code, msg, result) }
}

/// `napi_create_range_error`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_range_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { create_named("RangeError", env, code, msg, result) }
}

/// `napi_is_exception_pending`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_exception_pending(
    _env: napi_env,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::thrown() != 0 };
    napi_ok
}

/// `napi_get_and_clear_last_exception`.
///
/// Answers `undefined` when nothing is pending, which is what the ABI says and
/// is not the same as failing: an addon may ask without knowing.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_and_clear_last_exception(
    env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    let word = match rts_core::entry::thrown() != 0 {
        true => rts_core::entry::take_thrown(),
        false => rts_core::entry::undefined_value(),
    };
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(word);
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_is_error` — whether the value is one of the language's errors.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_error(
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
    // Asked exactly as the program would ask: `value instanceof Error`, through
    // the same global lookup and the same operator. The first version of this
    // looked for a `message` property, which is a heuristic — every object can
    // have one — and a heuristic here would be this crate deciding a language
    // question, which rule 5 forbids for good reason: the language already has
    // an answer and it is one call away.
    //
    // Three steps and three separate borrows, which is not style: `key_number`
    // and `global_get` are entry points that reach the thread's context
    // themselves, and calling one inside a `with_runtime` is a re-entrant
    // borrow. That is not a caught error — it panics across an `extern "C"`
    // frame, which aborts the process. It did, here, before this was split.
    let name = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, "Error")
    });
    let key = rts_core::entry::key_number(name);
    let constructor = rts_core::entry::global_get(key);
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::instance_of(word, constructor) };
    napi_ok
}
