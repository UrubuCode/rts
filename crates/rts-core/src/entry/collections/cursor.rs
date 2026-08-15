//! A position inside a LIVE collection, which is what `keys()`, `values()`,
//! `entries()` and `forEach` walk.
//!
//! # Why this is not a copy of the entries
//!
//! It was one, and the copy is observable. The language says an iterator over a
//! `Map` sees entries inserted after it was made, does not see entries deleted
//! before it reaches them, and re-visits a key that was deleted and added again
//! — all of which a snapshot gets wrong in the same direction: it answers what
//! the collection held at the moment the method was called.
//!
//! What makes a live walk possible without a borrow that spans a callback is
//! [`super::table`]'s sequence number. A cursor remembers the last number it
//! answered, so every step is a fresh, SHORT borrow that asks the table one
//! question — "what comes after this number?" — and gives the borrow straight
//! back before any user code runs. That is the rule the whole `collections`
//! module is written around: `with_current` holds a `RefCell` borrow for the
//! length of its body, a callback re-enters it, and an `extern "C"` frame cannot
//! unwind, so the process aborts rather than failing a test.
//!
//! # Why the iterator object is `ListIterator` and not a class of its own
//!
//! Because the helper methods — `map`, `take`, `toArray`, the ES2025 family —
//! are the same helpers, and a second class would be a second copy of them. So
//! the object is the one [`crate::entry::list_iterator::over`] already builds:
//! the cursor table holds the collection where it would hold an array, and the
//! only thing the object carries beyond that is WHICH of the three shapes it
//! yields.
//!
//! # Where that third field lives, and what it should be
//!
//! An own property under the `@@` prefix — the space [`crate::entry::symbol`]
//! reserves for keys a program cannot spell and enumeration filters out, which
//! is why it does not appear in `Object.keys(it)` or `JSON.stringify(it)`.
//!
//! It belongs in the cursor table beside the other two fields, and is not there
//! because that table stores a `(u64, u32)` pair and widening it is a decision
//! about `entry::context`. Stated rather than left to be discovered: the day
//! that pair grows a third field, this property and its reads come out.

use super::{Context, with_current};
use crate::entry::{list_iterator, objects, rooted};
use crate::value::Value;

/// Which of a collection's three views an iterator answers.
///
/// A `Set` stores each member as both key and value — [`super::table`] says why
/// — so `s.values()` and `s.keys()` are both [`Kind::Keys`] and `s.entries()`
/// answers the member twice, with no case of its own anywhere here.
#[derive(Clone, Copy)]
pub(in crate::entry) enum Kind {
    /// `m.keys()`, `s.values()`, `s.keys()`.
    Keys = 1,
    /// `m.values()`.
    Values = 2,
    /// `m.entries()`, `s.entries()`, and what `for-of` over a `Map` yields.
    Entries = 3,
}

impl Kind {
    /// The kind a stored number names, if it names one.
    fn of(number: f64) -> Option<Kind> {
        match number as u8 {
            1 => Some(Kind::Keys),
            2 => Some(Kind::Values),
            3 => Some(Kind::Entries),
            _ => None,
        }
    }
}

/// What one step of a cursor found.
///
/// Two shapes for a reason the borrow rule forces: an entry pair has to become a
/// two-element array, which ALLOCATES, and the borrow that read the table is the
/// one thing that must not be held across an allocation that can collect. So the
/// step answers the raw values and the caller builds the array.
pub(in crate::entry) enum Stepped {
    /// Nothing left — and the cursor is now marked spent, permanently.
    Done,
    /// One value: a key, or a value.
    One(u64),
    /// A key and its value, still to be paired.
    Pair(u64, u64),
}

/// The key under which an iterator records which view it answers.
///
/// `@@` is the prefix [`crate::entry::symbol::is_symbol_key`] filters
/// enumeration on, so this is invisible to `Object.keys` and to
/// `JSON.stringify` — see the module doc for why it is a property at all.
const KIND: &str = "@@collectionCursor";

/// The position of a cursor that has run off the end.
///
/// [`super::table`] reserves it, which is what makes exhaustion permanent: no
/// entry can ever carry a number above this one, so a spent cursor answers
/// `done` even after the collection grows. The language says an iterator that
/// has completed stays completed, and without a reserved value a `m.set` after
/// the last `next()` would restart it.
const SPENT: u32 = u32::MAX;

/// An iterator positioned before the first entry of a collection.
///
/// A value that is not one of these collections answers an iterator that yields
/// nothing, where the language throws — the same tolerance every refusal in this
/// module settles on while a throw from here cannot find a handler.
pub(in crate::entry) fn over(collection: u64, kind: Kind) -> u64 {
    let iterator = list_iterator::over(collection);
    with_current(|context| {
        if let Some(cell) = Value(iterator).as_slot() {
            let key = context.well_known(KIND);
            let kind = Value::from_f64(kind as u8 as f64).bits();
            objects::put(context, cell, key, kind);
        }
        iterator
    })
}

/// Advances a cursor by one entry.
///
/// `None` for anything that is not one of these — an iterator over an array, or
/// a value that is not an iterator at all — which is what lets
/// [`crate::entry::list_iterator`] keep its own path for those instead of this
/// growing a second answer to what an array is.
pub(in crate::entry) fn stepped(this: u64) -> Option<Stepped> {
    with_current(|context| {
        let cell = Value(this).as_slot()?;
        let (collection, at) = context.cursor_at(cell)?;
        // An iterator over a LIST is answered before the kind is read, and that
        // ordering is the whole cost of this branch on the path that does not
        // take it: `[1, 2, 3].values()` is by far the common iterator here, and
        // asking each of its `next()` calls for a property it does not have
        // would walk a prototype chain per element to learn nothing.
        if Value(collection)
            .as_slot()
            .is_some_and(|held| context.elements_at(held).is_some())
        {
            return None;
        }
        let kind = kind_at(context, cell)?;
        if at == SPENT {
            return Some(Stepped::Done);
        }
        let found = Value(collection)
            .as_slot()
            .and_then(|held| context.table_at(held))
            .and_then(|table| table.after(at));
        let Some((seq, key, value)) = found else {
            context.set_cursor(cell, collection, SPENT);
            return Some(Stepped::Done);
        };
        context.set_cursor(cell, collection, seq);
        Some(match kind {
            Kind::Keys => Stepped::One(key),
            Kind::Values => Stepped::One(value),
            Kind::Entries => Stepped::Pair(key, value),
        })
    })
}

/// The `{ value, done }` object a step answers to `next()`.
pub(in crate::entry) fn result_of(step: Stepped) -> u64 {
    match step {
        Stepped::Done => with_current(|context| {
            let absent = objects::undefined_of(context);
            crate::entry::generator::result(context, absent, true)
        }),
        Stepped::One(value) => {
            with_current(|context| crate::entry::generator::result(context, value, false))
        }
        // The pair is built OUTSIDE any borrow, for the reason [`Stepped`]
        // records: making the array allocates.
        Stepped::Pair(key, value) => {
            let pair = super::array_of(vec![key, value]);
            with_current(|context| crate::entry::generator::result(context, pair, false))
        }
    }
}

/// Everything a cursor has left, taking it to the end.
///
/// What the ES2025 helpers on an iterator read, and `None` for an iterator that
/// is not over a collection — the same distinction [`stepped`] draws and for the
/// same reason.
///
/// ROOTED, because a pair becomes an array and that allocation can collect: what
/// has been drained so far is named only by a `Vec` on the Rust heap until the
/// caller stores it somewhere the heap can see.
pub(in crate::entry) fn drained(this: u64) -> Option<Vec<u64>> {
    let mut held = rooted::Rooted::new();
    loop {
        match stepped(this)? {
            Stepped::Done => return Some(held.take()),
            Stepped::One(value) => held.values().push(value),
            Stepped::Pair(key, value) => {
                let pair = super::array_of(vec![key, value]);
                held.values().push(pair);
            }
        }
    }
}

/// The entry after `seq`, read without an iterator object in the way.
///
/// What `forEach` walks with: it has a position of its own on the Rust stack and
/// needs no object to keep one in, and the borrow this takes is over before the
/// callback runs.
pub(in crate::entry) fn after(collection: u64, seq: u32) -> Option<(u32, u64, u64)> {
    with_current(|context| {
        let cell = Value(collection).as_slot()?;
        context.table_at(cell)?.after(seq)
    })
}

/// Which view an iterator answers, if it is over a collection at all.
fn kind_at(context: &mut Context, cell: u32) -> Option<Kind> {
    let key = context.well_known(KIND);
    let found = objects::read_property(context, cell, key)?;
    Kind::of(found.numeric()?)
}
