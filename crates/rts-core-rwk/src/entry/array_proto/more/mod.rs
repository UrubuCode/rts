//! The rest of what an array inherits, and `Array.from`.
//!
//! # Why this mixes the two shapes the folder was split to keep apart
//!
//! [`super`] reads and answers inside one borrow; [`super::iterate`] lets the
//! borrow go because it calls user code. Every method here belongs to one of
//! those two, and they are together because the pairs that must agree straddle
//! the line: `flat` and `flatMap` are one flattening with a call in front of it,
//! and `findLast` and `lastIndexOf` are one search over the same reversed order.
//! Splitting by shape would put each pair in two files, which is where one of
//! them learns something the other does not.
//!
//! The rule the split exists to protect is therefore restated rather than
//! relied on: **nothing here calls user code from inside `with_current`.** Each
//! method that calls collects under a borrow, drops it, calls, and re-borrows.
//!
//! [`sorting`] is a file of its own for a different reason — not its shape, but
//! that ordering with a comparator is the one operation here whose correctness
//! argument is longer than its code.
//!
//! # Why `keys`, `values` and `entries` answer arrays
//!
//! The language answers an Array Iterator — an object with `next`. There are no
//! iterator objects in this engine on purpose: [`super::super::iterate`] records
//! why `for-of` materialises instead, and an iterator built here would be walked
//! by that function to exhaustion anyway.
//!
//! So these answer arrays, which `for-of` and `...` both accept. The divergence,
//! named: `a.values().next` is not a function, and `Array.isArray(a.keys())` is
//! true where the language says false. Both are visible failures rather than
//! wrong values, and both disappear when the emitter grows a lazy cursor.

mod sorting;

use super::super::native::Native;
use super::super::objects::undefined_of;
use super::super::string::{absent, relative};
use super::super::{Context, functions, with_current};
use super::{built, staged, store};
use crate::value::Value;

/// What an array's prototype holds beyond the eleven in [`super`] and the eight
/// in [`super::iterate`].
pub(in crate::entry) const NATIVES: &[(&str, Native)] = &[
    ("at", at),
    ("lastIndexOf", last_index_of),
    ("toString", to_string_),
    ("keys", keys),
    ("values", values),
    ("entries", entries),
    ("flat", flat),
    ("flatMap", flat_map),
    ("splice", splice),
    ("sort", sorting::sort),
    ("toSorted", sorting::to_sorted),
    ("toReversed", to_reversed),
    ("with", with),
    ("copyWithin", copy_within),
    ("reduceRight", reduce_right),
    ("findLast", find_last),
    ("findLastIndex", find_last_index),
];

/// What `Array` itself holds beyond `isArray` and `of`.
pub(in crate::entry) const STATICS: &[(&str, Native)] = &[("from", from)];

/// `a.at(i)` — negative counts from the end.
extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((_, elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let asked = Value(index).numeric().unwrap_or(0.0);
        let at = if asked < 0.0 {
            elements.len() as f64 + asked
        } else {
            asked
        };
        // Out of range is `undefined` rather than clamped, which is the whole
        // reason `at` was added beside indexing: `a.at(-1)` must be the last
        // element and `a.at(-99)` must be nothing, not the first.
        if at < 0.0 || at >= elements.len() as f64 {
            return undefined_of(context);
        }
        elements[at as usize]
    })
}

/// `a.lastIndexOf(x)` — strict equality, from the end.
///
/// Strict, like `indexOf` and unlike `includes`: `[NaN].lastIndexOf(NaN)` is -1.
extern "C" fn last_index_of(_e: u64, this: u64, search: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((_, elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let at = elements.iter().rposition(|held| {
            crate::value::strict_equals(Value(*held), Value(search), |a, b| context.same_text(a, b))
        });
        Value::from_f64(at.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `a.toString()` — `join` with a comma, which is what the language defines it
/// as.
///
/// Delegated rather than reimplemented. The two must agree that `null` joins as
/// the empty string, and a second implementation is where one of them would
/// produce `"1,null,2"`.
extern "C" fn to_string_(e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let missing = nothing();
    super::join(e, this, missing, missing, missing, missing)
}

/// `a.keys()` — the indices.
extern "C" fn keys(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = snapshot(this) else {
        return nothing();
    };
    built(
        (0..elements.len())
            .map(|index| Value::from_f64(index as f64).bits())
            .collect(),
    )
}

/// `a.values()` — the elements, in a fresh array.
///
/// A copy, because a loop over it must not see what its own body pushes to the
/// receiver — the same reason [`super::super::iterate::iterate`] copies an array
/// it is handed.
extern "C" fn values(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match snapshot(this) {
        Some(elements) => built(elements),
        None => nothing(),
    }
}

/// `a.entries()` — index and element, paired.
extern "C" fn entries(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = snapshot(this) else {
        return nothing();
    };
    // Each pair is its own allocation, and every one is made outside a borrow —
    // `built` reaches `array_new`, which takes the context.
    let pairs: Vec<u64> = elements
        .iter()
        .enumerate()
        .map(|(index, element)| built(vec![Value::from_f64(index as f64).bits(), *element]))
        .collect();
    built(pairs)
}

/// `a.flat(depth)` — one level by default.
///
/// One and not all the way down, which is the language: `[[[1]]].flat()` is
/// `[[1]]`. `Infinity` is how a program asks for all of it, and the `as i32`
/// cast saturates rather than wrapping — so infinity becomes the largest depth
/// rather than a negative one that flattens nothing.
extern "C" fn flat(_e: u64, this: u64, depth: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let flattened = with_current(|context| {
        let (_, elements) = staged(context, this)?;
        let asked = if absent(context, depth) {
            1
        } else {
            Value(depth).numeric().unwrap_or(0.0) as i32
        };
        let mut out = Vec::new();
        flattened_into(context, &elements, asked, &mut out);
        Some(out)
    });
    match flattened {
        Some(out) => built(out),
        None => nothing(),
    }
}

/// `a.flatMap(f)` — map, then flatten exactly one level.
///
/// One level always, with no depth argument, and that is the language rather
/// than an omission: the method exists so a callback can answer zero or several
/// elements, and a deeper flatten would also take apart an array the callback
/// deliberately produced as one element.
extern "C" fn flat_map(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = snapshot(this) else {
        return nothing();
    };
    let produced: Vec<u64> = elements
        .iter()
        .enumerate()
        .map(|(index, element)| call_with(callback, this, *element, index))
        .collect();
    // Flattened in a borrow taken after the last call, never between two.
    let out = with_current(|context| {
        let mut out = Vec::new();
        flattened_into(context, &produced, 1, &mut out);
        out
    });
    built(out)
}

/// `a.splice(start, count, x, y)` — removes, inserts, answers what it removed.
///
/// Two insertions, because the four argument slots are spent on the two controls
/// and what is left. The rest are refused at the call rather than dropped here —
/// see `super::super::functions::ARGUMENT_SLOTS`.
///
/// An absent count removes to the end, which is not the same as a count of zero:
/// `a.splice(1)` empties the tail and `a.splice(1, 0, x)` is a pure insertion.
/// A version defaulting the count to zero looks harmless and silently turns
/// every truncation in a program into nothing at all.
extern "C" fn splice(_e: u64, this: u64, start: u64, count: u64, x: u64, y: u64) -> u64 {
    let removed = with_current(|context| {
        let (cell, mut elements) = staged(context, this)?;
        let from = relative(Value(start).numeric().unwrap_or(0.0), elements.len());
        let left = (elements.len() - from) as f64;
        let taken = if absent(context, count) {
            left as usize
        } else {
            Value(count).numeric().unwrap_or(0.0).clamp(0.0, left) as usize
        };
        let removed: Vec<u64> = elements.drain(from..from + taken).collect();
        let inserted: Vec<u64> = [x, y]
            .into_iter()
            .filter(|given| !absent(context, *given))
            .collect();
        elements.splice(from..from, inserted);
        store(context, cell, elements);
        Some(removed)
    });
    match removed {
        Some(removed) => built(removed),
        None => nothing(),
    }
}

/// `a.toReversed()` — a new array, where `reverse` mutates.
extern "C" fn to_reversed(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match snapshot(this) {
        Some(mut elements) => {
            elements.reverse();
            built(elements)
        }
        None => nothing(),
    }
}

/// `a.with(i, v)` — a copy with one element replaced.
///
/// Out of range answers `undefined`, where the language throws a `RangeError` —
/// the stated gap every operation here has while a throw cannot find a handler.
/// Answering the unchanged copy was rejected: that is a wrong program that
/// keeps running, where this one fails at the next use of the result.
extern "C" fn with(_e: u64, this: u64, index: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(mut elements) = snapshot(this) else {
        return nothing();
    };
    let asked = Value(index).numeric().unwrap_or(f64::NAN);
    let at = if asked < 0.0 {
        elements.len() as f64 + asked
    } else {
        asked
    };
    if at.is_nan() || at < 0.0 || at >= elements.len() as f64 {
        return nothing();
    }
    elements[at as usize] = value;
    built(elements)
}

/// `a.copyWithin(target, from, to)` — in place, answering the receiver.
///
/// The copy reads out of the snapshot, so overlapping ranges see the elements as
/// they were. That is what the specification requires and the corner an in-place
/// loop gets wrong when the target lands inside the source.
extern "C" fn copy_within(_e: u64, this: u64, target: u64, from: u64, to: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((cell, mut elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let count = elements.len();
        let at = relative(Value(target).numeric().unwrap_or(0.0), count);
        let start = relative(Value(from).numeric().unwrap_or(0.0), count);
        let end = if absent(context, to) {
            count
        } else {
            relative(Value(to).numeric().unwrap_or(0.0), count)
        };
        let source: Vec<u64> = elements[start..end.max(start)].to_vec();
        for (offset, held) in source.into_iter().enumerate() {
            if at + offset >= count {
                break;
            }
            elements[at + offset] = held;
        }
        store(context, cell, elements);
        this
    })
}

/// `a.reduceRight(f, initial)` — `reduce` from the other end.
///
/// Its own loop rather than `reduce` over a reversed copy: the index handed to
/// the callback must be the element's real position, and reversing would hand it
/// the mirror of that. A program joining path segments with it would build them
/// with the right pieces at the wrong offsets, while every test of a sum still
/// passed.
extern "C" fn reduce_right(
    _e: u64,
    this: u64,
    callback: u64,
    initial: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(elements) = snapshot(this) else {
        return nothing();
    };
    let seeded = !with_current(|context| initial == undefined_of(context));
    let (mut carried, skip) = if seeded {
        (initial, 0)
    } else {
        // The last element seeds it, not `undefined` — the same reason
        // `super::iterate::reduce` seeds with the first.
        match elements.last() {
            Some(last) => (*last, 1),
            None => return nothing(),
        }
    };
    let receiver = nothing();
    for (index, element) in elements.iter().enumerate().rev().skip(skip) {
        carried = functions::call(
            callback,
            receiver,
            carried,
            *element,
            Value::from_f64(index as f64).bits(),
            this,
        );
    }
    carried
}

/// `a.findLast(f)`.
extern "C" fn find_last(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match sought_last(this, callback) {
        Some((element, _)) => element,
        None => nothing(),
    }
}

/// `a.findLastIndex(f)` — where that element was, or -1.
///
/// Shares the scan with [`find_last`], for the reason `super::iterate` shares
/// one between `find` and `findIndex`: a callback with a side effect makes "run
/// it twice and compare" a different program.
extern "C" fn find_last_index(
    _e: u64,
    this: u64,
    callback: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let at = sought_last(this, callback).map_or(-1.0, |(_, index)| index as f64);
    Value::from_f64(at).bits()
}

/// `Array.from(items, f)`.
///
/// Through the runtime's own iteration, so anything `for-of` walks this accepts
/// — including a string, which becomes one element per code point rather than
/// per unit.
///
/// # Why the array-like fallback is here and not in `iterate`
///
/// `Array.from({length: 2})` is `[undefined, undefined]` and
/// `for (const x of {length: 2})` is a `TypeError`: the language reads indices
/// off a `length` **only for this function**. Putting the fallback in `iterate`
/// would make `for-of` accept what the language refuses, which is a wrong
/// program that runs. So `iterate` still decides what is *iterable*, and this
/// decides what it additionally accepts when nothing was iterated.
extern "C" fn from(_e: u64, _this: u64, items: u64, mapper: u64, _a2: u64, _a3: u64) -> u64 {
    // Outside every borrow: `iterate` is an entry point, and it may run a
    // user-defined `Symbol.iterator` to exhaustion before it answers.
    let produced = match array_like(items) {
        Some(values) => built(values),
        None => super::super::iterate::iterate(items),
    };
    if !calls(mapper) {
        // Already a fresh array — `iterate` copies — so there is nothing to
        // build a second time.
        return produced;
    }
    let Some(elements) = snapshot(produced) else {
        return nothing();
    };
    let mapped: Vec<u64> = elements
        .iter()
        .enumerate()
        .map(|(index, element)| call_with(mapper, produced, *element, index))
        .collect();
    built(mapped)
}

/// One level of flattening, repeated to a depth.
///
/// The elements are cloned at each level rather than borrowed through, because
/// `elements_at` borrows the context and the recursion would need one live
/// borrow per level of nesting.
fn flattened_into(context: &Context, values: &[u64], depth: i32, out: &mut Vec<u64>) {
    for value in values {
        let nested = if depth > 0 {
            Value(*value)
                .as_slot()
                .and_then(|cell| context.elements_at(cell))
                .cloned()
        } else {
            None
        };
        match nested {
            Some(inner) => flattened_into(context, &inner, depth - 1, out),
            None => out.push(*value),
        }
    }
}

/// The last element a predicate accepted, and where it was.
fn sought_last(this: u64, callback: u64) -> Option<(u64, usize)> {
    let elements = snapshot(this)?;
    for (index, element) in elements.iter().enumerate().rev() {
        if super::super::primitives::to_boolean(call_with(callback, this, *element, index)) {
            return Some((*element, index));
        }
    }
    None
}

/// The indices of an array-like, if that is what this is and nothing else.
///
/// `None` for everything iteration already answers — an array, a string, a
/// collection, or an object declaring `Symbol.iterator` — so the fallback can
/// never shadow the real protocol. A `length` that is absent, negative or not a
/// number is `None` too, which is what keeps `Array.from({})` an empty array
/// rather than a guess.
///
/// A data read throughout: a `length` getter is not run, the same boundary
/// [`super::super::iterate`] draws for `next` and `done`.
fn array_like(items: u64) -> Option<Vec<u64>> {
    with_current(|context| {
        let cell = Value(items).as_slot()?;
        if context.elements_at(cell).is_some()
            || context.text_at(cell).is_some()
            || super::super::collections::iterated(context, cell).is_some()
        {
            return None;
        }
        let iterator = format!("{}iterator", super::super::symbol::PREFIX);
        if read(context, cell, &iterator).is_some() {
            return None;
        }
        let count = Value(read(context, cell, "length")?).numeric()?;
        if !(count >= 0.0) {
            return None;
        }
        let absent = undefined_of(context);
        Some(
            (0..count as usize)
                .map(|at| read(context, cell, &at.to_string()).unwrap_or(absent))
                .collect(),
        )
    })
}

/// One property, by name, as a value — `None` where the language reads
/// `undefined`.
fn read(context: &mut Context, cell: u32, name: &str) -> Option<u64> {
    let key = context.well_known(name);
    super::super::objects::read_property(context, cell, key).map(|found| found.bits())
}

/// A snapshot of the receiver's elements, the borrow ending here.
///
/// [`sorting`] uses it too, which is why it is here rather than beside its first
/// caller: a second copy is a second place the two-stage shape could be got
/// wrong.
fn snapshot(this: u64) -> Option<Vec<u64>> {
    with_current(|context| staged(context, this).map(|(_, elements)| elements))
}

/// One call of a callback, outside every borrow.
///
/// The three arguments the specification passes — element, index, array — with
/// `undefined` as the receiver, for the reason `super::iterate` states.
fn call_with(callback: u64, array: u64, element: u64, index: usize) -> u64 {
    let (receiver, at) = with_current(|context| {
        (
            undefined_of(context),
            Value::from_f64(index as f64).bits(),
        )
    });
    functions::call(callback, receiver, element, at, array, receiver)
}

/// Whether a value can be called at all.
fn calls(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.callable_at(cell).is_some())
    })
}

/// The `undefined` a method answers when there is nothing to answer.
fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}
