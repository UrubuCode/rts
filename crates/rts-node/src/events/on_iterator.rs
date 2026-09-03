//! `events.on(emitter, eventName, options?)` — an async iterator over every
//! matching event, `for await`-ready.
//!
//! # The shape, and where it is not invented here
//!
//! `node:timers/promises`' `setInterval` is the exact precedent: not a
//! promise itself but an object carrying `next()`/`return()`/
//! `[Symbol.asyncIterator]`, where each `next()` call is what mints and
//! answers ONE promise. This file is that shape again, over a listener
//! instead of a timer — `next()` either answers what is already buffered, or
//! parks a promise the next matching `emit()` will settle.
//!
//! # Buffering, because `for await` pulls slower than `emit` can push
//!
//! Two listeners are installed once, at [`on`] — not once per `next()` call,
//! the way [`super::once_promise`] installs and tears down its own per call.
//! An event that arrives with no `next()` outstanding goes on `@@#on_queue`
//! rather than being dropped; a `next()` call that finds the queue non-empty
//! answers immediately from its front. This is what makes
//! `for (const [chunk] of events.on(ee, 'data')) { await slow(chunk) }` not
//! silently lose an event that arrived while `slow` was running.
//!
//! # `'error'` ends iteration rather than being yielded
//!
//! Matching real Node: the next `next()` call's promise REJECTS with the error
//! (immediately, if nothing is buffered ahead of it; after the buffered items
//! drain, otherwise — see [`iterator_next`]), and both listeners this call
//! installed are removed. A program that wants `for await` to keep running
//! past an `'error'` has to catch it and call `events.on` again; that is
//! Node's own contract, not a gap of this file's.
//!
//! # Not implemented, by name
//!
//! **`options.signal`.** [`super::once_promise`] wires a real
//! `signal.addEventListener('abort', …)` because a single promise has no
//! later call to re-check from; this iterator DOES have one — every `next()`
//! — so the honest way to add it is to poll `signal.aborted` there, the
//! `node:timers/promises` way. Left out of this pass rather than added
//! untested: a wrong answer here would look like a hang (`for await` that
//! never sees its abort), which is worse than an iterator that plainly does
//! not support it yet.
//!
//! **`options.close`, `options.highWaterMark`, `options.lowWaterMark`.**
//! Node's backpressure and early-termination-by-event-name knobs; the queue
//! here is unbounded and only `.return()` or an error/abort ends iteration.
//!
//! **`iterator.throw()`** — the async-iterator protocol's third method, for
//! the same reason `node:timers/promises`' own doc gives: `for await` reaches
//! `next` and `return` and never it.
//!
//! **A real WHATWG `EventTarget` as `emitter`.** See
//! [`super::once_promise`]'s doc for why this is a refusal
//! (`ERR_INVALID_ARG_TYPE`) and not a silent no-op — the same
//! [`super::is_emitter_like`] check applies here.
//!
//! **More than three emitted arguments** — [`super::packed_args`]'s cap,
//! [`super::emit::emit`]'s own limit inherited rather than repeated.

use rts_core::entry::{self, Context, Provided};

/// The target emitter, hidden the way the parent module's own `__events__`
/// already is not — `@@`-prefixed keys are filtered from enumeration by
/// `rts-core`'s `symbol.rs`, which is what keeps `Object.keys(iterator)` from
/// listing this module's own bookkeeping.
const EMITTER: &str = "@@#on_emitter";
/// The event name being awaited.
const EVENT: &str = "@@#on_event";
/// Buffered `[args...]` arrays, oldest first, waiting for a `next()` to claim
/// them.
const QUEUE: &str = "@@#on_queue";
/// The promise a `next()` call minted when the queue was empty, or
/// `undefined` when none is outstanding.
const PENDING: &str = "@@#on_pending";
/// An `'error'` value waiting for the next `next()` call to reject with, once
/// the queue ahead of it has drained.
const ERROR: &str = "@@#on_error";
/// Whether iteration has ended — by `'error'`, or by `.return()`.
const DONE: &str = "@@#on_done";
/// The closure registered on `eventName`, kept so `.return()` can remove it.
const LISTENER: &str = "@@#on_listener";
/// The closure registered on `'error'`, or `undefined` when `eventName` IS
/// `'error'` (nothing to watch separately). Kept for the same reason as
/// [`LISTENER`].
const ERROR_LISTENER: &str = "@@#on_error_listener";

/// `events.on(emitter, eventName, options?)` — see the module doc for what
/// `options` this pass does not read.
pub(super) extern "C" fn on(_e: u64, _this: u64, emitter: u64, event: u64, _options: u64, _d: u64) -> u64 {
    if !super::is_emitter_like(emitter) {
        crate::errors::invalid_arg_type("emitter", "EventEmitter", emitter);
        return entry::undefined_value();
    }
    if !super::valid_event_name(event) {
        crate::errors::invalid_arg_type("eventName", "string or symbol", event);
        return entry::undefined_value();
    }
    let iterator = entry::with_runtime(|context| {
        let members: &[(&str, Provided)] = &[("next", iterator_next), ("return", iterator_return)];
        let iterator = entry::make_namespace(context, members);
        // `[Symbol.asyncIterator]() { return this }` — the wire spelling
        // `timers/promises.rs` already documents: a member written under the
        // `@@asyncIterator` key IS the property `for await` looks up.
        let itself = entry::make_callable(context, iterator_self);
        entry::put_member(context, iterator, "@@asyncIterator", itself);
        entry::put_member(context, iterator, EMITTER, emitter);
        entry::put_member(context, iterator, EVENT, event);
        let empty = entry::make_array_in(context, Vec::new());
        entry::put_member(context, iterator, QUEUE, empty);
        let absent = entry::undefined_in(context);
        entry::put_member(context, iterator, PENDING, absent);
        entry::put_member(context, iterator, ERROR, absent);
        entry::put_member(context, iterator, DONE, entry::boolean_value(false));
        entry::put_member(context, iterator, ERROR_LISTENER, absent);
        iterator
    });
    // Built OUTSIDE the borrow above: `closure_new` takes the runtime borrow
    // itself, and taking it twice aborts the process rather than failing.
    let listener = entry::closure_new(on_event as *const () as usize as i64, iterator);
    super::add_listener(emitter, event, listener, false, false);
    entry::with_runtime(|context| entry::put_member(context, iterator, LISTENER, listener));
    if entry::text_of(event).as_deref() != Some("error") {
        let error_listener = entry::closure_new(on_error as *const () as usize as i64, iterator);
        let error_key = super::string_key("error");
        super::add_listener(emitter, error_key, error_listener, false, false);
        entry::with_runtime(|context| entry::put_member(context, iterator, ERROR_LISTENER, error_listener));
    }
    iterator
}

/// `iterator[Symbol.asyncIterator]()` — the iterator itself.
extern "C" fn iterator_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `iterator.next()` — the buffered head, a rejection carried over from
/// `'error'`, `{ done: true }` once ended, or a fresh promise the next
/// matching event will settle.
extern "C" fn iterator_next(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (pending_error, queue_array, done) = entry::with_runtime(|context| {
        let error = entry::get_member(context, this, ERROR);
        let queue = entry::get_member(context, this, QUEUE);
        let done_flag = entry::get_member(context, this, DONE);
        let done = entry::to_boolean_in(context, done_flag);
        (error, queue, done)
    });
    // `collect_array` walks the array with its OWN borrow (`get_indexed`
    // opens one internally) — taken here, after the tuple above already
    // dropped its own, never nested inside it. Nesting is what
    // `promise_await`'s own doc and every listener in this file are careful
    // about: the runtime borrow aborts the process on re-entry rather than
    // failing.
    let mut items = super::collect_array(queue_array);
    let absent = entry::undefined_value();
    // `'error'` waits for whatever was already buffered to drain first, so a
    // consumer sees every event that arrived before the error before it sees
    // the error itself — the order they actually happened in.
    if pending_error != absent && items.is_empty() {
        entry::with_runtime(|context| {
            let cleared = entry::undefined_in(context);
            entry::put_member(context, this, ERROR, cleared);
        });
        return rejected(pending_error);
    }
    if !items.is_empty() {
        // `first` is already the packed-argument JS array [`on_event`] built
        // when it buffered this occurrence — it is the VALUE, not a list of
        // values to wrap again.
        let first = items.remove(0);
        let result = entry::with_runtime(|context| {
            let rest = entry::make_array_in(context, items);
            entry::put_member(context, this, QUEUE, rest);
            step_in(context, first, false)
        });
        return fulfilled(result);
    }
    if done {
        let result = entry::with_runtime(|context| {
            let value = entry::undefined_in(context);
            step_in(context, value, true)
        });
        return fulfilled(result);
    }
    // Nothing buffered and not yet ended: park a promise [`on_event`] or
    // [`on_error`] will settle whenever something next happens.
    let promise = entry::promise_new();
    entry::with_runtime(|context| entry::put_member(context, this, PENDING, promise));
    promise
}

/// `iterator.return()` — what `break`ing out of a `for await` calls. Removes
/// both listeners and, if a `next()` is parked with nothing left to answer
/// it, resolves that one to `{ done: true }` too rather than leaving it
/// stalled forever.
extern "C" fn iterator_return(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (emitter, event, listener, error_listener, pending) = entry::with_runtime(|context| {
        (
            entry::get_member(context, this, EMITTER),
            entry::get_member(context, this, EVENT),
            entry::get_member(context, this, LISTENER),
            entry::get_member(context, this, ERROR_LISTENER),
            entry::get_member(context, this, PENDING),
        )
    });
    entry::with_runtime(|context| {
        entry::put_member(context, this, DONE, entry::boolean_value(true));
        let cleared = entry::undefined_in(context);
        entry::put_member(context, this, PENDING, cleared);
    });
    super::listener::remove_listener(0, emitter, event, listener, 0, 0);
    let absent = entry::undefined_value();
    if error_listener != absent {
        super::listener::remove_listener(0, emitter, super::string_key("error"), error_listener, 0, 0);
    }
    let result = entry::with_runtime(|context| {
        let value = entry::undefined_in(context);
        step_in(context, value, true)
    });
    if pending != absent {
        entry::promise_settle(pending, result, 0);
    }
    fulfilled(result)
}

/// The event listener installed by [`on`]. Settles a parked `next()`
/// directly, or buffers the arguments for the next one to claim.
extern "C" fn on_event(iterator: u64, _this: u64, a0: u64, a1: u64, a2: u64, _d: u64) -> u64 {
    let args = super::packed_args(a0, a1, a2);
    let pending = entry::with_runtime(|context| entry::get_member(context, iterator, PENDING));
    let absent = entry::undefined_value();
    if pending != absent {
        let result = entry::with_runtime(|context| {
            let cleared = entry::undefined_in(context);
            entry::put_member(context, iterator, PENDING, cleared);
            let value = entry::make_array_in(context, args);
            step_in(context, value, false)
        });
        entry::promise_settle(pending, result, 0);
    } else {
        let queue = entry::with_runtime(|context| entry::get_member(context, iterator, QUEUE));
        let value = entry::with_runtime(|context| entry::make_array_in(context, args));
        entry::array_append(queue, value);
    }
    absent
}

/// The `'error'` listener installed by [`on`]. Ends iteration and settles or
/// stores the rejection — see [`iterator_next`] for which.
extern "C" fn on_error(iterator: u64, _this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (emitter, event, listener, pending) = entry::with_runtime(|context| {
        (
            entry::get_member(context, iterator, EMITTER),
            entry::get_member(context, iterator, EVENT),
            entry::get_member(context, iterator, LISTENER),
            entry::get_member(context, iterator, PENDING),
        )
    });
    entry::with_runtime(|context| entry::put_member(context, iterator, DONE, entry::boolean_value(true)));
    super::listener::remove_listener(0, emitter, event, listener, 0, 0);
    let absent = entry::undefined_value();
    if pending != absent {
        entry::with_runtime(|context| {
            let cleared = entry::undefined_in(context);
            entry::put_member(context, iterator, PENDING, cleared);
        });
        entry::promise_settle(pending, error, 1);
    } else {
        entry::with_runtime(|context| entry::put_member(context, iterator, ERROR, error));
    }
    absent
}

/// One `{ value, done }` iterator result, from a context already in hand.
fn step_in(context: &mut Context, value: u64, done: bool) -> u64 {
    let result = entry::make_object(context);
    entry::put_member(context, result, "value", value);
    entry::put_member(context, result, "done", entry::boolean_value(done));
    result
}

/// A promise already fulfilled with `value`.
fn fulfilled(value: u64) -> u64 {
    let promise = entry::promise_new();
    entry::promise_settle(promise, value, 0);
    promise
}

/// A promise already rejected with `value`.
fn rejected(value: u64) -> u64 {
    let promise = entry::promise_new();
    entry::promise_settle(promise, value, 1);
    promise
}
