//! `Readable.fromWeb`/`.toWeb`, `Writable.fromWeb`/`.toWeb`,
//! `Duplex.fromWeb`/`.toWeb` — adapters between this module's classes and the
//! WHATWG ones `node:stream/web` re-exports.
//!
//! # Reuse-check
//!
//! Both families already exist, whole, and this file builds neither: the
//! WHATWG side is `rts-std`'s `globals/streams/` (`ReadableStream`,
//! `WritableStream`, their readers and writers, all real globals — see
//! `web.rs`'s own doc), and the Node side is `readable.rs`/`writable.rs`. What
//! was missing was the NAME over a bridge between two things that already
//! work, so every function here is a handful of `entry::get_member`/`call`
//! calls wiring one side's hook into the other's, not a third stream
//! implementation.
//!
//! `helpers.rs`'s [`helpers::pull`]/[`helpers::Step`] is reused rather than
//! re-derived for the `toWeb` direction: a WHATWG `pull(controller)` needs
//! exactly what `map_read` etc. already need — one chunk off a Node
//! `Readable`, waited on for real — so [`web_pull`] is that same call.
//!
//! # Why `toWeb`'s bridge is a real wait and not a drain
//!
//! `helpers.rs`'s module doc already makes this case for the iteration
//! helpers, and it applies unchanged here: [`helpers::pull`] blocks on
//! [`entry::promise_await`], which pumps loop sources while it waits, so a
//! `ReadableStream` built over a Node source that produces from a timer or a
//! socket sees each chunk as it arrives rather than only after the whole
//! source has drained.
//!
//! # What a `WritableStream` sink here always answers with
//!
//! `writable.write()`, as this crate builds it, completes `_write`'s callback
//! before returning — `writable.rs`'s own module doc states the limit this
//! rests on. So [`web_sink_write`] never has anything to wait for and answers
//! `undefined` synchronously; a Node `'error'` raised by the write is NOT
//! threaded back into the WHATWG write promise's rejection, because nothing
//! here observes it happening — named rather than silently dropped.
//!
//! # Not implemented, by name
//!
//! `options.signal`/`highWaterMark`/`objectMode`/`decodeStrings` on any of the
//! six functions below — every one is accepted and ignored, the same
//! "accepted, not honoured" this module already states for the helper
//! family's own options bag.

use rts_core::entry;

use super::common::key;
use super::helpers::{self, Step};

fn global_class(name: &str) -> u64 {
    entry::with_runtime(|context| {
        let global = entry::global_object(context);
        entry::get_member(context, global, name)
    })
}

fn member(target: u64, name: &str) -> u64 {
    entry::with_runtime(|context| entry::get_member(context, target, name))
}

fn call0(fn_: u64, this: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::call(fn_, this, absent, absent, absent, absent)
}

fn call1(fn_: u64, this: u64, a: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::call(fn_, this, a, absent, absent, absent)
}

// -------------------------------------------------------------- Readable --

/// `Readable.toWeb(readable, options?)`.
pub(super) extern "C" fn readable_to_web(_e: u64, _this: u64, readable: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let source = entry::with_runtime(|context| entry::make_object(context));
    let pull_fn = entry::closure_new(web_pull as *const () as usize as i64, readable);
    let cancel_fn = entry::closure_new(web_cancel as *const () as usize as i64, readable);
    entry::with_runtime(|context| {
        entry::put_member(context, source, "pull", pull_fn);
        entry::put_member(context, source, "cancel", cancel_fn);
    });
    let ctor = global_class("ReadableStream");
    entry::construct(ctor, source, absent, absent, absent)
}

/// The `underlyingSource.pull(controller)` hook — one chunk off `readable`,
/// waited on for real; see the module doc.
extern "C" fn web_pull(readable: u64, _source: u64, controller: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match helpers::pull(readable) {
        Step::Thrown => {
            let reason = entry::take_thrown();
            call1(member(controller, "error"), controller, reason);
        }
        Step::Done => {
            call0(member(controller, "close"), controller);
        }
        Step::Chunk(chunk) => {
            call1(member(controller, "enqueue"), controller, chunk);
        }
    }
    entry::undefined_value()
}

extern "C" fn web_cancel(readable: u64, _source: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    call1(member(readable, "destroy"), readable, reason);
    entry::undefined_value()
}

/// `Readable.fromWeb(readableStream, options?)`.
pub(super) extern "C" fn readable_from_web(_e: u64, _this: u64, web_stream: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let reader = call0(member(web_stream, "getReader"), web_stream);
    let instance = super::readable::construct(0, entry::undefined_value(), options, 0, 0, 0);
    install_read(instance, reader);
    instance
}

fn install_read(instance: u64, reader: u64) {
    let read_hook = entry::closure_new(from_web_read as *const () as usize as i64, reader);
    entry::with_runtime(|context| entry::put_member(context, instance, "_read", read_hook));
}

/// A Node `_read` pulling from a WHATWG `reader.read()` — shared by
/// [`readable_from_web`] and [`duplex_from_web`].
extern "C" fn from_web_read(reader: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let promise = call0(member(reader, "read"), reader);
    let result = entry::promise_await(promise);
    if entry::thrown() != 0 {
        let reason = entry::take_thrown();
        super::readable::destroy(0, this, reason, 0, 0, 0);
        return entry::undefined_value();
    }
    let done = entry::to_boolean(entry::get_indexed(result, key("done")));
    let chunk = if done { entry::null_value() } else { entry::get_indexed(result, key("value")) };
    super::readable::push(0, this, chunk, entry::undefined_value(), 0, 0);
    entry::undefined_value()
}

// -------------------------------------------------------------- Writable --

/// `Writable.toWeb(writable)`.
pub(super) extern "C" fn writable_to_web(_e: u64, _this: u64, writable: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let sink = entry::with_runtime(|context| entry::make_object(context));
    let write_fn = entry::closure_new(web_sink_write as *const () as usize as i64, writable);
    let close_fn = entry::closure_new(web_sink_close as *const () as usize as i64, writable);
    let abort_fn = entry::closure_new(web_sink_abort as *const () as usize as i64, writable);
    entry::with_runtime(|context| {
        entry::put_member(context, sink, "write", write_fn);
        entry::put_member(context, sink, "close", close_fn);
        entry::put_member(context, sink, "abort", abort_fn);
    });
    let ctor = global_class("WritableStream");
    entry::construct(ctor, sink, absent, absent, absent)
}

/// See the module doc for why this never has anything to await.
extern "C" fn web_sink_write(writable: u64, _sink: u64, chunk: u64, _controller: u64, _c: u64, _d: u64) -> u64 {
    call1(member(writable, "write"), writable, chunk);
    entry::undefined_value()
}

extern "C" fn web_sink_close(writable: u64, _sink: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    call0(member(writable, "end"), writable);
    entry::undefined_value()
}

extern "C" fn web_sink_abort(writable: u64, _sink: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    call1(member(writable, "destroy"), writable, reason);
    entry::undefined_value()
}

/// `Writable.fromWeb(writableStream, options?)`.
pub(super) extern "C" fn writable_from_web(_e: u64, _this: u64, web_stream: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let writer = call0(member(web_stream, "getWriter"), web_stream);
    let instance = super::writable::construct(0, entry::undefined_value(), options, 0, 0, 0);
    install_write(instance, writer);
    instance
}

fn install_write(instance: u64, writer: u64) {
    let write_hook = entry::closure_new(from_web_write as *const () as usize as i64, writer);
    let final_hook = entry::closure_new(from_web_final as *const () as usize as i64, writer);
    entry::with_runtime(|context| {
        entry::put_member(context, instance, "_write", write_hook);
        entry::put_member(context, instance, "_final", final_hook);
    });
}

/// A Node `_write` relaying into a WHATWG `writer.write(chunk)` — shared by
/// [`writable_from_web`] and [`duplex_from_web`].
extern "C" fn from_web_write(writer: u64, _this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let promise = call1(member(writer, "write"), writer, chunk);
    entry::promise_await(promise);
    finish_callback(callback);
    entry::undefined_value()
}

/// A Node `_final` closing the WHATWG writer — shared the same way.
extern "C" fn from_web_final(writer: u64, _this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let promise = call0(member(writer, "close"), writer);
    entry::promise_await(promise);
    finish_callback(callback);
    entry::undefined_value()
}

/// `callback(error?)` — `error` is whatever `promise_await` just left thrown,
/// taken and cleared so the pending throw does not also unwind this native's
/// own caller (`writable::dispatch_one`/`maybe_finish`, neither of which
/// expects one).
fn finish_callback(callback: u64) {
    let absent = entry::undefined_value();
    let error = if entry::thrown() != 0 { entry::take_thrown() } else { absent };
    entry::call(callback, absent, error, absent, absent, absent);
}

// ---------------------------------------------------------------- Duplex --

/// `Duplex.fromWeb({ readable, writable }, options?)`.
pub(super) extern "C" fn duplex_from_web(_e: u64, _this: u64, pair: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let web_readable = entry::get_indexed(pair, key("readable"));
    let web_writable = entry::get_indexed(pair, key("writable"));
    let reader = call0(member(web_readable, "getReader"), web_readable);
    let writer = call0(member(web_writable, "getWriter"), web_writable);
    let instance = super::duplex::duplex_construct(0, entry::undefined_value(), options, 0, 0, 0);
    install_read(instance, reader);
    install_write(instance, writer);
    instance
}

/// `Duplex.toWeb(duplex)` — built from [`readable_to_web`]/[`writable_to_web`]
/// unchanged: a `Duplex` carries both APIs, so each bridge reaches exactly the
/// members it already reaches on a plain `Readable`/`Writable`.
pub(super) extern "C" fn duplex_to_web(_e: u64, _this: u64, duplex: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let readable_stream = readable_to_web(0, absent, duplex, absent, 0, 0);
    let writable_stream = writable_to_web(0, absent, duplex, 0, 0, 0);
    let pair = entry::with_runtime(|context| entry::make_object(context));
    entry::with_runtime(|context| {
        entry::put_member(context, pair, "readable", readable_stream);
        entry::put_member(context, pair, "writable", writable_stream);
    });
    pair
}
