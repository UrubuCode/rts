//! When `'end'` fires, and `Readable[Symbol.asyncIterator]` — the two halves
//! of flowing mode that a synchronous native cannot answer on its own.
//!
//! # Reuse-check (the `reuse-check` skill's search, and its answer)
//!
//! - **A deferred callback queue**: `rts-cranelift`'s `src/sched/` was read and
//!   does NOT answer it — it owns promise identity and the order continuations
//!   run in, not an externally-triggered queue a `node:` module owns. Exactly
//!   the finding `timers/mod.rs`'s own reuse-check records.
//! - **The loop that turns it**: [`rts_core::entry::declare_loop_source`]
//!   already exists and is called rather than re-derived. This module registers
//!   under the name `"node:stream"`; it does not grow a `drain` of its own, and
//!   it never sleeps — `entry::loops`'s rule.
//! - **Minting a promise from a native**: `entry::promise_new` /
//!   `entry::promise_settle` already exist and are what [`fulfilled`] calls.
//!   `timers/promises.rs` is the worked example this follows, including the
//!   `@@asyncIterator` key spelling.
//! - **The emitter**: `crate::events` builds the one real `EventEmitter`
//!   prototype and `common::emit` reaches it. No second emitter is written here.
//! - Nothing in this file hands out a number, so there is no second numbering.
//!
//! # Why `'end'` is deferred and `'data'` is not
//!
//! Real Node emits `'end'` from a `process.nextTick`, and that timing is not
//! cosmetic — it is what makes the universal spelling work:
//!
//! ```js
//! readable.on('data', chunk => …).on('end', () => …)
//! ```
//!
//! Attaching the `'data'` listener starts the flow. If `'end'` were emitted
//! inside that same call — which is what this module did before, because
//! `push(null)` on an empty buffer emitted it immediately — the `'end'`
//! listener on the next line does not exist yet and never runs. That is the
//! "fires at the wrong time" case, and it is worse than not firing at all
//! because the stream reports success while a program's completion handler is
//! silently skipped.
//!
//! So `'end'` is *scheduled* ([`schedule_end`]) and delivered from [`pump`],
//! which the host's loop runs after the program's last statement (see
//! `rts-host`'s `run.rs`) and which `promise_await` runs while parked.
//!
//! `'data'` stays synchronous, which IS a divergence from Node and is the one
//! `stream/mod.rs` already states: Node defers the first flow to a tick too, so
//! a program that pushes and only afterwards attaches `'data'` sees the chunk
//! here and would see it in Node as well, but a program that observes the
//! destination of a `.pipe()` in the same synchronous block sees data here that
//! Node would not have moved yet. Deferring it as well was considered and
//! rejected: it buys no case the `'end'` fix does not already buy, and it would
//! break every fixture that pipes and asserts in one statement sequence.
//!
//! # When `'end'` is allowed to fire at all
//!
//! Only for a stream somebody CONSUMED — [`is_consumed`]. Node does not emit
//! `'end'` on a `Readable` that was pushed to and ended but never read,
//! resumed, or iterated, and neither does this: `push(null)` alone leaves the
//! stream ended and silent. [`schedule_end`] is therefore called again from
//! each of the three consumption paths (`resume`, `read`, `next`) rather than
//! once at `push(null)`, and a scheduled end that is not yet eligible is
//! DROPPED at the pump instead of being retried forever — nothing here can
//! hold a program open (see [`source`]).
//!
//! # `Symbol.asyncIterator`
//!
//! `for await (const chunk of readable)` reaches
//! `readable[Symbol.asyncIterator]()`, which the compiler spells as the
//! property `"@@asyncIterator"` (`rts-codegen`'s `emit/for_await.rs`). The
//! iterator answers a promise per `next()`:
//!
//! - a buffered chunk, or one that `_read()` produces when prodded → an
//!   already-fulfilled promise, so an iteration that never has to wait costs no
//!   loop turn;
//! - end of stream → `{ value: undefined, done: true }`;
//! - otherwise a PENDING promise, remembered on the stream under [`WAITER`], to
//!   be settled by whichever `push()` comes next (a socket, a timer, a worker).
//!
//! # Not implemented, by name
//!
//! `iterator.throw()` — the async-iterator protocol's third method, absent for
//! the reason `timers/promises.rs` gives: `for await` reaches `next` and
//! `return` and never it, and swallowing a thrown value would be a lie about
//! where the throw went.
//!
//! `options.preventCancel` / `destroyOnReturn` — breaking out of a `for await`
//! DESTROYS the stream here, always, which is Node's default and not its only
//! setting.
//!
//! A parked `next()` answers [`Pending::Blocked`](entry::Pending), never
//! `In` — so an outstanding read does not hold the program open. The cost,
//! stated: `for await` over a stream whose only producer is a source that is
//! itself `Blocked` reports a stalled promise instead of hanging. A producer
//! driven by a timer, or one that pushes from `_read`, works; a stream that
//! simply never pushes again is reported rather than waited on forever, which
//! is this repository's rule about hangs. [`iterator_next`] then ENDS the
//! iteration on the following call rather than parking again — its own comment
//! says why that guard is what keeps a stalled read from becoming an endless
//! loop.

use std::cell::{Cell, RefCell};

use rts_core::entry::{self, Context, Pending, Provided};

use super::common::*;
use super::readable;

/// The pending `next()` promise a stream owes its async iterator, if any.
///
/// `@@`-prefixed, which is not decoration: `rts-core`'s `symbol.rs` filters
/// those keys out of enumeration, so this is invisible to `Object.keys` and to
/// `JSON.stringify` — and, unlike a side table keyed by identity, it is a
/// property of the stream, so the value is reachable through the object that
/// owns it. `timers/promises.rs` established the spelling.
const WAITER: &str = "@@#stream_waiter";
/// Whether anything ever read, resumed or iterated this stream.
const CONSUMED: &str = "@@#stream_consumed";
/// The stream an async iterator is iterating.
const STREAM: &str = "@@#stream_iterated";

thread_local! {
    /// Streams whose `'end'` is due at the next [`pump`].
    ///
    /// Per thread for the reason `timers/mod.rs`'s `TIMERS` gives at length: a
    /// stream is a cell in the region of the thread that made it, and a shared
    /// table lets one thread's pump emit another thread's event with the wrong
    /// context installed.
    static ENDING: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Streams with an outstanding [`WAITER`].
    static WAITING: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Whether [`source`] is registered with this thread's context.
    static DECLARED: Cell<bool> = const { Cell::new(false) };
}

/// Registers [`source`] from a context already in hand — what `mod.rs` calls.
pub(super) fn declare(context: &mut Context) {
    DECLARED.with(|flag| flag.set(true));
    entry::declare_loop_source(context, "node:stream", source);
}

/// Registers [`source`] from outside a borrow.
///
/// Needed because a `Readable` can exist without anybody having imported
/// `node:stream` — `fs`, `net` and `http` build one — so registration at
/// namespace-construction time alone would leave those streams with nothing to
/// deliver their `'end'`. `declare_loop_source` is idempotent by name; the flag
/// only saves the borrow.
fn ensure_source() {
    if DECLARED.with(|flag| flag.get()) {
        return;
    }
    entry::with_runtime(|context| declare(context));
}

/// Marks a stream as read/resumed/iterated — see the module doc for what this
/// gates.
pub(super) fn mark_consumed(this: u64) {
    entry::with_runtime(|context| set_bool(context, this, CONSUMED, true));
}

/// Whether anybody consumed this stream.
pub(super) fn is_consumed(this: u64) -> bool {
    get_bool(this, CONSUMED)
}

/// Whether `'end'` may be emitted for this stream right now.
pub(super) fn end_is_due(this: u64) -> bool {
    !get_bool(this, "destroyed")
        && !get_bool(this, "readableEnded")
        && get_bool(this, "__ended__")
        && get_num(this, "readableLength") <= 0.0
        && is_consumed(this)
}

/// Queues `'end'` for the next [`pump`]. A no-op for a stream already ended.
pub(super) fn schedule_end(this: u64) {
    if get_bool(this, "readableEnded") {
        return;
    }
    ensure_source();
    ENDING.with(|queue| {
        let mut queue = queue.borrow_mut();
        if !queue.contains(&this) {
            queue.push(this);
        }
    });
}

/// Whether a `next()` is parked on this stream.
fn has_waiter(this: u64) -> bool {
    get_value(this, WAITER) != entry::undefined_value()
}

/// Takes the parked promise, clearing both the property and the pump list.
fn take_waiter(this: u64) -> Option<u64> {
    let promise = get_value(this, WAITER);
    if promise == entry::undefined_value() {
        return None;
    }
    entry::with_runtime(|context| {
        let nothing = entry::undefined_in(context);
        set_value(context, this, WAITER, nothing);
    });
    WAITING.with(|queue| queue.borrow_mut().retain(|&held| held != this));
    Some(promise)
}

/// Hands `chunk` straight to a parked `next()`, if there is one.
///
/// Called from `push` BEFORE the buffer is touched: a parked reader means the
/// buffer is empty, so routing the chunk through it and back out again would
/// only add a copy and an ordering question.
pub(super) fn deliver_to_waiter(this: u64, chunk: u64) -> bool {
    let Some(promise) = take_waiter(this) else {
        return false;
    };
    let result = entry::with_runtime(|context| step(context, chunk, false));
    entry::promise_settle(promise, result, 0);
    true
}

/// Tells a parked `next()` the stream is over.
pub(super) fn end_waiter(this: u64) {
    let Some(promise) = take_waiter(this) else {
        return;
    };
    entry::promise_settle(promise, finished(), 0);
}

/// Delivers what is due: scheduled `'end'`s first, then parked readers.
///
/// `'end'` first because a reader parked on a stream that has just ended must
/// see `done` rather than wait another turn for it.
pub(super) fn pump() {
    // Taken, not iterated in place: `finish_end_now` emits an event, and a
    // listener that ends another stream would otherwise re-enter this borrow —
    // which in an `extern "C"` frame is an abort, the same rule `timers::pump`
    // is written around.
    let due: Vec<u64> = ENDING.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    for stream in due {
        if end_is_due(stream) {
            readable::finish_end_now(stream);
        }
    }
    let parked: Vec<u64> = WAITING.with(|queue| queue.borrow().clone());
    for stream in parked {
        service(stream);
    }
}

/// One parked reader: give it a chunk, or the end, or prod the producer.
fn service(stream: u64) {
    if !has_waiter(stream) {
        WAITING.with(|queue| queue.borrow_mut().retain(|&held| held != stream));
        return;
    }
    if get_bool(stream, "destroyed") {
        end_waiter(stream);
        return;
    }
    if let Some(chunk) = readable::shift_chunk(stream) {
        deliver_to_waiter(stream, chunk);
        return;
    }
    if get_bool(stream, "__ended__") {
        end_waiter(stream);
        schedule_end(stream);
        return;
    }
    // Prodding on every pass rather than once per `next()` is deliberate and is
    // a divergence stated here: Node calls `_read` once per read request, and a
    // `_read` that pushes nothing is not asked again until the next one. A
    // parked reader here has no other way to be woken by an implementation that
    // becomes ready between turns, and `_read` is documented as callable at any
    // time. A `_read` that pushes synchronously settles the waiter from inside
    // this call, which is why the state is re-read afterwards.
    readable::prod_read(stream);
    if !has_waiter(stream) {
        return;
    }
    if let Some(chunk) = readable::shift_chunk(stream) {
        deliver_to_waiter(stream, chunk);
    } else if get_bool(stream, "__ended__") {
        end_waiter(stream);
        schedule_end(stream);
    }
}

/// This module as a loop source — see the module doc for why a parked reader
/// answers [`Pending::Blocked`] and never `In`.
fn source() -> Pending {
    pump();
    let parked = WAITING.with(|queue| !queue.borrow().is_empty());
    match parked {
        true => Pending::Blocked,
        false => Pending::Idle,
    }
}

/// `readable[Symbol.asyncIterator]()`.
pub(super) extern "C" fn async_iterator(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    mark_consumed(this);
    ensure_source();
    entry::with_runtime(|context| {
        let members: &[(&str, Provided)] = &[("next", iterator_next), ("return", iterator_return)];
        let iterator = entry::make_namespace(context, members);
        // `[Symbol.asyncIterator]() { return this }` — what a nested `for await`
        // over the iterator itself reaches. The key text IS the wire format;
        // see `timers/promises.rs`.
        let itself = entry::make_callable(context, iterator_self);
        entry::put_member(context, iterator, "@@asyncIterator", itself);
        entry::put_member(context, iterator, STREAM, this);
        iterator
    })
}

/// `iterator[Symbol.asyncIterator]()` — the iterator itself.
extern "C" fn iterator_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `iterator.next()` — see the module doc for the three answers it has.
extern "C" fn iterator_next(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let stream = get_value(this, STREAM);
    if stream == entry::undefined_value() {
        return fulfilled(finished());
    }
    if has_waiter(stream) {
        // Entered with the PREVIOUS `next()` still parked, which can only mean
        // `await` came back without the promise settling — `promise_await`'s
        // stall path, taken when nothing outstanding can ever settle it. Ending
        // the iteration here is what stops the alternative: a `for await` that
        // reads `done` off `undefined`, finds it falsy, and parks again forever.
        // The stall has already printed its own diagnostic, so this is not a
        // silent truncation — it is the loop that follows one, cut short.
        take_waiter(stream);
        return fulfilled(finished());
    }
    mark_consumed(stream);
    // Anything already due is delivered before this read is answered, so a
    // `for await` that is the only thing driving the loop still sees an `'end'`
    // that was queued by the push before it.
    pump();
    if let Some(chunk) = readable::shift_chunk(stream) {
        return chunk_result(chunk);
    }
    if get_bool(stream, "destroyed") {
        return fulfilled(finished());
    }
    if !get_bool(stream, "__ended__") {
        readable::prod_read(stream);
        if let Some(chunk) = readable::shift_chunk(stream) {
            return chunk_result(chunk);
        }
    }
    if get_bool(stream, "__ended__") {
        schedule_end(stream);
        return fulfilled(finished());
    }
    // Nothing to hand over and the stream is not over: park. The promise is
    // minted outside any borrow — `promise_new` takes the runtime borrow itself
    // and taking it twice aborts the process.
    let promise = entry::promise_new();
    entry::with_runtime(|context| set_value(context, stream, WAITER, promise));
    WAITING.with(|queue| {
        let mut queue = queue.borrow_mut();
        if !queue.contains(&stream) {
            queue.push(stream);
        }
    });
    promise
}

/// `iterator.return()` — what `break` out of a `for await` calls.
///
/// Destroys the stream, which is Node's `destroyOnReturn` default; see the
/// module doc for the option that is not implemented.
extern "C" fn iterator_return(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let stream = get_value(this, STREAM);
    if stream != entry::undefined_value() {
        end_waiter(stream);
        readable::destroy(0, stream, entry::undefined_value(), 0, 0, 0);
    }
    fulfilled(finished())
}

/// A promise already fulfilled with `{ value: chunk, done: false }`.
fn chunk_result(chunk: u64) -> u64 {
    let result = entry::with_runtime(|context| step(context, chunk, false));
    fulfilled(result)
}

/// A promise already fulfilled with `value`.
fn fulfilled(value: u64) -> u64 {
    let promise = entry::promise_new();
    entry::promise_settle(promise, value, 0);
    promise
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
