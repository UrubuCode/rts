//! The iterator `Map`, `Set`, a string and a typed array answer.
//!
//! # Why it wraps a materialised list rather than walking the collection
//!
//! Because for what still reaches here, the collection is already walked: a
//! string's code points and a typed array's elements are built before the
//! iterator exists, and every one of them is reached by `for`-`of` through
//! exactly that array. Making them lazy is a different change — it means a
//! cursor into a shape or a table that a mutation during iteration has to keep
//! meaning something.
//!
//! # Half of that is already gone, twice
//!
//! A `Map` or a `Set` is walked LIVE — `collections::cursor` holds a position in
//! the table rather than a copy of it, because the copy was not merely slow but
//! WRONG: the language says an iterator sees entries added after it was made.
//! An ARRAY is walked live too, by `array_proto::cursor`, for the same reason.
//! The same object serves all of them, so [`ListIterator::next`] is where the
//! two paths meet.
//!
//! # Where the helpers went
//!
//! This file held eleven — `map`, `filter`, `take`, `drop`, `flatMap` and the
//! six terminal ones — and they were EAGER: each drained the iterator and
//! answered another one over the result. `ArrayCursor` held eleven more that
//! forwarded here, and a generator held none, so `g().map(f)` was a
//! `TypeError`.
//!
//! They live on `%IteratorPrototype%` now — one copy, lazy, inherited by all
//! three — and `entry::iterator` is both the object and the reason. What is
//! left here is the protocol: `next`, and the cursor it moves.

use super::with_current;
use crate::value::Value;

/// An iterator over a list that has already been built.
#[rtse::class("ListIterator")]
impl ListIterator {
    /// `it.next()` — the element the cursor is on, and then the next one.
    ///
    /// An iterator over a COLLECTION answers from the collection as it is now,
    /// not from a list taken when the iterator was made — see
    /// `collections::cursor` for what that costs and why the copy was wrong.
    fn next(this: u64) -> u64 {
        if let Some(step) = super::collections::stepped(this) {
            return super::collections::result_of(step);
        }
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
pub(in crate::entry) fn over(listed: u64, tag: &str) -> u64 {
    with_current(|context| {
        let prototype = match super::class_support::prototype(context, "ListIterator") {
            Some(prototype) => prototype,
            None => {
                register_list_iterator(context);
                // The helpers are inherited rather than owned, so the chain has
                // to be joined the moment this prototype exists — `adopt` is
                // idempotent and is called from each of the four registrations
                // for exactly that reason.
                super::iterator::adopt(context);
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

        // `Symbol.toStringTag`, so `Object.prototype.toString.call([].values())`
        // answers `[object Array Iterator]` rather than `[object Object]` —
        // the idiom a program uses to ask what something really is.
        //
        // The tag is a PARAMETER because one prototype serves every kind here:
        // the language gives an array's iterator, a map's, a set's and a
        // string's four different tags, and this engine builds all four from
        // this function. Hard-coding one would have made three of them lie,
        // which is worse than the `[object Object]` they answered before —
        // a wrong specific answer is believed where a generic one is not.
        //
        // On the INSTANCE for the reason a symbol-keyed member always is here:
        // the attribute helper names a member with a string and this key is a
        // symbol.
        let tag_key = context.well_known(&format!("{}toStringTag", super::symbol::PREFIX));
        let tag_value = context.intern_value(crate::text::Str::from_str(tag)).bits();
        super::objects::put(context, cell, tag_key, tag_value);
        Value::from_slot(cell).bits()
    })
}

