//! `Error.prototype.toString`, and the `name: message` header a trace opens with.
//!
//! # Why the two live together and apart from the constructors
//!
//! Because they answer the SAME question by two different rules, and keeping
//! them side by side is what stops the cheaper one being used for the dearer
//! one. [`described`] is `Error.prototype.toString`, written in terms of `Get`
//! and `ToString`, so a `name` accessor runs and `err.name = 7` prints `7`.
//! [`joined`] reads data properties only and answers `None` for a cell carrying
//! neither field, because its callers are an UNCAUGHT throw and a `.stack`
//! render — a program that has already failed must not be asked to run a getter
//! to describe its own failure.
//!
//! Out of `error.rs` because that file stood at 620 lines against this crate's
//! 500-line ceiling (rule 6) and this is the cohesive hundred: nothing here
//! constructs anything, and nothing in `error.rs` describes anything.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// `Error.prototype.toString` — `name` and `message` joined the way the
/// specification joins them.
///
/// # Why this does not go through [`joined`]
///
/// Because the two answer different questions and only one of them may run user
/// code. `joined` is what an UNCAUGHT throw prints, and a program that has
/// already failed must not be asked to run a getter to describe its own failure;
/// this is `Error.prototype.toString`, which the specification writes in terms
/// of `Get` and `ToString` — so `err.name = 7` prints `7`, an object with a
/// `toString` prints what it answers, and a `name` accessor runs.
///
/// It went through `joined` and inherited three wrong answers from doing so, all
/// three of them the same mistake — reading a property's ABSENCE and its
/// `undefined` as the same thing. `{ name: undefined }` printed `"undefined: m"`
/// where the language substitutes `"Error"`, `{ message: undefined }` printed a
/// trailing `": undefined"` where it substitutes the empty string, and a `name`
/// of `""` printed a leading `": "` where the language answers the message
/// alone.
pub(super) fn described(this: u64) -> u64 {
    // `Error.prototype.toString.call(1)` is a `TypeError`, not a description of
    // the number. `as_slot` is the wrong test for it — a string primitive has a
    // cell — so this asks the same "is it an object" every other coercion here
    // asks.
    if !with_current(|context| super::objects::is_object(context, this)) {
        super::throw::type_error("Error.prototype.toString called on non-object");
        return with_current(|context| undefined_of(context));
    }
    let Some(name) = field_text(this, "name", "Error") else {
        return with_current(|context| undefined_of(context));
    };
    let Some(message) = field_text(this, "message", "") else {
        return with_current(|context| undefined_of(context));
    };
    let joined = match (name.is_empty(), message.is_empty()) {
        (true, _) => message,
        (false, true) => name,
        (false, false) => format!("{name}: {message}"),
    };
    with_current(|context| context.intern_value(Str::from_str(&joined)).bits())
}

/// One of `toString`'s two fields: `Get` then `ToString`, with a default for
/// `undefined`.
///
/// The default is what the specification substitutes and it is substituted for
/// `undefined` ALONE — a missing property reads `undefined` through the chain
/// and lands here the same way, which is why one test covers both. Every other
/// value converts, `null` and `0` included: `{ name: null }` describes itself as
/// `"null"` in every runtime.
///
/// `None` is a throw in flight — the getter's or the conversion's — which the
/// caller propagates under rule 8.
fn field_text(this: u64, field: &str, default: &str) -> Option<String> {
    let key = with_current(|context| context.well_known_text(field));
    let found = super::computed::get_indexed(this, key);
    if super::throw::in_flight() {
        return None;
    }
    if found == with_current(|context| undefined_of(context)) {
        return Some(default.to_owned());
    }
    let text = super::text::to_string_value(found)?;
    with_current(|context| {
        super::text::to_text(context, Value(text))
            .and_then(|held| held.to_rust())
            .or(Some(String::new()))
    })
}

/// `name: message`, from properties alone.
///
/// `None` for a cell carrying neither, which is what makes this usable from
/// [`super::throw`]: an uncaught value that is not an error must not be
/// described as `"Error"`.
///
/// Nothing here runs user code. Both fields are read through
/// [`super::objects::read_property`], which answers data properties and walks
/// the chain — a getter is the accessor path and is deliberately not this one,
/// because the caller may be a program that has already failed.
pub(in crate::entry) fn joined(context: &mut Context, cell: u32) -> Option<String> {
    let read = |context: &mut Context, field: &str| {
        let key = context.well_known(field);
        let found = super::objects::read_property(context, cell, key)?;
        // `undefined` is ABSENT here, not the word. A property that is not there
        // and one holding `undefined` are the same thing to `Error.prototype.
        // toString`, which substitutes its default for both — and reading the
        // word is what printed `undefined: boom` for `err.name = undefined`.
        if found.bits() == undefined_of(context) {
            return None;
        }
        super::text::to_text(context, found)?.to_rust()
    };
    let name = read(context, "name");
    let message = read(context, "message");
    if name.is_none() && message.is_none() {
        return None;
    }
    let name = name.unwrap_or_else(|| "Error".to_owned());
    let message = message.unwrap_or_default();
    // An EMPTY name answers the message alone, which is the third arm the
    // language spells out and the one a `{ name: "" }` reaches: the join is
    // `name: message` only when there are two halves to join.
    Some(match (name.is_empty(), message.is_empty()) {
        (true, _) => message,
        (false, true) => name,
        (false, false) => format!("{name}: {message}"),
    })
}
