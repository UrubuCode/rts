//! What a value is, as far as JSON is concerned — asked once, in one borrow.

use super::super::super::Context;
use super::super::super::with_current;
use crate::value::{Kind, Value};

/// What a value is, as far as JSON is concerned.
///
/// Six kinds and an absence. A symbol is the one the language has that this
/// does not name: it serialises as an absence like a function, which is what
/// [`Shape::Absent`] already answers for it, so a variant would carry no
/// decision. A `BigInt` earns one because its rule is a `TypeError` rather than
/// text, which is why this is an enum rather than a chain of tests at the call
/// site.
///
/// A wrapper object is NOT one of them, and deliberately: the specification
/// says `SerializeJSONProperty` replaces `new Number(5)` by its
/// `[[NumberData]]` before it classifies anything, so it arrives here already
/// as `Number(5.0)`. A variant would be a second place deciding what a wrapper
/// serialises as, and the first place is where `valueOf` reads it from.
pub(in crate::entry::json) enum Shape {
    Null,
    Bool(bool),
    Number(f64),
    /// A string, BY THE CELL that holds it.
    ///
    /// It carried a `Str` — an owned copy of the whole buffer — and its only
    /// consumer took a reference to it. The copy existed because a `Shape` is
    /// carried out of the `with_current` closure that made it, and nothing had
    /// asked whether it needed to be.
    ///
    /// It does not: `quoted` touches no context, only `self.out`, so the write
    /// can happen inside the borrow — which is what `plain` already does for a
    /// member's KEY one screen below.
    Text(u32),
    /// An array, by the cell that identifies it.
    Array(u32),
    /// Anything else with properties.
    Object(u32),
    /// A bigint, which the language refuses to serialise rather than
    /// approximating. Its own variant because it is the one shape here that
    /// answers with a `TypeError` instead of with text.
    Big,
    /// `undefined`, a function, or anything with no JSON form.
    Absent,
}

pub(in crate::entry::json) fn shape_of(context: &Context, value: u64) -> Shape {
    // The wrapper's primitive, before anything else is asked. Without it a
    // `new Number(5)` reached `Shape::Object` and serialised as `{}` — the
    // object has no own properties, so the output was well-formed JSON that had
    // silently dropped the value. `Object(5)` is the same object by another
    // spelling and needs the same substitution, which is why this is here rather
    // than in the `Number` class.
    let value = Value(super::super::super::primitive_proto::unwrap(context, value));
    if let Some(number) = value.numeric() {
        return Shape::Number(number);
    }
    // Before the slot test: a bigint is a client value, not an object, and
    // asking `as_slot` first would file it among the objects and serialise it
    // as `{}` — well-formed JSON that lost the number, which is exactly what
    // this answered before.
    if super::super::super::bigints::digits_of(context, value.bits()).is_some() {
        return Shape::Big;
    }
    if let Some(flag) = value.as_bool() {
        return Shape::Bool(flag);
    }
    if let Some(cell) = value.as_slot() {
        if let Some(text) = context.text_at(cell) {
            // The cell rather than the text: see `Shape::Text`.
            let _ = text;
            return Shape::Text(cell);
        }
        // Asked before "does it have elements", because a callable is an object
        // too and the language says a function has no JSON form wherever it
        // appears. Getting the order wrong writes a function's properties.
        if context.callable_at(cell).is_some() {
            return Shape::Absent;
        }
        if context.elements_at(cell).is_some() {
            return Shape::Array(cell);
        }
        return Shape::Object(cell);
    }
    match value.kind() {
        Kind::Singleton(number) if number == context.singletons.null => Shape::Null,
        // `undefined` and any singleton this crate does not name. Answering
        // `null` for an unknown one would invent data; absence is recoverable.
        _ => Shape::Absent,
    }
}

/// One level of indentation, from the third argument to `stringify`.
///
/// A number of spaces or a string, both capped at ten, which is the
/// specification's own cap — and worth keeping rather than simplifying away,
/// because it is what stops `JSON.stringify(o, null, 1e9)` from asking for a
/// gigabyte of spaces per line.
pub(in crate::entry::json) fn indent_of(space: u64) -> Vec<u16> {
    with_current(|context| match shape_of(context, space) {
        Shape::Number(count) => {
            let count = count.floor().clamp(0.0, 10.0) as usize;
            vec![b' ' as u16; count]
        }
        Shape::Text(cell) => context
            .text_at(cell)
            .map_or_else(Vec::new, |text| text.units().take(10).collect()),
        _ => Vec::new(),
    })
}

/// Whether [`Writer::direct`] will write this value: text, a number, a boolean
/// or `null`. Not `undefined`, a symbol or a bigint — each has a rule of its
/// own (a skipped member, a `TypeError`) that the ordinary path states once.
// `#[inline]`: called per member from the runs in `walk.rs`. One file kept it
// inlined for free; across the split the clock read +10% on an eight-member
// object until it was asked for.
#[inline]
pub(super) fn is_primitive(context: &Context, value: u64) -> bool {
    if Value(value).numeric().is_some() {
        return true;
    }
    match Value(value).as_slot() {
        Some(cell) => context.text_at(cell).is_some(),
        None => matches!(shape_of(context, value), Shape::Null | Shape::Bool(_) | Shape::Number(_)),
    }
}
