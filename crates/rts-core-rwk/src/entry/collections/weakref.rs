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
//! [`super::weak`]'s own doc names what a real weak reference needs: the
//! `(slot, generation)` pair `heap/slab.rs`'s `Handle` already carries, bumped
//! on free. This engine now has a mark-sweep collector, so the mechanism a real
//! `WeakRef` waits on exists — reading a handle whose generation no longer
//! matches is how "was this collected" gets answered honestly. Wiring `deref`
//! through that is a separate change: it means the collector's sweep visiting
//! every live `WeakRef` and clearing the ones whose target it just freed, which
//! is not a fact this file can establish on its own. Until that lands, `deref`
//! reads the strongly-held property, and this comment is the record of why that
//! is the whole truth about it rather than an approximation.
//!
//! `FinalizationRegistry` is not implemented; it needs the same collector hook
//! and a callback queue the drain loop would have to pump, and nothing here
//! builds either.

use super::with_current;
use crate::entry::objects;
use crate::value::Value;

/// `WeakRef`.
#[rtse::class("WeakRef")]
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
