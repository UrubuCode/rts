//! `Map` — keys of any kind, in the order they were inserted.
//!
//! Every mutation follows the same three steps, and the shape is the safety
//! argument rather than a comment asking for care: take the table off the cell,
//! work on it with the context free, put it back with `size` in step. Nothing
//! here calls out to JavaScript with a borrow held, and the two members that
//! call out at all — `forEach` and `groupBy` — take one short borrow per step
//! and give it back before the call.

use super::{Context, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `Map`.
#[rtse::class("Map", tag)]
impl Map {
    /// `new Map(iterable?)`.
    ///
    /// The argument is an iterable of `[key, value]` pairs, which today means an
    /// **array of two-element arrays**: `new Map([[1, "a"], [2, "b"]])`. What it
    /// cannot yet take is another `Map`, a generator, or anything else declaring
    /// `Symbol.iterator` — see [`super::elements_of`] for why that is a gap in
    /// the iteration protocol rather than in this constructor.
    #[construct]
    fn build(this: u64, iterable: u64) -> u64 {
        // Read before any borrow is taken, because `iterate` is itself an entry
        // point and takes one of its own.
        let pairs = match super::nothing_to_fill_from(iterable) {
            true => Vec::new(),
            false => super::pairs_of(iterable),
        };
        with_current(|context| {
            let Some(cell) = super::built(context, this, "Map") else {
                return undefined_of(context);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return undefined_of(context);
            };
            for (key, value) in pairs {
                table.set(context, key, value);
            }
            super::restore_sized(context, cell, table);
            Value::from_slot(cell).bits()
        })
    }

    /// `m.get(k)` — the value, or `undefined`.
    fn get(this: u64, key: u64) -> u64 {
        with_current(|context| {
            let absent = undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let Some(table) = context.table_at(cell) else {
                return absent;
            };
            table.get(context, key).unwrap_or(absent)
        })
    }

    /// `m.set(k, v)` — the map, so that writes chain.
    fn set(this: u64, key: u64, value: u64) -> u64 {
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot()
                && let Some(mut table) = super::taken(context, cell)
            {
                table.set(context, key, value);
                super::restore_sized(context, cell, table);
            }
            this
        })
    }

    /// `m.has(k)`.
    fn has(this: u64, key: u64) -> bool {
        with_current(|context| held(context, this, key))
    }

    /// `m.delete(k)` — whether it was there.
    ///
    /// Named `remove` in Rust because `delete` is not an identifier a method can
    /// have here, and spelled back with `#[js]`.
    #[js("delete")]
    fn remove(this: u64, key: u64) -> bool {
        with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                return false;
            };
            let Some(mut table) = super::taken(context, cell) else {
                return false;
            };
            let removed = table.remove(context, key);
            super::restore_sized(context, cell, table);
            removed
        })
    }

    /// `m.clear()`.
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

    /// `m.forEach(cb, thisArg)` — `cb(value, key, map)`, in insertion order.
    ///
    /// A LIVE walk: each step asks the table what comes after the last entry it
    /// answered, and the borrow that asks is over before the callback runs. That
    /// is not tidiness on either count — the callback is arbitrary user code
    /// which will reach the runtime, a second `with_current` panics, and an
    /// `extern "C"` frame cannot unwind, so the process aborts — and a snapshot,
    /// which is what this was, makes an entry added by the callback invisible
    /// where the language says it is visited.
    fn for_each(this: u64, callback: u64, this_arg: u64) -> u64 {
        let mut at = 0;
        while let Some((seq, key, value)) = super::cursor::after(this, at) {
            at = seq;
            // A callback that throws stops the walk. Running it over the rest
            // produces effects the language says never happen.
            if super::invoke(callback, this_arg, value, key, this).is_none() {
                break;
            }
        }
        super::undefined()
    }

    /// `m.keys()` — a live iterator, for the reason [`super::cursor`] gives.
    fn keys(this: u64) -> u64 {
        super::cursor::over(this, super::cursor::Kind::Keys)
    }

    /// `m.values()`.
    fn values(this: u64) -> u64 {
        super::cursor::over(this, super::cursor::Kind::Values)
    }

    /// `m.entries()` — a `[key, value]` array per entry.
    ///
    /// Also what `m[Symbol.iterator]` is, and the SAME function object rather
    /// than one that agrees: [`super::register_map`] installs the second name.
    fn entries(this: u64) -> u64 {
        super::cursor::over(this, super::cursor::Kind::Entries)
    }

    /// `Map.groupBy(items, cb)` — ES2024.
    ///
    /// The buckets come out in first-seen key order, which is what the
    /// specification says and what falls out of the table preserving insertion
    /// order for free.
    ///
    /// # Why the map is built entry by entry rather than collected first
    ///
    /// Because the bucket a key belongs to is decided by SameValueZero, and the
    /// table is the one thing that already decides it. Collecting into a `Vec`
    /// of pairs first would mean a second answer to "have I seen this key" —
    /// written differently, as this crate's rule 2 keeps observing.
    #[stat]
    fn group_by(items: u64, callback: u64) -> u64 {
        let values = super::elements_of(items);
        let absent = super::undefined();
        let made = with_current(|context| super::fresh(context, "Map"));
        let Some(cell) = Value(made).as_slot() else {
            return made;
        };
        for (index, item) in values.into_iter().enumerate() {
            let at = Value::from_f64(index as f64).bits();
            // No borrow held: the callback is user code.
            let Some(key) = super::invoke(callback, absent, item, at, absent) else {
                break;
            };
            let bucket = with_current(|context| {
                context
                    .table_at(cell)
                    .and_then(|table| table.get(context, key))
            });
            match bucket {
                Some(bucket) => {
                    super::append(bucket, item);
                }
                None => {
                    let bucket = super::array_of(vec![item]);
                    with_current(|context| {
                        if let Some(mut table) = super::taken(context, cell) {
                            table.set(context, key, bucket);
                            super::restore_sized(context, cell, table);
                        }
                    });
                }
            }
        }
        made
    }
}

/// Whether a collection holds a key, from inside a borrow that already exists.
///
/// Shared with [`super::set`], which asks exactly the same question of exactly
/// the same table — the difference between a `Map` and a `Set` is what is stored
/// beside the key, not how the key is found.
pub(super) fn held(context: &Context, collection: u64, key: u64) -> bool {
    Value(collection)
        .as_slot()
        .and_then(|cell| context.table_at(cell))
        .is_some_and(|table| table.has(context, key))
}
