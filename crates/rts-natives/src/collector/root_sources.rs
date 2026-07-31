//! Off-stack root sources — the layers above contributing what the scanner
//! cannot see.
//!
//! The conservative scanner ([`super::scan::scan_all_roots`]) finds roots by
//! walking native stacks. That misses handles held **only** inside a Rust
//! container on the heap: a pending microtask's bound arguments, a queued
//! promise callback, a driver's saved generator state. A GC tick during
//! synchronous code would sweep them, and the async drive would then operate on
//! freed handles.
//!
//! A layer that owns such a container registers a marker here once; the cycle
//! calls every registered marker before sweeping.
//!
//! ## Why this is not the hook that was just deleted
//!
//! `GC_COLLECT_HOOK` was a function pointer that let the collector call
//! *upward* to run the collection cycle itself — an inversion that existed for
//! one reason: the two halves of the GC lived in different crates and the lower
//! one could not name the upper one's `finish_cycle`. Unifying the GC in
//! `rts-natives` deleted it outright.
//!
//! This is the opposite direction of knowledge. The collector owns the cycle
//! and always will; what it cannot own is the exhaustive list of every
//! container in every layer that might hold a handle off-stack — that would
//! require `rts-natives` to depend on every crate above it. Contribution by
//! registration is the design a correct GC needs, not a workaround for a bad
//! partition.
//!
//! ## Current registrants
//!
//! - `rts-std`, in `runtime_init`: the microtask queue's roots. That queue
//!   currently lives in `globals/text_encoding/instance.rs`, a 928-line file
//!   that has nothing to do with text encoding — the queue and the event loop
//!   belong in a module of their own, and once extracted, most of it qualifies
//!   for `rts-natives` under clause (a) ("the IR cannot express it"). Separate
//!   debt, deliberately not folded into this move (see `RTS_ORGANIZATION.md`).

use std::sync::Mutex;

/// Registered markers. A `Vec` behind a `Mutex`: registration happens a handful
/// of times at startup, and the read is once per GC cycle — the lock is never
/// contended and never on an allocation path.
static SOURCES: Mutex<Vec<fn()>> = Mutex::new(Vec::new());

/// Register `f` to be called on every collection, before the sweep. `f` must
/// mark each handle it owns with [`crate::heap::handles::mark_handle`], which is
/// transitive — marking a closure handle covers its captures.
///
/// Idempotency is the caller's business: `runtime_init` is itself idempotent,
/// and a marker registered twice only costs a second pass.
pub fn register(f: fn()) {
    SOURCES.lock().unwrap_or_else(|e| e.into_inner()).push(f);
}

/// Run every registered marker. Called by [`super::cycle::finish_cycle`].
///
/// The markers are copied out before being called: a marker allocates (marking
/// walks entries), an allocation can tick the collector, and re-entering the
/// cycle while holding this lock would deadlock.
pub(crate) fn mark_all() {
    let fns: Vec<fn()> = SOURCES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .copied()
        .collect();
    for f in fns {
        f();
    }
}
