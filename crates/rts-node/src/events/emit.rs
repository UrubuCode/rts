//! `emitter.emit(eventName, ...args)`, and what happens when `'error'` has
//! nobody listening — see the parent module's doc for why that ends the
//! process rather than throwing a value compiled code could catch.

use rts_core::entry;

/// `emitter.emit(eventName, ...args)` — up to three args, the most this
/// module's four call slots leave room for once the receiver and event name
/// each take one. `'error'` with zero listeners ends the process; see the
/// module doc.
pub(super) extern "C" fn emit(_e: u64, this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let wrappers = super::collect_array(array);
    if wrappers.is_empty() {
        if entry::text_of(event).as_deref() == Some("error") {
            crash_on_unhandled_error(a0);
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
    }
    entry::boolean_value(true)
}

/// The same diagnostic-then-exit real Node gives an unhandled `'error'`
/// event — see the module doc for why a native ends the process instead of
/// throwing.
fn crash_on_unhandled_error(error: u64) -> ! {
    match entry::described(error) {
        Some(text) => eprintln!("rts: uncaught 'error' event: {text}"),
        None => eprintln!("rts: uncaught 'error' event: an object"),
    }
    std::process::exit(1)
}
