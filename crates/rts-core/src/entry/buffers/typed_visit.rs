//! The members of a typed array that run **user code** over its elements.
//!
//! # Why they are apart from [`super::typed`]
//!
//! Everything in that module answers from bytes alone: it takes one borrow,
//! reads or writes a window, and answers. Every member here calls back into the
//! program between two element accesses, which is the opposite discipline — the
//! borrow has to be **gone** before the call and taken again after, because a
//! callback reaches the runtime and a `with_current` inside a `with_current`
//! aborts the process rather than panicking.
//!
//! So the shape is the same in all of them and is worth naming once: read the
//! whole element list under one borrow, call with none held, and ask
//! [`throw::in_flight`] after every call before looking at what came back —
//! rule 8 of this crate's README. A callback that threw answers `undefined`,
//! and `undefined` is a value: a `filter` that carried on would keep elements
//! the program never approved, and a `reduce` would fold a value that was never
//! produced.
//!
//! # Why the answers are built from element VALUES and not from bytes
//!
//! `map` and `filter` produce a new array of the receiver's own kind, and the
//! elements crossing between them are JS values — which is what lets the two
//! bigint classes take the same path as the other nine, since
//! [`super::element_word`] is the one place that decides what a kind accepts.
//! A byte copy would have been exact for `filter`, where no value changes, and
//! would then be a second copy rule beside `map`'s; the module the two live in
//! already states why one rule is worth more than one saved conversion.

use super::element::Kind;
use super::{View, typed, with_current};
use crate::entry::objects::undefined_of;
use crate::entry::{functions, throw};
use crate::value::Value;

/// Every element of the receiver, or `None` when it is not a view at all.
fn listed(this: u64) -> Option<Vec<u64>> {
    with_current(|context| {
        let view = super::view_of(context, this)?;
        Some(typed::elements(context, &view))
    })
}

/// The receiver's element kind.
fn kind_of(this: u64) -> Option<Kind> {
    with_current(|context| Some(super::view_of(context, this)?.kind))
}

/// A fresh typed array of `kind` holding these values.
///
/// The words are computed before the window is taken because
/// [`super::element_word`] reads the heap — a bigint's digits live in a slab —
/// and the write needs the same context mutably.
pub(super) fn built(kind: Kind, values: &[u64]) -> u64 {
    // The refusal is asked FIRST and raised outside the borrow: a value of the
    // wrong content type — a number into a bigint element or the other way — is
    // a `TypeError` in the language, and `throw::type_error` builds the
    // program's own error object, which needs the context this closure would
    // still be holding.
    let refused = with_current(|context| {
        values
            .iter()
            .any(|value| super::element_word(context, *value, kind).is_none())
    });
    if refused {
        throw::type_error("Cannot convert value to the array's element type");
        return super::undefined();
    }
    with_current(|context| {
        let size = kind.size();
        let words: Vec<Option<u64>> = values
            .iter()
            .map(|value| super::element_word(context, *value, kind))
            .collect();
        let Some(buffer) = super::new_buffer(context, values.len() * size) else {
            return undefined_of(context);
        };
        let view = View {
            buffer,
            offset: 0,
            length: values.len() * size,
            kind,
        };
        if let Some(window) = super::window_mut(context, &view) {
            for (at, word) in words.iter().enumerate() {
                // `None` is an element this kind refuses — the same refusal
                // `super` states for a write. The element keeps the zero it was
                // allocated with rather than taking a coerced value.
                if let Some(word) = word {
                    super::element::write_word(window, at * size, kind, *word, true);
                }
            }
        }
        typed::made(context, view)
    })
}

/// The array `map` and `filter` answer: the species protocol's object when a
/// program named one, and otherwise a fresh array of the receiver's own kind.
///
/// Through [`super::typed_species`] rather than a species read of its own —
/// `slice` decides the same question, and two readings of `constructor` and
/// `@@species` is where the two methods come to disagree about what a subclass
/// answers.
fn produced(this: u64, kind: Kind, values: &[u64]) -> u64 {
    let made = super::typed_species::derived(this, values.len());
    if throw::in_flight() {
        return super::undefined();
    }
    let Some(made) = made else {
        return built(kind, values);
    };
    if super::typed_species::mismatched(this, made) {
        return super::undefined();
    }
    // The elements go in through the bulk copy, which is the one place that
    // converts between a source and a destination kind.
    let listed = with_current(|context| crate::entry::array::built_in(context, values.to_vec()));
    typed::copy_from(made, listed, super::undefined());
    made
}

/// `callback(element, index, receiver)`, with none of this module's borrows held.
///
/// `None` when the call left a throw behind, which every caller here turns into
/// a return rather than into an answer.
fn visit(callback: u64, receiver: u64, this: u64, element: u64, at: usize) -> Option<u64> {
    let index = Value::from_f64(at as f64).bits();
    let answer = functions::call(callback, receiver, element, index, this, super::undefined());
    match throw::in_flight() {
        true => None,
        false => Some(answer),
    }
}

/// Whether a value is something to call.
fn calls(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.callable_at(cell).is_some())
    })
}

/// `t.forEach(callback, thisArg)`.
pub(in crate::entry) fn for_each(this: u64, callback: u64, receiver: u64) -> u64 {
    let Some(elements) = listed(this) else {
        return super::undefined();
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return super::undefined();
    }
    for (at, element) in elements.into_iter().enumerate() {
        if visit(callback, receiver, this, element, at).is_none() {
            break;
        }
    }
    super::undefined()
}

/// `t.map(callback, thisArg)` — a new array of the same kind.
pub(in crate::entry) fn map(this: u64, callback: u64, receiver: u64) -> u64 {
    let (Some(elements), Some(kind)) = (listed(this), kind_of(this)) else {
        return super::undefined();
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return super::undefined();
    }
    let mut answers = Vec::with_capacity(elements.len());
    for (at, element) in elements.into_iter().enumerate() {
        let Some(answer) = visit(callback, receiver, this, element, at) else {
            return super::undefined();
        };
        answers.push(answer);
    }
    produced(this, kind, &answers)
}

/// `t.filter(callback, thisArg)` — a new array of the same kind.
pub(in crate::entry) fn filter(this: u64, callback: u64, receiver: u64) -> u64 {
    let (Some(elements), Some(kind)) = (listed(this), kind_of(this)) else {
        return super::undefined();
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return super::undefined();
    }
    let mut kept = Vec::new();
    for (at, element) in elements.into_iter().enumerate() {
        let Some(answer) = visit(callback, receiver, this, element, at) else {
            return super::undefined();
        };
        if crate::entry::primitives::to_boolean(answer) {
            kept.push(element);
        }
    }
    produced(this, kind, &kept)
}

/// Which end a search walks from, and what it answers when it finds one.
///
/// One function for the four `find*` members because they differ in exactly two
/// bits and nothing else. Written four times, the one that ends up walking
/// forwards under a `Last` name is invisible in review and is a wrong answer
/// only for an array with two matches.
fn found(this: u64, callback: u64, receiver: u64, backwards: bool, want_index: bool) -> u64 {
    let Some(elements) = listed(this) else {
        return super::undefined();
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return super::undefined();
    }
    let count = elements.len();
    let order: Vec<usize> = match backwards {
        true => (0..count).rev().collect(),
        false => (0..count).collect(),
    };
    for at in order {
        let Some(answer) = visit(callback, receiver, this, elements[at], at) else {
            return super::undefined();
        };
        if crate::entry::primitives::to_boolean(answer) {
            return match want_index {
                true => Value::from_f64(at as f64).bits(),
                false => elements[at],
            };
        }
    }
    match want_index {
        true => Value::from_f64(-1.0).bits(),
        false => super::undefined(),
    }
}

/// `t.find(callback, thisArg)`.
pub(in crate::entry) fn find(this: u64, callback: u64, receiver: u64) -> u64 {
    found(this, callback, receiver, false, false)
}

/// `t.findIndex(callback, thisArg)`.
pub(in crate::entry) fn find_index(this: u64, callback: u64, receiver: u64) -> u64 {
    found(this, callback, receiver, false, true)
}

/// `t.findLast(callback, thisArg)`.
pub(in crate::entry) fn find_last(this: u64, callback: u64, receiver: u64) -> u64 {
    found(this, callback, receiver, true, false)
}

/// `t.findLastIndex(callback, thisArg)`.
pub(in crate::entry) fn find_last_index(this: u64, callback: u64, receiver: u64) -> u64 {
    found(this, callback, receiver, true, true)
}

/// `t.some(callback, thisArg)` and `t.every(callback, thisArg)`.
///
/// One function because they are the same walk under opposite polarity: `some`
/// stops at the first truthy answer and `every` at the first falsy one.
fn quantified(this: u64, callback: u64, receiver: u64, wanted: bool) -> bool {
    let Some(elements) = listed(this) else {
        return !wanted;
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return !wanted;
    }
    for (at, element) in elements.into_iter().enumerate() {
        let Some(answer) = visit(callback, receiver, this, element, at) else {
            return !wanted;
        };
        if crate::entry::primitives::to_boolean(answer) == wanted {
            return wanted;
        }
    }
    !wanted
}

/// `t.some(callback, thisArg)`.
pub(in crate::entry) fn some(this: u64, callback: u64, receiver: u64) -> bool {
    quantified(this, callback, receiver, true)
}

/// `t.every(callback, thisArg)`.
pub(in crate::entry) fn every(this: u64, callback: u64, receiver: u64) -> bool {
    quantified(this, callback, receiver, false)
}

/// `t.reduce(callback, initial)` and `t.reduceRight(callback, initial)`.
///
/// # The one place this diverges, said out loud
///
/// An absent initial value and an explicit `undefined` are the same bits by the
/// time a native sees them, so `t.reduce(f, undefined)` folds from the first
/// element instead of starting with `undefined`. Distinguishing them needs an
/// argument count the native calling convention does not carry, which is a
/// machine question and not this module's.
fn folded(this: u64, callback: u64, initial: u64, backwards: bool) -> u64 {
    let Some(elements) = listed(this) else {
        return super::undefined();
    };
    if !calls(callback) {
        throw::type_error("callback is not a function");
        return super::undefined();
    }
    let count = elements.len();
    let order: Vec<usize> = match backwards {
        true => (0..count).rev().collect(),
        false => (0..count).collect(),
    };
    let absent = super::undefined();
    let mut order = order.into_iter();
    let mut carried = match Value(initial).bits() == absent {
        false => initial,
        true => match order.next() {
            Some(at) => elements[at],
            // No initial value and nothing to take one from: a `TypeError` in
            // the language, and one this layer can now raise.
            None => {
                throw::type_error("Reduce of empty array with no initial value");
                return super::undefined();
            }
        },
    };
    for at in order {
        let index = Value::from_f64(at as f64).bits();
        carried = functions::call(callback, absent, carried, elements[at], index, this);
        if throw::in_flight() {
            return super::undefined();
        }
    }
    carried
}

/// `t.reduce(callback, initial)`.
pub(in crate::entry) fn reduce(this: u64, callback: u64, initial: u64) -> u64 {
    folded(this, callback, initial, false)
}

/// `t.reduceRight(callback, initial)`.
pub(in crate::entry) fn reduce_right(this: u64, callback: u64, initial: u64) -> u64 {
    folded(this, callback, initial, true)
}

/// `t.reverse()` — in place, answering the receiver so calls chain.
pub(in crate::entry) fn reverse(this: u64) -> u64 {
    let Some(mut elements) = listed(this) else {
        return super::undefined();
    };
    elements.reverse();
    write_back(this, &elements);
    this
}

/// `t.toReversed()` — a copy, reversed.
pub(in crate::entry) fn to_reversed(this: u64) -> u64 {
    let (Some(mut elements), Some(kind)) = (listed(this), kind_of(this)) else {
        return super::undefined();
    };
    elements.reverse();
    built(kind, &elements)
}

/// `t.with(index, value)` — a copy with one element replaced.
///
/// A negative index counts from the end and one outside the array is a
/// `RangeError`, which is the half that differs from a plain write: `t[99] = 1`
/// on a three-element array is silently dropped, and `t.with(99, 1)` refuses.
pub(in crate::entry) fn with(this: u64, index: f64, value: u64) -> u64 {
    let (Some(mut elements), Some(kind)) = (listed(this), kind_of(this)) else {
        return super::undefined();
    };
    let count = elements.len() as f64;
    let at = match index < 0.0 {
        true => index + count,
        false => index,
    };
    if !(at >= 0.0 && at < count) {
        throw::range_error("Invalid index");
        return super::undefined();
    }
    elements[at as usize] = value;
    built(kind, &elements)
}

/// `t.toSorted(compare)` — a copy, sorted by the same rules `sort` uses.
///
/// Through [`super::typed_order::sort`] on the copy rather than a second sort:
/// the numeric default, the comparator contract and the stable merge are one
/// decision, and a second implementation of it is where a tie starts breaking
/// the other way.
pub(in crate::entry) fn to_sorted(this: u64, comparator: u64) -> u64 {
    let (Some(elements), Some(kind)) = (listed(this), kind_of(this)) else {
        return super::undefined();
    };
    let copy = built(kind, &elements);
    super::typed_order::sort(copy, comparator)
}

/// `t.toString()` — the elements, comma separated, which is `join()`.
pub(in crate::entry) fn to_string(this: u64) -> u64 {
    typed::join(this, super::undefined())
}

/// Writes element values back into the receiver's own window.
fn write_back(this: u64, elements: &[u64]) {
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return;
        };
        let kind = view.kind;
        let size = kind.size();
        let words: Vec<Option<u64>> = elements
            .iter()
            .map(|value| super::element_word(context, *value, kind))
            .collect();
        if let Some(window) = super::window_mut(context, &view) {
            for (at, word) in words.iter().enumerate() {
                if let Some(word) = word {
                    super::element::write_word(window, at * size, kind, *word, true);
                }
            }
        }
    });
}

/// `T.from(source, mapFn, thisArg)`.
///
/// Over [`crate::entry::array_proto::more::from`] rather than a walk of its own.
/// What the two share is the whole hard half — which values the iteration
/// protocol answers, the array-like fallback, and closing an iterator whose
/// mapper threw — and this adds only the narrowing into elements of `kind`. A
/// second walk here would be a second answer to "what is iterable", which is
/// what the array module's own first page refuses.
pub(in crate::entry) fn from(kind: Kind, source: u64, mapper: u64, receiver: u64) -> u64 {
    let absent = super::undefined();
    let listed = crate::entry::array_proto::more::from::from(0, absent, source, mapper, receiver, absent);
    if throw::in_flight() {
        return absent;
    }
    let values = with_current(|context| {
        Value(listed)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).map(|held| held.to_vec()))
            .unwrap_or_default()
    });
    built(kind, &values)
}
