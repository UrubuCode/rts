//! `emitter.emit(eventName, ...args)`, and what happens when `'error'` has
//! nobody listening — a catchable throw, which is what the parent module's doc
//! now records and used to deny.

use rts_core::entry;

/// `emitter.emit(eventName, ...args)` — up to three args, the most this
/// module's four call slots leave room for once the receiver and event name
/// each take one.
///
/// `'error'` with zero listeners raises: the error value itself when it is one,
/// otherwise `Error [ERR_UNHANDLED_ERROR]` carrying it on `.context`. See
/// [`entry::unhandled_error`], which is the one copy of that contract both
/// `EventEmitter`s in this workspace reach.
pub(super) extern "C" fn emit(_e: u64, this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let wrappers = super::collect_array(array);
    if wrappers.is_empty() {
        if entry::text_of(event).as_deref() == Some("error") {
            entry::unhandled_error(a0);
            // The raise is recorded, not unwound: the compiled call site
            // re-raises once this native returns. `false` is what Node's own
            // `emit` would have answered for an event with no listener, and it
            // is never read — the throw is in flight.
            return entry::boolean_value(false);
        }
        return entry::boolean_value(false);
    }
    // `once` listeners are dropped from storage before any of them runs, so a
    // listener re-entering `emit` for the same event does not see them twice.
    let remaining: Vec<u64> = wrappers.iter().copied().filter(|&w| !super::wrapper_once(w)).collect();
    if remaining.len() != wrappers.len() {
        super::store_array(events, event, remaining);
    }
    let absent = entry::undefined_value();
    for wrapper in wrappers {
        let listener = super::wrapper_fn(wrapper);
        entry::call(listener, this, a0, a1, a2, absent);
        // `rts-core`'s rule 8: a native that called user code asks whether it
        // threw before carrying on. Node stops at the listener that threw —
        // the remaining listeners for this event do not run — and without this
        // check they all did, because `entry::call` answers `undefined` for a
        // call that did not finish and `undefined` is a value.
        // `entry::thrown()` and not `throw::in_flight()`: that one is private to
        // `rts-core`, and this is the same question it asks — the flag read
        // without clearing it, so the compiled call site above still re-raises.
        if entry::thrown() != 0 {
            return entry::boolean_value(true);
        }
    }
    entry::boolean_value(true)
}
