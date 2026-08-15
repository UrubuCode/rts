//! `Headers`, `FormData`, `Request` and `Response` — the four classes the
//! WHATWG **Fetch Standard** defines beside the function it is named after.
//!
//! # Why these ship and `fetch` does not
//!
//! Because they are not the same promise. Each of these four is a container: a
//! header list with the standard's own combining and ordering rules, an ordered
//! multimap of form entries, and the request/response pair that holds a method,
//! a URL, a status, a header list and a body. Every one of them does the whole
//! of what its name means with nothing but memory, which is why programs use
//! them standalone — building a `Response` in a test, parsing a header list,
//! assembling a `FormData` — and why they are worth having before any socket is
//! opened.
//!
//! `fetch` is the opposite case, and this repository has a rule for it: a
//! surface that cannot do what its name means does not ship. `fetch` means "go
//! to the network", and a `fetch` that resolved to something assembled locally
//! would be the `sync.mutex_lock` mistake again — passing a test and lying to a
//! reader. It is absent, so it fails at the call.
//!
//! # What reuse-check found
//!
//! - **`rts-cranelift`**: nothing. Header lists and multipart bodies are runtime
//!   objects; the compiler emits none of it.
//! - **`rts-core`'s host surface**: the codecs (`decode_bytes`, `encode_text`,
//!   `make_bytes`, `bytes_of`), the settled promise (`settled`), the call
//!   (`call`, `construct`) and the accessor installer (`member_key` +
//!   `define_getter`). Every one of those is reached rather than repeated —
//!   there is no UTF-8 walker, no promise, and no JSON parser in this folder.
//! - **`node:http`'s `Headers`**: a DIFFERENT thing under one name. That one is
//!   the HTTP/1 wire parser's view — raw lines, `rawHeaders`, the
//!   `IncomingMessage` shape — and it is in another crate this one cannot
//!   depend on. Merging them would mean one class answering to two standards.
//! - **`node:url`'s `URLSearchParams`**: the closest existing shape, and where
//!   [`headers`]'s ordered-pairs-in-a-table arrangement comes from.
//! - **`node:buffer`'s `Blob`/`File`**: reached through the GLOBAL object rather
//!   than rebuilt, which is the only route across the crate boundary — see
//!   [`form_data`] for what needs it and what happens when the global is absent.
//!
//! # Where each class keeps its entries, and why that is two answers
//!
//! [`headers`] holds Rust `String`s in a process-global table keyed by a number
//! the instance carries — the shape `URLSearchParams` and `fs::dir` already use.
//! A header name and value are text by definition, so nothing in that table is a
//! value the collector would have to see.
//!
//! [`form_data`] cannot do that. A form entry's value may be a `Blob`, which is
//! a JS object, and a `u64` sitting in a process-global map is **invisible to
//! the collector** — the slot would be reused underneath it. So its entries live
//! in an ordinary JS array hanging off the instance, which the collector reaches
//! by the same walk it reaches everything else. Two mechanisms, each decided by
//! what it stores rather than by preference.
//!
//! # Not implemented, by name
//!
//! - **`fetch`.** See above.
//! - **`body` as a `ReadableStream`, and `Response.body`/`Request.body`.**
//!   Nothing in this engine is a `ReadableStream`; the raw body is kept under
//!   `__body` and read through `text()`/`json()`/`arrayBuffer()`/`bytes()`.
//! - **`formData()` on a `Request`/`Response`.** It parses `multipart/form-data`
//!   and `application/x-www-form-urlencoded` bodies, which is a parser this
//!   folder does not have and would be a second copy of `form_urlencoded`'s for
//!   half the job.
//! - **`Request`/`Response` `signal`, `cache`, `credentials`, `mode`,
//!   `referrer`, `integrity`.** Every one of them only means something to a
//!   network fetch.
//! - **Iteration as an `IterableIterator`.** `entries()`/`keys()`/`values()`
//!   answer plain JS arrays, so `Array.from`, `for`-`of` and spread all work and
//!   `.next()` does not. `node:url` states the same limit for the same reason:
//!   a host module cannot install a `Symbol.iterator`-keyed member.
//! - **Bun's two divergences from the standard, which this follows the standard
//!   on.** Bun iterates a `Headers` in insertion order where the standard sorts
//!   by name (Node sorts), and Bun sets no `Content-Type` for a string body
//!   where the standard sets `text/plain;charset=UTF-8` (Node sets it). Both
//!   were checked against a running engine rather than argued.

pub mod form_data;
pub mod headers;
pub mod message;

use rts_core::entry::{self, Context, Provided};

/// Installs all four classes as globals.
pub fn install(context: &mut Context) {
    let headers = headers::class(context);
    entry::declare_global(context, "Headers", headers);
    let form_data = form_data::class(context);
    entry::declare_global(context, "FormData", form_data);
    let (request, response) = message::classes(context);
    entry::declare_global(context, "Request", request);
    entry::declare_global(context, "Response", response);
}

/// A class: its prototype, its constructor, and the link both ways.
///
/// `constructor` on the prototype is the language's own requirement
/// (`x.constructor === C`) and, for the classes here that are reached from more
/// than one place, the per-CONTEXT record that makes a second call answer the
/// first one's cell. A `static` would be process-global where a context is
/// per-thread.
///
/// The recorded constructor counts only when its own `prototype` is the one
/// asked about. `get_member` walks the chain, so a prototype linked to another
/// class's would otherwise answer that class's `constructor` — which is exactly
/// what made `File` and `Blob` one class in `node:buffer` before the same guard
/// was put there.
///
/// # Why the PROTOTYPE is an argument and not built here
///
/// `entry::make_prototype` records the FILE that first installed a real method
/// table under a name and panics when a second file claims the same one — a
/// mechanism against two classes colliding. Building the prototype here made
/// this file the owner of every name in the folder, and each class's own
/// constructor — which asks for its prototype again on every `new` — was then
/// the second claimant. The panic was correct and the fix is to have exactly one
/// file ask per name, which is the class's own.
pub(super) fn class_of(
    context: &mut Context,
    name: &str,
    prototype: u64,
    construct: Provided,
) -> u64 {
    let held = entry::get_member(context, prototype, "constructor");
    if held != entry::undefined_in(context)
        && entry::get_member(context, held, "prototype") == prototype
    {
        return held;
    }
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    // `name` is a data property here because a native callable has none: this
    // engine records no `Function.prototype.name` for one, so `x.constructor
    // .name` — which is how a program identifies what it caught — reads
    // `undefined` for every host class in this workspace. Written for the
    // classes this folder owns rather than left to the engine, because the
    // engine-wide fix belongs beside `make_callable` and is not this folder's.
    let held = entry::make_string(context, name);
    entry::put_member(context, ctor, "name", held);
    ctor
}

/// `this` when `new` already made one, else a fresh instance.
pub(super) fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// An argument as text, `None` for an absent one.
///
/// `text_of` is `ToString`, which is what a `USVString`/`ByteString` parameter
/// takes — so `headers.append("a", 1)` records `"1"`, as it does in Node.
/// Ambient, so every caller is outside a borrow.
pub(super) fn text(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

/// A string value.
pub(super) fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An array of strings.
pub(super) fn string_array(values: &[String]) -> u64 {
    entry::with_runtime(|context| {
        let held = values.iter().map(|value| entry::make_string(context, value)).collect();
        entry::make_array_in(context, held)
    })
}

/// A JS array's elements — the same recipe `node:url` and `node:stream` use.
pub(super) fn elements(array: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length = entry::number_of(entry::get_indexed(array, string("length"))).unwrap_or(0.0);
    let count = match length.is_finite() && length > 0.0 {
        true => length as usize,
        false => 0,
    };
    (0..count).map(|at| entry::get_indexed(array, entry::make_number(at as f64))).collect()
}

/// One global by name — the only route to a class another crate owns.
///
/// `node:buffer`'s `File` is the case: `rts-std` cannot depend on `rts-node`, so
/// the alternative to this is a second `File` class, which would make
/// `fd.get("x") instanceof File` false for the very object this folder built.
/// `undefined` when nothing installed the name, which is what lets a caller
/// degrade instead of crash.
pub(super) fn global(name: &str) -> u64 {
    let key = entry::with_runtime(|context| i64::from(entry::member_key(context, name)));
    entry::global_get(key)
}
