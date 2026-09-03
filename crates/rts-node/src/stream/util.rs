//! `Readable.from`, `pipeline`, `finished`, `getDefaultHighWaterMark`, and the
//! `is*` predicate family (`isWritable`/`isReadable`/`isErrored`/
//! `isDisturbed`) — the module-level helpers that read state rather than
//! change it.
//!
//! # `Readable.from`: materialized, not pulled
//!
//! [`rts_core::entry::iterate`]'s own doc says why: this engine has no
//! lazy iterator protocol yet, only "walk the whole thing into an array
//! first". So `Readable.from(iterable)` drains `iterable` to completion
//! BEFORE the call returns, then feeds every element through `push()` —
//! an infinite generator hangs here exactly where `entry::iterate` already
//! says it would, not a new limit this file adds.
//!
//! # `pipeline`/`finished`: four call slots, not N streams
//!
//! Real Node's `pipeline` takes an arbitrary-length stream chain. A member
//! here has four argument slots total (the doc's own rule), so [`pipeline`]
//! reads at most three streams plus a trailing callback, or four streams
//! with no callback — never the open list Node allows. A pipeline needing a
//! fifth stage is refused by name: nothing here builds it silently short.

use rts_core::entry::{self, Provided};

use super::common::*;

/// `stream.Readable.from(iterable, options?)`.
pub(super) extern "C" fn readable_from(_e: u64, _this: u64, iterable: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let instance = super::readable::construct(0, 0, options, 0, 0, 0);
    let elements = entry::iterate(iterable);
    let values = collect_array(elements);
    let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
    let absent = entry::undefined_value();
    for value in values {
        entry::call(push_fn, instance, value, absent, absent, absent);
    }
    entry::call(push_fn, instance, entry::null_value(), absent, absent, absent);
    instance
}

fn is_callable(value: u64) -> bool {
    let absent = entry::undefined_value();
    value != absent && entry::with_runtime(|context| entry::get_member(context, value, "call")) != absent
}

/// `stream.pipeline(...streams, callback?)` — see the module doc for the
/// slot ceiling. Chains every adjacent pair with `.pipe()`, then reports the
/// LAST stream's `'finish'`/`'end'` (success) or any stream's `'error'` to
/// `callback(error?)`, at most once per event actually observed (not
/// deduplicated across the two — see `finished`'s own note).
pub(super) extern "C" fn pipeline(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let absent = entry::undefined_value();
    let raw = [a, b, c, d];
    let callback = raw.iter().rev().find(|&&v| is_callable(v)).copied();
    let streams: Vec<u64> = raw.into_iter().filter(|&v| v != absent && Some(v) != callback).collect();
    for pair in streams.windows(2) {
        let pipe_fn = entry::with_runtime(|context| entry::get_member(context, pair[0], "pipe"));
        if pipe_fn != absent {
            entry::call(pipe_fn, pair[0], pair[1], absent, absent, absent);
        }
    }
    if let (Some(callback), Some(&last)) = (callback, streams.last()) {
        attach_finished(last, callback);
        for &stream in &streams {
            attach_error(stream, callback);
        }
    }
    streams.last().copied().unwrap_or(absent)
}

/// `stream.finished(stream, callback)` — registers `callback(error?)` once
/// against the stream's own terminal event (`'finish'` for a writable side,
/// `'end'` for a readable-only stream) and once against `'error'`. **Not**
/// deduplicated across the two: a stream that both ends cleanly and later
/// errors (`'error'` after `'end'`) calls `callback` twice — real Node's
/// `finished` guarantees exactly once; this does not, and a caller relying
/// on "exactly once" should guard its own callback.
pub(super) extern "C" fn finished(_e: u64, _this: u64, stream: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    attach_finished(stream, callback);
    attach_error(stream, callback);
    entry::undefined_value()
}

fn attach_finished(stream: u64, callback: u64) {
    let absent = entry::undefined_value();
    let event = if get_value(stream, "writable") != absent { "finish" } else { "end" };
    once_relay(stream, event, callback, false);
}

fn attach_error(stream: u64, callback: u64) {
    once_relay(stream, "error", callback, true);
}

/// `stream.once(event, wrapper)`, where `wrapper` calls `callback(error)` —
/// `error` is the emitted value when `is_error`, else `undefined`, matching
/// `callback(error?)`'s contract.
fn once_relay(stream: u64, event: &str, callback: u64, is_error: bool) {
    let absent = entry::undefined_value();
    let once_fn = entry::with_runtime(|context| entry::get_member(context, stream, "once"));
    if once_fn == absent {
        return;
    }
    let wrapper = if is_error { RELAY_ERROR } else { RELAY_OK };
    let bound = entry::with_runtime(|context| entry::make_callable(context, wrapper));
    // `callback` is stashed as an own property on the wrapper's eventual
    // `this` is not available (a listener is called with the emitter as
    // `this`, not with anything we control) — so it is stashed on the
    // STREAM instead, one slot per relay kind, and read back by the relay.
    let slot = if is_error { "__onError__" } else { "__onFinished__" };
    entry::with_runtime(|context| set_value(context, stream, slot, callback));
    let event_key = key(event);
    entry::call(once_fn, stream, event_key, bound, absent, absent);
}

extern "C" fn relay_ok(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let callback = get_value(this, "__onFinished__");
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    absent
}

extern "C" fn relay_error(_e: u64, this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let callback = get_value(this, "__onError__");
    if callback != absent {
        entry::call(callback, absent, error, absent, absent, absent);
    }
    absent
}

const RELAY_OK: Provided = relay_ok;
const RELAY_ERROR: Provided = relay_error;

/// `stream.getDefaultHighWaterMark(objectMode)` — Node's own fixed defaults
/// (16 entries in object mode, 16 KiB of bytes otherwise), which is also
/// what every class in this module already falls back to when no
/// `options.highWaterMark` is given (`readable.rs`/`writable.rs`'s own
/// `init`) — this is that same constant, exposed as the function Node ships.
pub(super) extern "C" fn get_default_high_water_mark(_e: u64, _this: u64, object_mode: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let value = if entry::to_boolean(object_mode) { 16.0 } else { 16384.0 };
    entry::make_number(value)
}

/// `stream.isWritable(stream)` — reads the same `writable` data property
/// every class here keeps in sync (`common.rs`'s convention), rather than a
/// duck-typed method probe: a plain object with a `write` method is not a
/// stream, and Node's own check is the internal state flag, not a shape test.
pub(super) extern "C" fn is_writable(_e: u64, _this: u64, stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(get_bool(stream, "writable"))
}

/// `stream.isReadable(stream)` — same shape as [`is_writable`], reading the
/// `readable` data property every `Readable`/`Duplex` instance keeps in sync.
pub(super) extern "C" fn is_readable(_e: u64, _this: u64, stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(get_bool(stream, "readable"))
}

/// `stream.isErrored(stream)` — `stream.errored` is `null` until
/// `.destroy(err)` sets it (`readable::init`/`writable::init` both start it
/// there), so "not `null`" is exactly "has an error", with no second flag to
/// keep in step.
pub(super) extern "C" fn is_errored(_e: u64, _this: u64, stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let errored = get_value(stream, "errored");
    entry::boolean_value(errored != entry::null_value() && errored != entry::undefined_value())
}

/// `stream.isDisturbed(stream)` — "true if the stream has been read from or
/// errored" (Node's own wording). `readableDidRead` is the same flag
/// `readable.rs`'s `deliver_data`/`read` already set on the first chunk
/// actually handed to a consumer, so this reads it rather than tracking a
/// second one.
pub(super) extern "C" fn is_disturbed(_e: u64, _this: u64, stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let errored = get_value(stream, "errored");
    let has_error = errored != entry::null_value() && errored != entry::undefined_value();
    entry::boolean_value(get_bool(stream, "readableDidRead") || has_error)
}
