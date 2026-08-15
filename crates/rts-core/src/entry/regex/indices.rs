//! `m.indices` — where each group of a match was, for a `d` pattern.
//!
//! # Why this is a module and not four lines in `exec`
//!
//! Because three call sites answer a match object — `exec`,
//! `String.prototype.match` without `g`, and `matchAll` — and the language says
//! all three answer the SAME shape. Three copies of "an array of pairs with a
//! `groups` object hanging off it" is three chances to disagree about which of
//! them names a group that took part in no alternative, which is the one case
//! that is not a pair at all.
//!
//! # Why the pair is built once and stored twice
//!
//! `m.indices[1]` and `m.indices.groups.year` are the SAME array in every
//! engine, because the specification builds one pair and writes it into both.
//! Answering two equal-looking arrays would pass every test that compares
//! contents and fail the one that compares identity, so the object written into
//! `groups` is read back out of the array rather than built again.
//!
//! # Why positions are converted here rather than by the caller
//!
//! The spans come from the matcher in **bytes** and the language counts UTF-16
//! code units. A caller converting them would be a fourth place that knows the
//! difference, and `"é".length` is 1 against a UTF-8 length of 2 — so a pair
//! handed back unconverted names the middle of a character.

use super::super::Context;
use super::super::array::built_in;
use super::super::objects::{put, undefined_of};
use super::compile::Spans;
use super::methods::units_before;
use crate::value::Value;

/// Writes `indices` onto a match array that has already been built.
///
/// Takes the context rather than fetching one, because every caller is already
/// inside a borrow filling the same array — and a second `with_current` from in
/// there re-enters the `RefCell`, which is the deadlock this crate has paid for
/// once already.
///
/// `names` is the matcher's positional list — one entry per group, `None` for
/// an unnamed one — which is what pairs a name with the pair that belongs to it
/// without a second traversal deciding the order.
pub(in crate::entry) fn onto(
    context: &mut Context,
    array: u64,
    subject: &str,
    spans: &Spans,
    names: &[Option<String>],
) {
    let Some(cell) = Value(array).as_slot() else {
        return;
    };
    let value = of_match(context, subject, spans, names);
    let key = context.well_known("indices");
    put(context, cell, key, value);
}

/// The `indices` array of one match, with its `groups` object already on it.
fn of_match(context: &mut Context, subject: &str, spans: &Spans, names: &[Option<String>]) -> u64 {
    let absent = undefined_of(context);
    // The outer array FIRST, sized and empty. Every pair below allocates, and
    // an allocation collects — a pair that has landed in the array is reachable
    // through it, where one waiting in a Rust `Vec` is not. The same two-step
    // shape `exec` uses to fill a match array, for the same reason.
    let array = built_in(context, vec![absent; spans.len()]);
    let Some(cell) = Value(array).as_slot() else {
        return array;
    };
    for (at, span) in spans.iter().enumerate() {
        // A group that took part in no alternative has no pair at all —
        // `undefined`, which is what `/(\d+)(?:\.(\d+))?/d` answers for its
        // second group against `"42"`. An empty array would compare equal to
        // nothing a program tests for.
        let Some((from, to)) = *span else { continue };
        let pair = built_in(
            context,
            vec![
                Value::from_f64(units_before(subject, from) as f64).bits(),
                Value::from_f64(units_before(subject, to) as f64).bits(),
            ],
        );
        if let Some(elements) = context.elements_at_mut(cell) {
            elements[at] = pair;
        }
    }
    let groups = named_object(context, cell, names);
    let key = context.well_known("groups");
    put(context, cell, key, groups);
    array
}

/// `m.indices.groups` — the pairs of the named groups, or `undefined`.
///
/// The same two answers [`super::groups_object`] gives for `m.groups`, and for
/// the same reason: `m.indices.groups?.x` distinguishes an object with no such
/// name from a pattern that named nothing, and a plain object would have
/// inherited `Object.prototype`.
fn named_object(context: &mut Context, array: u32, names: &[Option<String>]) -> u64 {
    let absent = undefined_of(context);
    if names.iter().all(Option::is_none) {
        return absent;
    }
    let Some(holder) = super::super::native::plain(context) else {
        return absent;
    };
    let bare = super::methods::null_of(context);
    context.set_prototype(holder, bare);
    for (at, name) in names.iter().enumerate() {
        let Some(name) = name else { continue };
        // Read back out of the array rather than built again: the pair is one
        // object reached two ways, which is what the specification says and
        // what an identity comparison sees.
        let pair = context
            .elements_at(array)
            .and_then(|elements| elements.get(at).copied())
            .unwrap_or(absent);
        let key = context.well_known(name);
        put(context, holder, key, pair);
    }
    Value::from_slot(holder).bits()
}
