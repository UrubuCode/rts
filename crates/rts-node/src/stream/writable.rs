//! `stream.Writable` — a sink, with the backpressure contract described in
//! `mod.rs`'s doc.
//!
//! # Completing a write: the stack that stands in for a bound callback
//!
//! Real Node hands `_write(chunk, encoding, callback)` a `callback` bound to
//! the specific write it completes. This crate has no closures a native can
//! build — [`rts_core::entry::make_callable`] hands back a fixed
//! function pointer with no environment slot (`events.rs`'s own doc names
//! the same gap for `rawListeners`) — so there is no way to bake "which
//! write" into the callback value itself.
//!
//! Instead [`PENDING`], a thread-local stack, stands in for that binding:
//! [`dispatch_one`] pushes the write it is about to hand to `_write` just
//! before calling it, and the shared [`write_done`] native (the same
//! function value for every write, on every instance) pops the top entry
//! when called. This is correct exactly when `_write` calls its callback
//! before returning — the overwhelmingly common shape for an in-memory or
//! synchronous sink, and everything this crate's own `Writable`/`Transform`
//! subclasses do. **What it does not support**: a `_write` that defers its
//! callback past its own call returning (queues it for a later microtask,
//! say) — the stack entry it should pop is no longer on top by the time it
//! runs, and the completion would resolve the WRONG write. Named rather than
//! silently wrong: such a program's stream never drains, which is visible,
//! not a swapped result that looks fine.

use rts_core::entry::{self, Provided};
use std::cell::RefCell;

use super::common::*;

pub(super) const METHODS: &[(&str, Provided)] = &[
    ("write", write),
    ("end", end),
    ("cork", cork),
    ("uncork", uncork),
    ("setDefaultEncoding", set_default_encoding),
    ("destroy", destroy),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    chained_prototype(context, "Stream", "Writable", METHODS)
}

pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        init(context, instance, options);
        instance
    })
}

pub(super) fn init(context: &mut entry::Context, instance: u64, options: u64) {
    let absent = entry::undefined_in(context);
    let object_mode = super::option_flag(context, options, "objectMode");
    let hwm = super::option_number(context, options, "highWaterMark").unwrap_or(if object_mode { 16.0 } else { 16384.0 });
    set_bool(context, instance, "writableObjectMode", object_mode);
    set_num(context, instance, "writableHighWaterMark", hwm);
    set_num(context, instance, "writableLength", 0.0);
    set_num(context, instance, "writableCorked", 0.0);
    set_bool(context, instance, "writableEnded", false);
    set_bool(context, instance, "writableFinished", false);
    set_bool(context, instance, "writableNeedDrain", false);
    set_bool(context, instance, "writableAborted", false);
    set_bool(context, instance, "writable", true);
    set_bool(context, instance, "destroyed", false);
    set_bool(context, instance, "closed", false);
    set_value(context, instance, "errored", entry::null_in(context));
    set_value(context, instance, "__defaultEncoding__", entry::null_in(context));
    set_array(context, instance, "__wqueue__", Vec::new());
    let emit_close = !super::option_flag_default(context, options, "emitClose").is_some_and(|v| !v);
    set_bool(context, instance, "__emitClose__", emit_close);
    for (name, member) in [("_write", "write"), ("_final", "final"), ("_destroy", "destroy"), ("_construct", "construct")] {
        let function = super::option_member(context, options, member);
        if function != absent {
            set_value(context, instance, name, function);
        }
    }
}

fn size_of(this: u64, chunk: u64) -> f64 {
    if get_bool(this, "writableObjectMode") {
        return 1.0;
    }
    if let Some(text) = entry::text_of(chunk) {
        return text.len() as f64;
    }
    entry::with_runtime(|context| entry::bytes_of(context, chunk)).map(|bytes| bytes.len() as f64).unwrap_or(0.0)
}

thread_local! {
    // One entry: the instance a pending `_write`/`_final` call belongs to,
    // the chunk's byte/object size (unused for `_final`), and whether it is
    // the `_final` call rather than a `_write`.
    static PENDING: RefCell<Vec<(u64, f64, bool)>> = const { RefCell::new(Vec::new()) };
}

/// `writable.write(chunk, encoding?, callback?)`.
extern "C" fn write(_e: u64, this: u64, chunk: u64, encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if get_bool(this, "writableEnded") {
        // `ERR_STREAM_WRITE_AFTER_END` in real Node; refused silently — see
        // this crate's stated no-throw convention.
        return entry::boolean_value(false);
    }
    let chunk = normalize_chunk(this, chunk, encoding);
    let size = size_of(this, chunk);
    let new_length = get_num(this, "writableLength") + size;
    entry::with_runtime(|context| set_num(context, this, "writableLength", new_length));
    let mut queue = get_array(this, "__wqueue__");
    queue.push(chunk);
    entry::with_runtime(|context| set_array(context, this, "__wqueue__", queue));
    if callback != absent {
        let event = key("__onceWrite__");
        // Not a real event: fired directly, once, right after this specific
        // write's own completion — see [`finalize_write`].
        let mut pending_cbs = get_array(this, "__wcbs__");
        pending_cbs.push(callback);
        entry::with_runtime(|context| set_array(context, this, "__wcbs__", pending_cbs));
        let _ = event;
    }
    let ok = new_length < get_num(this, "writableHighWaterMark");
    entry::with_runtime(|context| set_bool(context, this, "writableNeedDrain", !ok));
    if get_num(this, "writableCorked") <= 0.0 {
        drain_queue(this);
    }
    entry::boolean_value(ok)
}

fn normalize_chunk(this: u64, chunk: u64, encoding: u64) -> u64 {
    if get_bool(this, "writableObjectMode") {
        return chunk;
    }
    let Some(text) = entry::text_of(chunk) else { return chunk };
    let name = entry::text_of(encoding).or_else(|| entry::text_of(get_value(this, "__defaultEncoding__")));
    match name.as_deref() {
        Some(enc) if entry::canonical_encoding(enc).is_some() && enc != "utf8" => {
            let bytes = entry::encode_text(&text, entry::canonical_encoding(enc).unwrap()).unwrap_or_default();
            entry::with_runtime(|context| entry::make_bytes(context, &bytes))
        }
        _ => chunk,
    }
}

/// Hands every currently queued, not-yet-dispatched chunk to `_write`, one
/// at a time, in order — see the module doc for why this only works when
/// `_write` completes before returning.
fn drain_queue(this: u64) {
    loop {
        let mut queue = get_array(this, "__wqueue__");
        if queue.is_empty() || get_num(this, "writableCorked") > 0.0 {
            break;
        }
        let chunk = queue.remove(0);
        entry::with_runtime(|context| set_array(context, this, "__wqueue__", queue));
        dispatch_one(this, chunk);
    }
}

fn dispatch_one(this: u64, chunk: u64) {
    let absent = entry::undefined_value();
    let write_hook = entry::with_runtime(|context| entry::get_member(context, this, "_write"));
    if write_hook == absent {
        // No implementer override — nothing to send this chunk to; see the
        // crate's no-throw convention (real Node throws
        // `ERR_METHOD_NOT_IMPLEMENTED` for a base `Writable`).
        finalize_write(this, size_of(this, chunk));
        return;
    }
    let size = size_of(this, chunk);
    PENDING.with(|stack| stack.borrow_mut().push((this, size, false)));
    let callback = write_callback();
    let encoding = key("buffer");
    entry::call(write_hook, this, chunk, encoding, callback, absent);
}

/// The one native function value every dispatched `_write` call is handed —
/// see the module doc for what "the same function value" buys.
fn write_callback() -> u64 {
    entry::with_runtime(|context| entry::make_callable(context, write_done))
}

extern "C" fn write_done(_e: u64, _this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some((instance, size, is_final)) = PENDING.with(|stack| stack.borrow_mut().pop()) else {
        return entry::undefined_value();
    };
    let absent = entry::undefined_value();
    if error != absent && error != entry::null_value() {
        entry::with_runtime(|context| set_value(context, instance, "errored", error));
        emit(instance, "error", error, absent, absent);
    }
    if is_final {
        finish(instance);
    } else {
        finalize_write(instance, size);
    }
    entry::undefined_value()
}

/// Runs after one chunk's `_write` completes (or immediately, when there was
/// no `_write` to call): shrinks `writableLength`, fires the per-write
/// callback queued by [`write`], fires `'drain'` if backpressure just
/// cleared, and checks for `'finish'`.
fn finalize_write(this: u64, size: f64) {
    let remaining = (get_num(this, "writableLength") - size).max(0.0);
    entry::with_runtime(|context| set_num(context, this, "writableLength", remaining));
    let absent = entry::undefined_value();
    let mut callbacks = get_array(this, "__wcbs__");
    if !callbacks.is_empty() {
        let callback = callbacks.remove(0);
        entry::with_runtime(|context| set_array(context, this, "__wcbs__", callbacks));
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    if get_bool(this, "writableNeedDrain") && remaining < get_num(this, "writableHighWaterMark") {
        entry::with_runtime(|context| set_bool(context, this, "writableNeedDrain", false));
        emit(this, "drain", absent, absent, absent);
    }
    maybe_finish(this);
}

fn maybe_finish(this: u64) {
    if !get_bool(this, "writableEnded") || get_bool(this, "writableFinished") {
        return;
    }
    if !get_array(this, "__wqueue__").is_empty() {
        return;
    }
    let absent = entry::undefined_value();
    let final_hook = entry::with_runtime(|context| entry::get_member(context, this, "_final"));
    if final_hook != absent && !get_bool(this, "__finalCalled__") {
        entry::with_runtime(|context| set_bool(context, this, "__finalCalled__", true));
        PENDING.with(|stack| stack.borrow_mut().push((this, 0.0, true)));
        let callback = write_callback();
        entry::call(final_hook, this, callback, absent, absent, absent);
        return;
    }
    finish(this);
}

fn finish(this: u64) {
    if get_bool(this, "writableFinished") {
        return;
    }
    entry::with_runtime(|context| {
        set_bool(context, this, "writableFinished", true);
        set_bool(context, this, "writable", false);
    });
    let absent = entry::undefined_value();
    emit(this, "finish", absent, absent, absent);
}

/// `writable.end(chunk?, encoding?, callback?)` — the four overloads Node
/// documents collapse to reading `chunk`/`encoding`/`callback` positionally
/// and treating a function found in `chunk`'s or `encoding`'s slot as the
/// callback instead — matching how a plain `.end(cb)` / `.end(chunk, cb)`
/// call is meant to read.
extern "C" fn end(_e: u64, this: u64, chunk: u64, encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (chunk, encoding, callback) = match entry::text_of(chunk).is_none() && chunk != absent && is_callable(chunk) {
        true => (absent, absent, chunk),
        false => match entry::text_of(encoding).is_none() && encoding != absent && is_callable(encoding) {
            true => (chunk, absent, encoding),
            false => (chunk, encoding, callback),
        },
    };
    if chunk != absent {
        write(0, this, chunk, encoding, absent, 0);
    }
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            let finish_event = key("finish");
            entry::call(once_fn, this, finish_event, callback, absent, absent);
        }
    }
    entry::with_runtime(|context| set_bool(context, this, "writableEnded", true));
    maybe_finish(this);
    this
}

fn is_callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::get_member(context, value, "call")) != entry::undefined_value()
}

/// `writable.cork()`.
extern "C" fn cork(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let depth = get_num(this, "writableCorked") + 1.0;
    entry::with_runtime(|context| set_num(context, this, "writableCorked", depth));
    entry::undefined_value()
}

/// `writable.uncork()`.
extern "C" fn uncork(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let depth = (get_num(this, "writableCorked") - 1.0).max(0.0);
    entry::with_runtime(|context| set_num(context, this, "writableCorked", depth));
    if depth <= 0.0 {
        drain_queue(this);
    }
    entry::undefined_value()
}

/// `writable.setDefaultEncoding(encoding)`.
extern "C" fn set_default_encoding(_e: u64, this: u64, encoding: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| set_value(context, this, "__defaultEncoding__", encoding));
    this
}

/// `writable.destroy(error?)` — idempotent, same shape as
/// `readable::destroy`.
pub(super) extern "C" fn destroy(_e: u64, this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if get_bool(this, "destroyed") {
        return this;
    }
    entry::with_runtime(|context| {
        set_bool(context, this, "destroyed", true);
        set_bool(context, this, "writable", false);
    });
    let absent = entry::undefined_value();
    let has_error = error != absent && error != entry::null_value();
    if has_error {
        entry::with_runtime(|context| set_value(context, this, "errored", error));
        if !get_bool(this, "writableFinished") {
            entry::with_runtime(|context| set_bool(context, this, "writableAborted", true));
        }
        emit(this, "error", error, absent, absent);
    }
    let destroy_hook = entry::with_runtime(|context| entry::get_member(context, this, "_destroy"));
    if destroy_hook != absent {
        entry::call(destroy_hook, this, error, absent, absent, absent);
    }
    if get_bool(this, "__emitClose__") {
        entry::with_runtime(|context| set_bool(context, this, "closed", true));
        emit(this, "close", absent, absent, absent);
    }
    this
}

