//! `node:url` — the WHATWG `URL`/`URLSearchParams` classes, plus Node's own
//! file-URL and legacy-parsing utilities, over `docs/reference/node/url.md`.
//!
//! # What reuse-check found
//!
//! `rts-cranelift` and `rts-core`'s `entry` module have no URL parser, no
//! percent-encoding table and no IDNA/Punycode implementation anywhere — this
//! is genuinely new surface, not a second copy of one. The three crates the
//! task brief names (`url`, `percent-encoding`, `idna`) are already declared
//! in this crate's `Cargo.toml`, vetted by `docs/reference/node/crates.md`
//! §4.8, so this module calls them rather than hand-rolling a parser — the
//! one thing the reference document is explicit about NOT doing again.
//! `querystring.rs` has its own small percent-encode/-decode pair, but it is
//! `private` to that file (not exported) and implements the historical
//! `application/x-www-form-urlencoded` convention (`+` for space) rather than
//! the WHATWG URL encode sets this module needs — reusing it would have
//! meant reusing the wrong table, so `URLSearchParams` goes through
//! `form_urlencoded` instead, which is what `url.md` §5.1 names for exactly
//! this.
//!
//! `fs/dir.rs`'s `TABLE`/`NEXT_ID` pattern (a `Mutex`-backed map keyed by a
//! number an instance carries) is reused verbatim for both classes here: a
//! parsed `url::Url` and a `Vec<(String, String)>` are native Rust values
//! this crate's value API cannot hold *inside* a JS object, exactly the
//! reason `dir.rs` gives for its own table.
//!
//! # How `URL` got its property setters, and what was stale
//!
//! This doc used to say a host module could not install an accessor at all:
//! `entry::define_getter`/`define_setter` take a key NUMBER out of the
//! compiler's registry and "nothing on this surface mints one from a name". That
//! stopped being true when `entry::member_key` was added — it is exactly the
//! `&str` → key-number function that was missing, and `rts-ui` was already
//! calling it. So every `URL` property below is a real accessor pair on
//! `URL.prototype`, which is where the specification puts them, and the eleven
//! snapshot data properties this module used to stamp onto each instance are
//! gone.
//!
//! That is what makes `url.searchParams` LIVE in both directions: the instance
//! carries nothing but `__urlId` and the one `URLSearchParams` object bound to
//! it, both sides read and write the single parsed `url::Url` in `class.rs`'s
//! table, and there is no snapshot left to fall out of step.
//!
//! Two things it forced, and each is stated where it happens. The accessors are
//! installed on FIRST USE rather than at `install`, because `define_getter` is
//! ambient and the host hands this crate a `&mut Context` before any context is
//! on the thread (`class::prototype`). And no property below may also exist as
//! an own data property on an instance, because `accessor::setter_for` stops the
//! chain walk at one — an own `pathname` would silently shadow the setter.
//!
//! # Not implemented, by name
//!
//! - **`url.href = x` throwing for an unparseable `x`.** WHATWG makes `href` the
//!   one setter that throws; the other nine are specified as "if it fails,
//!   return", which is what they do here. A failed `href` assignment leaves the
//!   URL as it was.
//! - **A thrown `TypeError`/`URIError` for a malformed CONSTRUCTOR argument.**
//!   `new URL("nonsense")` answers an INERT instance — one carrying no
//!   `__urlId`, whose every property reads `""` — where Node throws. A native
//!   here can raise now (`entry::throw_type_error`), so this is a decision
//!   rather than a wall: `URL.canParse`/`URL.parse` are what a program tests
//!   with, and turning every existing inert instance into a process-visible
//!   throw is a behaviour change this module has not measured against the
//!   suite.
//! - **`URLPattern`.** A WICG pattern-matcher over five percent-encode sets
//!   and a router-style wildcard compiler; nothing in `url`/`idna` supplies
//!   it, and building one is its own module's worth of work, not a URL
//!   parsing task.
//! - **`URL.createObjectURL`/`URL.revokeObjectURL`.** `url.md` §5.7 flags
//!   this itself: the registry the Blob-URL pair needs is a `Blob` this
//!   module cannot reach (`node:buffer`'s handle table, a sibling module)
//!   without a cross-module registry neither side owns yet.
//! - **Ambient globals.** `URL`/`URLSearchParams` are exported as named
//!   members of this module's namespace; wiring them onto the global object
//!   with no import is `lib.rs`'s/`global.rs`'s call, not this folder's —
//!   this module does not touch either file.
//! - **`Object.keys(new URL(...))` answering `[]`.** It answers
//!   `["__urlId", "searchParams"]`: the two own properties an instance carries.
//!   That is two names where Node has none, and it is a large improvement on the
//!   eleven this module used to stamp — the remaining pair is the instance↔table
//!   key and the live view, and neither has a place to hide while a host module
//!   cannot mark a property non-enumerable.
//! - **`URLSearchParams.prototype.size` as an accessor.** It is a data property
//!   resynced after every mutating method, which is right for a standalone
//!   instance and STALE for one bound to a `URL` whose `search` was assigned
//!   directly. Everything a program reads through a method (`get`, `getAll`,
//!   `toString`, `forEach`, …) reads the URL live; only `size` is a snapshot.
//! - **`URLSearchParams` iteration protocol** (`Symbol.iterator`,
//!   `for...of`, spread). `entries()`/`keys()`/`values()` answer plain JS
//!   arrays here rather than `IterableIterator`s — this crate's value API
//!   has no iterator-protocol constructor for a host module to reach.
//! - **The `options` argument on `URLSearchParams` methods beyond
//!   `delete`/`has`'s 2-arg `value` overload** — matches what is wired.
//! - **`url.parse`'s exact lenient quirk set and `url.resolve`'s exact
//!   legacy algorithm.** Implemented as a best-effort approximation (see
//!   `legacy.rs`'s own doc), not verified byte-for-byte against Node — the
//!   same caveat `querystring.rs` states for its own encode table.
//! - **IDNA's disallowed-character/UTS-46 mapping pass beyond what `idna`
//!   itself performs.** `domainToASCII`/`domainToUnicode` are thin wrappers
//!   over the `idna` crate; `url.md` §5.1 point 3 flags this same precision
//!   gap for the reference implementation itself.

mod class;
mod fileurl;
mod legacy;
mod search_params;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:url` is.
pub fn namespace(context: &mut Context) -> u64 {
    let url_ctor = class::install(context);
    let search_params_ctor = search_params::install(context);

    let members: &[(&str, Provided)] = &[
        ("parse", legacy::parse),
        ("format", legacy::format),
        ("resolve", legacy::resolve),
        ("domainToASCII", fileurl::domain_to_ascii),
        ("domainToUnicode", fileurl::domain_to_unicode),
        ("fileURLToPath", fileurl::file_url_to_path),
        ("fileURLToPathBuffer", fileurl::file_url_to_path_buffer),
        ("pathToFileURL", fileurl::path_to_file_url),
        ("urlToHttpOptions", fileurl::url_to_http_options),
    ];
    let namespace = entry::make_namespace(context, members);
    entry::put_member(context, namespace, "URL", url_ctor);
    entry::put_member(context, namespace, "URLSearchParams", search_params_ctor);
    namespace
}

/// A callable constructor: `make_callable(construct)`, `prototype` recorded
/// under `"prototype"` — the recipe `stream/mod.rs::class_ctor` already
/// works out, generalised to a `Provided` prototype-builder so `class.rs`
/// and `search_params.rs` both use it without a second copy.
///
/// # `entry::declare_host_class`, not the two hand-written lines this used to be
///
/// This used to write `ctor.name` itself (a bare `put_member`) and stop
/// there. That fixes `URL.name` (a STATIC read off the class) but not
/// `new URL(x).constructor.name` (an INSTANCE read, which walks
/// `prototype.constructor` — never written here at all) — the exact split
/// `crate::stream::class_ctor`'s doc names. `declare_host_class` does both:
/// `.name`/`.length` on the constructor AND the `prototype.constructor`
/// back-link, in the one call.
pub(super) fn class_ctor(context: &mut Context, name: &str, arity: u32, construct: Provided, prototype: u64) -> u64 {
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, name, arity);
    ctor
}

/// An argument as text, `None` for an absent (`undefined`) one — the same
/// convention `path.rs::text` and `querystring.rs::argument_text` use.
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

/// A string value, from a context already in hand.
pub(super) fn string_in(context: &mut Context, text: &str) -> u64 {
    entry::make_string(context, text)
}

/// A plain data property, read as text.
pub(super) fn get_text(this: u64, name: &str) -> Option<String> {
    text(entry::get_indexed(this, string(name)))
}

/// One accessor property — `get x()`, and `set x(v)` when there is one.
///
/// # Why this exists here and could not before
///
/// `entry::define_getter` takes a property key as the NUMBER the compiler's
/// registry issued, and `entry::member_key` is what turns a `&str` into one.
/// Both are ambient — each takes its own borrow of the context — so this
/// collects everything it needs inside ONE `with_runtime` and makes the two
/// definition calls after that borrow is gone. A second borrow inside an
/// `extern "C"` frame is a panic that cannot unwind: it aborts the process.
pub(super) fn define_accessor(
    object: u64,
    name: &str,
    getter: Provided,
    setter: Option<Provided>,
) {
    let (key, getter, setter) = entry::with_runtime(|context| {
        (
            i64::from(entry::member_key(context, name)),
            entry::make_callable(context, getter),
            setter.map(|code| entry::make_callable(context, code)),
        )
    });
    entry::define_getter(object, key, getter);
    if let Some(setter) = setter {
        entry::define_setter(object, key, setter);
    }
}

/// `this` if it is already an object (a `new` over a subclass hands one in),
/// else a fresh instance of `prototype` — `stream/common.rs::self_or_new`'s
/// own recipe, copied rather than reached for: that function is `pub(super)`
/// to the `stream` module, and duplicating four lines was cheaper than
/// widening another module's visibility for one caller.
pub(super) fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}
