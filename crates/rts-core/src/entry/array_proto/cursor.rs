//! A position inside a LIVE array, which is what `keys()`, `values()`,
//! `entries()` and `[Symbol.iterator]()` answer.
//!
//! # Why this is not a copy of the elements
//!
//! It was one — [`crate::entry::list_iterator`] over an array `values()` had
//! just built — and the copy is observable in both directions. The language's
//! array iterator holds the receiver and an index, reads `length` at every step
//! and reads the element at the moment it is reached, so an element pushed
//! after the iterator was made IS seen and an array shortened mid-walk ends it
//! early. A snapshot answers what the array held when the method was called:
//!
//! ```text
//! const a = [1, 2]; const it = a[Symbol.iterator]();
//! it.next(); a.push(3);
//! it.next().value   // 2 either way
//! it.next().value   // 3 in the language; undefined from a snapshot
//! ```
//!
//! # Why it also reads a receiver that is not an array
//!
//! `Array.prototype.values` is defined over an array-LIKE: `ToObject`, then
//! `LengthOfArrayLike`, then `Get` per index. So `Object.create([7, 8, 9])`
//! iterates through the inherited method, and the snapshot could not — it asked
//! `elements_at`, found nothing because the child object holds no elements of
//! its own, and answered `undefined`, which `for`-`of` then refused as "not
//! iterable". A genuine array still takes the direct path: reading `length` and
//! every index through the property protocol would put two calls into user code
//! where the elements are already in hand.
//!
//! **What that still does not finish, measured rather than assumed.** The walk
//! now runs — `Object.create([7, 8, 9])` yields three positions, because
//! `length` is an ordinary property the chain walk finds — but every element
//! reads `undefined`, because `computed::access::get_indexed` asks
//! `elements_at` of the RECEIVER only and an array's elements are not shape
//! properties, so the chain walk behind it cannot see them either. That is a
//! gap in the indexed read and not in this file: `c[0]` answers `undefined` and
//! `0 in c` answers `false` for the same object with no iterator involved. It
//! is written here because this is where a reader will notice it.
//!
//! # Where the helper family went
//!
//! It was ELEVEN methods here, each draining this cursor and forwarding the
//! result to a `ListIterator` that held eleven more. Both copies were eager,
//! and a generator — which reaches neither — had none at all.
//!
//! They are `%IteratorPrototype%`'s now, lazy, and this prototype INHERITS
//! them: `crate::entry::iterator` is the object and the reason. What is left
//! here is the one thing that genuinely is this file's, which is what `next`
//! means for a live view of an array-like.
//!
//! # Where the third field lives, and what it should be
//!
//! An own property under the `@@` prefix — the space [`crate::entry::symbol`]
//! reserves for keys a program cannot spell and enumeration filters out, so it
//! does not appear in `Object.keys(it)` or `JSON.stringify(it)`. The reasoning
//! is `collections::cursor`'s and so is the consequence: it belongs in the
//! cursor table beside the other two fields, and is not there because that table
//! stores a `(u64, u32)` pair and widening it is a decision about
//! `entry::context`. The day that pair grows a third field, this property and
//! its reads come out — along with that module's.

use super::super::{
    array, computed, generator, native, objects, symbol, throw,
    with_current,
};
use crate::text::Str;
use crate::value::Value;

/// Which of the three views a cursor answers.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// `a.keys()` — the indices.
    Keys = 1,
    /// `a.values()`, and `a[Symbol.iterator]()`, which is the same function.
    Values = 2,
    /// `a.entries()` — index and element, paired.
    Entries = 3,
}

impl Kind {
    /// The view a stored number names, if it names one.
    fn of(number: f64) -> Option<Kind> {
        match number as u8 {
            1 => Some(Kind::Keys),
            2 => Some(Kind::Values),
            3 => Some(Kind::Entries),
            _ => None,
        }
    }
}

/// The key under which a cursor records which view it answers.
const KIND: &str = "@@arrayCursor";

/// The position of a cursor that has run off the end.
///
/// Reserved rather than "the index it stopped at", because exhaustion is
/// PERMANENT and a live length would otherwise undo it: an iterator driven past
/// the end of a two-element array, followed by a `push`, would start answering
/// again. The language says a completed iterator stays completed — it drops the
/// receiver — and this is that, in the one field there is room for.
///
/// No array can reach it: an index is bounded by 2^32-2 as a property key.
const SPENT: u32 = u32::MAX;

/// The iterator itself, and the ES2025 helpers a program reaches through it.
#[rtse::class("ArrayCursor")]
impl ArrayCursor {
    /// `it.next()` — the position the cursor is on, read from the array AS IT IS
    /// NOW, and then the next one.
    fn next(this: u64) -> u64 {
        // Outside every borrow: an array-like receiver answers its `length` and
        // its elements through `Get`, which runs a getter or a proxy trap.
        let step = stepped(this);
        with_current(|context| match step {
            Some(value) => generator::result(context, value, false),
            None => {
                let absent = objects::undefined_of(context);
                generator::result(context, absent, true)
            }
        })
    }
}

/// A cursor positioned before the first element of `receiver`.
///
/// The receiver is HELD, not read: nothing is walked here, so `a.entries()` on a
/// million-element array costs one object rather than a million pairs.
pub(super) fn over(receiver: u64, kind: Kind) -> u64 {
    with_current(|context| {
        let prototype = match super::super::class_support::prototype(context, "ArrayCursor") {
            Some(prototype) => prototype,
            None => {
                // Lazily, the way `list_iterator::over` registers its own: this
                // class is not a global name, so `entry::global` has no row that
                // would ever reach it.
                register_array_cursor(context);
                // The helpers are inherited rather than owned now, so the chain
                // is joined the moment this prototype exists. `adopt` is
                // idempotent and every registration calls it, because which
                // iterator a program reaches first is the program's choice.
                super::super::iterator::adopt(context);
                if let Some(found) = super::super::class_support::prototype(context, "ArrayCursor")
                    && let Some(cell) = Value(found).as_slot()
                {
                    // `Symbol.toStringTag` on the PROTOTYPE, which is where the
                    // specification puts %ArrayIteratorPrototype%'s — so
                    // `Object.prototype.toString.call([].values())` answers
                    // `[object Array Iterator]` and the tag is an own property
                    // of the prototype rather than of every cursor made.
                    let key =
                        context.well_known(&format!("{}toStringTag", symbol::PREFIX));
                    let tag = context
                        .intern_value(Str::from_str("Array Iterator"))
                        .bits();
                    objects::put(context, cell, key, tag);
                    native::hidden(context, cell, key);
                    // The primordial `next`, remembered at the ONE moment it is
                    // knowably primordial: this prototype did not exist a line
                    // ago, so nothing can have replaced the method yet.
                    // `super::super::pattern::array_pattern_direct` compares the
                    // CURRENT `next` against this before letting a pattern read
                    // by index, because skipping the protocol is only
                    // indistinguishable while the step it skips is this one.
                    let next = context.well_known("next");
                    context.array_cursor_next = objects::own_property(context, cell, next)
                        .map(|found| found.bits());
                    // And WHERE it lives, so the guard never has to ask
                    // `class_support::prototype` for it — that walks `classes`
                    // comparing a string per entry, and the program that pays
                    // most for it is the one that never makes a cursor at all
                    // and walks the whole list to be told so.
                    context.array_cursor_prototype = Some(cell);
                }
                match super::super::class_support::prototype(context, "ArrayCursor") {
                    Some(prototype) => prototype,
                    None => return objects::undefined_of(context),
                }
            }
        };
        let Some(cell) = native::plain(context) else {
            return objects::undefined_of(context);
        };
        context.set_prototype(cell, prototype);
        context.set_cursor(cell, receiver, 0);
        let key = context.well_known(KIND);
        let kind = Value::from_f64(kind as u8 as f64).bits();
        objects::put(context, cell, key, kind);

        Value::from_slot(cell).bits()
    })
}

/// One step: the value this cursor's view answers, or `None` when it is spent.
///
/// The three reads happen in the specification's order — position, then
/// `length`, then the element — because a receiver may answer any of them with
/// user code, and a cursor that read the element before deciding it was in range
/// would run a getter the language never reaches.
fn stepped(this: u64) -> Option<u64> {
    let (receiver, at) = position(this)?;
    if at == SPENT {
        return None;
    }
    let count = length_of(receiver)?;
    if at as usize >= count {
        moved(this, SPENT);
        return None;
    }
    moved(this, at + 1);
    let index = at as usize;
    let counted = Value::from_f64(index as f64).bits();
    match kind_of(this)? {
        Kind::Keys => Some(counted),
        Kind::Values => element_of(receiver, index),
        // The pair is built OUTSIDE any borrow, which is why the element is read
        // first and paired second: making an array allocates.
        Kind::Entries => {
            let element = element_of(receiver, index)?;
            Some(super::built(vec![counted, element]))
        }
    }
}

/// What this cursor walks, and where it has got to.
fn position(this: u64) -> Option<(u64, u32)> {
    with_current(|context| context.cursor_at(Value(this).as_slot()?))
}

/// Moves the cursor, keeping the receiver it already holds.
fn moved(this: u64, at: u32) {
    with_current(|context| {
        if let Some(cell) = Value(this).as_slot()
            && let Some((receiver, _)) = context.cursor_at(cell)
        {
            context.set_cursor(cell, receiver, at);
        }
    });
}

/// Which view this cursor answers.
fn kind_of(this: u64) -> Option<Kind> {
    with_current(|context| {
        let cell = Value(this).as_slot()?;
        let key = context.well_known(KIND);
        let found = objects::read_property(context, cell, key)?;
        Kind::of(found.numeric()?)
    })
}

/// How many positions the receiver has RIGHT NOW.
///
/// `None` means a `length` getter threw, which rule 8 says stops the walk rather
/// than being read as zero — an exhausted cursor and a failed read answer the
/// same `{ done: true }` to the program, and the compiled call site above
/// re-raises what is in flight.
fn length_of(receiver: u64) -> Option<usize> {
    let direct = with_current(|context| {
        Value(receiver)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .map(Vec::len)
    });
    if let Some(count) = direct {
        return Some(count);
    }
    // An array-LIKE, so `length` is a property to read rather than a vector to
    // measure — and `ToLength` of whatever it answered, which is what makes
    // `{length: "3"}` three positions rather than a refusal.
    let key = with_current(|context| context.intern_value(Str::from_str("length")).bits());
    let claimed = computed::get_indexed(receiver, key);
    (!throw::in_flight()).then(|| super::numeric::length(claimed))
}

/// The element at a position, as the program SEES it.
///
/// A hole reads `undefined` rather than the word that marks one: the iteration
/// protocol has no holes, so `[...[1,,3]]` is three elements and none of them is
/// a marker `array::visible` exists to keep out of the program.
fn element_of(receiver: u64, index: usize) -> Option<u64> {
    let direct = with_current(|context| {
        let cell = Value(receiver).as_slot()?;
        let held = *context.elements_at(cell)?.get(index)?;
        Some(array::visible(context, held))
    });
    if let Some(found) = direct {
        return Some(found);
    }
    // `Get`, because that is what the specification says for an array-like: a
    // getter runs and a proxy is asked, neither of which a shape walk reaches.
    let read = computed::get_indexed(receiver, Value::from_f64(index as f64).bits());
    (!throw::in_flight()).then_some(read)
}

