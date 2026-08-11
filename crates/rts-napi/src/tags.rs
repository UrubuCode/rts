//! A 128-bit mark an addon puts on an object to recognise it later.
//!
//! # What this is for, and why `napi_unwrap` is not enough
//!
//! An addon handed an object wants to know it is one of ITS objects before
//! reading a pointer out of it. `napi_unwrap` answers a pointer for anything
//! wrapped — including something another addon wrapped — so unwrapping first
//! and trusting the result is how one addon reads another's `Box`. A type tag
//! is the check that makes it safe: a UUID the addon compiled in, compared
//! before the pointer is touched.
//!
//! # Why the tag is kept here and not on the object
//!
//! Putting it on the object as a property would make it visible to the
//! program, writable by it, and enumerable in `Object.keys` — three things a
//! private mark must not be. The engine's `Aside` machinery holds one word
//! beside a cell and `crate::wrap` already spends it on the wrapped pointer, so
//! this keeps its own table keyed by a weak watch: the entry goes when the
//! object does, and a recycled cell cannot inherit a dead object's tag.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::value_of;

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// The ABI's tag: two sixty-four-bit halves of a UUID.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct napi_type_tag {
    /// The low half.
    pub lower: u64,
    /// The high half.
    pub upper: u64,
}

thread_local! {
    /// Which watched object carries which tag.
    ///
    /// A `Vec` rather than a map because an addon tags a handful of classes,
    /// not a heap: the linear scan is over single digits, and a hash table
    /// would cost more to allocate than it ever saves.
    static TAGS: core::cell::RefCell<Vec<(u32, napi_type_tag)>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// The tag on a value, if it has one and is still alive.
fn tag_of(word: u64) -> Option<napi_type_tag> {
    TAGS.with_borrow_mut(|tags| {
        // Dead watches are dropped as they are met rather than swept on a
        // timer: the walk is already here, and a table nobody reads does not
        // need collecting.
        tags.retain(|(watch, _)| rts_core::entry::weak_peek(*watch).is_some());
        tags.iter()
            .find(|(watch, _)| rts_core::entry::weak_peek(*watch).flatten() == Some(word))
            .map(|(_, tag)| *tag)
    })
}

/// `napi_type_tag_object` — mark it.
///
/// Tagging twice is refused, which the ABI requires: a second tag would make
/// `check` answer for whichever one the table found first, and an addon relying
/// on the mark to prove ownership would be reading another's object.
///
/// # Safety
///
/// The ABI's, and `type_tag` must point at a readable [`napi_type_tag`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_type_tag_object(
    _env: napi_env,
    value: napi_value,
    type_tag: *const napi_type_tag,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if type_tag.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let tag = unsafe { *type_tag };
    let Some(watch) = rts_core::entry::weak_watch(word) else {
        // A number cannot be tagged: there is nothing for the mark to outlive.
        return napi_object_expected;
    };
    if tag_of(word).is_some() {
        rts_core::entry::weak_forget(watch);
        return napi_invalid_arg;
    }
    TAGS.with_borrow_mut(|tags| tags.push((watch, tag)));
    napi_ok
}

/// `napi_check_object_type_tag` — is this mine?
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_check_object_type_tag(
    _env: napi_env,
    value: napi_value,
    type_tag: *const napi_type_tag,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if type_tag.is_null() || result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let wanted = unsafe { *type_tag };
    let matches = tag_of(word) == Some(wanted);
    // SAFETY: the caller's contract.
    unsafe { *result = matches };
    napi_ok
}
