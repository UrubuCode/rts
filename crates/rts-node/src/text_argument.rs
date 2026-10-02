//! Reading a text ARGUMENT: the three questions, and one form for each.
//!
//! # Why this module exists
//!
//! Three modules wrote the same private helper — `url/mod.rs::text`,
//! `path.rs::text`, `querystring.rs::argument_text` — each spelling
//! "`undefined` is absent", and `url/mod.rs`'s own comment said it was "the
//! same convention" as the other two. One convention, written three times,
//! answering **one** of the three questions a native asks about a text
//! argument.
//!
//! The other two were then wrong wherever they were needed, and wrong
//! silently. Measured against node 22 on 2026-10-02: ten of thirteen text
//! argument readings diverged, in BOTH directions — `URL`,
//! `URLSearchParams`, `querystring.escape` and the global `console`'s label
//! refused where Node coerces; `path.join`, `path.basename`, `path.resolve`
//! and `querystring.parse` coerced where Node refuses. That is #2850, and
//! `docs/engine/one-form-per-question.md` carries the entry.
//!
//! # The three questions
//!
//! They differ for `undefined` specifically, which is why one function cannot
//! serve them:
//!
//! | the parameter | the form | `undefined` gives |
//! |---|---|---|
//! | WebIDL `USVString`, required | [`usv`] | `"undefined"` |
//! | must BE a string | [`string_or_refuse`] | a raised `ERR_INVALID_ARG_TYPE` |
//! | has a default | [`optional`] | `None`, so the default applies |
//!
//! `rts_core::entry::text_of` is the mechanism behind two of them and the
//! answer to none on its own: it never answers `None` for `undefined`, so
//! `text_of(x).unwrap_or(default)` is a default that cannot run.

use rts_core::entry;

/// A parameter with a DEFAULT: `None` for an absent (`undefined`) one.
///
/// The check comes BEFORE any coercion, because the language fires a default
/// parameter on `undefined` specifically rather than on "the argument was
/// omitted" — `function f(x = 1)` defaults for an explicit `f(undefined)` too.
///
/// Used by `querystring`'s `sep` and `eq`, `basename`'s `suffix`, `URL`'s
/// `base`, and `console`'s label.
pub(crate) fn optional(value: u64) -> Option<String> {
    match value == entry::undefined_value() {
        true => None,
        false => entry::text_of(value),
    }
}

/// A parameter that is `ToString`ed, `undefined` included.
///
/// `None` only for an object, whose `toString` would call user code an entry
/// point cannot call — the boundary every conversion in `rts-core` stops at.
///
/// Used by `querystring.escape`/`unescape`.
pub(crate) fn coerced(value: u64) -> Option<String> {
    entry::text_of(value)
}

/// A required WebIDL `USVString`.
///
/// `ToString` runs on whatever arrives, so `undefined` is the five characters
/// `undefined` — `new URL(undefined, "http://h/")` is `"http://h/undefined"` in
/// every engine — and an unpaired surrogate becomes `U+FFFD` rather than a
/// refusal, which is what the IDL conversion is DEFINED as.
///
/// Used by `URL`'s `url`, and every `name`/`value` of `URLSearchParams`.
pub(crate) fn usv(value: u64) -> Option<String> {
    entry::usv_text_of(value)
}

/// A parameter that must BE a string — `None` with the refusal already RAISED.
///
/// `string_in` and not `text_of`: coercing here is what let `path.join(7, 8)`
/// answer `"7/8"` and `path.resolve(undefined)` answer the working directory,
/// where Node refuses each with `ERR_INVALID_ARG_TYPE`.
///
/// `name` is which argument the message names, because Node's is not always
/// `"path"`: `relative` says `"from"` and `"to"`, and `basename`'s second
/// parameter is `"suffix"`. Measured — a message naming the wrong argument
/// sends a reader to the wrong line.
pub(crate) fn string_or_refuse(name: &str, value: u64) -> Option<String> {
    match entry::with_runtime(|context| entry::string_in(context, value)) {
        Some(text) => Some(text),
        None => {
            crate::errors::invalid_arg_type(name, "string", value);
            None
        }
    }
}
