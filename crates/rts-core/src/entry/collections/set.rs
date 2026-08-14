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
//! # What the set operations read, and what the specification says they read
//!
//! These reach the other collection's **table** directly. The specification
//! describes a "Set-like" protocol — it reads `size`, then calls `has` and
//! `keys` off the argument — so a plain object supplying those three is a valid
//! argument there and is not one here. Implementing the protocol version means
//! calling user code per member from inside an operation, which is the borrow
//! rule's exact hazard, and it buys a case no program in this engine can
//! currently construct: there is no `Symbol.iterator` for a `keys()` result to
//! satisfy. The divergence is recorded rather than approximated.

use super::with_current;
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `Set`.
#[rtse::class("Set", tag)]
impl Set {
    /// `new Set(iterable?)` — from an array or a string, today.
    #[construct]
    fn build(this: u64, iterable: u64) -> u64 {
        let absent = super::undefined();
        let values = match iterable == absent {
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
                table.set(context, value, value);
            }
            super::restore_sized(context, cell, table);
            Value::from_slot(cell).bits()
        })
    }

    /// `s.add(v)` — the set, so that adds chain.
    fn add(this: u64, value: u64) -> u64 {
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
    /// Snapshotted before the first call, for the reason `Map.forEach` records:
    /// a callback reaching the runtime under a held borrow aborts the process.
    fn for_each(this: u64, callback: u64, this_arg: u64) -> u64 {
        for (value, _) in super::entries_of(this) {
            // Stops on a throw, for the reason `Map.forEach` states.
            if super::invoke(callback, this_arg, value, value, this).is_none() {
                break;
            }
        }
        super::undefined()
    }

    /// `s.values()` — an array, for the reason the module documentation gives.
    fn values(this: u64) -> u64 {
        crate::entry::list_iterator::over(super::array_of(members(this)))
    }

    /// `s.keys()` — the same as `values`, which is what a set's keys are.
    fn keys(this: u64) -> u64 {
        crate::entry::list_iterator::over(super::array_of(members(this)))
    }

    /// `s.entries()` — `[v, v]` pairs, for the same parity with `Map`.
    fn entries(this: u64) -> u64 {
        crate::entry::list_iterator::over(super::pairs_array(super::entries_of(this)))
    }

    /// `s.union(other)`.
    fn union(this: u64, other: u64) -> u64 {
        let mut values = members(this);
        values.extend(members(other));
        // The duplicates are dropped by the table, which is the one place that
        // decides what a duplicate is.
        assembled(values)
    }

    /// `s.intersection(other)`.
    fn intersection(this: u64, other: u64) -> u64 {
        assembled(kept(this, other, true))
    }

    /// `s.difference(other)`.
    fn difference(this: u64, other: u64) -> u64 {
        assembled(kept(this, other, false))
    }

    /// `s.symmetricDifference(other)` — in each, in neither both.
    fn symmetric_difference(this: u64, other: u64) -> u64 {
        let mut values = kept(this, other, false);
        values.extend(kept(other, this, false));
        assembled(values)
    }

    /// `s.isSubsetOf(other)`.
    fn is_subset_of(this: u64, other: u64) -> bool {
        kept(this, other, false).is_empty()
    }

    /// `s.isSupersetOf(other)`.
    fn is_superset_of(this: u64, other: u64) -> bool {
        kept(other, this, false).is_empty()
    }

    /// `s.isDisjointFrom(other)`.
    fn is_disjoint_from(this: u64, other: u64) -> bool {
        kept(this, other, true).is_empty()
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

/// The members of `source` that `other` does or does not hold.
///
/// One function for six operations, because they differ only in which side is
/// walked and which answer keeps a member — and six copies of "walk this table,
/// ask that one" is where they would come to disagree about a `NaN` member.
///
/// An `other` that is not one of these collections is treated as **empty**,
/// which makes `s.difference(3)` answer a copy of `s` where the language throws.
/// The stated gap every refusal in this crate has while a throw cannot find a
/// handler.
fn kept(source: u64, other: u64, wanted: bool) -> Vec<u64> {
    with_current(|context| {
        let Some(cell) = Value(source).as_slot() else {
            return Vec::new();
        };
        let Some(table) = context.table_at(cell) else {
            return Vec::new();
        };
        let held = table.keys().to_vec();
        let against = Value(other)
            .as_slot()
            .and_then(|other_cell| context.table_at(other_cell));
        let Some(against) = against else {
            return match wanted {
                true => Vec::new(),
                false => held,
            };
        };
        held.into_iter()
            .filter(|value| against.has(context, *value) == wanted)
            .collect()
    })
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
                table.set(context, value, value);
            }
            super::restore_sized(context, cell, table);
        }
        made
    })
}
