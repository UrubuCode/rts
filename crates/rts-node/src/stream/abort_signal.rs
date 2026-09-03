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
//!
//! # Why this destroy path must NOT crash on an unlistened stream, and every
//! other `destroy(error)` path must
//!
//! Measured against real Node (`tests/claude-node-stream-abortsignal-crash.test.ts`):
//! `addAbortSignal(signal, s)` followed by `controller.abort()`, with NO
//! `'error'` listener anywhere a program wrote one, does not crash Node —
//! while a plain `s.destroy(new Error("boom"))` with no listener crashes
//! both engines identically (that IS the correct, shared behaviour; an
//! unhandled `'error'` event ending the process is `events::emit`'s own
//! rule and stream errors are meant to obey it). So the divergence is
//! narrower than "abort never crashes" — it is specific to THIS entry
//! point.
//!
//! The mechanism, once traced rather than guessed at: Node's own
//! `addAbortSignal` registers its abort handler AND calls `stream.finished`
//! internally, to unhook the abort listener once the stream is done — and
//! `finished`'s own implementation (mirrored here in `util::finished`)
//! attaches a real `'error'` listener as half of what "finished" means.
//! That listener is invisible from user code (nothing added it, nothing a
//! program can see by reading its own listener list before this call), but
//! it is not ABSENT — and `events::emit`'s crash test is exactly "is the
//! listener list empty", not "did the PROGRAM add one". So a stream that
//! ever passed through `addAbortSignal` is never truly unlistened for
//! `'error'` again, which is what stops the crash without changing what an
//! ordinary unlistened `destroy(err)` does.
//!
//! [`util::attach_error`] is that same relay, called here with an ABSENT
//! callback: this call does not want `finished`'s notification, only the
//! listener slot it leaves behind. One collision named rather than hidden —
//! `util::attach_error`/`attach_finished` share ONE property slot per
//! relay kind across every caller on a stream (`util.rs`'s own doc already
//! states this is not deduplicated), so a program that calls
//! `stream.finished`/`stream.pipeline` on the SAME stream after this runs
//! overwrites the slot this leaves behind — harmless for what THIS call
//! needs (an absent callback does nothing whether it fires zero or three
//! times), but it means two callers' intents share one slot, same as any
//! other pair of `finished`/`pipeline` calls on one stream already did.

use rts_core::entry;

use super::common::key;

/// `stream.addAbortSignal(signal, stream)` — returns `stream`, for chaining.
pub(super) extern "C" fn add_abort_signal(_e: u64, _this: u64, signal: u64, stream: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !is_abort_signal(signal) {
        entry::invalid_arg_instance("signal", "AbortSignal", signal);
        return absent;
    }
    // The crash guard — see the module doc's "must NOT crash" section. Placed
    // before EITHER branch below (the deferred `addEventListener` case and the
    // immediate already-aborted case), because both end at the same
    // `destroy_with` and both need the listener slot already in place before
    // that runs.
    super::util::attach_error(stream, absent);
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
