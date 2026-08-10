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
//! # Why `URL`/`URLSearchParams` have no property SETTERS
//!
//! `rts_core::entry::accessor` (`get x()`/`set x(v)`) is reachable only
//! from `#[rtse::class]`-declared classes with compile-time-interned keys —
//! `rts-node` is restricted to `rts_core::entry::modules`, which
//! exports no accessor-installing function (confirmed by reading it in
//! full). `stream/common.rs` names the same restriction and answers it the
//! same way this module does: every property below is an ordinary **data**
//! property, computed once at construction (or, for `URLSearchParams`,
//! resynced by hand after every mutating method) and never a live getter. A
//! program assigning `url.pathname = '/x'` writes a plain property that
//! `href`/`origin`/`toString()` do not see change — refused by name below,
//! not silently approximated as a no-op.
//!
//! # Not implemented, by name
//!
//! - **Every `URL` property setter** (`href=`, `protocol=`, `username=`,
//!   `password=`, `host=`, `hostname=`, `port=`, `pathname=`, `search=`,
//!   `hash=`) — see above.
//! - **`URLSearchParams` as a live view of `url.searchParams`.** The
//!   `URLSearchParams` instance a `URL` hands back is a snapshot built once
//!   at construction; mutating it does not update `url.search`/`url.href`
//!   and vice versa, because that link needs the setter machinery above.
//! - **A thrown `TypeError`/`URIError` anywhere in this module.** No
//!   `rts_core::entry::modules` function raises a catchable exception —
//!   `rts-core::entry::throw` ends the whole program rather than
//!   unwinding to a handler (see its own doc). Every failure mode Node
//!   documents as throwing answers `undefined`/`null`/an inert instance
//!   here instead, the same trade `path.rs` and `querystring.rs` already
//!   make, extended to this module.
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
pub(super) fn class_ctor(context: &mut Context, construct: Provided, prototype: u64) -> u64 {
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
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
