//! Handle scopes, and the escape hatch out of one.
//!
//! A scope is a bracket around the handles a piece of addon code makes: opened,
//! filled, closed, and every handle in it released at once. [`crate::env::Env`]
//! already keeps the stack — this is the ABI's door to it.
//!
//! # Why the handles are not the scope's to name
//!
//! The ABI hands back a `napi_handle_scope` and takes it again at close, and an
//! addon that closes them out of order is a bug the ABI says is undefined. This
//! keeps the stack discipline instead: close pops the innermost, and a mismatch
//! answers `napi_handle_scope_mismatch` rather than unwinding to a scope in the
//! middle. The handle it hands back is the DEPTH, which is what makes the check
//! possible at all.
//!
//! # Escaping
//!
//! `napi_escape_handle` moves one value out of a scope that is about to close,
//! into the one below it. That is the only way an addon returns a value it made
//! inside a scope of its own, and it is once per scope — the ABI has a status
//! for the second attempt, which is a real mistake rather than a nuisance.

use crate::abi::{
    napi_env, napi_escapable_handle_scope, napi_handle_scope, napi_status, napi_value,
};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_ok};

/// A scope handle is its depth, so a mismatch is checkable.
fn as_handle(depth: usize) -> *mut core::ffi::c_void {
    depth as *mut core::ffi::c_void
}

/// `napi_open_handle_scope`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_handle_scope(
    env: napi_env,
    result: *mut napi_handle_scope,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let depth = env.open();
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = napi_handle_scope(as_handle(depth)) };
    napi_ok
}

/// `napi_close_handle_scope`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_handle_scope(
    env: napi_env,
    scope: napi_handle_scope,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    // Closing anything but the innermost is the addon's bug, and the ABI has a
    // word for it. Unwinding to the named scope instead would release handles
    // an inner scope is still using.
    if scope.0 as usize != env.depth() {
        return napi_status::napi_handle_scope_mismatch;
    }
    match env.close() {
        true => napi_ok,
        false => napi_status::napi_handle_scope_mismatch,
    }
}

/// `napi_open_escapable_handle_scope`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_escapable_handle_scope(
    env: napi_env,
    result: *mut napi_escapable_handle_scope,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let depth = env.open();
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = napi_escapable_handle_scope(as_handle(depth)) };
    napi_ok
}

/// `napi_close_escapable_handle_scope`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_escapable_handle_scope(
    env: napi_env,
    scope: napi_escapable_handle_scope,
) -> napi_status {
    // SAFETY: the caller's contract.
    unsafe { napi_close_handle_scope(env, napi_handle_scope(scope.0)) }
}

/// `napi_escape_handle` — one value out, into the scope below.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_escape_handle(
    env: napi_env,
    _scope: napi_escapable_handle_scope,
    escapee: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(escapee) }) else {
        return napi_invalid_arg;
    };
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    // The value is re-handled in the scope BELOW the innermost, which is where
    // it has to live to survive the close the addon is about to do.
    let Some(handle) = env.handle_below(word) else {
        return napi_status::napi_escape_called_twice;
    };
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_open_callback_scope`.
///
/// An async context, which this engine does not model: there is no
/// `async_hooks` here and nothing observes one. Accepted and recorded as a
/// depth so an addon's open/close pairs balance, which is all it can observe
/// through this ABI.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_callback_scope(
    env: napi_env,
    _resource_object: napi_value,
    _context: *mut core::ffi::c_void,
    result: *mut *mut core::ffi::c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    unsafe { napi_open_handle_scope(env, result.cast::<napi_handle_scope>()) }
}

/// `napi_close_callback_scope`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_callback_scope(
    env: napi_env,
    scope: *mut core::ffi::c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    unsafe { napi_close_handle_scope(env, napi_handle_scope(scope)) }
}
