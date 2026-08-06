//! **NOT REGISTERED.** `lib.rs` does not name this module as a specifier, and
//! the reason is measured rather than cautious: building a `Readable` aborts the
//! process. The cause is this crate's one fatal rule — `with_runtime` holds a
//! `RefCell` borrow for its body, an entry point called inside one is a nested
//! borrow, and an `extern "C"` frame cannot unwind. Two such calls were already
//! found and fixed here (`set_prototype` at four sites, ambient where the host
//! holds the context); at least one more remains on the construction path.
//!
//! Registering it anyway would trade "the module is absent" for "the program
//! dies", which is the worse of the two and the one this repository refuses.
//! Finding the last one is a bisect over the construction path, not a redesign.
//!
//! `node:stream` — `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`,
//! chained onto the REAL `EventEmitter` prototype `crate::events` builds
//! (see its module doc), against `docs/reference/node/stream.md`.
//!
//! # WHEN a `'data'` event fires
//!
//! Synchronously, inside whatever call caused it — there is no event loop
//! this crate can post to (the same limit `fs/watch.rs`'s doc names).
//! Concretely: a chunk handed to [`readable::push`] while the stream is
//! ALREADY flowing (after `.pipe()`/`.on('data', …)`/`.resume()` was called)
//! is emitted as `'data'` before `push()` returns — in the same native call,
//! on whatever thread called it. A chunk pushed while paused is buffered and
//! delivered later, still synchronously, from inside whichever call flips
//! the stream to flowing (`.resume()`, `.pipe()`, or attaching the first
//! `'data'` listener promotes an emitter, though — named precisely — THIS
//! module only promotes on `.pipe()`/`.resume()`, not merely on
//! `.on('data', …)`; see [`readable`]'s own doc for the buffered-unit
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
//! `stream/web` (WHATWG `ReadableStream`/`WritableStream`/`TransformStream`
//! — a separate, already-ambient ArrayBuffer-backed API this crate does not
//! reach from here), `stream/promises`, `stream/consumers`, `compose`,
//! `duplexPair`, `isErrored`/`isReadable`/`isWritable`, `addAbortSignal`,
//! `get/setDefaultHighWaterMark`, `Readable.fromWeb`/`toWeb`,
//! `Writable.fromWeb`/`toWeb`, `Duplex.from`/`fromWeb`/`toWeb`, `.wrap()`,
//! `.compose()`, the whole async-iteration helper family
//! (`map`/`filter`/`forEach`/`toArray`/`reduce`/…), `Symbol.asyncIterator`/
//! `Symbol.asyncDispose` (both need a `Promise` a native can construct and
//! drive, the same gap `events.rs`'s doc names for `events.on`/`once`),
//! `_writev`, `unshift`, `wrap`. `pipeline`/`finished` are capped at four
//! call slots total — see `util.rs`'s own doc.

mod common;
mod duplex;
mod readable;
mod util;
mod writable;

use rts_core_rwk::entry::{self, Context, Provided};

/// The namespace `node:stream` is.
pub fn namespace(context: &mut Context) -> u64 {
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

    let members: &[(&str, Provided)] = &[("pipeline", util::pipeline), ("finished", util::finished)];
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
pub(super) fn option_flag_default(options: u64, name: &str) -> Option<bool> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    if value == absent { None } else { Some(entry::to_boolean(value)) }
}

/// A boolean option, `false` when absent — see [`option_flag_default`] for
/// when the distinction from "explicitly `false`" matters.
pub(super) fn option_flag(options: u64, name: &str) -> bool {
    option_flag_default(options, name).unwrap_or(false)
}

/// A numeric option, `None` when absent or not a number.
pub(super) fn option_number(options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::number_of(value)
}

/// A function-valued option (`options.read`/`.write`/`.transform`/…),
/// `undefined` when absent or not that shape — no validation beyond
/// presence; a non-function value stored this way fails loudly the first
/// time this crate tries to `call` it, which is `entry::call`'s own concern.
pub(super) fn option_member(options: u64, name: &str) -> u64 {
    let absent = entry::undefined_value();
    if options == absent {
        return absent;
    }
    entry::with_runtime(|context| entry::get_member(context, options, name))
}
