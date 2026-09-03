//! `events.once(emitter, name, options?)` — a `Promise` for the next matching
//! event.
//!
//! # The premise this crate's own audit asked to be checked first
//!
//! `rts_core::entry::promise_new`/`promise_settle` already exist and are
//! already used from a native this way — `crates/rts-node/src/stream/
//! promises.rs`'s `pipeline`/`finished`, and `timers/promises.rs`'s whole
//! module. The refusal this file replaces (this crate's previous top-of-file
//! doc: *"Building either needs constructing a `Promise` and driving it from a
//! native, which is not an operation this crate's entry surface exposes"*) was
//! stale, not wrong-headed — written before either of those two landed, and
//! never re-checked after. This module is that re-check.
//!
//! # What settles the promise, and when
//!
//! Nothing here polls. A listener is registered on the emitter through the
//! ordinary [`super::add_listener`] storage, and `EventEmitter::emit` calls a
//! listener SYNCHRONOUSLY — on whatever thread is already running the program.
//! So `await events.once(ee, 'x')` finishes the instant something already
//! running calls `ee.emit('x', …)`, whether that is a line of the program
//! after this call, or a `setTimeout` callback `rts_core::entry::promise_await`
//! reaches by pumping `node:timers`' own loop source while it waits — the same
//! mechanism that makes `await new Promise(r => setTimeout(r, 5))` finish
//! rather than report a stalled promise. Nothing in this file drives that
//! loop; it only registers what the loop, once turning, will call.
//!
//! # Three listeners, at most one of which ever fires
//!
//! The event itself, `'error'` (unless the event IS `'error'`), and — with
//! `options.signal` — the signal's `'abort'`. Whichever fires first settles
//! the promise and [`cleanup`] removes the other two, so a later `'error'` on
//! an emitter this call already resolved through does not try to settle an
//! already-settled promise, and does not leak a listener nobody will ever see
//! fire.
//!
//! # `options.signal`
//!
//! Real, not polled — `signal.addEventListener('abort', …, { once: true })`,
//! the SAME real subscription [`super::abort::add_abort_listener`] already
//! uses. `node:timers/promises`'s `options.signal` can afford to poll because
//! it is always re-checked at that module's own next call; this promise has no
//! such next call to be re-checked from, so a value that only LOOKS like a
//! signal (something carrying `aborted` with no real `addEventListener`) is
//! refused up front — see [`super::abort::is_abort_signal`]'s own doc for why
//! that check, and not the looser one, is the right one here.
//!
//! An already-aborted signal rejects immediately, with `signal.reason` (or a
//! constructed `Error` if a program built a signal with none).
//!
//! # Not implemented, by name
//!
//! **A real WHATWG `EventTarget` as `emitter`.** Real Node accepts one —
//! `events.once(document, 'click')` — by dispatching through
//! `addEventListener` instead of `.on()`. This module refuses one instead of
//! accepting it and doing nothing: `emitter` is required to carry a callable
//! `.on` (see [`super::is_emitter_like`]), which a plain `EventTarget` does
//! not — it stores listeners under `__listeners__` and dispatches through
//! `dispatchEvent`, a completely different mechanism from `__events__`/`emit`.
//! Registering onto `__events__` on one anyway would build a promise that
//! looks right and never settles, which is exactly the "hollow surface" this
//! crate's own rule refuses. `ERR_INVALID_ARG_TYPE` is thrown instead.
//!
//! **More than three emitted arguments.** [`super::packed_args`] collects
//! `a0..a2`, the same cap [`super::emit::emit`] itself already documents.

use rts_core::entry;

/// `events.once(emitter, name, options?)`.
pub(super) extern "C" fn once(_e: u64, _this: u64, emitter: u64, name: u64, options: u64, _d: u64) -> u64 {
    if !super::is_emitter_like(emitter) {
        crate::errors::invalid_arg_type("emitter", "EventEmitter", emitter);
        return entry::undefined_value();
    }
    if !super::valid_event_name(name) {
        crate::errors::invalid_arg_type("name", "string or symbol", name);
        return entry::undefined_value();
    }
    let signal = signal_of(options);
    if let Some(signal) = signal {
        if !super::abort::is_abort_signal(signal) {
            crate::errors::invalid_arg_instance("options.signal", "AbortSignal", signal);
            return entry::undefined_value();
        }
        if is_aborted(signal) {
            let promise = entry::promise_new();
            let reason = abort_reason(signal);
            entry::promise_settle(promise, reason, 1);
            return promise;
        }
    }

    // Minted first and outside every borrow below: `promise_new` and
    // `closure_new` each take the runtime borrow themselves, and taking it
    // twice aborts the process rather than failing — see the module doc.
    let promise = entry::promise_new();
    let state = entry::with_runtime(|context| {
        let state = entry::make_object(context);
        entry::put_member(context, state, "promise", promise);
        entry::put_member(context, state, "emitter", emitter);
        entry::put_member(context, state, "event", name);
        let absent = entry::undefined_in(context);
        entry::put_member(context, state, "errorListener", absent);
        entry::put_member(context, state, "signal", absent);
        entry::put_member(context, state, "abortListener", absent);
        state
    });

    let event_listener = entry::closure_new(on_event as *const () as usize as i64, state);
    super::add_listener(emitter, name, event_listener, true, false);
    entry::with_runtime(|context| entry::put_member(context, state, "eventListener", event_listener));

    // `'error'` watches itself when it IS the awaited name — a second
    // registration on the same event would just be two listeners racing to
    // settle the one promise first.
    if entry::text_of(name).as_deref() != Some("error") {
        let error_key = super::string_key("error");
        let error_listener = entry::closure_new(on_error as *const () as usize as i64, state);
        super::add_listener(emitter, error_key, error_listener, true, false);
        entry::with_runtime(|context| entry::put_member(context, state, "errorListener", error_listener));
    }

    if let Some(signal) = signal {
        let abort_listener = entry::closure_new(on_abort as *const () as usize as i64, state);
        let (add, once_opts) = entry::with_runtime(|context| {
            let opts = entry::make_object(context);
            entry::put_member(context, opts, "once", entry::boolean_value(true));
            (entry::get_member(context, signal, "addEventListener"), opts)
        });
        let absent = entry::undefined_value();
        entry::call(add, signal, super::string_key("abort"), abort_listener, once_opts, absent);
        entry::with_runtime(|context| {
            entry::put_member(context, state, "signal", signal);
            entry::put_member(context, state, "abortListener", abort_listener);
        });
    }

    promise
}

/// Fired when the awaited event arrives. Settles with the packed argument
/// array and undoes the other two registrations — see [`cleanup`].
extern "C" fn on_event(state: u64, _this: u64, a0: u64, a1: u64, a2: u64, _d: u64) -> u64 {
    let (promise, args) =
        entry::with_runtime(|context| (entry::get_member(context, state, "promise"), super::packed_args(a0, a1, a2)));
    cleanup(state);
    let array = entry::make_array(args);
    entry::promise_settle(promise, array, 0);
    entry::undefined_value()
}

/// Fired when `'error'` arrives first. Rejects with the error value itself —
/// the same value a real Node listener would have received.
extern "C" fn on_error(state: u64, _this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let promise = entry::with_runtime(|context| entry::get_member(context, state, "promise"));
    cleanup(state);
    entry::promise_settle(promise, error, 1);
    entry::undefined_value()
}

/// Fired when `options.signal` aborts first. Rejects with the signal's
/// reason.
extern "C" fn on_abort(state: u64, _this: u64, _event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (promise, signal) = entry::with_runtime(|context| {
        (entry::get_member(context, state, "promise"), entry::get_member(context, state, "signal"))
    });
    cleanup(state);
    let reason = abort_reason(signal);
    entry::promise_settle(promise, reason, 1);
    entry::undefined_value()
}

/// Removes every registration this call made that has not already fired.
/// Idempotent against the one that HAS: `emit()` drops a `once` listener from
/// storage before calling it, so the wrapper that just ran is already gone by
/// the time this looks for it, and removing an absent one is a no-op.
fn cleanup(state: u64) {
    let (emitter, event, event_listener, error_listener, signal, abort_listener) = entry::with_runtime(|context| {
        (
            entry::get_member(context, state, "emitter"),
            entry::get_member(context, state, "event"),
            entry::get_member(context, state, "eventListener"),
            entry::get_member(context, state, "errorListener"),
            entry::get_member(context, state, "signal"),
            entry::get_member(context, state, "abortListener"),
        )
    });
    let absent = entry::undefined_value();
    super::listener::remove_listener(0, emitter, event, event_listener, 0, 0);
    if error_listener != absent {
        super::listener::remove_listener(0, emitter, super::string_key("error"), error_listener, 0, 0);
    }
    if signal != absent && abort_listener != absent {
        let remove = entry::with_runtime(|context| entry::get_member(context, signal, "removeEventListener"));
        entry::call(remove, signal, super::string_key("abort"), abort_listener, absent, absent);
    }
}

/// `options.signal`, when `options` is an object carrying one.
fn signal_of(options: u64) -> Option<u64> {
    entry::with_runtime(|context| {
        if !entry::is_object(context, options) {
            return None;
        }
        let signal = entry::get_member(context, options, "signal");
        match entry::is_object(context, signal) {
            true => Some(signal),
            false => None,
        }
    })
}

/// Whether `signal.aborted` is already `true`.
fn is_aborted(signal: u64) -> bool {
    let value = entry::with_runtime(|context| entry::get_member(context, signal, "aborted"));
    entry::to_boolean(value)
}

/// `signal.reason`, or a constructed `Error` when a program built a signal
/// with none.
fn abort_reason(signal: u64) -> u64 {
    let reason = entry::with_runtime(|context| entry::get_member(context, signal, "reason"));
    match reason == entry::undefined_value() {
        true => entry::make_named_error("Error", "The operation was aborted").unwrap_or(reason),
        false => reason,
    }
}
