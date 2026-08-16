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

use super::{Context, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `Set`.
#[rtse::class("Set", tag)]
impl Set {
    /// `new Set(iterable?)` — from an array or a string, today.
    #[construct]
    fn build(this: u64, iterable: u64) -> u64 {
        let values = match super::nothing_to_fill_from(iterable) {
            true => Vec::new(),
            false => super::elements_of(iterable),
        };
        with_current(|context| {
            let Some(cell) = super::built(context, this, "Set") else {
                return undefined_of(context);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return undefined_of(context);
            };
            for value in values {
                let value = super::table::canonical(value);
                table.set(context, value, value);
            }
            super::restore_sized(context, cell, table);
            Value::from_slot(cell).bits()
        })
    }

    /// `s.add(v)` — the set, so that adds chain.
    ///
    /// The member is canonicalised BEFORE it is stored, in both columns: a set
    /// keeps one value where a map keeps a key and a value, so normalising only
    /// the key — which [`super::table::Table::set`] does — would leave `-0` in
    /// the half `s.entries()` reads out as the value.
    fn add(this: u64, value: u64) -> u64 {
        let value = super::table::canonical(value);
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot()
                && let Some(mut table) = super::taken(context, cell)
            {
                table.set(context, value, value);
                super::restore_sized(context, cell, table);
            }
            this
        })
    }

    /// `s.has(v)`.
    fn has(this: u64, value: u64) -> bool {
        with_current(|context| super::map::held(context, this, value))
    }

    /// `s.delete(v)`.
    #[js("delete")]
    fn remove(this: u64, value: u64) -> bool {
        with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                return false;
            };
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
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot()
                && let Some(mut table) = super::taken(context, cell)
            {
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
    fn for_each(this: u64, callback: u64, this_arg: u64) -> u64 {
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
        super::cursor::over(this, super::cursor::Kind::Keys, "Set Iterator")
    }

    /// `s.entries()` — `[v, v]` pairs, for parity with `Map`.
    fn entries(this: u64) -> u64 {
        super::cursor::over(this, super::cursor::Kind::Entries, "Set Iterator")
    }

    /// `s.union(other)`.
    fn union(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(other) else {
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
        let Some(other) = other_of(other) else {
            return super::undefined();
        };
        assembled(kept(this, &other, true))
    }

    /// `s.difference(other)`.
    fn difference(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(other) else {
            return super::undefined();
        };
        assembled(kept(this, &other, false))
    }

    /// `s.symmetricDifference(other)` — in each, in neither both.
    ///
    /// This side's members first, then the argument's: the specification builds
    /// it as a copy of the receiver with the shared members removed and the rest
    /// appended, and that order is what a program printing the result sees.
    fn symmetric_difference(this: u64, other: u64) -> u64 {
        let Some(other) = other_of(other) else {
            return super::undefined();
        };
        let mut values = kept(this, &other, false);
        values.extend(other.members().into_iter().filter(|value| !held_by(this, *value)));
        assembled(values)
    }

    /// `s.isSubsetOf(other)`.
    fn is_subset_of(this: u64, other: u64) -> bool {
        let Some(other) = other_of(other) else {
            return false;
        };
        kept(this, &other, false).is_empty()
    }

    /// `s.isSupersetOf(other)`.
    fn is_superset_of(this: u64, other: u64) -> bool {
        let Some(other) = other_of(other) else {
            return false;
        };
        other.members().into_iter().all(|value| held_by(this, value))
    }

    /// `s.isDisjointFrom(other)`.
    fn is_disjoint_from(this: u64, other: u64) -> bool {
        let Some(other) = other_of(other) else {
            return false;
        };
        kept(this, &other, true).is_empty()
    }
}

/// The members of a set, in insertion order.
fn members(collection: u64) -> Vec<u64> {
    with_current(|context| {
        Value(collection)
            .as_slot()
            .and_then(|cell| context.table_at(cell))
            .map(|table| table.keys().to_vec())
            .unwrap_or_default()
    })
}

/// Whether the receiver holds a value, by its own table.
fn held_by(collection: u64, value: u64) -> bool {
    with_current(|context| super::map::held(context, collection, value))
}

/// The argument to a set operation, as `GetSetRecord` reads it.
///
/// # Why the whole record is read before any member is
///
/// Because the language checks the argument before it does any work, and a
/// program can see the difference: `a.union([1, 2])` throws without a partial
/// result existing anywhere. Reading `size`, `has` and `keys` up front is also
/// what makes the refusal one place instead of seven.
struct Other {
    /// The object itself, which is the receiver its own methods are called on.
    object: u64,
    /// Its `has`, for an argument with no table of its own.
    has: u64,
    /// Its `keys`, likewise.
    keys: u64,
    /// The cell whose table holds its members, when it has one.
    table: Option<u32>,
}

impl Other {
    /// Its members, in its own order.
    ///
    /// Through the table when there is one and through `keys()` otherwise — the
    /// module doc says why the fast path is not a shortcut around the protocol.
    fn members(&self) -> Vec<u64> {
        if let Some(cell) = self.table {
            return with_current(|context| {
                context
                    .table_at(cell)
                    .map(|table| table.keys().to_vec())
                    .unwrap_or_default()
            });
        }
        let absent = super::undefined();
        let iterator = crate::entry::functions::call(
            self.keys,
            self.object,
            absent,
            absent,
            absent,
            absent,
        );
        // Rule 8: `call` answers `undefined` for a call that did not run, and
        // `undefined` is a value — iterating it would answer no members and let
        // the operation carry on producing a result the language never reaches.
        if crate::entry::throw::in_flight() {
            return Vec::new();
        }
        super::elements_of(iterator)
    }

    /// Whether it holds a value.
    fn holds(&self, value: u64) -> bool {
        if let Some(cell) = self.table {
            return with_current(|context| {
                context
                    .table_at(cell)
                    .is_some_and(|table| table.has(context, value))
            });
        }
        let absent = super::undefined();
        let answered =
            crate::entry::functions::call(self.has, self.object, value, absent, absent, absent);
        !crate::entry::throw::in_flight() && crate::entry::primitives::to_boolean(answered)
    }
}

/// Reads the argument, raising the `TypeError` the language raises.
///
/// `None` means a throw is in flight and the caller must stop — the discipline
/// `crates/rts-core/README.md` states as rule 8, in the direction where THIS is
/// the native that found the fault rather than a callee.
fn other_of(other: u64) -> Option<Other> {
    let read = with_current(|context| {
        let cell = Value(other).as_slot()?;
        // A string has a cell and is not an object, and a set operation over one
        // is the same refusal an array gets.
        if !crate::entry::primitive::is_object_in(context, other) {
            return None;
        }
        // `size` first, and it decides the whole refusal: an array has none, so
        // it is absent, and absent is what makes `a.union([1, 2])` a `TypeError`
        // rather than a union with an empty set.
        //
        // Required to BE a number, where the specification runs `ToNumber` over
        // it. The difference is an object whose `size` is `"2"` or has a
        // `valueOf`, and coercing here would run user code from inside the
        // borrow this reads under — the one thing every function in this module
        // is shaped to avoid. It is a refusal where the language accepts, which
        // is the direction a program notices and can correct.
        let key = context.well_known("size");
        let size = crate::entry::objects::read_property(context, cell, key)
            .and_then(|found| found.numeric())?;
        if size.is_nan() {
            return None;
        }
        let has = named(context, cell, "has")?;
        let keys = named(context, cell, "keys")?;
        Some(Other {
            object: other,
            has,
            keys,
            table: context.table_at(cell).map(|_| cell),
        })
    });
    if read.is_none() {
        crate::entry::throw::type_error(
            "a set operation takes a Set or a set-like object: one with a numeric \
             `size` and callable `has` and `keys`",
        );
    }
    read
}

/// One CALLABLE member of an object, by name.
fn named(context: &mut Context, cell: u32, name: &str) -> Option<u64> {
    let key = context.well_known(name);
    let found = crate::entry::objects::read_property(context, cell, key)?.bits();
    let slot = Value(found).as_slot()?;
    context.callable_at(slot)?;
    Some(found)
}

/// The members of `source` that `other` does or does not hold.
///
/// One function for five operations, because they differ only in which answer
/// keeps a member — and five copies of "walk this table, ask that one" is where
/// they would come to disagree about a `NaN` member.
///
/// ROOTED: `other.holds` may be a call into user code, which allocates, and what
/// has been kept so far is named only by a `Vec` on the Rust heap.
fn kept(source: u64, other: &Other, wanted: bool) -> Vec<u64> {
    let mut held = crate::entry::rooted::Rooted::new();
    for value in members(source) {
        if other.holds(value) == wanted {
            held.values().push(value);
        }
    }
    held.take()
}

/// A new `Set` over these values, duplicates dropped.
fn assembled(values: Vec<u64>) -> u64 {
    with_current(|context| {
        let made = super::fresh(context, "Set");
        let Some(cell) = Value(made).as_slot() else {
            return made;
        };
        if let Some(mut table) = super::taken(context, cell) {
            for value in values {
                let value = super::table::canonical(value);
                table.set(context, value, value);
            }
            super::restore_sized(context, cell, table);
        }
        made
    })
}
