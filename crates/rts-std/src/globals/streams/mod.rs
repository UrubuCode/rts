//! The WHATWG **Streams Standard** — `ReadableStream`, `WritableStream`,
//! `TransformStream`, and the four transform-shaped classes other standards
//! define on top of them (`CompressionStream`, `DecompressionStream`,
//! `TextEncoderStream`, `TextDecoderStream`).
//!
//! # What reuse-check found
//!
//! - **`rts-cranelift`**: nothing, and that is right rather than a gap. A queue
//!   of chunks and a promise waiting on one are runtime objects; the compiler
//!   emits neither. The one machine concern nearby — `src/sched/`'s promise
//!   scheduling — is reached through `rts-core`'s `promise_new` /
//!   `promise_settle`, never re-derived.
//! - **`rts-core`'s host surface**: the promises (`promise_new`,
//!   `promise_settle`, `settled`), the calls (`call`, `construct`), the byte
//!   codecs (`make_bytes`, `bytes_of`, `encode_text`) and the array store
//!   (`make_array_in`, `array_append`, `element_at`). **There is no promise, no
//!   microtask queue and no UTF-8 walker in this folder.**
//! - **`node:stream`** — the closest existing thing, and it answers a
//!   DIFFERENT question. Node's `Readable` is an `EventEmitter`: a program
//!   consumes it with `.on('data', …)`, `.pipe()` and `.read()`, and its
//!   backpressure is `push()` answering `false`. A WHATWG `ReadableStream` has
//!   no events at all — it is `getReader()` and a promise per chunk — and the
//!   two object models share no method. That crate's own `stream/mod.rs`
//!   already names `stream/web` as not implemented, for the same reason. What
//!   IS shared is the shape of the decision, not code: both keep per-instance
//!   JS values on the instance rather than in a Rust table, which is the rule
//!   the next section states.
//! - **`rts-std`'s `TextDecoder`** — reached rather than rebuilt.
//!   [`text`]'s `TextDecoderStream` holds a real `TextDecoder` instance and
//!   calls `decode(chunk, { stream: true })` on it, so the UTF-8 boundary
//!   arithmetic that decides where a chunk may be split exists once, in
//!   `globals/text/decoder.rs`, and not again here.
//! - **`node:zlib`'s `codec.rs`** — the same `flate2` `write::` adapters over a
//!   drained `Vec`, and NOT reachable: it is `pub(super)` inside a crate this
//!   one does not depend on. [`codec`] says what it therefore repeats and what
//!   it deliberately does not.
//!
//! # Where a stream keeps its state, and why it is not a Rust table
//!
//! Every queue here holds **JavaScript values** — a chunk is whatever the
//! program enqueued, a pending `read()` is a promise object. A `u64` sitting in
//! a process-global `HashMap` is invisible to the collector, so the slot would
//! be reused underneath it. That is the exact split `globals/fetch/mod.rs`
//! records between `headers` (Rust `String`s, a Rust table) and `form_data`
//! (JS values, a JS array on the instance), and this folder is on the second
//! side of it: the queue, the pending reads and the piped destination are
//! ordinary properties of the stream object, which the collector reaches by the
//! same walk it reaches everything else.
//!
//! The one Rust table is [`codec`]'s, and it is on the first side for the same
//! reason `Blob`'s is: a half-finished deflate stream is bytes, not a value.
//!
//! The cost of properties is that they are **assignable**: a program that wrote
//! `stream.__queue = null` would corrupt its own stream rather than be ignored.
//! `Response`'s `__body` and `TextDecoder`'s `__decoderId` already make that
//! trade, and there is no private-slot mechanism a host module can reach.
//!
//! # Chunks move synchronously
//!
//! An `enqueue` that finds a waiting `read()` settles that promise inside the
//! `enqueue` call, and a pipe forwards into its destination's sink in the same
//! call. Nothing here posts to the event loop. That is a real difference from a
//! specification written in terms of microtask-ordered "steps", and it is
//! visible in exactly one way: a program that can observe WHICH microtask its
//! chunk arrived on will see it arrive earlier here. Every `await` still
//! suspends, because the promise a `read()` answers is the engine's own — so
//! ordering between two awaiting tasks is the engine's, not this folder's.
//!
//! The alternative — a loop source per stream, the way `node:stream`'s
//! `flowing.rs` defers `'end'` — buys ordering this has no test for and costs a
//! wake-up per chunk. Named rather than taken.
//!
//! # Not implemented, by name
//!
//! - **BYOB readers** — `getReader({ mode: "byob" })`, `ReadableByteStreamController`,
//!   `ReadableStreamBYOBRequest`. A byte stream reads into the caller's buffer,
//!   which is a second controller with its own queue discipline; nothing in
//!   this workspace asks for one.
//! - **Queuing strategies.** `CountQueuingStrategy`, `ByteLengthQueuingStrategy`
//!   and the `highWaterMark`/`size` arguments are accepted and IGNORED: the
//!   queue here is unbounded, so `desiredSize` would be a number that means
//!   nothing and `writer.ready` would be a promise that is always already
//!   resolved. Both are absent rather than fabricated.
//! - **`stream.locked` as a getter, and locking as an error.** It is a data
//!   property this folder writes, and a second `getReader()` answers a second
//!   reader instead of throwing `TypeError`.
//! - **`reader.closed` / `writer.closed` / `writer.desiredSize`.**
//! - **`ReadableStream.from`, `tee`, `values`, `Symbol.asyncIterator`.** A host
//!   module cannot install a `Symbol`-keyed member — the limit `node:url` and
//!   `globals/fetch/` both state — so `for await (const x of stream)` does not
//!   work, and `tee` has no consumer here.
//! - **`AbortSignal` on `pipeTo`, and `preventClose`/`preventAbort`/
//!   `preventCancel`.**
//! - **Errors propagating across a pipe.** `controller.error(e)` rejects the
//!   reads of the stream it was called on; a stream piped INTO another does not
//!   carry the failure across.
//! - **A hook that answers a promise being awaited before the next step.**
//!   `start` and `flush` are called and their answer discarded, so a
//!   `TransformStream` whose `flush` is `async` ends its readable half before
//!   that function resumes. `write` is the exception and is not a gap: a
//!   thenable from a sink IS adopted, so `await writer.write(x)` waits — see
//!   [`writable`] for what it then fulfils with. The general fix is a `.then`
//!   on user code from inside a native, which nothing in this workspace does
//!   yet.

mod codec;
mod readable;
mod text;
mod transform;
mod writable;

use rts_core::entry::{self, Context, Provided};

/// Installs the seven classes as globals.
pub fn install(context: &mut Context) {
    let stream = readable::class(context);
    entry::declare_global(context, "ReadableStream", stream);
    let stream = writable::class(context);
    entry::declare_global(context, "WritableStream", stream);
    let stream = transform::class(context);
    entry::declare_global(context, "TransformStream", stream);
    let (compression, decompression) = codec::classes(context);
    entry::declare_global(context, "CompressionStream", compression);
    entry::declare_global(context, "DecompressionStream", decompression);
    let (encoder, decoder) = text::classes(context);
    entry::declare_global(context, "TextEncoderStream", encoder);
    entry::declare_global(context, "TextDecoderStream", decoder);
}

/// A class: its prototype, its constructor, and the link both ways.
///
/// Four lines that `globals/fetch/mod.rs` and `node:buffer`'s `blob.rs` each
/// already have. Copied rather than reached for because both are `pub(super)`
/// inside a module this one is a sibling of, and widening someone else's
/// visibility to save four lines is a worse trade than the four lines — the
/// same call `globals/text/mod.rs` already documents making for `self_or_new`.
fn class_of(context: &mut Context, name: &str, prototype: u64, construct: Provided) -> u64 {
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    // `name` as a data property: a native callable carries none in this engine,
    // so `x.constructor.name` reads `undefined` without it.
    let held = entry::make_string(context, name);
    entry::put_member(context, ctor, "name", held);
    ctor
}

/// `this` when `new` already made one, else a fresh instance.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// A string value. Ambient, so every caller is outside a borrow.
fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// One global by name — the route [`text`]'s `TextDecoderStream` takes to the
/// one `TextDecoder` in this workspace, whose own module is private. The same
/// recipe `globals/fetch/mod.rs` uses to reach `node:buffer`'s `Blob`, and for
/// the same reason: one class, not two that fail an `instanceof`.
fn global(name: &str) -> u64 {
    let key = entry::with_runtime(|context| i64::from(entry::member_key(context, name)));
    entry::global_get(key)
}

/// One property of an object, by name. Ambient.
fn field(owner: u64, name: &str) -> u64 {
    entry::get_indexed(owner, string(name))
}

/// Writes one. Ambient, so it must not be called from inside a borrow.
fn set_field(owner: u64, name: &str, value: u64) {
    entry::with_runtime(|context| entry::put_member(context, owner, name, value));
}

/// Whether a property holds `true` — a bit comparison rather than `to_boolean`,
/// because every flag this folder reads is one it wrote as a real boolean.
fn flag(owner: u64, name: &str) -> bool {
    field(owner, name) == entry::boolean_value(true)
}

/// One member of an object when it is callable, `None` otherwise.
///
/// The shape every optional hook here is read with: an `underlyingSource`
/// without `pull`, a transformer without `flush`. A non-callable member is the
/// same answer as an absent one, which is what the specification's "if
/// `IsCallable` is false" steps say.
fn hook(owner: u64, name: &str) -> Option<u64> {
    let absent = entry::undefined_value();
    if owner == absent || !entry::with_runtime(|context| entry::is_object(context, owner)) {
        return None;
    }
    let member = field(owner, name);
    entry::with_runtime(|context| entry::is_callable_in(context, member)).then_some(member)
}

/// Whether the call that just returned left a throw in flight.
///
/// Rule 8 of `crates/rts-core/README.md`: every site here that calls user code
/// — a `start`, a `pull`, a `transform`, a `flush`, a sink's `write` — asks this
/// before looking at the answer, and returns without acting on it. The compiled
/// call site above re-raises, so the throw reaches the program's own `catch`.
///
/// Without it a `transform` that threw would have its `undefined` enqueued as a
/// chunk, which is the "one silent wrong answer" that rule exists to refuse.
fn threw() -> bool {
    entry::thrown() != 0
}

/// A settled promise of `undefined` — what `write`, `close` and `abort` answer
/// when the sink underneath them is synchronous, which every sink in this
/// folder is.
fn settled_undefined() -> u64 {
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        entry::settled(context, absent, false)
    })
}

/// A promise rejected with the program's own `TypeError`.
fn refuse(message: &str) -> u64 {
    let error = entry::make_named_error("TypeError", message).unwrap_or_else(entry::undefined_value);
    entry::with_runtime(|context| entry::settled(context, error, true))
}
