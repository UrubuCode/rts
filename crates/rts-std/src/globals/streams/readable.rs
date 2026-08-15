//! `ReadableStream`, `ReadableStreamDefaultController` and
//! `ReadableStreamDefaultReader`.
//!
//! # The two queues, and why a head index rather than `shift`
//!
//! A stream holds chunks nobody has asked for yet, and `read()` promises no
//! chunk has arrived for. Only one of the two is ever non-empty, and the
//! transition between them is the whole of what this file does.
//!
//! Both are JS arrays on the instance — see the folder's module doc for why
//! they cannot be a Rust table — read through a head INDEX rather than through
//! `Array.prototype.shift`. Two reasons, and the second is the load-bearing
//! one: shifting is `O(n)` per chunk, and it is a method a program may replace,
//! so a stream's internals would answer whatever a monkey-patched `shift` did.
//! The array is swapped for a fresh one the moment it drains, so a long-lived
//! stream does not keep every chunk it ever carried alive.

use rts_core::entry::{self, Context, Provided};

use super::{field, flag, hook, set_field, threw, writable};

/// Chunks waiting for a reader, and how far into that array reading has got.
const QUEUE: &str = "__queue";
const QUEUE_HEAD: &str = "__queueHead";
/// `read()` promises waiting for a chunk, on the same arrangement.
const READS: &str = "__reads";
const READS_HEAD: &str = "__readsHead";
/// Closed — no more chunks will arrive.
const DONE: &str = "__done";
/// Errored, and with what.
const FAILED: &str = "__failed";
const REASON: &str = "__reason";
/// The `WritableStream` this one forwards into, once piped.
const PIPE: &str = "__pipe";
/// The promise `pipeTo` answered, settled when the source closes.
const PIPE_PROMISE: &str = "__pipeDone";
/// The `underlyingSource` the constructor was given.
const SOURCE: &str = "__source";
const CONTROLLER: &str = "__controller";
/// The stream a reader or a controller belongs to.
const STREAM: &str = "__stream";

const STREAM_METHODS: &[(&str, Provided)] = &[
    ("getReader", get_reader),
    ("pipeThrough", pipe_through),
    ("pipeTo", pipe_to),
    ("cancel", cancel),
];

const CONTROLLER_METHODS: &[(&str, Provided)] = &[
    ("enqueue", controller_enqueue),
    ("close", controller_close),
    ("error", controller_error),
];

const READER_METHODS: &[(&str, Provided)] =
    &[("read", reader_read), ("cancel", reader_cancel), ("releaseLock", release_lock)];

/// The `ReadableStream` constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_of(context, "ReadableStream", prototype, construct)
}

fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "ReadableStream", STREAM_METHODS)
}

fn controller_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "ReadableStreamDefaultController", CONTROLLER_METHODS)
}

fn reader_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "ReadableStreamDefaultReader", READER_METHODS)
}

/// A stream with no underlying source — the readable half of every
/// transform-shaped pair in this folder.
pub(super) fn make(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    let instance = entry::make_instance(context, prototype);
    let absent = entry::undefined_in(context);
    initialise(context, instance, absent);
    instance
}

/// Every field a stream reads about itself, written before anything can ask.
fn initialise(context: &mut Context, instance: u64, source: u64) {
    let queue = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, QUEUE, queue);
    let reads = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, READS, reads);
    let zero = entry::make_number(0.0);
    entry::put_member(context, instance, QUEUE_HEAD, zero);
    entry::put_member(context, instance, READS_HEAD, zero);
    let untrue = entry::boolean_value(false);
    entry::put_member(context, instance, DONE, untrue);
    entry::put_member(context, instance, FAILED, untrue);
    entry::put_member(context, instance, "locked", untrue);
    entry::put_member(context, instance, SOURCE, source);
    let prototype = controller_prototype(context);
    let controller = entry::make_instance(context, prototype);
    entry::put_member(context, controller, STREAM, instance);
    entry::put_member(context, instance, CONTROLLER, controller);
}

// ------------------------------------------------------------------ the queues

fn push(owner: u64, name: &str, value: u64) {
    entry::array_append(field(owner, name), value);
}

/// The front of one queue, `None` when it is empty.
fn take(owner: u64, name: &str, head_name: &str) -> Option<u64> {
    let queue = field(owner, name);
    let length = entry::number_of(field(queue, "length")).unwrap_or(0.0);
    let head = entry::number_of(field(owner, head_name)).unwrap_or(0.0);
    if head >= length {
        return None;
    }
    let value = entry::element_at(queue, entry::make_number(head));
    match head + 1.0 >= length {
        true => entry::with_runtime(|context| {
            let fresh = entry::make_array_in(context, Vec::new());
            entry::put_member(context, owner, name, fresh);
            entry::put_member(context, owner, head_name, entry::make_number(0.0));
        }),
        false => set_field(owner, head_name, entry::make_number(head + 1.0)),
    }
    Some(value)
}

/// The `{ value, done }` a `read()` fulfils with.
fn result(value: u64, done: bool) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "value", value);
        entry::put_member(context, object, "done", entry::boolean_value(done));
        object
    })
}

/// A promise already fulfilled with a value.
fn fulfilled(value: u64) -> u64 {
    entry::with_runtime(|context| entry::settled(context, value, false))
}

// ------------------------------------------------------- what a producer does

/// A chunk into the stream: to a waiting reader, to the pipe, or onto the queue.
pub(super) fn enqueue(stream: u64, chunk: u64) {
    let pipe = field(stream, PIPE);
    if pipe != entry::undefined_value() {
        writable::accept(pipe, chunk);
        return;
    }
    match take(stream, READS, READS_HEAD) {
        Some(waiting) => entry::promise_settle(waiting, result(chunk, false), 0),
        None => push(stream, QUEUE, chunk),
    }
}

/// No more chunks. Every waiting `read()` answers `done`, and a pipe carries
/// the close on to its destination.
pub(super) fn close(stream: u64) {
    set_field(stream, DONE, entry::boolean_value(true));
    let absent = entry::undefined_value();
    while let Some(waiting) = take(stream, READS, READS_HEAD) {
        entry::promise_settle(waiting, result(absent, true), 0);
    }
    let pipe = field(stream, PIPE);
    if pipe != absent {
        writable::finish(pipe);
    }
    settle_pipe(stream);
}

/// Fulfils the promise `pipeTo` answered, once.
///
/// Cleared by the settle, and reached from BOTH ends for a reason a test found:
/// piping a stream that has ALREADY closed never runs [`close`] again, so a
/// `pipeTo` written after the last chunk would have parked a promise nothing
/// could ever settle — an `await` that hangs rather than an answer.
fn settle_pipe(stream: u64) {
    let absent = entry::undefined_value();
    let waiting = field(stream, PIPE_PROMISE);
    if waiting != absent {
        set_field(stream, PIPE_PROMISE, absent);
        entry::promise_settle(waiting, absent, 0);
    }
}

/// The stream failed. Waiting reads reject with the reason; a later `read()`
/// rejects with the same one.
pub(super) fn fail(stream: u64, reason: u64) {
    set_field(stream, FAILED, entry::boolean_value(true));
    set_field(stream, REASON, reason);
    while let Some(waiting) = take(stream, READS, READS_HEAD) {
        entry::promise_settle(waiting, reason, 1);
    }
}

/// Forwards a stream into a writable — now, for what is already queued, and for
/// every later chunk.
fn attach(stream: u64, destination: u64) {
    set_field(stream, "locked", entry::boolean_value(true));
    set_field(stream, PIPE, destination);
    while let Some(chunk) = take(stream, QUEUE, QUEUE_HEAD) {
        writable::accept(destination, chunk);
        if threw() {
            return;
        }
    }
    if flag(stream, DONE) {
        writable::finish(destination);
        settle_pipe(stream);
    }
}

// ------------------------------------------------------- what a consumer does

/// The promise one `read()` answers.
///
/// `pull` is asked only when there is nothing to hand over, which is the
/// specification's own condition, and its answer is re-examined rather than
/// assumed: a `pull` that enqueued satisfies this read without a promise ever
/// being parked.
fn read_from(stream: u64) -> u64 {
    let absent = entry::undefined_value();
    if let Some(chunk) = take(stream, QUEUE, QUEUE_HEAD) {
        return fulfilled(result(chunk, false));
    }
    if flag(stream, FAILED) {
        let reason = field(stream, REASON);
        return entry::with_runtime(|context| entry::settled(context, reason, true));
    }
    if flag(stream, DONE) {
        return fulfilled(result(absent, true));
    }
    let source = field(stream, SOURCE);
    if let Some(pull) = hook(source, "pull") {
        let controller = field(stream, CONTROLLER);
        entry::call(pull, source, controller, absent, absent, absent);
        if threw() {
            return absent;
        }
        if let Some(chunk) = take(stream, QUEUE, QUEUE_HEAD) {
            return fulfilled(result(chunk, false));
        }
        if flag(stream, DONE) {
            return fulfilled(result(absent, true));
        }
    }
    let waiting = entry::promise_new();
    push(stream, READS, waiting);
    waiting
}

/// `stream.cancel(reason)` — the source is told, then the stream closes.
fn cancel_stream(stream: u64, reason: u64) -> u64 {
    let absent = entry::undefined_value();
    let source = field(stream, SOURCE);
    if let Some(hook) = hook(source, "cancel") {
        entry::call(hook, source, reason, absent, absent, absent);
        if threw() {
            return absent;
        }
    }
    close(stream);
    super::settled_undefined()
}

// --------------------------------------------------------------- the natives

/// `new ReadableStream(underlyingSource?, strategy?)`.
///
/// The strategy is accepted and ignored — the folder's module doc says why an
/// unbounded queue makes `highWaterMark` meaningless here rather than merely
/// unimplemented.
extern "C" fn construct(_e: u64, this: u64, source: u64, _strategy: u64, _c: u64, _d: u64) -> u64 {
    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        initialise(context, instance, source);
        instance
    });
    // OUTSIDE the borrow above: `start` is user code, and a borrow held across
    // a call into it aborts the process rather than failing.
    if let Some(start) = hook(source, "start") {
        let controller = field(instance, CONTROLLER);
        let absent = entry::undefined_value();
        entry::call(start, source, controller, absent, absent, absent);
        if threw() {
            return absent;
        }
    }
    instance
}

extern "C" fn get_reader(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_field(this, "locked", entry::boolean_value(true));
    entry::with_runtime(|context| {
        let prototype = reader_prototype(context);
        let reader = entry::make_instance(context, prototype);
        entry::put_member(context, reader, STREAM, this);
        reader
    })
}

/// `readable.pipeThrough(transform, options?)` — the transform's readable half,
/// with this stream feeding its writable half.
extern "C" fn pipe_through(_e: u64, this: u64, transform: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let destination = field(transform, "writable");
    let onward = field(transform, "readable");
    attach(this, destination);
    onward
}

/// `readable.pipeTo(destination, options?)` — a promise that fulfils when this
/// stream closes.
extern "C" fn pipe_to(_e: u64, this: u64, destination: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let waiting = entry::promise_new();
    set_field(this, PIPE_PROMISE, waiting);
    attach(this, destination);
    waiting
}

extern "C" fn cancel(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    cancel_stream(this, reason)
}

extern "C" fn controller_enqueue(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    enqueue(field(this, STREAM), chunk);
    entry::undefined_value()
}

extern "C" fn controller_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    close(field(this, STREAM));
    entry::undefined_value()
}

extern "C" fn controller_error(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    fail(field(this, STREAM), reason);
    entry::undefined_value()
}

extern "C" fn reader_read(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    read_from(field(this, STREAM))
}

extern "C" fn reader_cancel(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    cancel_stream(field(this, STREAM), reason)
}

extern "C" fn release_lock(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_field(field(this, STREAM), "locked", entry::boolean_value(false));
    entry::undefined_value()
}
