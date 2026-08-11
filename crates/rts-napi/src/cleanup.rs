//! What an addon asks to have run when its environment goes away.
//!
//! # Why the handle is a heap record and not an index
//!
//! `napi_remove_async_cleanup_hook` takes the handle ALONE — no environment.
//! So the handle has to be enough to find both the hook and the environment
//! that holds it, which an index into a per-environment list is not. A leaked
//! `Box` is: the pointer is the identity, and it carries the owner.
//!
//! # What "async" means here, and what it does not
//!
//! The ABI's async cleanup hook may finish later: it is handed its own handle
//! and is expected to call `napi_remove_async_cleanup_hook` when it is done,
//! which is how teardown waits for it. **Nothing waits here.** The hooks run
//! during [`crate::env::destroy`], in the order they were added, and the
//! environment is torn down straight afterwards.
//!
//! That is a real difference from Node and it is stated rather than hidden: an
//! addon whose hook schedules work and returns will have that work run against
//! an environment that is already gone. The alternative — a teardown that
//! blocks on a callback that may never come — trades a documented divergence
//! for a hang, which `CLAUDE.md`'s honesty floor calls the worse of the two.

use core::ffi::c_void;

use crate::abi::{napi_env, napi_status};
use crate::handles::env_of;

use napi_status::{napi_invalid_arg, napi_ok};

/// The opaque handle the ABI hands back, and hands to the hook.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct napi_async_cleanup_hook_handle(pub *mut c_void);

/// What runs at teardown.
pub type napi_async_cleanup_hook =
    Option<unsafe extern "C" fn(handle: napi_async_cleanup_hook_handle, data: *mut c_void)>;

thread_local! {
    /// Every record this module has leaked and not yet freed.
    ///
    /// The reason it exists is one ordering the ABI positively invites: a hook
    /// is handed its OWN handle and is expected to remove itself, so `remove`
    /// can run while [`run`] is holding the same record. Freeing on both paths
    /// would be a double free. Membership here is the one authority on whether
    /// a record is still owned, so whichever path is second does nothing.
    static LIVE: core::cell::RefCell<Vec<usize>> = const { core::cell::RefCell::new(Vec::new()) };
}

/// Claims a record, answering whether this caller is the one that frees it.
fn claim(handle: napi_async_cleanup_hook_handle) -> bool {
    LIVE.with_borrow_mut(|live| match live.iter().position(|held| *held == handle.0 as usize) {
        Some(at) => {
            live.remove(at);
            true
        }
        None => false,
    })
}

/// One registered hook. Leaked; the handle is its address.
pub struct Hook {
    /// The environment that owns it, so `remove` can find the list.
    pub owner: *mut c_void,
    /// What to call.
    pub hook: napi_async_cleanup_hook,
    /// The addon's word, handed back untouched.
    pub data: *mut c_void,
}

/// `napi_add_async_cleanup_hook`.
///
/// # Safety
///
/// The ABI's: `env` live, `remove_handle` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_async_cleanup_hook(
    env: napi_env,
    hook: napi_async_cleanup_hook,
    arg: *mut c_void,
    remove_handle: *mut napi_async_cleanup_hook_handle,
) -> napi_status {
    if hook.is_none() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let Some(held) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let record = Box::into_raw(Box::new(Hook {
        owner: env.0,
        hook,
        data: arg,
    }));
    let handle = napi_async_cleanup_hook_handle(record.cast());
    LIVE.with_borrow_mut(|live| live.push(handle.0 as usize));
    held.cleanup.push(handle);
    if !remove_handle.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *remove_handle = handle };
    }
    napi_ok
}

/// `napi_remove_async_cleanup_hook` — forget it, and free the record.
///
/// # Safety
///
/// `remove_handle` must be one [`napi_add_async_cleanup_hook`] produced and not
/// yet removed, and its environment must still be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_async_cleanup_hook(
    remove_handle: napi_async_cleanup_hook_handle,
) -> napi_status {
    if remove_handle.0.is_null() {
        return napi_invalid_arg;
    }
    if !claim(remove_handle) {
        // Already being torn down by `run`, which will free it. Answering `ok`
        // rather than a failure because the addon did exactly what the ABI asks
        // of a hook: remove itself when finished.
        return napi_ok;
    }
    // SAFETY: `claim` answered true, so this is a record this module leaked and
    // nothing else will free.
    let record = unsafe { Box::from_raw(remove_handle.0.cast::<Hook>()) };
    // SAFETY: the caller's contract — the owner outlives the hook.
    if let Some(held) = unsafe { env_of(napi_env(record.owner)) } {
        held.cleanup.retain(|held| *held != remove_handle);
    }
    drop(record);
    napi_ok
}

/// Runs and frees every hook an environment still holds.
///
/// Called from [`crate::env::destroy`]. Drains first and iterates after: a hook
/// that removes itself — which the ABI expects it to — would otherwise mutate
/// the list being walked.
///
/// # Safety
///
/// Every handle in the list must be a live record, which is this module's own
/// invariant: nothing else pushes to or pops from that list.
pub unsafe fn run(handles: Vec<napi_async_cleanup_hook_handle>) {
    for handle in handles {
        if handle.0.is_null() || !claim(handle) {
            continue;
        }
        // SAFETY: `claim` answered true — this module leaked it and no other
        // path will free it, including a `remove` the hook itself performs.
        let record = unsafe { Box::from_raw(handle.0.cast::<Hook>()) };
        if let Some(hook) = record.hook {
            // SAFETY: the addon's own function pointer, called with the two
            // words it registered.
            unsafe { hook(handle, record.data) };
        }
        drop(record);
    }
}
