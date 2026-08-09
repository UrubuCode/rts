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

use super::{Context, with_current};
use crate::value::Value;

/// An iterator over a list that has already been built.
#[rtse::class("ListIterator")]
impl ListIterator {
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
