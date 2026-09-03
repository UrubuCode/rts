//! `stream.addAbortSignal(signal, stream)`.
//!
//! # Reuse-check
//!
//! `events.rs`'s `add_abort_listener` already worked out the whole mechanism —
//! `AbortSignal` is an ambient global with a real `EventTarget` surface, a
//! closure can carry exactly one captured value, and a signal already
//! aborted must fire the effect synchronously rather than through a
//! registration that will never see the event. This file is that same
//! recipe aimed at `stream.destroy` instead of a listener call, not a second
//! reading of `AbortSignal`.
//!
//! The abort REASON is read off `signal.reason` rather than built here: the
//! global `AbortSignal` machinery (`rts-std`'s `globals/events/abort.rs`)
//! already constructs the right value there — the caller's own reason when
//! `controller.abort(x)` gave one, a `DOMException` named `"AbortError"`
//! otherwise — and reaching for it is one property read against a second
//! `AbortError` construction that could name it differently.

use rts_core::entry;

use super::common::key;

/// `stream.addAbortSignal(signal, stream)` — returns `stream`, for chaining.
pub(super) extern "C" fn add_abort_signal(_e: u64, _this: u64, signal: u64, stream: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !is_abort_signal(signal) {
        entry::invalid_arg_instance("signal", "AbortSignal", signal);
        return absent;
    }
    let add_fn = entry::with_runtime(|context| entry::get_member(context, signal, "addEventListener"));
    if add_fn != absent {
        let callback = entry::closure_new(on_abort as *const () as usize as i64, stream);
        let once_options = entry::with_runtime(|context| {
            let options = entry::make_object(context);
            entry::put_member(context, options, "once", entry::boolean_value(true));
            options
        });
        entry::call(add_fn, signal, key("abort"), callback, once_options, absent);
        if entry::thrown() != 0 {
            return absent;
        }
    }
    if entry::to_boolean(entry::get_indexed(signal, key("aborted"))) {
        destroy_with(stream, signal);
    }
    stream
}

/// The listener `addEventListener` calls: `this` is the signal (the
/// dispatching `EventTarget`), which is where [`destroy_with`] reads
/// `.reason` from — see the module doc for why nothing here builds one.
extern "C" fn on_abort(stream: u64, signal: u64, _event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    destroy_with(stream, signal);
    entry::undefined_value()
}

fn destroy_with(stream: u64, signal: u64) {
    let reason = entry::get_indexed(signal, key("reason"));
    let destroy_fn = entry::with_runtime(|context| entry::get_member(context, stream, "destroy"));
    let absent = entry::undefined_value();
    if destroy_fn != absent {
        entry::call(destroy_fn, stream, reason, absent, absent, absent);
    }
}

/// Same probe `events.rs::is_abort_signal` makes, duplicated rather than
/// exported: it is one `instanceof` against the global `AbortSignal`
/// constructor, and `events` and `stream` share no other coupling worth
/// opening a door for.
fn is_abort_signal(value: u64) -> bool {
    let constructor = entry::with_runtime(|context| {
        let global = entry::global_object(context);
        entry::get_member(context, global, "AbortSignal")
    });
    constructor != entry::undefined_value() && entry::instance_of(value, constructor)
}
