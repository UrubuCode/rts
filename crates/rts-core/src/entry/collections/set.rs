//! `Set` — members of any kind, in the order they were added, plus the ES2025
//! set operations.
//!
//! # Why a member is stored as both key and value
//!
//! So that one table serves both classes. The reason is in [`super::table`]: a
//! `Set` storing keys alone would need its own shift on `delete` and its own
//! bounds everywhere, and the invariant "the values vector is empty here and
//! populated there" is the kind that holds until one function forgets.
//!
//! # What the set operations read
//!
//! The **Set-like protocol** the specification describes: `size`, then `has` and
//! `keys` off the argument. So `a.union(someMap)` and `a.union({ size, has,
//! keys })` both work, and `a.union([1, 2])` is the `TypeError` the language
//! says it is rather than a silent union with nothing.
//!
//! This module read the other side's **table** directly before, which made every
//! non-collection argument count as empty — a wrong answer where the language
//! refuses. The table is still the fast path when the argument HAS one, because
//! a `Map` or a `Set` already holds its members here and asking it for them
//! through two calls per member into itself is the same answer, slower.
//!
//! The divergence that leaves, named: an argument that has a table and has
//! OVERRIDDEN `keys` or `has` is read from the table anyway, so the override
//! does not run. It is still validated — an object with no `size` is refused
//! whatever it holds — and the case is a subclass deliberately lying about its
//! own contents.

use super::ops::{assembled, held_by, kept, members, other_of, toggled};
use super::with_current;
use crate::entry::objects::undefined_of;

/// `Set`.
#[rtse::class("Set", tag)]
impl Set {
    /// `new Set(iterable?)` — anything the iteration protocol walks.
    ///
    /// Filled through `this.add`, for the three reasons [`super::adder`] gives:
    /// a subclass's override is used, the method is read once, and an element
    /// whose `add` throws closes the iterator where it failed.
    /// Arity 0, not 1: the specification pins `Set.length` at zero because the
    /// iterable is optional in the way `length` counts.
    #[construct]
    #[arity(0)]
    fn build(this: u64, iterable: u64) -> u64 {
        // Before the argument is read: `Set()` without `new` is a `TypeError`,
        // and the order is observable.
        if !super::requires_new(this, "Set") {
            return super::undefined();
        }
        let Some(set) = super::emptied(this, "Set") else {
            return super::undefined();
        };
        if !super::nothing_to_fill_from(iterable) {
            super::adder::fill(set, iterable, "add", super::adder::Shape::Members);
        }
        set
    }

    /// `s.add(v)` — the set, so that adds chain.
    ///
    /// The member is canonicalised BEFORE it is stored, in both columns: a set
    /// keeps one value where a map keeps a key and a value, so normalising only
    /// the key — which [`super::table::Table::set`] does — would leave `-0` in
    /// the half `s.entries()` reads out as the value.
    fn add(this: u64, value: u64) -> u64 {
        let Some(cell) = super::branded(this, super::Brand::Set) else {
            return super::undefined();
        };
        let value = super::table::canonical(value);
        with_current(|context| {
            if let Some(mut table) = super::taken(context, cell) {
                table.set(context, value, value);
                super::restore_sized(context, cell, table);
            }
            this
        })
    }

    /// `s.has(v)`.
    fn has(this: u64, value: u64) -> bool {
        if super::branded(this, super::Brand::Set).is_none() {
            return false;
        }
        with_current(|context| super::map::held(context, this, value))
    }

    /// `s.delete(v)`.
    #[js("delete")]
    fn remove(this: u64, value: u64) -> bool {
        let Some(cell) = super::branded(this, super::Brand::Set) else {
            return false;
        };
        with_current(|context| {
            let Some(mut table) = super::taken(context, cell) else {
                return false;
            };
            let removed = table.remove(context, value);
            super::restore_sized(context, cell, table);
            removed
        })
    }

    /// `s.clear()`.
    fn clear(this: u64) -> u64 {
        let Some(cell) = super::branded(this, super::Brand::Set) else {
            return super::undefined();
        };
        with_current(|context| {
            if let Some(mut table) = super::taken(context, cell) {
                table.clear();
                super::restore_sized(context, cell, table);
            }
            undefined_of(context)
        })
    }

    /// `s.forEach(cb, thisArg)` — `cb(value, value, set)`.
    ///
    /// The value twice, which looks like a mistake and is the language: the
    /// signature matches `Map.prototype.forEach` so that a callback written for
    /// one works on the other, and a set's key is its value.
    ///
    /// A LIVE walk, for the reason `Map.forEach` records: a snapshot makes a
    /// member added by the callback invisible, where the language visits it.
    /// Arity 1: `thisArg` is optional in the way `length` counts.
    #[arity(1)]
    fn for_each(this: u64, callback: u64, this_arg: u64) -> u64 {
        if super::branded(this, super::Brand::Set).is_none() {
            return super::undefined();
        }
        let mut at = 0;
        while let Some((seq, value, _)) = super::cursor::after(this, at) {
            at = seq;
            // Stops on a throw, for the reason `Map.forEach` states.
            if super::invoke(callback, this_arg, value, value, this).is_none() {
                break;
            }
        }
        super::undefined()
    }

    /// `s.values()` — a live iterator, for the reason [`super::cursor`] gives.
    ///
    /// Also `s.keys()` and `s[Symbol.iterator]`, and the SAME function object
    /// rather than three that agree — [`super::register_set`] installs the other
    /// two names, which is why neither is written here.
    fn values(this: u64) -> u64 {
        match super::branded(this, super::Brand::Set) {
            Some(_) => super::cursor::over(this, super::cursor::Kind::Keys, "Set Iterator"),
            None => super::undefined(),
        }
    }

    /// `s.entries()` — `[v, v]` pairs, for parity with `Map`.
    fn entries(this: u64) -> u64 {
        match super::branded(this, super::Brand::Set) {
            Some(_) => super::cursor::over(this, super::cursor::Kind::Entries, "Set Iterator"),
            None => super::undefined(),
        }
    }

    /// `s.union(other)`.
    fn union(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(this, other) else {
            return super::undefined();
        };
        let mut values = members(this);
        values.extend(other.members());
        // The duplicates are dropped by the table, which is the one place that
        // decides what a duplicate is.
        assembled(values)
    }

    /// `s.intersection(other)`.
    fn intersection(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(this, other) else {
            return super::undefined();
        };
        assembled(kept(this, &other, true))
    }

    /// `s.difference(other)`.
    fn difference(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(this, other) else {
            return super::undefined();
        };
        assembled(kept(this, &other, false))
    }

    /// `s.symmetricDifference(other)` — in each, in neither both.
    ///
    /// This side's members first, then the argument's: the specification builds
    /// it as a copy of the receiver with the shared members removed and the rest
    /// appended, and that order is what a program printing the result sees.
    ///
    /// It asks the argument for its `keys` and **never for its `has`**, which is
    /// the one set operation of the seven that does not — every membership
    /// question here is about the RECEIVER's own data, which the language reads
    /// directly. Asking `has` answered the same sets and was observable: a
    /// set-like counting its own calls saw three where the language performs
    /// none, which is what `collections/claude2-set-ops-protocol-observation`
    /// measures.
    fn symmetric_difference(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(this, other) else {
            return super::undefined();
        };
        let mine = members(this);
        let theirs = other.members();
        // Rule 8: `members` runs the argument's `keys` and then its iterator, so
        // an empty answer here can mean "it threw" as easily as "it is empty".
        if crate::entry::throw::in_flight() {
            return super::undefined();
        }
        toggled(mine, theirs)
    }

    /// `s.isSubsetOf(other)`.
    fn is_subset_of(this: u64, other: u64) -> bool {
        let Some(other) = other_of(this, other) else {
            return false;
        };
        kept(this, &other, false).is_empty()
    }

    /// `s.isSupersetOf(other)`.
    fn is_superset_of(this: u64, other: u64) -> bool {
        let Some(other) = other_of(this, other) else {
            return false;
        };
        other.members().into_iter().all(|value| held_by(this, value))
    }

    /// `s.isDisjointFrom(other)`.
    fn is_disjoint_from(this: u64, other: u64) -> bool {
        let Some(other) = other_of(this, other) else {
            return false;
        };
        kept(this, &other, true).is_empty()
    }
}
