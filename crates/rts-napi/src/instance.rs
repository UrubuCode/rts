//! The pointer an addon keeps beside its environment.
//!
//! # What this is for
//!
//! An addon compiled once and loaded into two environments may not keep its
//! state in a `static`: both copies would share it. `napi_set_instance_data`
//! is the ABI's answer — one pointer per environment, handed back by
//! `napi_get_instance_data`, freed by a finalizer when the environment goes.
//!
//! # Why it lives on the `Env` and not in a table here
//!
//! Because that is what "per instance" means. The old crate kept it in a
//! process-wide `Mutex<usize>`, which answers correctly for exactly one addon
//! and silently hands the second one the first's pointer. One field on the
//! record the ABI already gives every call a pointer to has no such failure.

use core::ffi::c_void;

use crate::abi::{napi_env, napi_finalize, napi_status};
use crate::handles::env_of;

use napi_status::{napi_invalid_arg, napi_ok};

/// A pointer an environment holds for its addon, and how to let go of it.
pub struct Instance {
    /// The addon's pointer. Opaque here — never dereferenced.
    pub data: *mut c_void,
    /// What to run when the environment is torn down.
    pub finalize: napi_finalize,
    /// The second word the finalizer is handed.
    pub hint: *mut c_void,
}

/// `napi_set_instance_data`.
///
/// Replacing an existing pointer does NOT run the old finalizer, which is what
/// Node does: the addon that set it is the one that knows whether the old
/// pointer is still owned somewhere else.
///
/// # Safety
///
/// The ABI's: `env` live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_instance_data(
    env: napi_env,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(held) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    held.instance = Some(Instance {
        data,
        finalize: finalize_cb,
        hint: finalize_hint,
    });
    napi_ok
}

/// `napi_get_instance_data`.
///
/// Answers null when nothing was set, rather than a status: the ABI says an
/// environment with no instance data answers `NULL` and `napi_ok`, and an addon
/// that checks the status would otherwise treat "I never set one" as a failure.
///
/// # Safety
///
/// The ABI's: `env` live, `data` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_instance_data(
    env: napi_env,
    data: *mut *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(held) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    if data.is_null() {
        return napi_invalid_arg;
    }
    let pointer = match &held.instance {
        Some(instance) => instance.data,
        None => core::ptr::null_mut(),
    };
    // SAFETY: the caller's contract.
    unsafe { *data = pointer };
    napi_ok
}
