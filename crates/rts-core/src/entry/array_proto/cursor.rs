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
//! # Why the helper family is forwarded rather than written again
//!
//! `map`, `take`, `toArray` and the rest of the ES2025 set are
//! [`crate::entry::list_iterator`]'s, and a second copy is exactly what
//! `collections::cursor` refused when it reused that object for `Map` and `Set`.
//! This cannot reuse the OBJECT — its `next` has to be this file's, and the
//! cursor table `ListIterator` walks holds one array where this needs a
//! receiver, an index and a view — so it reuses the METHODS instead: a helper
//! drains this cursor the way its own view says, hands the result to a
//! `ListIterator`, and calls the same-named method on it. One implementation,
//! reached the way a program reaches it.
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

use super::super::rooted::Rooted;
use super::super::{
    array, computed, functions, generator, list_iterator, native, objects, symbol, throw,
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

    /// `it.map(f)` — the same positions, each through `f`.
    fn map(this: u64, callback: u64) -> u64 {
        forwarded(this, "map", callback, absent())
    }

    /// `it.filter(p)` — the positions `p` answers truthy for.
    fn filter(this: u64, callback: u64) -> u64 {
        forwarded(this, "filter", callback, absent())
    }

    /// `it.take(n)` — at most the first `n`.
    fn take(this: u64, count: u64) -> u64 {
        forwarded(this, "take", count, absent())
    }

    /// `it.drop(n)` — everything after the first `n`.
    fn drop(this: u64, count: u64) -> u64 {
        forwarded(this, "drop", count, absent())
    }

    /// `it.flatMap(f)` — `f`'s answers, one level flattened.
    fn flat_map(this: u64, callback: u64) -> u64 {
        forwarded(this, "flatMap", callback, absent())
    }

    /// `it.toArray()` — what is left, as an array.
    fn to_array(this: u64) -> u64 {
        forwarded(this, "toArray", absent(), absent())
    }

    /// `it.forEach(f)` — `f` over each, answering nothing.
    fn for_each(this: u64, callback: u64) -> u64 {
        forwarded(this, "forEach", callback, absent())
    }

    /// `it.reduce(f, initial)`.
    fn reduce(this: u64, callback: u64, initial: u64) -> u64 {
        forwarded(this, "reduce", callback, initial)
    }

    /// `it.some(p)` — whether any remaining position satisfies `p`.
    fn some(this: u64, callback: u64) -> u64 {
        forwarded(this, "some", callback, absent())
    }

    /// `it.every(p)` — whether all of them do.
    fn every(this: u64, callback: u64) -> u64 {
        forwarded(this, "every", callback, absent())
    }

    /// `it.find(p)` — the first that satisfies `p`, or `undefined`.
    fn find(this: u64, callback: u64) -> u64 {
        forwarded(this, "find", callback, absent())
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

        // `Symbol.iterator` answering the cursor itself, which is what makes
        // `for`-`of` and spread reach one. On the instance rather than the
        // prototype for the reason `list_iterator::over` installs its own there:
        // the attribute names a member with a string and this key is a symbol.
        let key = context.well_known(&format!("{}iterator", symbol::PREFIX));
        let itself = native::callable(context, itself as native::Native);
        objects::put(context, cell, key, itself);
        Value::from_slot(cell).bits()
    })
}

/// `it[Symbol.iterator]()` — the cursor itself.
extern "C" fn itself(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
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

/// Everything this cursor has not answered yet, in its own view's shape.
///
/// Draining CONSUMES it, which is what the specification says a helper does to
/// the iterator it was called on — and what stops `it.take(1)` and `it.take(1)`
/// from both answering the first element.
///
/// ROOTED, because a pair is an allocation and an allocation collects: what has
/// been drained so far would otherwise live only in a `Vec` on the Rust heap,
/// which no scan of ours reaches. See [`super::super::rooted`].
fn drained(this: u64) -> Vec<u64> {
    let mut produced = Rooted::new();
    while let Some(value) = stepped(this) {
        produced.values().push(value);
    }
    produced.take()
}

/// One helper, answered by a `ListIterator` over what this cursor had left.
///
/// The method is READ off that iterator and called, rather than reached from
/// Rust: `list_iterator`'s helpers are private to their own module, and going
/// through the property protocol is both the only way in and the one that stays
/// right if a program replaced one of them.
fn forwarded(this: u64, name: &str, a0: u64, a1: u64) -> u64 {
    let listed = super::built(drained(this));
    if throw::in_flight() {
        return absent();
    }
    let iterator = list_iterator::over(listed);
    let method = with_current(|context| {
        let Some(cell) = Value(iterator).as_slot() else {
            return objects::undefined_of(context);
        };
        let key = context.well_known(name);
        match objects::read_property(context, cell, key) {
            Some(found) => found.bits(),
            None => objects::undefined_of(context),
        }
    });
    let nothing = absent();
    functions::call(method, iterator, a0, a1, nothing, nothing)
}

/// The `undefined` a helper passes for an argument the program did not give.
fn absent() -> u64 {
    with_current(|context| objects::undefined_of(context))
}
