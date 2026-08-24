//! Registering it anyway would trade "the module is absent" for "the program
//! dies", which is the worse of the two and the one this repository refuses.
//! Finding the last one is a bisect over the construction path, not a redesign.
//!
//! `node:stream` — `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`,
//! chained onto the REAL `EventEmitter` prototype `crate::events` builds
//! (see its module doc), against `docs/reference/node/stream.md`.
//!
//! # WHEN a `'data'` event fires, and when `'end'` does
//!
//! **They differ, and `flowing.rs`'s module doc is where that is decided** —
//! `'data'` is synchronous, `'end'` is delivered from a loop source. The short
//! version: emitting `'end'` inside the `.on('data', …)` call that started the
//! flow runs it before the `.on('end', …)` on the next line exists, so the
//! universal `readable.on('data', …).on('end', …)` spelling silently loses its
//! completion handler. `'end'` also never fires for a stream nobody consumed.
//! The rest of this section is about `'data'` and is unchanged.
//!
//! Synchronously, inside whatever call caused it — there is no event loop
//! this crate can post to (the same limit `fs/watch.rs`'s doc names).
//! Concretely: a chunk handed to [`readable::push`] while the stream is
//! ALREADY flowing (after `.pipe()`/`.on('data', …)`/`.resume()` was called)
//! is emitted as `'data'` before `push()` returns — in the same native call,
//! on whatever thread called it. A chunk pushed while paused is buffered and
//! delivered later, still synchronously, from inside whichever call flips
//! the stream to flowing (`.resume()`, `.pipe()`, or attaching the first
//! `'data'` listener — all three promote, matching real Node; `readable.rs`'s
//! own `on` doc says why this one used to be a documented gap that was
//! actually a bug, and [`readable`]'s own doc for the buffered-unit
//! simplification `read()` makes at the same time). **What a program
//! observes**: `readable.push(chunk)` and the corresponding `'data'`
//! listener run back-to-back, never interleaved with anything else queued —
//! this engine has nothing else to interleave with. A program written
//! against real Node's microtask-deferred timing (assuming `'data'` never
//! fires before the listener line that attaches it finishes executing) can
//! observe a difference if it pushes before attaching a listener in the same
//! synchronous block; `fs/watch.rs` names the same class of deviation for
//! the same reason.
//!
//! # Backpressure
//!
//! `Readable.push()` and `Writable.write()` each answer `false` from a REAL
//! comparison against the buffered length they track (`readableLength`/
//! `writableLength` against `readableHighWaterMark`/`writableHighWaterMark`)
//! — never a constant `true`. `'drain'` fires when a queued write actually
//! completes and brings `writableLength` back under the high-water mark; see
//! `writable.rs`'s own doc for the one real limit on "completes": a custom
//! `_write` must call its callback before returning, because there is no
//! bound-closure mechanism here to complete a LATER call correctly (a
//! deferred callback resolves the wrong queued write, not this one — named,
//! not silently wrong).
//!
//! # Object model
//!
//! ```text
//! EventEmitter (crate::events, SHARED prototype)
//!   └─ Stream (this module, empty — legacy base, kept for instanceof)
//!       ├─ Readable
//!       │    └─ Duplex (mixes in Writable's own methods, not a 2nd `extends`)
//!       │         └─ Transform (_write routed through _transform; see duplex.rs)
//!       │              └─ PassThrough (identity _transform, inherited)
//!       └─ Writable
//! ```
//!
//! # Not implemented, by name
//!
//! ~~`stream/web`~~ — it exists, in [`web`], and the entry is kept because the
//! reason given for refusing it was wrong in an instructive way: *"a separate,
//! already-ambient ArrayBuffer-backed API this crate does not reach from
//! here"*. Already-ambient was the answer, not the obstacle — the classes are
//! GLOBALS, so reaching them is a read of the global object, and what was
//! missing was the NAME over an implementation that already existed.
//!
//! `stream/promises`, `stream/consumers`, `compose`,
//! `isErrored`/`isReadable`, `addAbortSignal`,
//! `setDefaultHighWaterMark`, `Readable.fromWeb`/`toWeb`,
//! `Writable.fromWeb`/`toWeb`, `Duplex.from`/`fromWeb`/`toWeb`, `.wrap()`,
//! `.compose()`, the whole async-iteration helper family
//! (`map`/`filter`/`forEach`/`toArray`/`reduce`/…), `Symbol.asyncDispose`,
//! `_writev`, `unshift`, `wrap`.
//!
//! `Symbol.asyncIterator` used to be on that list, refused for want of "a
//! `Promise` a native can construct and drive". That was stale rather than
//! wrong-headed — `entry::promise_new`/`promise_settle` are exported — and
//! `flowing.rs` implements it. The refusal is written down rather than deleted,
//! because a "cannot" that outlives its cause is how a feature stays unwritten
//! for reasons that stopped being true. `pipeline`/`finished` are capped at four
//! call slots total — see `util.rs`'s own doc.

mod common;
mod duplex;
mod flowing;
mod readable;
mod util;
mod web;
mod writable;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:stream/web` is — see `web.rs` for why it is a
/// re-export of the globals rather than a second set of classes.
pub fn web_namespace(context: &mut Context) -> u64 {
    web::namespace(context)
}

/// The namespace `node:stream` is.
pub fn namespace(context: &mut Context) -> u64 {
    // What delivers a deferred `'end'` and wakes a parked `for await`. Declared
    // here with the context already in hand, and again lazily by `flowing.rs`
    // for the streams `fs`/`net`/`http` build without this namespace ever being
    // constructed; `declare_loop_source` is idempotent by name.
    flowing::declare(context);
    let stream_prototype = common::chained_prototype(context, "EventEmitter", "Stream", &[]);
    let stream_ctor = entry::make_callable(context, stream_construct);
    entry::put_member(context, stream_ctor, "prototype", stream_prototype);

    let readable_ctor = class_ctor(context, readable::construct, readable::prototype);
    let from_fn = entry::make_callable(context, util::readable_from);
    entry::put_member(context, readable_ctor, "from", from_fn);

    let writable_ctor = class_ctor(context, writable::construct, writable::prototype);
    let duplex_ctor = class_ctor(context, duplex::duplex_construct, duplex::duplex_prototype);
    let transform_ctor = class_ctor(context, duplex::transform_construct, duplex::transform_prototype);
    let pass_through_ctor = class_ctor(context, duplex::pass_through_construct, duplex::pass_through_prototype);

    let members: &[(&str, Provided)] = &[
        ("pipeline", util::pipeline),
        ("finished", util::finished),
        ("duplexPair", duplex::duplex_pair),
        ("getDefaultHighWaterMark", util::get_default_high_water_mark),
        ("isWritable", util::is_writable),
    ];
    let namespace = entry::make_namespace(context, members);
    entry::put_member(context, namespace, "Stream", stream_ctor);
    entry::put_member(context, namespace, "Readable", readable_ctor);
    entry::put_member(context, namespace, "Writable", writable_ctor);
    entry::put_member(context, namespace, "Duplex", duplex_ctor);
    entry::put_member(context, namespace, "Transform", transform_ctor);
    entry::put_member(context, namespace, "PassThrough", pass_through_ctor);
    namespace
}

fn class_ctor(context: &mut Context, construct: Provided, prototype_of: fn(&mut Context) -> u64) -> u64 {
    let ctor = entry::make_callable(context, construct);
    let prototype = prototype_of(context);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}

/// `new stream.Stream()` — the legacy base; no behaviour of its own beyond
/// being an `EventEmitter`. See the module doc's object-model diagram.
extern "C" fn stream_construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = common::chained_prototype(context, "EventEmitter", "Stream", &[]);
        let instance = common::self_or_new(context, this, prototype);
        common::init_emitter(context, instance);
        instance
    })
}

/// A boolean option read off an options object; `None` when the argument is
/// absent, not an object, or does not carry `name` — distinct from
/// [`option_flag`], which collapses that into `false`, because a caller
/// choosing between "explicitly set" and "left at the default" needs the
/// third answer.
pub(super) fn option_flag_default(
    context: &mut entry::Context,
    options: u64,
    name: &str,
) -> Option<bool> {
    // Takes the context rather than reaching for it: every caller reads options
    // while BUILDING a stream, which happens inside `with_runtime` — where the
    // ambient form is a nested borrow and therefore an abort. That is exactly
    // where `new Readable()` died.
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    // `to_boolean_in`, which is what the comment above was asking for and did
    // not get: the ambient form here aborted `new Readable({objectMode: true})`
    // and every other stream built with an option object.
    if value == absent { None } else { Some(entry::to_boolean_in(context, value)) }
}

/// A boolean option, `false` when absent — see [`option_flag_default`] for
/// when the distinction from "explicitly `false`" matters.
pub(super) fn option_flag(context: &mut entry::Context, options: u64, name: &str) -> bool {
    option_flag_default(context, options, name).unwrap_or(false)
}

/// A numeric option, `None` when absent or not a number.
pub(super) fn option_number(context: &mut entry::Context, options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::number_of(value)
}

/// A function-valued option (`options.read`/`.write`/`.transform`/…),
/// `undefined` when absent or not that shape — no validation beyond
/// presence; a non-function value stored this way fails loudly the first
/// time this crate tries to `call` it, which is `entry::call`'s own concern.
pub(super) fn option_member(context: &mut entry::Context, options: u64, name: &str) -> u64 {
    let absent = entry::undefined_in(context);
    if options == absent {
        return absent;
    }
    entry::get_member(context, options, name)
}
