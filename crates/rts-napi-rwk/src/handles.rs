//! What a `napi_value` actually points at, and how long it lives.
//!
//! # The decision
//!
//! A `napi_value` is a pointer to a SLOT holding one encoded value, not the
//! encoded value reinterpreted as a pointer. Rule 2 of this crate's README says
//! why in one line; the long version is that an addon may hold a handle across
//! a call, and nothing in the addon's memory is a root the collector can find.
//! A slot can be registered as one — [`rts_core::entry::external`] exists for
//! exactly this — and a raw word cannot.
//!
//! # Why the slots are chunked
//!
//! A handle's address must stay valid for as long as its scope does, and a
//! `Vec<u64>` that grows moves everything it holds. So a scope owns a list of
//! fixed-size chunks and hands out interior pointers: pushing a chunk never
//! moves the ones already there, and a handle taken on the first call is still
//! good on the thousandth.
//!
//! `Box<u64>` per handle would also be stable and is what the obvious version
//! does. Rejected: an addon that loops over an array creates one handle per
//! element, and that is one allocation each against one per [`CHUNK`].
//!
//! # What closing a scope does
//!
//! Releases every external hold the scope took, then drops the chunks. After
//! that the addon's handles dangle — which is exactly what the ABI says: using
//! a `napi_value` after its scope closed is undefined, and this crate cannot
//! make it defined without keeping every handle forever.

use crate::abi::{napi_env, napi_value};

/// Handles per chunk.
///
/// 64 words is 512 bytes — one allocation for the common callback, which
/// touches a handful, and eight for a loop over a hundred-element array.
const CHUNK: usize = 64;

/// One scope's handles.
///
/// Not `Copy`, not `Clone`: a scope is a position on a stack, and duplicating
/// one would duplicate the releases its close performs.
pub struct Scope {
    chunks: Vec<Box<[u64; CHUNK]>>,
    /// How many slots of the last chunk are used.
    used: usize,
    /// The external-root identifier taken for each slot handed out, in order.
    holds: Vec<u32>,
}

impl Scope {
    /// An empty scope, allocating nothing until it hands out its first handle.
    pub fn new() -> Self {
        Scope {
            chunks: Vec::new(),
            used: CHUNK,
            holds: Vec::new(),
        }
    }

    /// Puts `value` in a slot, roots it, and answers the handle.
    ///
    /// The root is taken per handle rather than per distinct value: two handles
    /// to one object are two holds, released independently, which is what
    /// `entry::external`'s own test states it is (one hold per call, not a
    /// count).
    pub fn handle(&mut self, value: u64) -> napi_value {
        if self.used == CHUNK {
            self.chunks.push(Box::new([0u64; CHUNK]));
            self.used = 0;
        }
        let chunk = self
            .chunks
            .last_mut()
            .expect("a chunk was just pushed if there was none");
        chunk[self.used] = value;
        let slot: *mut u64 = &mut chunk[self.used];
        self.used += 1;
        self.holds.push(rts_core::entry::hold_current(value));
        napi_value(slot.cast())
    }

    /// How many handles this scope has handed out. For tests and diagnostics.
    pub fn len(&self) -> usize {
        self.holds.len()
    }

    /// Whether it has handed out none.
    pub fn is_empty(&self) -> bool {
        self.holds.is_empty()
    }
}

impl Default for Scope {
    fn default() -> Self {
        Scope::new()
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        for id in self.holds.drain(..) {
            // The value is allowed to be gone already: a program that ended its
            // context before dropping the scope has nothing to release against,
            // and refusing here would panic across an FFI boundary to report
            // something nobody can act on.
            let _ = rts_core::entry::release_current(id);
        }
    }
}

/// The word a handle names, or `None` for a null handle.
///
/// # Safety
///
/// `handle` must be one this crate handed out, from a scope that is still open.
/// That is the ABI's own contract with the addon and cannot be checked here: a
/// pointer carries no evidence of where it came from.
pub unsafe fn value_of(handle: napi_value) -> Option<u64> {
    let slot: *mut u64 = handle.0.cast();
    match slot.is_null() {
        true => None,
        // SAFETY: the caller's contract, above.
        false => Some(unsafe { *slot }),
    }
}

/// Writes `value` into the out-parameter an entry point was given.
///
/// Every `napi_*` that produces a value takes a `*mut napi_value` and answers a
/// status, so this shape appears in every one of them.
///
/// # Safety
///
/// `out` must be null or point at a writable `napi_value` the addon owns.
pub unsafe fn write_out(out: *mut napi_value, value: napi_value) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: the caller's contract, above.
    unsafe { *out = value };
    true
}

/// A null handle, for a failed call's out-parameter.
///
/// The ABI leaves the out-parameter untouched on failure, so this is for the
/// places that need a `napi_value` to exist rather than to be written.
pub fn none() -> napi_value {
    napi_value(core::ptr::null_mut())
}

/// The environment pointer an entry point was handed, as a reference.
///
/// # Safety
///
/// `env` must be one [`crate::env::Env::into_raw`] produced and not yet
/// destroyed.
pub unsafe fn env_of<'a>(env: napi_env) -> Option<&'a mut crate::env::Env> {
    // SAFETY: the caller's contract, above.
    unsafe { env.0.cast::<crate::env::Env>().as_mut() }
}
