//! `node:timers/promises` — the promise-returning half of [`super`].
//!
//! # Why it is here and not beside it
//!
//! Because it is the SAME queue. `timersPromises.setTimeout` differs from
//! `timers.setTimeout` in what happens when the deadline arrives — a promise is
//! fulfilled instead of a function being called — and in nothing else: the
//! ordering, the clamping, the per-thread table and the loop source are all one
//! mechanism, and a sibling module would have needed its own copy of each. A
//! program that mixes `setTimeout(cb, 5)` with `await sleep(5)` gets one order
//! rather than two tables guessing at each other's.
//!
//! So the whole of this file is: read the arguments Node documents, mint a
//! promise, and put a [`super::Deliver::Settle`] in the parent's table.
//!
//! # Reuse-check
//!
//! - `entry::promise_new` / `entry::promise_settle` **already exist** and are
//!   what this calls. [`super`]'s module doc used to refuse this entire module
//!   for want of them; that refusal was stale rather than wrong-headed, and the
//!   note there says so.
//! - `rts-cranelift`'s `sched` owns promise identity, wait sets and settlement —
//!   reached through those two entry points, never re-derived here.
//! - The waiting is the host's: `promise_await` pumps loop sources and sleeps,
//!   and `node:timers` is registered as one. That is why `await setTimeout(50)`
//!   finishes rather than reporting a stalled promise, and it costs this module
//!   nothing.
//! - Ids come from the parent's `NEXT_ID`. No second numbering.
//!
//! # `options.signal` — supported, with its timing stated
//!
//! An aborted signal REJECTS the promise, with the signal's own `reason`. It is
//! noticed by polling in [`super::pump`] rather than delivered at the moment of
//! the abort, because `AbortSignal` records an abort as two plain properties and
//! offers no registry to subscribe to; `super::reject_aborted` documents what
//! that costs.
//!
//! A signal is taken to be "an object carrying `aborted`". There is no brand
//! check available to a host crate, so `{ aborted: true }` is honoured and Node
//! would refuse it with `ERR_INVALID_ARG_TYPE`. That is a wrong ACCEPT, not a
//! wrong answer, and the alternative — refusing everything but a real
//! `AbortSignal` — cannot be written without a brand this crate cannot reach.
//!
//! # Not implemented, by name
//!
//! `iterator.throw()` — the async-iterator protocol's third method. `for await`
//! reaches `next` and `return` and never it, and an implementation that
//! swallowed a thrown value would be a lie about where the throw went.
//!
//! An interval's tick is scheduled by `next()` rather than run free, so a slow
//! consumer sees `delay` between the END of one iteration and the next, where
//! Node measures from the previous TICK and can deliver several in a row after a
//! stall. Sequential `for await` is indistinguishable; a body that takes longer
//! than the delay is not.
//!
//! `options.ref` — `unref` semantics need an event loop that can be kept alive
//! by a pending timer, and this host's is a drain rather than a loop. The
//! parent's module doc states the same gap for `.unref()`.

use std::time::{Duration, Instant};

use rts_core_rwk::entry::{self, Context, Provided};

use super::{Deliver, clamp_delay, forget, pump, register};

/// Where an interval iterator keeps what it needs, hidden from a program.
///
/// The `@@` prefix is not decoration: `rts-core-rwk`'s `symbol.rs` filters keys
/// that begin with it out of enumeration, which is how a private class member is
/// spelled. So these are invisible to `Object.keys` and to `JSON.stringify`
/// without a side table keyed by object identity — and, unlike a side table,
/// each is a property of the iterator, so the collector traces the value it
/// holds through the object that owns it.
const DELAY: &str = "@@#interval_delay";
/// The value each tick yields.
const VALUE: &str = "@@#interval_value";
/// The `AbortSignal` given at construction, or `undefined`.
const SIGNAL: &str = "@@#interval_signal";
/// The raw id of the tick currently outstanding, or `undefined`.
const TIMER: &str = "@@#interval_timer";
/// Whether `return()` has been called.
const DONE: &str = "@@#interval_done";

/// The namespace `node:timers/promises` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("setTimeout", set_timeout),
        ("setImmediate", set_immediate),
        ("setInterval", set_interval),
    ];
    let made = entry::make_namespace(context, members);
    let scheduler: &[(&str, Provided)] = &[("wait", scheduler_wait), ("yield", scheduler_yield)];
    let scheduling = entry::make_namespace(context, scheduler);
    entry::put_member(context, made, "scheduler", scheduling);
    made
}

/// `setTimeout(delay = 1, value, options)` → a promise for `value`.
extern "C" fn set_timeout(
    _e: u64,
    _this: u64,
    delay: u64,
    value: u64,
    options: u64,
    _d: u64,
) -> u64 {
    pump();
    after(clamp_delay(delay, 0), value, signal_in(options))
}

/// `setImmediate(value, options)` → a promise for `value`, due at the next pump.
extern "C" fn set_immediate(
    _e: u64,
    _this: u64,
    value: u64,
    options: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    pump();
    at(Instant::now(), value, signal_in(options))
}

/// `scheduler.wait(delay, options)` → a promise for `undefined`.
///
/// Node defines it as `setTimeout(delay, undefined, options)` and that is
/// literally what it is here — not a second implementation of waiting.
extern "C" fn scheduler_wait(
    _e: u64,
    _this: u64,
    delay: u64,
    options: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    pump();
    after(
        clamp_delay(delay, 0),
        entry::undefined_value(),
        signal_in(options),
    )
}

/// `scheduler.yield()` → a promise for `undefined`, due at the next pump.
///
/// No `options` at all, which is the specification's own shape: this one cannot
/// be cancelled because there is no window in which to cancel it.
extern "C" fn scheduler_yield(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    pump();
    at(Instant::now(), entry::undefined_value(), None)
}

/// A promise fulfilled with `value` after `delay_ms`.
fn after(delay_ms: u64, value: u64, signal: Option<u64>) -> u64 {
    at(
        Instant::now() + Duration::from_millis(delay_ms),
        value,
        signal,
    )
}

/// A promise fulfilled with `value` at `deadline`.
///
/// The promise is minted BEFORE the timer is registered, and outside any borrow:
/// `promise_new` takes the runtime borrow itself, and taking it twice aborts the
/// process rather than failing.
fn at(deadline: Instant, value: u64, signal: Option<u64>) -> u64 {
    let promise = entry::promise_new();
    register(
        Deliver::Settle {
            promise,
            value,
            signal,
        },
        deadline,
        None,
    );
    promise
}

/// A promise already fulfilled with `value`.
///
/// Used where the answer is known and the protocol still demands a promise —
/// `next()` after `return()`, and `return()` itself.
fn fulfilled(value: u64) -> u64 {
    let promise = entry::promise_new();
    entry::promise_settle(promise, value, 0);
    promise
}

/// `options.signal`, when `options` is an object carrying one.
///
/// See the module doc for why "carries `aborted`" stands in for a brand check.
fn signal_in(options: u64) -> Option<u64> {
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

/// `setInterval(delay = 1, value, options)` → an async iterable.
///
/// Not a promise, which is the one member of this module that breaks the shape:
/// `for await (const v of setInterval(100, 'tick'))` yields forever, so what it
/// answers is the ITERATOR and each `next()` is the promise.
extern "C" fn set_interval(
    _e: u64,
    _this: u64,
    delay: u64,
    value: u64,
    options: u64,
    _d: u64,
) -> u64 {
    pump();
    let period = clamp_delay(delay, 0);
    let signal = signal_in(options).unwrap_or_else(entry::undefined_value);
    entry::with_runtime(|context| {
        let members: &[(&str, Provided)] =
            &[("next", iterator_next), ("return", iterator_return)];
        let iterator = entry::make_namespace(context, members);
        // `[Symbol.asyncIterator]() { return this }` — what `for await` reaches
        // for first. The key text IS the wire format: a `[Symbol.asyncIterator]`
        // member of a class body writes `@@asyncIterator`, so a member written
        // here under that name is the same property the language looks up.
        let itself = entry::make_callable(context, iterator_self);
        entry::put_member(context, iterator, "@@asyncIterator", itself);
        let millis = entry::make_number(period as f64);
        entry::put_member(context, iterator, DELAY, millis);
        entry::put_member(context, iterator, VALUE, value);
        entry::put_member(context, iterator, SIGNAL, signal);
        let nothing = entry::undefined_in(context);
        entry::put_member(context, iterator, TIMER, nothing);
        let running = entry::boolean_value(false);
        entry::put_member(context, iterator, DONE, running);
        iterator
    })
}

/// `iterator[Symbol.asyncIterator]()` — the iterator itself.
extern "C" fn iterator_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `iterator.next()` — a promise for `{ value, done: false }`, one tick away.
extern "C" fn iterator_next(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    pump();
    let closed = entry::with_runtime(|context| {
        entry::get_member(context, this, DONE) == entry::boolean_value(true)
    });
    if closed {
        return fulfilled(finished());
    }
    let (delay, result, signal) = entry::with_runtime(|context| {
        let delay = entry::number_of(entry::get_member(context, this, DELAY)).unwrap_or(1.0);
        let value = entry::get_member(context, this, VALUE);
        let result = step(context, value, false);
        let signal = entry::get_member(context, this, SIGNAL);
        let signal = match entry::is_object(context, signal) {
            true => Some(signal),
            false => None,
        };
        (delay, result, signal)
    });
    let promise = entry::promise_new();
    let id = register(
        Deliver::Settle {
            promise,
            value: result,
            signal,
        },
        Instant::now() + Duration::from_millis(delay as u64),
        None,
    );
    // Remembered so `return()` can cancel a tick nobody will wait for. Written
    // after the timer exists rather than before, so the property never names an
    // id that was not registered.
    entry::with_runtime(|context| {
        let held = entry::make_number(id as f64);
        entry::put_member(context, this, TIMER, held);
    });
    promise
}

/// `iterator.return()` — stops the interval, answers `{ done: true }`.
///
/// What `break` out of a `for await` calls. The outstanding tick is forgotten
/// rather than left to fire: it would fulfil a promise nothing awaits, and it
/// would hold `source` open for a delay the program has already abandoned.
extern "C" fn iterator_return(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let outstanding = entry::with_runtime(|context| {
        let done = entry::boolean_value(true);
        entry::put_member(context, this, DONE, done);
        let held = entry::number_of(entry::get_member(context, this, TIMER));
        let nothing = entry::undefined_in(context);
        entry::put_member(context, this, TIMER, nothing);
        held
    });
    if let Some(id) = outstanding {
        forget(id as u64);
    }
    fulfilled(finished())
}

/// `{ value: undefined, done: true }`.
fn finished() -> u64 {
    entry::with_runtime(|context| {
        let nothing = entry::undefined_in(context);
        step(context, nothing, true)
    })
}

/// One iterator result object, from a context already in hand.
fn step(context: &mut Context, value: u64, done: bool) -> u64 {
    let result = entry::make_object(context);
    entry::put_member(context, result, "value", value);
    let ended = entry::boolean_value(done);
    entry::put_member(context, result, "done", ended);
    result
}
