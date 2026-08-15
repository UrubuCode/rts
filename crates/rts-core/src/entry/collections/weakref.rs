//! `WeakRef` — honestly, the half of it this engine has today.
//!
//! # What is real and what is not
//!
//! `new WeakRef(target).deref()` answers `target` while it is alive, and that
//! half is genuine: nothing here pretends a stronger guarantee. What is NOT
//! real is the weakness itself — `deref()` never starts answering `undefined`,
//! because nothing collects the target out from under this reference. The
//! target is held as an ordinary own property (`"__target__"`), which keeps it
//! alive exactly the way [`super::weak`]'s `WeakMap`/`WeakSet` keep their keys
//! alive: strongly, as a leak, until something clears it.
//!
//! **The mechanism this was waiting for now exists**, and it is not the
//! `(slot, generation)` pair this paragraph used to propose:
//! [`crate::entry::weak`] watches a value and the sweep clears the watch as it
//! frees the cell, which answers "was this collected" without a generation to
//! compare. It was built for `rts-napi`'s `napi_ref` at refcount zero, and
//! this is its second, obvious client.
//!
//! Wiring `deref` through it is still a separate change, and deliberately so:
//! it is language-VISIBLE. `deref()` would begin answering `undefined` where it
//! has always answered the target, which is correct and which no fixture in the
//! suite has ever seen — so it belongs in a change whose suite run is about
//! that and nothing else. Until then `deref` reads the strongly-held property,
//! and this comment is the record of why that is the whole truth about it
//! rather than an approximation.
//!
//! `FinalizationRegistry` **is** implemented, in [`super::finalization`], and
//! this paragraph said it was not "because it needs the same collector hook and
//! a callback queue the drain loop would have to pump, and nothing here builds
//! either". Both existed already, under other names and for another client:
//! [`crate::entry::finalize`] is the hook AND the queue — the sweep queues, and
//! `drain_microtasks` runs what it queued — and it was built for `rts-napi`'s
//! `napi_wrap`.
//!
//! That is worth reading beside this file rather than only in that one, because
//! it says what is left HERE: a registry learns that a target died and this does
//! not, so the two now use different mechanisms for what is nearly one question.
//! `deref` could be answered by [`crate::entry::weak`] as the paragraph above
//! says, or by a death notice that clears the property; that choice belongs in
//! the change that makes `deref` answer `undefined`, which is still the
//! language-visible change it has always been.

use super::with_current;
use crate::entry::objects;
use crate::value::Value;

/// `WeakRef`.
#[rtse::class("WeakRef", tag)]
impl WeakRef {
    /// `new WeakRef(target)`.
    #[construct]
    fn build(this: u64, target: u64) -> u64 {
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot() {
                let key = context.well_known("__target__");
                objects::put(context, cell, key, target);
            }
        });
        this
    }

    /// `w.deref()` — the target, while nothing collects it out from under this
    /// reference. See the module doc: this is not weak yet.
    fn deref(this: u64) -> u64 {
        with_current(|context| {
            let absent = objects::undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let key = context.well_known("__target__");
            objects::read_property(context, cell, key)
                .map(|value: Value| value.bits())
                .unwrap_or(absent)
        })
    }
}
