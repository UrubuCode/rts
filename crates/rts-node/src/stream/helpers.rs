//! The `Readable.prototype` async-iteration helper family — `toArray`,
//! `forEach`, `map`, `filter`, `reduce`, `some`, `every`, `find`, `drop`,
//! `take`, `flatMap` — plus [`pull`]/[`Step`], the one driver every one of
//! them and `web_bridge.rs` pull a chunk through.
//!
//! # Reuse-check
//!
//! Nothing here is a second consumption path. [`pull`] reaches a stream
//! through the SAME `[Symbol.asyncIterator]()` `flowing.rs` already builds for
//! `for await` — it asks the iterator object for `next()` and blocks on the
//! answer with [`entry::promise_await`], which is the identical machine a
//! compiled `await` reaches (`promise/machine.rs`'s own doc: *"a frame that
//! parks resumes here, so from the caller's side this looks like a call that
//! took a long time"*). Nothing here re-derives buffering, backpressure, or
//! `'end'` timing — [`flowing::async_iterator`] already answers all three, and
//! `promise_await` already pumps loop sources while it waits, so a chunk that
//! arrives from a timer or a socket wakes these exactly as it wakes a real
//! `for await`.
//!
//! # Why these are genuinely asynchronous and not a materialised drain
//!
//! `Readable.from`'s own doc names the drain-first shape this module does NOT
//! take: [`pull`] blocks the CALLING native on one chunk at a time rather than
//! walking the whole source first, so a producer that pushes from a timer or a
//! socket is waited on exactly as `for await (const x of readable)` would wait
//! on it — the loop is Rust rather than compiled bytecode, but the wait is the
//! same `promise_await`. What is synchronous is only the OUTER call: by the
//! time `toArray()` returns, the whole source has been consumed, because the
//! promise it hands back is already settled — the same shape `stream/promises`
//! and `promisify` already commit to (mint, compute, settle, return).
//!
//! # `map`/`filter`/`drop`/`take`/`flatMap` are lazy, not eager
//!
//! These four answer a new `Readable` synchronously and pull nothing from the
//! source until the derived stream is itself consumed: each installs a `_read`
//! hook (the same own-property mechanism `readable.rs`'s module doc describes
//! for `options.read`), and `_read` is called only when `prod_read` needs one
//! more chunk. So `readable.take(2)` over an endless source never touches
//! elements past the second, exactly as Node's does.
//!
//! # What is NOT honoured, named
//!
//! - **`options.signal`** — every helper accepts an options bag and reads
//!   nothing from it. Wiring it needs the same registration `addAbortSignal`
//!   this crate now has, and threading it through eleven call sites was not
//!   done in this pass; a caller relying on mid-iteration cancellation gets an
//!   iteration that runs to completion instead.
//! - **`options.concurrency`** — accepted, ignored. Every pull is serial
//!   (`pull` blocks until one chunk answers before the next is asked for),
//!   which is Node's own default (`concurrency: 1`) and a strict subset of
//!   what a caller asking for more gets: the same correct results, without the
//!   overlap. Not a fake concurrency — a real absence of it, named.
//! - **`reduce`'s "was `initial` given" distinguishes `f(fn)` from
//!   `f(fn, undefined)`** via [`entry::rest_arguments`], which reads the real
//!   argument count a compiled call site recorded — not by treating an
//!   explicit `undefined` as "omitted", which would be the wrong answer that
//!   runs for `arr.reduce(fn, undefined)`.
//! - **`flatMap`'s inner value** is flattened only when
//!   [`entry::iterate`] accepts it — arrays, `Map`/`Set`, anything declaring
//!   `Symbol.iterator` — with a string treated as ONE chunk rather than walked
//!   by code point (matching Node, which special-cases strings the same way).
//!   A `Buffer`/typed array is NOT special-cased and so IS flattened
//!   byte-by-byte, which diverges from Node's "treat as one chunk" rule —
//!   named rather than fixed, because telling "a byte view worth keeping
//!   whole" apart from "an array worth flattening" has no single answer
//!   elsewhere in this crate either. An async-iterable inner value is not
//!   supported, for the reason `Readable.from`'s own doc gives: this engine
//!   walks a sync `Symbol.iterator` only.

use rts_core::entry::{self, Provided};

use super::common::*;
use super::flowing;

/// One step of pulling a chunk through a stream's own async iterator.
pub(super) enum Step {
    Chunk(u64),
    Done,
    Thrown,
}

/// Asks `source`'s `[Symbol.asyncIterator]().next()` for one chunk, blocking
/// on the answer — see the module doc for why this is a real wait and not a
/// drain. Never takes the pending throw itself: every caller does, right
/// where it decides what to do with it, so nothing here can lose one on a
/// path that forgot to check.
pub(super) fn pull(source: u64) -> Step {
    let absent = entry::undefined_value();
    let iterator = flowing::async_iterator(0, source, 0, 0, 0, 0);
    let next_fn = entry::with_runtime(|context| entry::get_member(context, iterator, "next"));
    let promise = entry::call(next_fn, iterator, absent, absent, absent, absent);
    let result = entry::promise_await(promise);
    if entry::thrown() != 0 {
        return Step::Thrown;
    }
    if get_bool(result, "done") {
        return Step::Done;
    }
    Step::Chunk(get_value(result, "value"))
}

/// Ends a source early — what a short-circuiting helper (`some`/`every`/
/// `find`) and `take` reach once they stop pulling, matching the
/// `destroyOnReturn` default `flowing.rs`'s `iterator_return` gives a `for
/// await` that `break`s out early.
fn stop_early(source: u64) {
    super::readable::destroy(0, source, entry::undefined_value(), 0, 0, 0);
}

/// Whether a value is callable — the same one-property probe every other file
/// in this module makes (`writable.rs`'s own `is_callable`), duplicated
/// rather than shared because each copy is one line and a shared one would be
/// a fourth file for a helper this small.
fn require_fn(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

fn reject_not_a_function(name: &str, value: u64) -> u64 {
    entry::invalid_arg_type(name, "function", value);
    entry::undefined_value()
}

/// Calls `fn_(a)`, awaiting the answer when it is a promise — see the module
/// doc: `promise_await` on a non-promise value returns it unchanged, so this
/// is correct for a synchronous callback too.
fn call1(fn_: u64, a: u64) -> Result<u64, u64> {
    let absent = entry::undefined_value();
    let result = entry::call(fn_, absent, a, absent, absent, absent);
    if entry::thrown() != 0 {
        return Err(entry::take_thrown());
    }
    let awaited = entry::promise_await(result);
    if entry::thrown() != 0 {
        return Err(entry::take_thrown());
    }
    Ok(awaited)
}

/// The two-argument form `reduce`'s callback needs.
fn call2(fn_: u64, a: u64, b: u64) -> Result<u64, u64> {
    let absent = entry::undefined_value();
    let result = entry::call(fn_, absent, a, b, absent, absent);
    if entry::thrown() != 0 {
        return Err(entry::take_thrown());
    }
    let awaited = entry::promise_await(result);
    if entry::thrown() != 0 {
        return Err(entry::take_thrown());
    }
    Ok(awaited)
}

/// What one pulled chunk does to a promise-returning helper's loop.
enum Outcome {
    Continue,
    /// Stop pulling and settle with this value — the source is destroyed
    /// (see [`stop_early`]) exactly once, here.
    Stop(u64),
}

/// Drives `source` to completion or to an early [`Outcome::Stop`], calling
/// `on_chunk` for every pulled value. `Err` carries a rejection reason —
/// either the source's own thrown error or one `on_chunk` raised.
fn drive<F: FnMut(u64) -> Result<Outcome, u64>>(source: u64, mut on_chunk: F) -> Result<Option<u64>, u64> {
    loop {
        match pull(source) {
            Step::Thrown => return Err(entry::take_thrown()),
            Step::Done => return Ok(None),
            Step::Chunk(value) => match on_chunk(value) {
                Ok(Outcome::Continue) => continue,
                Ok(Outcome::Stop(result)) => {
                    stop_early(source);
                    return Ok(Some(result));
                }
                Err(reason) => return Err(reason),
            },
        }
    }
}

/// Mints a promise and settles it with `outcome` — the same "mint, compute,
/// settle" shape `stream/promises.rs`'s own doc names.
fn settle(outcome: Result<u64, u64>) -> u64 {
    let promise = entry::promise_new();
    match outcome {
        Ok(value) => entry::promise_settle(promise, value, 0),
        Err(reason) => entry::promise_settle(promise, reason, 1),
    }
    promise
}

/// `readable.toArray(options?)`.
pub(super) extern "C" fn to_array(_e: u64, this: u64, _options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let mut items = Vec::new();
    let outcome = drive(this, |chunk| {
        items.push(chunk);
        Ok(Outcome::Continue)
    });
    settle(outcome.map(|_| entry::make_array(items)))
}

/// `readable.forEach(fn, options?)`.
pub(super) extern "C" fn for_each(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let outcome = drive(this, |chunk| call1(fn_, chunk).map(|_| Outcome::Continue));
    settle(outcome.map(|_| entry::undefined_value()))
}

/// `readable.some(fn, options?)` — short-circuits on the first truthy answer.
pub(super) extern "C" fn some(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let outcome = drive(this, |chunk| -> Result<Outcome, u64> {
        let matched = entry::to_boolean(call1(fn_, chunk)?);
        Ok(if matched { Outcome::Stop(entry::boolean_value(true)) } else { Outcome::Continue })
    });
    settle(outcome.map(|found| found.unwrap_or_else(|| entry::boolean_value(false))))
}

/// `readable.every(fn, options?)` — short-circuits on the first falsy answer.
pub(super) extern "C" fn every(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let outcome = drive(this, |chunk| -> Result<Outcome, u64> {
        let matched = entry::to_boolean(call1(fn_, chunk)?);
        Ok(if matched { Outcome::Continue } else { Outcome::Stop(entry::boolean_value(false)) })
    });
    settle(outcome.map(|found| found.unwrap_or_else(|| entry::boolean_value(true))))
}

/// `readable.find(fn, options?)`.
pub(super) extern "C" fn find(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let outcome = drive(this, |chunk| -> Result<Outcome, u64> {
        let matched = entry::to_boolean(call1(fn_, chunk)?);
        Ok(if matched { Outcome::Stop(chunk) } else { Outcome::Continue })
    });
    settle(outcome.map(|found| found.unwrap_or(entry::undefined_value())))
}

/// `readable.reduce(fn, initial?, options?)` — see the module doc for how
/// "was `initial` given" is answered without treating an explicit `undefined`
/// as absence.
pub(super) extern "C" fn reduce(_e: u64, this: u64, fn_: u64, initial: u64, options: u64, d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let given = collect_array(entry::rest_arguments(1, fn_, initial, options, d));
    let mut acc: Option<u64> = given.first().copied();
    let outcome = drive(this, |chunk| -> Result<Outcome, u64> {
        match acc {
            None => {
                acc = Some(chunk);
            }
            Some(current) => {
                acc = Some(call2(fn_, current, chunk)?);
            }
        }
        Ok(Outcome::Continue)
    });
    settle(outcome.and_then(|_| {
        acc.ok_or_else(|| {
            entry::make_named_error("TypeError", "Reduce of empty stream with no initial value")
                .unwrap_or(entry::undefined_value())
        })
    }))
}

/// Options common to `map`/`filter`/`drop`/`take`/`flatMap`: always built in
/// object mode — see the module doc for why that is a stated choice rather
/// than an inference of the source's own mode.
fn derived_options() -> u64 {
    entry::with_runtime(|context| {
        let options = entry::make_object(context);
        entry::put_member(context, options, "objectMode", entry::boolean_value(true));
        options
    })
}

/// Builds a `Readable` whose `_read` is `hook`, closed over `[source, fn_]` —
/// the shared shape every stream-returning helper below constructs.
fn new_derived(source: u64, fn_: u64, hook: Provided) -> u64 {
    let options = derived_options();
    let absent = entry::undefined_value();
    let instance = super::readable::construct(0, absent, options, 0, 0, 0);
    let environment = entry::make_array(vec![source, fn_]);
    let read_hook = entry::closure_new(hook as *const () as usize as i64, environment);
    entry::with_runtime(|context| entry::put_member(context, instance, "_read", read_hook));
    instance
}

fn env_at(environment: u64, index: f64) -> u64 {
    entry::get_indexed(environment, entry::make_number(index))
}

fn fail(this: u64, reason: u64) {
    super::readable::destroy(0, this, reason, 0, 0, 0);
}

fn end(this: u64) {
    super::readable::push(0, this, entry::null_value(), entry::undefined_value(), 0, 0);
}

fn emit_one(this: u64, value: u64) {
    super::readable::push(0, this, value, entry::undefined_value(), 0, 0);
}

extern "C" fn map_read(environment: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let source = env_at(environment, 0.0);
    let fn_ = env_at(environment, 1.0);
    match pull(source) {
        Step::Thrown => fail(this, entry::take_thrown()),
        Step::Done => end(this),
        Step::Chunk(chunk) => match call1(fn_, chunk) {
            Ok(value) => emit_one(this, value),
            Err(reason) => fail(this, reason),
        },
    }
    entry::undefined_value()
}

/// `readable.map(fn, options?)`.
pub(super) extern "C" fn map(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    new_derived(this, fn_, map_read)
}

extern "C" fn filter_read(environment: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let source = env_at(environment, 0.0);
    let fn_ = env_at(environment, 1.0);
    loop {
        match pull(source) {
            Step::Thrown => {
                fail(this, entry::take_thrown());
                return entry::undefined_value();
            }
            Step::Done => {
                end(this);
                return entry::undefined_value();
            }
            Step::Chunk(chunk) => match call1(fn_, chunk) {
                Ok(keep) => {
                    if entry::to_boolean(keep) {
                        emit_one(this, chunk);
                        return entry::undefined_value();
                    }
                }
                Err(reason) => {
                    fail(this, reason);
                    return entry::undefined_value();
                }
            },
        }
    }
}

/// `readable.filter(fn, options?)`.
pub(super) extern "C" fn filter(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    new_derived(this, fn_, filter_read)
}

extern "C" fn drop_read(environment: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let source = env_at(environment, 0.0);
    loop {
        let remaining = get_num(this, "__dropRemaining__");
        match pull(source) {
            Step::Thrown => {
                fail(this, entry::take_thrown());
                return entry::undefined_value();
            }
            Step::Done => {
                end(this);
                return entry::undefined_value();
            }
            Step::Chunk(chunk) => {
                if remaining > 0.0 {
                    entry::with_runtime(|context| set_num(context, this, "__dropRemaining__", remaining - 1.0));
                    continue;
                }
                emit_one(this, chunk);
                return entry::undefined_value();
            }
        }
    }
}

/// `readable.drop(limit, options?)`.
pub(super) extern "C" fn drop(_e: u64, this: u64, limit: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let count = entry::number_of(limit).unwrap_or(0.0).max(0.0);
    let derived = new_derived(this, entry::undefined_value(), drop_read);
    entry::with_runtime(|context| set_num(context, derived, "__dropRemaining__", count));
    derived
}

extern "C" fn take_read(environment: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let source = env_at(environment, 0.0);
    let remaining = get_num(this, "__takeRemaining__");
    if remaining <= 0.0 {
        end(this);
        stop_early(source);
        return entry::undefined_value();
    }
    match pull(source) {
        Step::Thrown => fail(this, entry::take_thrown()),
        Step::Done => end(this),
        Step::Chunk(chunk) => {
            entry::with_runtime(|context| set_num(context, this, "__takeRemaining__", remaining - 1.0));
            emit_one(this, chunk);
        }
    }
    entry::undefined_value()
}

/// `readable.take(limit, options?)`.
pub(super) extern "C" fn take(_e: u64, this: u64, limit: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let count = entry::number_of(limit).unwrap_or(0.0).max(0.0);
    let derived = new_derived(this, entry::undefined_value(), take_read);
    entry::with_runtime(|context| set_num(context, derived, "__takeRemaining__", count));
    derived
}

/// `fn`'s answer, flattened — see the module doc's stated Buffer divergence.
fn flatten_or_single(value: u64) -> Vec<u64> {
    if entry::text_of(value).is_some() {
        return vec![value];
    }
    let elements = entry::iterate(value);
    if entry::thrown() != 0 {
        entry::take_thrown();
        return vec![value];
    }
    collect_array(elements)
}

extern "C" fn flatmap_read(environment: u64, this: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let source = env_at(environment, 0.0);
    let fn_ = env_at(environment, 1.0);
    loop {
        let mut queue = get_array(this, "__flatQueue__");
        if !queue.is_empty() {
            let value = queue.remove(0);
            entry::with_runtime(|context| set_array(context, this, "__flatQueue__", queue));
            emit_one(this, value);
            return entry::undefined_value();
        }
        match pull(source) {
            Step::Thrown => {
                fail(this, entry::take_thrown());
                return entry::undefined_value();
            }
            Step::Done => {
                end(this);
                return entry::undefined_value();
            }
            Step::Chunk(chunk) => match call1(fn_, chunk) {
                Ok(result) => {
                    let items = flatten_or_single(result);
                    if items.is_empty() {
                        continue;
                    }
                    entry::with_runtime(|context| set_array(context, this, "__flatQueue__", items));
                }
                Err(reason) => {
                    fail(this, reason);
                    return entry::undefined_value();
                }
            },
        }
    }
}

/// `readable.flatMap(fn, options?)`.
pub(super) extern "C" fn flat_map(_e: u64, this: u64, fn_: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    if !require_fn(fn_) {
        return reject_not_a_function("fn", fn_);
    }
    let derived = new_derived(this, fn_, flatmap_read);
    entry::with_runtime(|context| set_array(context, derived, "__flatQueue__", Vec::new()));
    derived
}
