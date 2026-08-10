//! A value an addon keeps after the call that produced it.
//!
//! P4. A `napi_ref` outlives its handle scope, which is the whole point of it:
//! an addon caches a constructor, a prototype or a callback once and uses it on
//! every later call.
//!
//! # A reference is one of two things, and it changes between them
//!
//! At a refcount above zero it HOLDS — `rts_core::entry::external`, the same
//! root a handle takes. At zero it WATCHES — `rts_core::entry::weak`, which the
//! collector clears as it frees the cell. `napi_reference_unref` to zero and
//! `napi_reference_ref` back up move it from one to the other, and that
//! transition is the only interesting code in this file.
//!
//! The two are separate mechanisms in the engine rather than one with a flag,
//! and that is right: holding and watching are opposite instructions to the
//! collector, and a flag would be a single table whose meaning depends on a
//! bit — which is how a value comes to be kept by a reference that promised not
//! to.
//!
//! # Once collected, never alive again
//!
//! `napi_reference_ref` on a reference whose value the collector already took
//! answers a count and nothing more: there is no value left to hold. The ABI
//! has no status for "too late", and inventing one would be worse than the
//! honest reading — `napi_get_reference_value` answers a null handle, which is
//! exactly what it answers for a weak reference whose value is gone, and is
//! what an addon already has to check for.

use core::cell::RefCell;

use crate::abi::{napi_env, napi_ref, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_ok};

/// What a `napi_ref` points at.
///
/// Boxed and handed to the addon as a pointer, like [`crate::env::Env`]: the
/// ABI's type is opaque and pointer-shaped, and a pointer into a table would
/// have the stability problem `entry::external`'s own doc names.
struct Reference {
    /// How many times the addon has asked for it to be kept.
    count: u32,
    /// The external hold, while `count` is above zero.
    held: Option<u32>,
    /// The watch, while `count` is zero.
    watched: Option<u32>,
    /// Which environment made it, so `forget` can find its own.
    owner: *mut core::ffi::c_void,
}

thread_local! {
    /// Every reference alive on this thread, by address.
    ///
    /// A list rather than the boxes alone, because destroying an environment
    /// has to free the references it made — an addon that unloads without
    /// calling `napi_delete_reference` is the common case, not the exotic one.
    static LIVE: RefCell<Vec<*mut Reference>> = const { RefCell::new(Vec::new()) };
}

/// Turns a reference into a holder, if it is not one already.
fn hold(reference: &mut Reference, value: u64) {
    if reference.held.is_none() {
        reference.held = Some(rts_core::entry::hold_current(value));
    }
    if let Some(watch) = reference.watched.take() {
        rts_core::entry::weak_forget(watch);
    }
}

/// Turns it into a watcher, releasing whatever it held.
fn watch(reference: &mut Reference, value: u64) {
    if let Some(held) = reference.held.take() {
        rts_core::entry::release_current(held);
    }
    if reference.watched.is_none() {
        reference.watched = rts_core::entry::weak_watch(value);
    }
}

/// The value a reference names, or `None` once the collector has taken it.
fn value_in(reference: &Reference) -> Option<u64> {
    if let Some(held) = reference.held {
        return rts_core::entry::held_current(held);
    }
    reference.watched.and_then(rts_core::entry::weak_peek).flatten()
}

/// Frees every reference an environment made.
///
/// Called from [`crate::env::destroy`], for the reason the function registry is
/// cleared there too: the addon is going away and its references with it.
pub fn forget(owner: napi_env) {
    let doomed: Vec<*mut Reference> = LIVE.with_borrow_mut(|live| {
        let (mine, theirs) = live.iter().partition(|&&reference| {
            // SAFETY: every pointer in this list came from `Box::into_raw` here
            // and is removed before it is freed.
            unsafe { (*reference).owner == owner.0 }
        });
        *live = theirs;
        mine
    });
    for reference in doomed {
        // SAFETY: taken out of the list above, so nothing else will free it.
        drop(unsafe { release(reference) });
    }
}

/// Gives up whatever a reference was holding or watching, and the box.
///
/// # Safety
///
/// `reference` must be a pointer this module boxed and not yet released.
unsafe fn release(reference: *mut Reference) -> Box<Reference> {
    // SAFETY: the caller's contract.
    let mut boxed = unsafe { Box::from_raw(reference) };
    if let Some(held) = boxed.held.take() {
        rts_core::entry::release_current(held);
    }
    if let Some(watched) = boxed.watched.take() {
        rts_core::entry::weak_forget(watched);
    }
    boxed
}

/// `napi_create_reference`.
///
/// # Safety
///
/// The ABI's: `value` a handle from an open scope, `result` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_reference(
    env: napi_env,
    value: napi_value,
    initial_refcount: u32,
    result: *mut napi_ref,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }

    let mut reference = Reference {
        count: initial_refcount,
        held: None,
        watched: None,
        owner: env.0,
    };
    match initial_refcount {
        0 => watch(&mut reference, word),
        _ => hold(&mut reference, word),
    }
    let raw = Box::into_raw(Box::new(reference));
    LIVE.with_borrow_mut(|live| live.push(raw));
    // SAFETY: the caller's contract.
    unsafe { *result = napi_ref(raw.cast()) };
    napi_ok
}

/// The reference a handle names.
///
/// # Safety
///
/// `reference` must be one [`napi_create_reference`] produced and not deleted.
unsafe fn reference_of<'a>(reference: napi_ref) -> Option<&'a mut Reference> {
    // SAFETY: the caller's contract.
    unsafe { reference.0.cast::<Reference>().as_mut() }
}

/// `napi_reference_ref` — one more reason to keep it.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_ref(
    _env: napi_env,
    reference: napi_ref,
    result: *mut u32,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(reference) = (unsafe { reference_of(reference) }) else {
        return napi_invalid_arg;
    };
    reference.count = reference.count.saturating_add(1);
    // Nothing to hold if the value is already gone — see the module doc for why
    // that is not an error.
    if let Some(word) = value_in(reference) {
        hold(reference, word);
    }
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = reference.count };
    }
    napi_ok
}

/// `napi_reference_unref` — one fewer, and at zero it stops keeping.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_unref(
    _env: napi_env,
    reference: napi_ref,
    result: *mut u32,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(reference) = (unsafe { reference_of(reference) }) else {
        return napi_invalid_arg;
    };
    // Unreffing at zero is the addon's bug and the ABI says to refuse it, which
    // is worth doing rather than saturating: the alternative silently balances
    // a `ref` the addon never made.
    if reference.count == 0 {
        return napi_status::napi_generic_failure;
    }
    reference.count -= 1;
    if reference.count == 0 && let Some(word) = value_in(reference) {
        watch(reference, word);
    }
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = reference.count };
    }
    napi_ok
}

/// `napi_get_reference_value` — what it names, or a null handle once gone.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_reference_value(
    env: napi_env,
    reference: napi_ref,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(reference) = (unsafe { reference_of(reference) }) else {
        return napi_invalid_arg;
    };
    let word = value_in(reference);
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = match word {
        Some(word) => env.current().handle(word),
        // A null handle, which is the ABI's "it was collected". Not
        // `undefined`: an addon must be able to tell a reference to `undefined`
        // from a reference whose value is gone.
        None => crate::handles::none(),
    };
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_delete_reference`.
///
/// # Safety
///
/// `reference` must not be used again afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_reference(
    _env: napi_env,
    reference: napi_ref,
) -> napi_status {
    if reference.0.is_null() {
        return napi_invalid_arg;
    }
    let raw = reference.0.cast::<Reference>();
    let known = LIVE.with_borrow_mut(|live| {
        match live.iter().position(|&alive| alive == raw) {
            Some(at) => {
                live.remove(at);
                true
            }
            // Refused rather than freed: a pointer this module did not box is
            // not one to call `Box::from_raw` on, and a double delete arrives
            // here as exactly that.
            None => false,
        }
    });
    if !known {
        return napi_invalid_arg;
    }
    // SAFETY: taken out of the live list, so this is the only release.
    drop(unsafe { release(raw) });
    napi_ok
}
