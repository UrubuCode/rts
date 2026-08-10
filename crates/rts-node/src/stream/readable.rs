//! `stream.Readable` — a source, over a plain JS array as its internal
//! buffer. See `stream/mod.rs` for WHEN `'data'` fires and what backpressure
//! means here; this file is the mechanics that doc describes.
//!
//! # `_read`/`_construct`/`_destroy`: one lookup covers subclass AND options
//!
//! Real Node lets an implementer override `_read` either by subclassing
//! (`class Foo extends Readable { _read() {…} }`) or by passing
//! `{ read(size) {…} }` in the constructor options. Both are honored here by
//! the SAME mechanism: an options function is stored as an OWN property
//! (`this._read = options.read`), and every call site reads `_read` back
//! with [`rts_core::entry::get_member`], which walks the prototype chain
//! — an own property (from options) is found first; a subclass's prototype
//! method is found if there is no own property. No branch anywhere asks
//! which case it is.

use rts_core::entry::{self, Provided};

use super::common::*;

pub(super) const METHODS: &[(&str, Provided)] = &[
    ("push", push),
    ("read", read),
    ("pipe", pipe),
    ("unpipe", unpipe),
    ("pause", pause),
    ("resume", resume),
    ("isPaused", is_paused),
    ("setEncoding", set_encoding),
    ("destroy", destroy),
    ("on", on),
    ("addListener", on),
    ("once", once),
    // `for await (const chunk of readable)`. `"@@asyncIterator"` is the wire
    // spelling of `[Symbol.asyncIterator]` — see `flowing.rs`'s module doc.
    ("@@asyncIterator", super::flowing::async_iterator),
];

/// `readable.on(eventName, listener)` / `.addListener(...)` — registers
/// through the real `EventEmitter.prototype.on` (fetched by name; see
/// `entry::make_prototype`'s own idempotent-by-name contract — an empty
/// method list for an already-built name, the same call `chained_prototype`
/// already makes), then promotes to flowing mode on the FIRST `'data'`
/// listener.
///
/// This used to be missing: the module doc claimed attaching a `'data'`
/// listener never promotes a `Readable` — only `.pipe()`/`.resume()` did —
/// stated as a deliberate divergence. Checked against a real Node: it does
/// promote (`r.on('data', cb)` alone flows a stream with no `.resume()`
/// call), so that was a real bug wearing a documented-limitation's clothes.
extern "C" fn on(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    register_listener(this, event, listener, "on")
}

/// `readable.once(eventName, listener)` — promotes for the same reason [`on`]
/// does, and it has to be overridden separately: `events.rs`'s `once` appends
/// to the listener table directly rather than routing through `this.on`, so a
/// `Readable` inheriting it unchanged left `readable.once('data', …)` paused
/// forever while `readable.on('data', …)` flowed. Real Node's `once` DOES go
/// through the overridden `on`, so both promote there.
extern "C" fn once(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    register_listener(this, event, listener, "once")
}

/// Registers through `EventEmitter.prototype.<method>`, then promotes to
/// flowing mode when the event registered for is `'data'`.
fn register_listener(this: u64, event: u64, listener: u64, method: &str) -> u64 {
    let registrar = entry::with_runtime(|context| {
        let emitter_prototype = entry::make_prototype(context, "EventEmitter", &[]);
        entry::get_member(context, emitter_prototype, method)
    });
    let absent = entry::undefined_value();
    if registrar != absent {
        entry::call(registrar, this, event, listener, absent, absent);
    }
    if entry::text_of(event).as_deref() == Some("data") {
        resume(0, this, 0, 0, 0, 0);
    }
    this
}

/// Builds the class object `stream.Readable` is, with its own prototype
/// chained onto `Stream`/`EventEmitter` — see `mod.rs::namespace`.
pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    chained_prototype(context, "Stream", "Readable", METHODS)
}

/// `new stream.Readable(options?)`.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        init(context, instance, options);
        instance
    })
}

/// The state every `Readable` — including `Duplex`, which mixes this file's
/// methods in rather than re-deriving them — starts with. Split out so
/// `duplex::construct` can call it without going through [`construct`]'s own
/// `self_or_new`/`init_emitter` (`Duplex` does those once for both halves).
pub(super) fn init(context: &mut entry::Context, instance: u64, options: u64) {
    let absent = entry::undefined_in(context);
    let object_mode = super::option_flag(context, options, "objectMode");
    let hwm = super::option_number(context, options, "highWaterMark").unwrap_or(if object_mode { 16.0 } else { 16384.0 });
    set_bool(context, instance, "readableObjectMode", object_mode);
    set_num(context, instance, "readableHighWaterMark", hwm);
    set_num(context, instance, "readableLength", 0.0);
    set_value(context, instance, "readableEncoding", entry::null_in(context));
    set_bool(context, instance, "readableEnded", false);
    set_value(context, instance, "readableFlowing", entry::null_in(context));
    set_bool(context, instance, "readable", true);
    set_bool(context, instance, "readableDidRead", false);
    set_bool(context, instance, "readableAborted", false);
    set_bool(context, instance, "destroyed", false);
    set_bool(context, instance, "closed", false);
    set_value(context, instance, "errored", entry::null_in(context));
    set_array(context, instance, "__buf__", Vec::new());
    set_array(context, instance, "__pipes__", Vec::new());
    set_array(context, instance, "__pipeEnd__", Vec::new());
    set_bool(context, instance, "__ended__", false);
    let emit_close = !super::option_flag_default(context, options, "emitClose").is_some_and(|v| !v);
    set_bool(context, instance, "__emitClose__", emit_close);
    for (name, member) in [("_read", "read"), ("_destroy", "destroy"), ("_construct", "construct")] {
        let function = super::option_member(context, options, member);
        if function != absent {
            set_value(context, instance, name, function);
        }
    }
}

fn size_of(this: u64, chunk: u64) -> f64 {
    if get_bool(this, "readableObjectMode") {
        return 1.0;
    }
    if let Some(text) = entry::text_of(chunk) {
        return text.len() as f64;
    }
    entry::with_runtime(|context| entry::bytes_of(context, chunk)).map(|bytes| bytes.len() as f64).unwrap_or(0.0)
}

/// Encodes `chunk` per `setEncoding`, if any was set and the chunk is bytes.
fn apply_encoding(this: u64, chunk: u64) -> u64 {
    let encoding = get_value(this, "readableEncoding");
    if encoding == entry::null_value() {
        return chunk;
    }
    let Some(name) = entry::text_of(encoding) else { return chunk };
    let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, chunk)) else { return chunk };
    let text = entry::decode_bytes(&bytes, entry::canonical_encoding(&name).unwrap_or("utf8"));
    key(&text)
}

/// `readable.push(chunk, encoding?)` — see the module doc for the flowing
/// vs. buffered split this makes.
extern "C" fn push(_e: u64, this: u64, chunk: u64, _encoding: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let null = entry::null_value();
    if chunk == null {
        set_bool_ctx(this, "__ended__", true);
        // A parked `next()` is told at once — it is already waiting, so there
        // is nothing to defer for it. The `'end'` EVENT is still scheduled
        // rather than emitted here; see `flowing.rs`'s module doc.
        super::flowing::end_waiter(this);
        if get_num(this, "readableLength") <= 0.0 {
            super::flowing::schedule_end(this);
        }
        return entry::boolean_value(false);
    }
    if get_bool(this, "__ended__") {
        // `ERR_STREAM_PUSH_AFTER_EOF` in real Node; no throw here — see the
        // crate's own stated convention (`fs/mod.rs`'s doc), silently refused.
        return entry::boolean_value(false);
    }
    let chunk = apply_encoding(this, chunk);
    // A reader parked on `[Symbol.asyncIterator]` takes precedence over the
    // buffer: it is only ever parked with the buffer empty, so handing the
    // chunk straight over costs one settle instead of a store and a shift.
    if super::flowing::deliver_to_waiter(this, chunk) {
        return entry::boolean_value(true);
    }
    let flowing = get_value(this, "readableFlowing") == entry::boolean_value(true);
    let buffered = get_num(this, "readableLength");
    if flowing && buffered <= 0.0 {
        deliver_data(this, chunk);
    } else {
        let mut buf = get_array(this, "__buf__");
        let was_empty = buf.is_empty();
        buf.push(chunk);
        entry::with_runtime(|context| set_array(context, this, "__buf__", buf));
        set_num_ctx(this, "readableLength", buffered + size_of(this, chunk));
        if was_empty {
            emit(this, "readable", absent, absent, absent);
        }
    }
    entry::boolean_value(get_num(this, "readableLength") < get_num(this, "readableHighWaterMark"))
}

/// `readable.read(size?)` — the module doc's read simplification: each
/// `push()` is one indivisible unit here, never split or concatenated to
/// satisfy `size`.
extern "C" fn read(_e: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Calling `read()` at all is consumption, even when it answers `null`: a
    // consumer that asks an ended stream for a chunk has reached EOF, and that
    // is one of the three things `flowing::is_consumed` gates `'end'` on.
    super::flowing::mark_consumed(this);
    let Some(chunk) = shift_chunk(this) else {
        if get_bool(this, "__ended__") {
            super::flowing::schedule_end(this);
        }
        return entry::null_value();
    };
    set_bool_ctx(this, "readableDidRead", true);
    if get_num(this, "readableLength") <= 0.0 && get_bool(this, "__ended__") {
        super::flowing::schedule_end(this);
    }
    chunk
}

/// Removes and answers the oldest buffered chunk, keeping `readableLength` in
/// step — the one place the internal buffer is shortened, so `read()`,
/// [`drain_all`] and the async iterator cannot disagree about its length.
pub(super) fn shift_chunk(this: u64) -> Option<u64> {
    let mut buf = get_array(this, "__buf__");
    if buf.is_empty() {
        return None;
    }
    let chunk = buf.remove(0);
    let remaining = get_num(this, "readableLength") - size_of(this, chunk);
    entry::with_runtime(|context| {
        set_array(context, this, "__buf__", buf);
        set_num(context, this, "readableLength", remaining.max(0.0));
    });
    Some(chunk)
}

/// Calls `this._read(highWaterMark)` if there is one — how a pull-based
/// implementation is asked to produce. Looked up through the prototype chain,
/// which is what makes a subclass's `_read` and an `options.read` the same
/// mechanism (see this file's own module doc).
pub(super) fn prod_read(this: u64) {
    let absent = entry::undefined_value();
    let read_hook = entry::with_runtime(|context| entry::get_member(context, this, "_read"));
    if read_hook == absent {
        return;
    }
    let size = entry::make_number(get_num(this, "readableHighWaterMark"));
    entry::call(read_hook, this, size, absent, absent, absent);
}

/// Delivers one chunk in flowing mode: the `'data'` event, then a direct
/// `write()` on every piped destination — see `mod.rs`'s doc for why `pipe`
/// is wired this way instead of through a synthesized `'data'` listener.
fn deliver_data(this: u64, chunk: u64) {
    let absent = entry::undefined_value();
    set_bool_ctx(this, "readableDidRead", true);
    emit(this, "data", chunk, absent, absent);
    let pipes = get_array(this, "__pipes__");
    for dest in pipes {
        let write_fn = entry::with_runtime(|context| entry::get_member(context, dest, "write"));
        if write_fn == absent {
            continue;
        }
        let ok = entry::call(write_fn, dest, chunk, absent, absent, absent);
        if ok == entry::boolean_value(false) {
            pause_flow(this);
        }
    }
}

/// Drains the whole backlog into `'data'`/pipes, oldest first, then ends if
/// `push(null)` already arrived.
fn drain_all(this: u64) {
    while get_value(this, "readableFlowing") == entry::boolean_value(true) {
        let Some(chunk) = shift_chunk(this) else { break };
        deliver_data(this, chunk);
    }
    if get_num(this, "readableLength") <= 0.0 && get_bool(this, "__ended__") {
        super::flowing::schedule_end(this);
    }
}

/// Emits `'end'` and forwards it to every piped destination.
///
/// Called ONLY from `flowing::pump`, never inline: a synchronous `'end'` is
/// emitted before the `.on('end', …)` of a chained call exists, which is the
/// whole reason `flowing.rs` exists — read its module doc before calling this
/// from anywhere else.
pub(super) fn finish_end_now(this: u64) {
    if get_bool(this, "readableEnded") {
        return;
    }
    set_bool_ctx(this, "readableEnded", true);
    set_bool_ctx(this, "readable", false);
    let absent = entry::undefined_value();
    emit(this, "end", absent, absent, absent);
    let pipes = get_array(this, "__pipes__");
    let ends = get_array(this, "__pipeEnd__");
    for (dest, flag) in pipes.into_iter().zip(ends) {
        if flag == entry::boolean_value(true) {
            let end_fn = entry::with_runtime(|context| entry::get_member(context, dest, "end"));
            if end_fn != absent {
                entry::call(end_fn, dest, absent, absent, absent, absent);
            }
        }
    }
    entry::with_runtime(|context| set_array(context, this, "__pipes__", Vec::new()));
    entry::with_runtime(|context| set_array(context, this, "__pipeEnd__", Vec::new()));
}

fn pause_flow(this: u64) {
    if get_value(this, "readableFlowing") == entry::boolean_value(true) {
        set_value_ctx(this, "readableFlowing", entry::boolean_value(false));
        let absent = entry::undefined_value();
        emit(this, "pause", absent, absent, absent);
    }
}

/// `readable.pipe(destination, options?)`.
extern "C" fn pipe(_e: u64, this: u64, destination: u64, options: u64, _c: u64, _d: u64) -> u64 {
    // Read through its own borrow: this is a native entry, not a builder, so
    // there is no context in hand here.
    let ends = entry::with_runtime(|context| {
        !super::option_flag_default(context, options, "end").is_some_and(|v| !v)
    });
    let mut pipes = get_array(this, "__pipes__");
    let mut flags = get_array(this, "__pipeEnd__");
    pipes.push(destination);
    flags.push(entry::boolean_value(ends));
    entry::with_runtime(|context| {
        set_array(context, this, "__pipes__", pipes);
        set_array(context, this, "__pipeEnd__", flags);
    });
    let absent = entry::undefined_value();
    let pipe_fn = entry::with_runtime(|context| entry::get_member(context, destination, "emit"));
    if pipe_fn != absent {
        let event = key("pipe");
        entry::call(pipe_fn, destination, event, this, absent, absent);
    }
    resume(0, this, 0, 0, 0, 0);
    destination
}

/// `readable.unpipe(destination?)`.
extern "C" fn unpipe(_e: u64, this: u64, destination: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let pipes = get_array(this, "__pipes__");
    let flags = get_array(this, "__pipeEnd__");
    let mut kept_pipes = Vec::new();
    let mut kept_flags = Vec::new();
    for (dest, flag) in pipes.into_iter().zip(flags) {
        let matches = destination == absent || entry::strict_equals(dest, destination);
        if matches {
            emit(this, "unpipe", dest, absent, absent);
        } else {
            kept_pipes.push(dest);
            kept_flags.push(flag);
        }
    }
    entry::with_runtime(|context| {
        set_array(context, this, "__pipes__", kept_pipes);
        set_array(context, this, "__pipeEnd__", kept_flags);
    });
    this
}

/// `readable.pause()`.
extern "C" fn pause(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    pause_flow(this);
    this
}

/// `readable.resume()`.
extern "C" fn resume(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::flowing::mark_consumed(this);
    let was_flowing = get_value(this, "readableFlowing") == entry::boolean_value(true);
    set_value_ctx(this, "readableFlowing", entry::boolean_value(true));
    if !was_flowing {
        let absent = entry::undefined_value();
        emit(this, "resume", absent, absent, absent);
    }
    drain_all(this);
    this
}

/// `readable.isPaused()`.
extern "C" fn is_paused(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(get_value(this, "readableFlowing") != entry::boolean_value(true))
}

/// `readable.setEncoding(encoding)`.
extern "C" fn set_encoding(_e: u64, this: u64, encoding: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_value_ctx(this, "readableEncoding", encoding);
    this
}

/// `readable.destroy(error?)` — idempotent; see the module doc's `'error'`
/// note (shared with `events.rs`: an unhandled `'error'` still crashes).
pub(super) extern "C" fn destroy(_e: u64, this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if get_bool(this, "destroyed") {
        return this;
    }
    set_bool_ctx(this, "destroyed", true);
    set_bool_ctx(this, "readable", false);
    let absent = entry::undefined_value();
    let has_error = error != absent && error != entry::null_value();
    if has_error {
        set_value_ctx(this, "errored", error);
        if get_bool(this, "readableEnded") {
            set_bool_ctx(this, "readableAborted", true);
        }
        emit(this, "error", error, absent, absent);
    }
    let destroy_hook = entry::with_runtime(|context| entry::get_member(context, this, "_destroy"));
    if destroy_hook != absent {
        entry::call(destroy_hook, this, error, absent, absent, absent);
    }
    if get_bool(this, "__emitClose__") {
        set_bool_ctx(this, "closed", true);
        emit(this, "close", absent, absent, absent);
    }
    this
}

fn set_bool_ctx(this: u64, name: &str, value: bool) {
    entry::with_runtime(|context| set_bool(context, this, name, value));
}
fn set_num_ctx(this: u64, name: &str, value: f64) {
    entry::with_runtime(|context| set_num(context, this, name, value));
}
fn set_value_ctx(this: u64, name: &str, value: u64) {
    entry::with_runtime(|context| set_value(context, this, name, value));
}
