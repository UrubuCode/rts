//! `Date`, across the boundary.
//!
//! # Why this goes through the language and not the engine's clock
//!
//! `rts-core` has a `Date` class, and it is reached here the way a program
//! reaches it: `globalThis.Date`, constructed with a millisecond count. Not
//! because that is cheaper — it is not — but because rule 5 says this crate
//! does not decide language questions, and "what is a Date" is one. A date
//! built from an internal field would be one `instanceof Date` might not
//! recognise, and the ABI's `napi_is_date` has to agree with the program's
//! `x instanceof Date` about the same object.
//!
//! # The three-borrow shape, again
//!
//! Every function here takes several separate borrows of the runtime rather
//! than one. `key_number`, `global_get` and `call_with_args` each reach the
//! thread's context themselves, so nesting them inside a `with_runtime` is a
//! re-entrant borrow, which aborts the process across the FFI boundary — the
//! failure `errors.rs` records at length. The steps are ugly and the alternative
//! is a crash an addon cannot diagnose.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_date_expected, napi_invalid_arg, napi_ok};

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

/// A global by name, through the same door a program uses.
pub(crate) fn global(name: &str) -> u64 {
    let text = rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, name));
    let key = rts_core::entry::key_number(text);
    rts_core::entry::global_get(key)
}

/// A member by name, through the indexed path.
///
/// `get_indexed` rather than `get_member` for the reason `objects.rs` records:
/// the member path holds the runtime's borrow across the read, so a property
/// backed by a getter cannot run. A method is not a getter, but one door for
/// both is what keeps the next caller from picking the wrong one.
pub(crate) fn member(object: u64, name: &str) -> u64 {
    let text = rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, name));
    rts_core::entry::get_indexed(object, text)
}

/// `napi_create_date` — a `Date` from milliseconds since the epoch.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_date(
    env: napi_env,
    time: f64,
    result: *mut napi_value,
) -> napi_status {
    let constructor = global("Date");
    let milliseconds = rts_core::entry::make_number(time);
    let arguments = rts_core::entry::make_array(vec![milliseconds]);
    let date = rts_core::entry::construct_with_args(constructor, arguments);
    // SAFETY: forwarded.
    unsafe { produce(env, result, date) }
}

/// `napi_get_date_value` — the milliseconds a date holds.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_date_value(
    _env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if !is_a_date(word) {
        return napi_date_expected;
    }
    let get_time = member(word, "getTime");
    let arguments = rts_core::entry::make_array(Vec::new());
    let answered = rts_core::entry::call_with_args(get_time, word, arguments);
    let Some(milliseconds) = rts_core::entry::number_of(answered) else {
        // A `Date` whose `getTime` was replaced with something that answers a
        // string. The ABI has no word for it, and `napi_date_expected` is the
        // nearest true one: what came back is not a date's value.
        return napi_date_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = milliseconds };
    napi_ok
}

/// `napi_is_date`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_date(
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
    // SAFETY: the caller's contract.
    unsafe { *result = is_a_date(word) };
    napi_ok
}

/// Whether a word is `instanceof Date`.
fn is_a_date(word: u64) -> bool {
    rts_core::entry::instance_of(word, global("Date"))
}
