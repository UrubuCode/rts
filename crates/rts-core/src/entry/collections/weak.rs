//! `WeakMap` and `WeakSet` — the surface, with none of the weakness.
//!
//! These hold their keys **strongly**. What makes the real ones weak is a
//! reference that can be observed to have died, which is the `(slot,
//! generation)` pair `PLAN.md` phase C1 describes: a handle whose generation no
//! longer matches names a cell that is gone, and that is the only honest way to
//! answer `has` false for something the program dropped. Until it exists, an
//! entry here lives as long as the collection does — which is a leak, and the
//! same leak every other collection in this engine has while nothing collects.
//!
//! # Why they exist at all before that
//!
//! Because a program that uses one as a side table — associating data with an
//! object without writing a property onto it — gets the right answers today. The
//! only observable difference is memory, and the alternative is a name that does
//! not resolve, which is a program that does not compile rather than a program
//! that holds more than it needs.
//!
//! # Why there is no `size` and no iteration
//!
//! Not an omission: the language deliberately withholds them, because both would
//! expose *when* the collector ran. Adding them because they are easy here would
//! be writing a different class with the same name.
//!
//! # Why the lookup is a linear scan
//!
//! The key is a reference, and every reference hashes to one bucket — see
//! [`super::table`] for why object identity is not hashable while the heap is
//! heading for a moving collector. So an index over these would be one chain
//! holding everything, which is the scan with a vector's worth of overhead on
//! top.

use super::{Context, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `WeakMap`.
#[rtse::class("WeakMap", tag)]
impl WeakMap {
    /// `new WeakMap(iterable?)` — an array of `[key, value]` pairs.
    #[construct]
    fn build(this: u64, iterable: u64) -> u64 {
        let absent = super::undefined();
        let pairs = match iterable == absent {
            true => Vec::new(),
            false => super::pairs_of(iterable),
        };
        with_current(|context| {
            let Some(cell) = super::built(context, this, "WeakMap") else {
                return undefined_of(context);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return undefined_of(context);
            };
            for (key, value) in pairs {
                write(context, &mut table, key, value);
            }
            super::restore(context, cell, table);
            Value::from_slot(cell).bits()
        })
    }

    /// `w.get(k)`.
    fn get(this: u64, key: u64) -> u64 {
        with_current(|context| {
            let absent = undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let Some(table) = context.table_at(cell) else {
                return absent;
            };
            match table.identical(context, key) {
                Some(at) => table.value_at(at),
                None => absent,
            }
        })
    }

    /// `w.set(k, v)` — the map, so that writes chain.
    ///
    /// A primitive key is **refused**, silently. The language throws a
    /// `TypeError`, which is the one thing this layer cannot do where a handler
    /// could catch it — and refusing is the less wrong of the two answers, since
    /// storing a primitive would make a collection whose whole contract is
    /// "keyed by object identity" hold something with none.
    fn set(this: u64, key: u64, value: u64) -> u64 {
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot()
                && let Some(mut table) = super::taken(context, cell)
            {
                write(context, &mut table, key, value);
                super::restore(context, cell, table);
            }
            this
        })
    }

    /// `w.has(k)`.
    fn has(this: u64, key: u64) -> bool {
        with_current(|context| found(context, this, key).is_some())
    }

    /// `w.delete(k)`.
    #[js("delete")]
    fn remove(this: u64, key: u64) -> bool {
        with_current(|context| dropped(context, this, key))
    }
}

/// `WeakSet`.
#[rtse::class("WeakSet", tag)]
impl WeakSet {
    /// `new WeakSet(iterable?)`.
    #[construct]
    fn build(this: u64, iterable: u64) -> u64 {
        let absent = super::undefined();
        let values = match iterable == absent {
            true => Vec::new(),
            false => super::elements_of(iterable),
        };
        with_current(|context| {
            let Some(cell) = super::built(context, this, "WeakSet") else {
                return undefined_of(context);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return undefined_of(context);
            };
            for value in values {
                write(context, &mut table, value, value);
            }
            super::restore(context, cell, table);
            Value::from_slot(cell).bits()
        })
    }

    /// `w.add(v)` — the set. A primitive is refused, as in `WeakMap.set`.
    fn add(this: u64, value: u64) -> u64 {
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot()
                && let Some(mut table) = super::taken(context, cell)
            {
                write(context, &mut table, value, value);
                super::restore(context, cell, table);
            }
            this
        })
    }

    /// `w.has(v)`.
    fn has(this: u64, value: u64) -> bool {
        with_current(|context| found(context, this, value).is_some())
    }

    /// `w.delete(v)`.
    #[js("delete")]
    fn remove(this: u64, value: u64) -> bool {
        with_current(|context| dropped(context, this, value))
    }
}

/// Writes an entry, if the key is something a weak collection may hold.
///
/// One function for both classes and for the constructor as well, because "a
/// weak key is a reference" is one rule — and this crate keeps refusing to write
/// a rule twice, on the grounds that the second copy is where the two come to
/// disagree.
fn write(context: &Context, table: &mut super::Table, key: u64, value: u64) {
    if Value(key).as_slot().is_none() {
        return;
    }
    match table.identical(context, key) {
        Some(at) => table.set_value_at(at, value),
        None => table.push_unindexed(key, value),
    }
}

/// Where a key is in a weak collection, from inside a borrow that exists.
fn found(context: &Context, collection: u64, key: u64) -> Option<usize> {
    let cell = Value(collection).as_slot()?;
    context.table_at(cell)?.identical(context, key)
}

/// Removes a key, answering whether it was there.
fn dropped(context: &mut Context, collection: u64, key: u64) -> bool {
    let Some(at) = found(context, collection, key) else {
        return false;
    };
    let Some(cell) = Value(collection).as_slot() else {
        return false;
    };
    let Some(mut table) = super::taken(context, cell) else {
        return false;
    };
    table.remove_at(at);
    super::restore(context, cell, table);
    true
}
