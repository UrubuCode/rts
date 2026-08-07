//! What a value has to become to survive leaving its region.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search found `node:v8`'s
//! `serialize`/`deserialize`, which is the nearest thing here and is **not**
//! this: it walks a graph and rebuilds it in the SAME region, so what it
//! answers is a reference — and a reference belongs to the region that made
//! it. Nothing in `rts-cranelift` answers this either; a region is a machine
//! concept but "what may cross one" is a decision about values, which is the
//! language layer's and therefore ours.
//!
//! So this is a second copier and deliberately so, and the two differ in the
//! one way that matters: nothing here holds a reference at any point. A
//! `Portable` is plain Rust data with no cell in it, which is exactly what
//! makes it safe to hand to another thread.
//!
//! # What crosses, and what is refused by name
//!
//! `undefined`, `null`, a boolean, a number, a string, and an array or plain
//! object built out of those — an object by its enumerable own properties,
//! which `entry::member_names` answers with `Object.keys`'s own walk. That is a
//! subset of structured clone and the module doc says which parts are missing:
//! a `Map`/`Set`, a `Date`, a `RegExp`, an `ArrayBuffer`, a typed array, a
//! function, a class instance's prototype, and a cycle.
//!
//! A cycle is refused rather than resolved: the depth limit below turns one
//! into [`Portable::Unsupported`], so a program gets a named marker rather
//! than this thread looping until the stack ends.

use rts_core_rwk::entry::{self, Context};

/// A value with no cell in it, so it can cross a thread and a region.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Portable {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Portable>),
    Record(Vec<(String, Portable)>),
    /// Something structured clone would carry and this cannot. Carries the
    /// reason, and a receiver sees the STRING of that reason rather than a
    /// value silently becoming `undefined` — a wrong answer that runs is the
    /// outcome this repository refuses, and a message that arrives shaped
    /// wrong is exactly that.
    Unsupported(&'static str),
}

/// How deep a value may nest before it is refused.
///
/// A cycle is why there is a limit at all. Sixteen because a message is data a
/// program assembled to send, not an object graph it happened to have — and a
/// limit low enough to hit by accident is better than a stack that ends.
const DEPTH: usize = 16;

/// Reads a value into its portable form, from a context already in hand.
pub(super) fn portable(context: &mut Context, value: u64, depth: usize) -> Portable {
    if depth >= DEPTH {
        return Portable::Unsupported("too deeply nested, or a cycle");
    }
    if value == entry::undefined_in(context) {
        return Portable::Undefined;
    }
    if value == entry::null_in(context) {
        return Portable::Null;
    }
    if value == entry::boolean_value(true) {
        return Portable::Bool(true);
    }
    if value == entry::boolean_value(false) {
        return Portable::Bool(false);
    }
    // `string_in`, never `text_in`: the second is `ToString` and answers "42"
    // for the NUMBER 42. This asked it as a type test, so every number crossed
    // as a string and the copy looked right until `value.a + value.b.c`
    // answered "12" instead of 3.
    //
    // Before the number check either way, because a string has a numeric
    // coercion too — asking `number_of` first turns "3" into 3.
    if let Some(text) = entry::string_in(context, value) {
        return Portable::Text(text);
    }
    if let Some(number) = entry::number_of(value) {
        return Portable::Number(number);
    }
    if entry::is_array_in(context, value) {
        let length = entry::get_member(context, value, "length");
        let count = entry::number_of(length).unwrap_or(0.0) as usize;
        let mut items = Vec::with_capacity(count);
        for index in 0..count {
            let held = entry::get_member(context, value, &index.to_string());
            items.push(portable(context, held, depth + 1));
        }
        return Portable::List(items);
    }
    if entry::is_object(context, value) {
        // `entry::member_names` is `Object.keys`'s own walk, reached rather than
        // repeated. Before it existed this had no way to ask an object what its
        // properties are, and the plausible workaround — crossing as an empty
        // object — is the answer that looks like it worked.
        let fields = entry::member_names(context, value)
            .into_iter()
            .map(|name| {
                let held = entry::get_member(context, value, &name);
                let carried = portable(context, held, depth + 1);
                (name, carried)
            })
            .collect();
        return Portable::Record(fields);
    }
    Portable::Unsupported("not a value structured clone carries here")
}

/// Rebuilds a portable value as a real one, in whatever region `context` is.
///
/// This is the half that must run on the RECEIVING thread: every cell it makes
/// belongs to that thread's region, which is the point of the round trip.
pub(super) fn rebuild(context: &mut Context, value: &Portable) -> u64 {
    match value {
        Portable::Undefined => entry::undefined_in(context),
        Portable::Null => entry::null_in(context),
        Portable::Bool(held) => entry::boolean_value(*held),
        Portable::Number(held) => entry::make_number(*held),
        Portable::Text(held) => entry::make_string(context, held),
        Portable::List(items) => {
            let values = items.iter().map(|item| rebuild(context, item)).collect();
            entry::make_array_in(context, values)
        }
        Portable::Record(fields) => {
            let object = entry::make_object(context);
            for (name, held) in fields {
                let carried = rebuild(context, held);
                entry::put_member(context, object, name, carried);
            }
            object
        }
        Portable::Unsupported(reason) => entry::make_string(context, reason),
    }
}
