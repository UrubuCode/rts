//! The rest of what an array inherits.
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
//! argument is longer than its code. [`from`] is one because it is the only
//! method that reads something which is not an array at all.
//!
//! # What `keys`, `values` and `entries` answer
//!
//! A LIVE cursor — [`super::cursor`] is the object — which holds the receiver
//! and an index and reads both `length` and the element at every step.
//!
//! They answered an iterator over a list built here, and that list is gone
//! rather than merely deferred: it was wrong in both directions, since
//! `for (const [i, v] of a.entries())` did not see an element the body pushed
//! and did not stop early when the body shortened the receiver. It was also the
//! reason none of the three worked on a receiver that is an array-LIKE, which is
//! what the language defines all three over.

pub(in crate::entry) mod from;
mod from_async;
mod natural_run;
mod sorting;
mod splice;

use super::super::native::Native;
use super::super::objects::undefined_of;
use super::super::string::absent;
use super::super::{Context, functions, with_current};
use super::{built, cursor, iterate, staged};
use crate::value::Value;

/// What an array's prototype holds beyond the eleven in [`super`] and the eight
/// in [`super::iterate`].
pub(in crate::entry) const NATIVES: &[(&str, Native)] = &[
    ("at", at),
    ("lastIndexOf", last_index_of),
    ("toString", to_string_),
    ("toLocaleString", to_locale_string),
    ("keys", keys),
    ("values", values),
    ("entries", entries),
    ("flat", flat),
    ("flatMap", flat_map),
    ("splice", splice::splice),
    ("toSpliced", splice::to_spliced),
    ("sort", sorting::sort),
    ("toSorted", sorting::to_sorted),
    ("toReversed", to_reversed),
    ("with", splice::with),
    ("copyWithin", splice::copy_within),
    ("reduceRight", reduce_right),
    ("findLast", find_last),
    ("findLastIndex", find_last_index),
];

/// What `Array` itself holds beyond `isArray` and `of`.
pub(in crate::entry) const STATICS: &[(&str, Native)] =
    &[("from", from::from), ("fromAsync", from_async::from_async)];

/// `a.at(i)` — negative counts from the end.
///
/// The index is converted BEFORE the borrow, and that is the whole reason this
/// is two statements: `ToIntegerOrInfinity` reaches a `valueOf` the program
/// wrote, and calling one inside `with_current` re-enters the `RefCell`. See
/// [`super::numeric`] for the three answers the conversion changes.
extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let asked = super::numeric::integer_or_infinity(index);
    with_current(|context| {
        // Generic, the same fallback `slice` takes and for the same reason: the
        // specification defines `at` over `LengthOfArrayLike(ToObject(this))`,
        // so `Array.prototype.at.call({ length: 3, 2: "c" }, -1)` is `"c"` and
        // answered `undefined` here for every receiver that was not a real
        // array.
        let elements = match staged(context, this) {
            Some((_, elements)) => elements,
            None => match super::array_like(context, this) {
                Some(elements) => elements,
                None => return undefined_of(context),
            },
        };
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
        // Ponto de saída direta, como `pop`/`shift`: `[,1].at(0)` é `undefined`.
        super::super::array::visible(context, elements[at as usize])
    })
}

/// `a.lastIndexOf(x, from)` — strict equality, from the end.
///
/// Strict, like `indexOf` and unlike `includes`: `[NaN].lastIndexOf(NaN)` is -1.
///
/// Not `super::forward_from`, because `relative` clamps a negative index to zero
/// — right for a forward search, where `indexOf(x, -99)` scans everything, and
/// wrong here: `lastIndexOf(x, -99)` searches NOTHING, and clamping would make it
/// find an element at position 0 the caller asked it to look past.
extern "C" fn last_index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((_, elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let count = elements.len();
        let end = if absent(context, from) {
            count
        } else {
            let asked = Value(from).numeric().unwrap_or(0.0).trunc();
            let at = match asked < 0.0 {
                true => count as f64 + asked,
                // Past the end searches the whole array rather than nothing,
                // which is the mirror of the negative case being empty.
                false => asked.min(count as f64 - 1.0),
            };
            if at < 0.0 {
                return Value::from_f64(-1.0).bits();
            }
            // Inclusive: `lastIndexOf(x, 2)` may answer 2.
            at as usize + 1
        };
        let at = elements[..end].iter().rposition(|held| {
            crate::value::strict_equals(Value(*held), Value(search), |a, b| context.same_text(a, b))
        });
        Value::from_f64(at.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `a.toString()` — `join` with a comma, which is what the language defines it
/// as.
///
/// Delegated rather than reimplemented — and delegated through the PROPERTY,
/// not to this file's `join`.
///
/// The specification is explicit: `Get(array, "join")`, and if what comes back
/// is not callable, `%Object.prototype.toString%` instead. Calling
/// `joining::join` directly ignored both halves, so an object with its own
/// `join` answered `1,2,3` where every other engine answers what the program
/// installed, and `Array.prototype.toString.call({ })` answered the empty
/// string where the language answers `[object Object]`.
extern "C" fn to_string_(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let missing = nothing();
    // Through the same accessor-running read the species protocol uses: `join`
    // may be a getter, and it may be inherited.
    let joiner = super::species::property(this, "join").unwrap_or(missing);
    if super::super::throw::in_flight() {
        return missing;
    }
    if super::super::iterate::callable(joiner) {
        return functions::call(joiner, this, missing, missing, missing, missing);
    }
    // `%Object.prototype.toString%`, read off the object every plain object
    // inherits from rather than reimplemented: the `[object X]` rule lives
    // there, tag included, and a second spelling of it here is a second answer
    // to what an object calls itself.
    let fallback = with_current(|context| {
        let prototype = super::super::class_support::prototype(context, "Object.prototype")?;
        let cell = Value(prototype).as_slot()?;
        let key = context.well_known("toString");
        super::super::objects::read_property(context, cell, key).map(|found| found.bits())
    });
    match fallback {
        Some(found) => functions::call(found, this, missing, missing, missing, missing),
        None => missing,
    }
}

/// `a.toLocaleString()` — each element's own `toLocaleString`, comma-joined.
///
/// # Why this is not [`to_string_`] with another name
///
/// Because it calls a DIFFERENT method on every element. It was absent
/// altogether, so the lookup walked past `Array.prototype` to
/// `Object.prototype.toLocaleString`, which delegates to `this.toString()` —
/// which is the array's `join`, which stringifies each element with its
/// `toString`. The result for an array of objects declaring a `toLocaleString`
/// was `[object Object],[object Object]`: every element's own method skipped,
/// by a chain of three correct delegations to the wrong thing.
///
/// `null` and `undefined` join as the empty string, exactly as in `join`, and
/// that is the one rule the two share. A hole reads as `undefined` and joins
/// the same way.
///
/// The separator is a plain comma. The specification says it is
/// implementation-defined and locale-dependent; every engine uses a comma, and
/// inventing a locale-aware one here would make this the only place in the crate
/// that guesses at a locale — `crates/rts-core/src/entry/intl` is where that
/// question is answered, and it answers it with real CLDR data or not at all.
extern "C" fn to_locale_string(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let staged = with_current(|context| {
        let cell = Value(this).as_slot()?;
        let elements: Vec<u64> = context
            .elements_at(cell)?
            .clone()
            .iter()
            .map(|held| super::super::array::visible(context, *held))
            .collect();
        let key = context.well_known("toLocaleString");
        Some((elements, key, undefined_of(context)))
    });
    let Some((elements, key, absent)) = staged else {
        return with_current(|context| undefined_of(context));
    };
    let null = with_current(|context| Value::from_singleton(context.singletons.null).bits());

    // Assembled as UTF-16 UNITS, for the reason `joining::join` records: a lone
    // surrogate is not well-formed text, `Str::to_rust` refuses it, and going
    // through Rust strings would silently drop it.
    let mut parts: Vec<Vec<u16>> = Vec::with_capacity(elements.len());
    for element in elements {
        if element == absent || element == null {
            parts.push(Vec::new());
            continue;
        }
        // The element's own method, read through the chain so an INHERITED one
        // participates — which is the whole point, since the method is almost
        // always on a prototype.
        let method = with_current(|context| {
            Value(element)
                .as_slot()
                .and_then(|cell| super::super::objects::read_property(context, cell, key))
                .map_or(absent, |found| found.bits())
        });
        let answered = match method == absent {
            // No method at all: fall back to the element's text form rather
            // than skipping it, so the shape of the output does not depend on
            // which elements happened to declare one.
            true => super::super::primitive::to_primitive(element, crate::coerce::Hint::String),
            false => functions::call(method, element, absent, absent, absent, absent),
        };
        // Rule 8: a callee that threw did not answer, and `undefined` is a
        // VALUE — carrying on would join the word "undefined" into the result of
        // a call that never happened.
        if super::super::throw::in_flight() {
            return with_current(|context| undefined_of(context));
        }
        parts.push(
            with_current(|context| super::super::text::to_text(context, Value(answered)))
                .map(|text| text.units().collect())
                .unwrap_or_default(),
        );
    }

    let comma = u16::from(b',');
    let mut joined: Vec<u16> = Vec::new();
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            joined.push(comma);
        }
        joined.extend_from_slice(part);
    }
    with_current(|context| context.intern_value(crate::text::Str::from_utf16(&joined)).bits())
}

/// `a.keys()` — the indices.
extern "C" fn keys(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cursor::over(this, cursor::Kind::Keys)
}

/// `a.values()` — the elements, one at a time.
///
/// A LIVE cursor and not a copy: this used to answer an iterator over an array
/// built here, so a loop did not see what its own body pushed and did not stop
/// early when the body shortened the receiver. [`super::cursor`] states what the
/// copy got wrong in both directions.
pub(in crate::entry) extern "C" fn values(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cursor::over(this, cursor::Kind::Values)
}

/// `a.entries()` — index and element, paired.
///
/// Each pair is built when the position is REACHED rather than up front, which
/// is what makes `a.entries()` on a large array cost one object instead of one
/// per element — and what lets the pair carry an element written after the
/// iterator was made.
extern "C" fn entries(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cursor::over(this, cursor::Kind::Entries)
}

/// `a.flat(depth)` — one level by default.
///
/// One and not all the way down, which is the language: `[[[1]]].flat()` is
/// `[[1]]`. `Infinity` is how a program asks for all of it, and the `as i32`
/// cast saturates rather than wrapping — so infinity becomes the largest depth
/// rather than a negative one that flattens nothing.
extern "C" fn flat(_e: u64, this: u64, depth: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // Whether the argument was given at all is decided first, because an absent
    // depth is ONE and `ToIntegerOrInfinity(undefined)` is zero — the difference
    // between `[[1]].flat()` answering `[1]` and answering `[[1]]`. The
    // conversion itself is outside every borrow, for the reason `at` states.
    let given = with_current(|context| !absent(context, depth));
    let asked = match given {
        true => super::numeric::integer_or_infinity(depth),
        false => 1.0,
    };
    let flattened = with_current(|context| {
        let (_, elements) = staged(context, this)?;
        let mut out = Vec::new();
        flattened_into(context, &elements, asked as i32, &mut out);
        Some(out)
    });
    match flattened {
        Some(out) => super::species::collected(this, out),
        None => nothing(),
    }
}

/// `a.flatMap(f, thisArg)` — map, then flatten exactly one level.
///
/// One level always, with no depth argument, and that is the language rather
/// than an omission: the method exists so a callback can answer zero or several
/// elements, and a deeper flatten would also take apart an array the callback
/// deliberately produced as one element.
extern "C" fn flat_map(
    _e: u64,
    this: u64,
    callback: u64,
    receiver: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(count) = iterate::len_of(this) else {
        return nothing();
    };
    // ROOTED and a loop rather than a `collect`, because the callback allocates
    // and an allocation collects — see `entry::rooted` for the measurement.
    let mut produced = super::super::rooted::Rooted::new();
    for index in 0..count {
        // An absent position is not visited, the same as `map`'s — but it
        // contributes NOTHING here rather than a hole, because the flatten that
        // follows drops holes anyway and a hole pushed now would only be dropped
        // one line later.
        let Some(element) = iterate::existing(this, index) else {
            continue;
        };
        // Rule 8: a callback that threw answers `undefined`, and flattening that
        // would append one wrong element per remaining position.
        let Some(answered) = iterate::visit(callback, receiver, this, element, index) else {
            break;
        };
        produced.values().push(answered);
    }
    let produced = produced.take();
    // Flattened in a borrow taken after the last call, never between two.
    let out = with_current(|context| {
        let mut out = Vec::new();
        flattened_into(context, &produced, 1, &mut out);
        out
    });
    built(out)
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
    let Some(count) = iterate::len_of(this) else {
        return nothing();
    };
    let seeded = !with_current(|context| initial == undefined_of(context));
    // `below` is where the fold still has to go, exclusive: everything when a
    // seed was given, and everything under the seed's own position when it was
    // taken from the array.
    let (mut carried, below) = if seeded {
        (initial, count)
    } else {
        // The last element that EXISTS seeds it, not `undefined` and not a
        // hole — the same reason `super::iterate::reduce` seeds with the first,
        // and the empty case is the same `TypeError` for the same reason.
        match (0..count)
            .rev()
            .find_map(|index| iterate::existing(this, index).map(|held| (held, index)))
        {
            Some(found) => found,
            None => {
                super::super::throw::type_error("Reduce of empty array with no initial value");
                return nothing();
            }
        }
    };
    let receiver = nothing();
    for index in (0..below).rev() {
        let Some(element) = iterate::existing(this, index) else {
            continue;
        };
        carried = functions::call(
            callback,
            receiver,
            carried,
            element,
            Value::from_f64(index as f64).bits(),
            this,
        );
        // Stops on a throw, for the reason `reduce` states: the next pass would
        // fold `undefined` into the accumulator.
        if super::super::throw::in_flight() {
            break;
        }
    }
    carried
}

/// `a.findLast(f, thisArg)`.
extern "C" fn find_last(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    match sought_last(this, callback, receiver) {
        Some((element, _)) => element,
        None => nothing(),
    }
}

/// `a.findLastIndex(f, thisArg)` — where that element was, or -1.
///
/// Shares the scan with [`find_last`], for the reason `super::iterate` shares
/// one between `find` and `findIndex`: a callback with a side effect makes "run
/// it twice and compare" a different program.
extern "C" fn find_last_index(
    _e: u64,
    this: u64,
    callback: u64,
    receiver: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let at = sought_last(this, callback, receiver).map_or(-1.0, |(_, index)| index as f64);
    Value::from_f64(at).bits()
}

/// One level of flattening, repeated to a depth.
///
/// The elements are cloned at each level rather than borrowed through, because
/// `elements_at` borrows the context and the recursion would need one live
/// borrow per level of nesting.
fn flattened_into(context: &Context, values: &[u64], depth: i32, out: &mut Vec<u64>) {
    for value in values {
        // `flat` SALTA buracos — o resultado é denso, e `[1,,3].flat()` é
        // `[1,3]`. Empilhá-los dava o comprimento errado E entregava o word do
        // buraco a quem lesse o resultado.
        if crate::entry::array::is_hole(context, *value) {
            continue;
        }
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
///
/// Visits an absent position with `undefined` rather than skipping it, which is
/// the contract `find`/`findIndex` have and the opposite of `forEach`'s — the
/// specification defines this pair over the RANGE and that one over the keys.
fn sought_last(this: u64, callback: u64, receiver: u64) -> Option<(u64, usize)> {
    let count = iterate::len_of(this)?;
    for index in (0..count).rev() {
        let seen = iterate::present(this, index);
        // Rule 8: a predicate that threw answers `undefined`, which is falsy —
        // so without this the scan would carry on and report the next element
        // the throw never let it judge.
        let answered = iterate::visit(callback, receiver, this, seen, index)?;
        if super::super::primitives::to_boolean(answered) {
            return Some((seen, index));
        }
    }
    None
}

/// A snapshot of the receiver's elements, the borrow ending here.
///
/// [`sorting`] uses it too, which is why it is here rather than beside its first
/// caller: a second copy is a second place the two-stage shape could be got
/// wrong.
pub(super) fn snapshot(this: u64) -> Option<Vec<u64>> {
    with_current(|context| staged(context, this).map(|(_, elements)| elements))
}

/// The same, with every hole already converted to `undefined`.
///
/// For the operations that HAND an element to the program rather than deciding
/// something about it — the iterators behind `values()` and `entries()`. A
/// hole reaching `.value` is a word that names no JavaScript value at all,
/// which is the leak `array::visible` exists to close; the iterator was three
/// layers past the last place that called it.
pub(super) fn seen(this: u64) -> Option<Vec<u64>> {
    with_current(|context| {
        let (_, elements) = staged(context, this)?;
        Some(
            elements
                .iter()
                .map(|held| crate::entry::array::visible(context, *held))
                .collect(),
        )
    })
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
pub(super) fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}
