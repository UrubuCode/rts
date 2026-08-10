//! The iterator `values()`, `keys()` and `entries()` answer.
//!
//! # Why it wraps a materialised list rather than walking the collection
//!
//! Because the collection is already walked: `Map::keys` and the array
//! prototype's three each build the list they describe, and every one of them
//! is reached by `for`-`of` today through exactly that array. Making them lazy
//! is a different change — it means a cursor into a shape or a table that a
//! mutation during iteration has to keep meaning something — and doing it here
//! would decide that question for six methods at once, inside a change about
//! `.next()`.
//!
//! What was actually missing is the PROTOCOL. `[1, 2].values()` answered an
//! array, so `for (const x of [1,2].values())` worked and `.next()` did not
//! exist at all. This is the object that has one.
//!
//! # What it costs, said rather than hidden
//!
//! The whole list is built before the first `next()`. For `[1, 2].values()`
//! that is nothing; for a million-element array iterated to the third element
//! it is a million-element copy. The lazy form is where that goes, and this is
//! the thing it will replace rather than something it will sit beside.

use super::with_current;
use crate::value::Value;

/// `Iterator` — the ES2025 helper base, as a namespace carrying `from`.
///
/// Not a class with a `prototype` every iterator inherits: this engine's
/// iterators (`ListIterator`, the generator wrapper) each carry their own
/// helper methods rather than reaching one shared prototype, which
/// `ListIterator`'s own doc already states as the shape. `Iterator.from(x)`
/// is the one static real Node/the spec put directly on the constructor, and
/// it is what a program reaches for to turn a plain iterable into something
/// with `.map`/`.take`/`.toArray` on it — exactly what `over` already builds.
#[rtse::class("Iterator", namespace)]
impl Iterator {
    /// `Iterator.from(iterable)` — an iterator over `iterable`'s elements,
    /// with the ES2025 helpers on it.
    ///
    /// Spec `Iterator.from` also accepts an iterator-*like* object (something
    /// with a bare `next()` and no `Symbol.iterator`) and answers it wrapped
    /// rather than copied. That half is not built: [`super::iterate::iterate`]
    /// is this engine's one answer to "what does iterable mean", and it already
    /// materialises everything it walks — an array, a string, a `Map`/`Set`, or
    /// an object declaring `Symbol.iterator` — so `from` reuses exactly that
    /// rather than adding a second notion of "iterable-ish" beside it.
    fn from(iterable: u64) -> u64 {
        let array = super::iterate::iterate(iterable);
        over(array)
    }
}

/// An iterator over a list that has already been built.
#[rtse::class("ListIterator")]
impl ListIterator {
    /// `it.map(f)` — the same elements, each through `f`.
    ///
    /// ES2025's iterator helper. Answers an ITERATOR, which is what makes
    /// `.map(f).toArray()` the way an array comes back out and `.map(f).join()`
    /// a mistake — the same shape Node has.
    fn map(this: u64, callback: u64) -> u64 {
        let mut produced = Vec::new();
        for (index, element) in remaining(this).into_iter().enumerate() {
            produced.push(apply(callback, element, index));
        }
        over(listed(produced))
    }

    /// `it.filter(p)` — the elements `p` answers truthy for.
    fn filter(this: u64, callback: u64) -> u64 {
        let mut kept = Vec::new();
        for (index, element) in remaining(this).into_iter().enumerate() {
            if super::primitives::to_boolean(apply(callback, element, index)) {
                kept.push(element);
            }
        }
        over(listed(kept))
    }

    /// `it.take(n)` — at most the first `n`.
    fn take(this: u64, count: f64) -> u64 {
        let count = at_most(count);
        over(listed(remaining(this).into_iter().take(count).collect()))
    }

    /// `it.drop(n)` — everything after the first `n`.
    fn drop(this: u64, count: f64) -> u64 {
        let count = at_most(count);
        over(listed(remaining(this).into_iter().skip(count).collect()))
    }

    /// `it.flatMap(f)` — `f`'s answers, one level flattened.
    fn flat_map(this: u64, callback: u64) -> u64 {
        let mut produced = Vec::new();
        for (index, element) in remaining(this).into_iter().enumerate() {
            let answered = apply(callback, element, index);
            // Whatever the callback answered, iterated: an array, another
            // iterator, or a string. `iterate` is the one place that decides
            // what "iterable" means, and asking it here is what keeps this from
            // being a second answer to that.
            let inner = super::iterate::iterate(answered);
            let flattened = with_current(|context| {
                Value(inner)
                    .as_slot()
                    .and_then(|cell| context.elements_at(cell))
                    .map(|elements| elements.clone())
            });
            match flattened {
                Some(values) => produced.extend(values),
                None => produced.push(answered),
            }
        }
        over(listed(produced))
    }

    /// `it.toArray()` — the elements, as an array.
    fn to_array(this: u64) -> u64 {
        listed(remaining(this))
    }

    /// `it.forEach(f)` — `f` over each, answering nothing.
    fn for_each(this: u64, callback: u64) -> u64 {
        for (index, element) in remaining(this).into_iter().enumerate() {
            apply(callback, element, index);
        }
        with_current(|context| super::objects::undefined_of(context))
    }

    /// `it.reduce(f, initial)`.
    ///
    /// Without an initial value the first element is one, and an EMPTY iterator
    /// with no initial value answers `undefined` here where the specification
    /// throws a `TypeError` — a native cannot yet raise a catchable error, which
    /// `crates/rts-core/README.md` states as the gap it is.
    fn reduce(this: u64, callback: u64, initial: u64) -> u64 {
        let absent = with_current(|context| super::objects::undefined_of(context));
        let elements = remaining(this);
        let mut walking = elements.into_iter();
        let mut carried = match initial == absent {
            true => match walking.next() {
                Some(first) => first,
                None => return absent,
            },
            false => initial,
        };
        for (index, element) in walking.enumerate() {
            carried = super::functions::call(
                callback,
                absent,
                carried,
                element,
                super::modules::make_number(index as f64),
                absent,
            );
        }
        carried
    }

    /// `it.some(p)` — whether any element satisfies `p`.
    fn some(this: u64, callback: u64) -> bool {
        remaining(this)
            .into_iter()
            .enumerate()
            .any(|(index, element)| {
                super::primitives::to_boolean(apply(callback, element, index))
            })
    }

    /// `it.every(p)` — whether all of them do.
    fn every(this: u64, callback: u64) -> bool {
        remaining(this)
            .into_iter()
            .enumerate()
            .all(|(index, element)| {
                super::primitives::to_boolean(apply(callback, element, index))
            })
    }

    /// `it.find(p)` — the first that satisfies `p`, or `undefined`.
    fn find(this: u64, callback: u64) -> u64 {
        for (index, element) in remaining(this).into_iter().enumerate() {
            if super::primitives::to_boolean(apply(callback, element, index)) {
                return element;
            }
        }
        with_current(|context| super::objects::undefined_of(context))
    }

    /// `it.next()` — the element the cursor is on, and then the next one.
    fn next(this: u64) -> u64 {
        with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                let absent = super::objects::undefined_of(context);
                return super::generator::result(context, absent, true);
            };
            let Some((listed, at)) = context.cursor_at(cell) else {
                let absent = super::objects::undefined_of(context);
                return super::generator::result(context, absent, true);
            };
            let found = Value(listed)
                .as_slot()
                .and_then(|list| context.elements_at(list))
                .and_then(|elements| elements.get(at as usize).copied());
            match found {
                Some(value) => {
                    context.set_cursor(cell, listed, at + 1);
                    super::generator::result(context, value, false)
                }
                // Exhausted, and the cursor is left where it is: a `next()` after
                // the end answers `{ undefined, true }` every time rather than
                // wrapping around, which is what makes an exhausted iterator
                // stay exhausted.
                None => {
                    let absent = super::objects::undefined_of(context);
                    super::generator::result(context, absent, true)
                }
            }
        })
    }
}

/// An iterator positioned at the start of `listed`.
///
/// `listed` is an array value — what every caller already had — and it is held
/// rather than copied, so nothing walks it twice.
pub(in crate::entry) fn over(listed: u64) -> u64 {
    with_current(|context| {
        let prototype = match super::class_support::prototype(context, "ListIterator") {
            Some(prototype) => prototype,
            None => {
                register_list_iterator(context);
                match super::class_support::prototype(context, "ListIterator") {
                    Some(prototype) => prototype,
                    None => return super::objects::undefined_of(context),
                }
            }
        };
        let Some(cell) = super::native::plain(context) else {
            return super::objects::undefined_of(context);
        };
        context.set_prototype(cell, prototype);
        context.set_cursor(cell, listed, 0);

        // `Symbol.iterator` answering the iterator itself, which is what makes
        // `for`-`of` and spread reach one. Installed on the instance rather than
        // the prototype for the reason the generator's is installed by hand: the
        // attribute names a member with a string and this key is a symbol.
        let key = context.well_known(&format!("{}iterator", super::symbol::PREFIX));
        let itself = super::native::callable(context, itself as super::native::Native);
        super::objects::put(context, cell, key, itself);
        Value::from_slot(cell).bits()
    })
}

/// `it[Symbol.iterator]()` — the iterator itself.
extern "C" fn itself(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

/// The elements an iterator has not answered yet.
///
/// Taking them CONSUMES the iterator: the cursor moves to the end, so a helper
/// followed by `next()` answers done. That is what the specification says a
/// helper does to the iterator it was called on, and it is what stops
/// `it.take(1)` and `it.take(1)` from both answering the first element.
///
/// Read outside any borrow the caller may still be holding, because everything
/// here goes on to call user code.
fn remaining(this: u64) -> Vec<u64> {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return Vec::new();
        };
        let Some((listed, at)) = context.cursor_at(cell) else {
            return Vec::new();
        };
        let elements = match Value(listed).as_slot().and_then(|list| context.elements_at(list)) {
            Some(elements) => elements[(at as usize).min(elements.len())..].to_vec(),
            None => Vec::new(),
        };
        context.set_cursor(cell, listed, at + elements.len() as u32);
        elements
    })
}

/// One call of a helper's callback: the element and its position.
fn apply(callback: u64, element: u64, index: usize) -> u64 {
    let absent = with_current(|context| super::objects::undefined_of(context));
    let counted = super::modules::make_number(index as f64);
    super::functions::call(callback, absent, element, counted, absent, absent)
}

/// The array a helper's result is carried in.
fn listed(values: Vec<u64>) -> u64 {
    super::modules::make_array(values)
}

/// A count, as a length: negative and `NaN` are none, and infinity is all.
fn at_most(count: f64) -> usize {
    match count.is_nan() || count < 0.0 {
        true => 0,
        false => count.min(usize::MAX as f64) as usize,
    }
}
