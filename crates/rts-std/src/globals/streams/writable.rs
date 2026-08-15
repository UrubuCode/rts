//! `WritableStream`, its default controller and its default writer.
//!
//! # One sink protocol, three producers of it
//!
//! A `WritableStream` knows nothing about transforms, gzip or text. It holds an
//! `underlyingSink` and calls `write(chunk, controller)`, `close()` and
//! `abort(reason)` on it — which is the specification's own arrangement, and is
//! what lets [`super::transform`] and [`super::codec`] be nothing but sinks
//! whose three members happen to be natives of this crate rather than a
//! program's functions. Teaching this file what a `CompressionStream` is would
//! have put a second dispatch beside the one the standard already defines.
//!
//! # What `write()` fulfils with
//!
//! `undefined` when the sink answered anything but a thenable, and the sink's
//! own answer when it answered one — because waiting matters more than the
//! value. The specification says `undefined` in both cases, and this diverges
//! only in the second: an asynchronous sink here resolves the write with what
//! its promise resolved with. Named rather than silently substituted, and
//! nothing in this workspace has such a sink — every sink in this folder is a
//! synchronous native.

use rts_core::entry::{self, Context, Provided};

use super::{field, flag, hook, set_field, threw};

/// The `underlyingSink` this stream writes into.
const SINK: &str = "__sink";
const CLOSED: &str = "__closed";
const FAILED: &str = "__failed";
const REASON: &str = "__reason";
const CONTROLLER: &str = "__controller";
/// The stream a writer or a controller belongs to.
const STREAM: &str = "__stream";

const STREAM_METHODS: &[(&str, Provided)] = &[("getWriter", get_writer), ("abort", abort_stream), ("close", close_stream)];

const CONTROLLER_METHODS: &[(&str, Provided)] = &[("error", controller_error)];

const WRITER_METHODS: &[(&str, Provided)] = &[
    ("write", writer_write),
    ("close", writer_close),
    ("abort", writer_abort),
    ("releaseLock", release_lock),
];

/// The `WritableStream` constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_of(context, "WritableStream", prototype, construct)
}

fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "WritableStream", STREAM_METHODS)
}

fn controller_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "WritableStreamDefaultController", CONTROLLER_METHODS)
}

fn writer_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "WritableStreamDefaultWriter", WRITER_METHODS)
}

/// A stream over a sink built by this crate — the writable half of every
/// transform-shaped pair in this folder.
pub(super) fn make(context: &mut Context, sink: u64) -> u64 {
    let prototype = prototype(context);
    let instance = entry::make_instance(context, prototype);
    initialise(context, instance, sink);
    instance
}

fn initialise(context: &mut Context, instance: u64, sink: u64) {
    entry::put_member(context, instance, SINK, sink);
    let untrue = entry::boolean_value(false);
    entry::put_member(context, instance, CLOSED, untrue);
    entry::put_member(context, instance, FAILED, untrue);
    entry::put_member(context, instance, "locked", untrue);
    let prototype = controller_prototype(context);
    let controller = entry::make_instance(context, prototype);
    entry::put_member(context, controller, STREAM, instance);
    entry::put_member(context, instance, CONTROLLER, controller);
}

/// The promise a sink call answers with: its own when it is a thenable, a
/// fulfilled `undefined` otherwise. See the module doc for the divergence.
fn adopted(answer: u64) -> u64 {
    match hook(answer, "then") {
        Some(_) => entry::with_runtime(|context| entry::settled(context, answer, false)),
        None => super::settled_undefined(),
    }
}

/// The promise a write to an unusable stream answers.
fn unusable(stream: u64) -> Option<u64> {
    if flag(stream, FAILED) {
        let reason = field(stream, REASON);
        return Some(entry::with_runtime(|context| entry::settled(context, reason, true)));
    }
    flag(stream, CLOSED).then(|| super::refuse("Cannot write to a closed WritableStream"))
}

/// One chunk into the sink. The write half of the pipe [`super::readable`]
/// drives, and what `writer.write(chunk)` is.
pub(super) fn accept(stream: u64, chunk: u64) -> u64 {
    if let Some(answer) = unusable(stream) {
        return answer;
    }
    let sink = field(stream, SINK);
    let Some(write) = hook(sink, "write") else {
        return super::settled_undefined();
    };
    let absent = entry::undefined_value();
    let controller = field(stream, CONTROLLER);
    let answer = entry::call(write, sink, chunk, controller, absent, absent);
    if threw() {
        return absent;
    }
    adopted(answer)
}

/// No more chunks. Idempotent, because a pipe and a `writer.close()` can both
/// reach it for one stream.
pub(super) fn finish(stream: u64) -> u64 {
    if flag(stream, CLOSED) || flag(stream, FAILED) {
        return super::settled_undefined();
    }
    set_field(stream, CLOSED, entry::boolean_value(true));
    let sink = field(stream, SINK);
    let Some(close) = hook(sink, "close") else {
        return super::settled_undefined();
    };
    let absent = entry::undefined_value();
    let answer = entry::call(close, sink, absent, absent, absent, absent);
    if threw() {
        return absent;
    }
    adopted(answer)
}

/// Aborted: the sink is told, and every later write rejects with the reason.
fn abort_with(stream: u64, reason: u64) -> u64 {
    if flag(stream, FAILED) {
        return super::settled_undefined();
    }
    set_field(stream, FAILED, entry::boolean_value(true));
    set_field(stream, REASON, reason);
    let sink = field(stream, SINK);
    let Some(abort) = hook(sink, "abort") else {
        return super::settled_undefined();
    };
    let absent = entry::undefined_value();
    let answer = entry::call(abort, sink, reason, absent, absent, absent);
    if threw() {
        return absent;
    }
    adopted(answer)
}

// --------------------------------------------------------------- the natives

/// `new WritableStream(underlyingSink?, strategy?)`.
extern "C" fn construct(_e: u64, this: u64, sink: u64, _strategy: u64, _c: u64, _d: u64) -> u64 {
    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        initialise(context, instance, sink);
        instance
    });
    // OUTSIDE the borrow: `start` is user code — see `readable::construct`.
    if let Some(start) = hook(sink, "start") {
        let controller = field(instance, CONTROLLER);
        let absent = entry::undefined_value();
        entry::call(start, sink, controller, absent, absent, absent);
        if threw() {
            return absent;
        }
    }
    instance
}

extern "C" fn get_writer(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_field(this, "locked", entry::boolean_value(true));
    entry::with_runtime(|context| {
        let prototype = writer_prototype(context);
        let writer = entry::make_instance(context, prototype);
        entry::put_member(context, writer, STREAM, this);
        writer
    })
}

extern "C" fn close_stream(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    finish(this)
}

extern "C" fn abort_stream(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    abort_with(this, reason)
}

extern "C" fn controller_error(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let stream = field(this, STREAM);
    set_field(stream, FAILED, entry::boolean_value(true));
    set_field(stream, REASON, reason);
    entry::undefined_value()
}

extern "C" fn writer_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    accept(field(this, STREAM), chunk)
}

extern "C" fn writer_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    finish(field(this, STREAM))
}

extern "C" fn writer_abort(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    abort_with(field(this, STREAM), reason)
}

extern "C" fn release_lock(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_field(field(this, STREAM), "locked", entry::boolean_value(false));
    entry::undefined_value()
}
