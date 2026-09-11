//! The argument protocol the seven set operations share, and the assembly of
//! their results.
//!
//! Its own module because it is one rule with seven callers and `set.rs` is the
//! file that would otherwise hold both the class and the protocol: `GetSetRecord`
//! reads `size`, `has` and `keys` off the argument BEFORE any member is walked,
//! so `a.union([1, 2])` throws without a partial result existing anywhere, and
//! the refusal is one place instead of seven.
//!
//! What each operation still decides for itself is which side is walked and
//! which is asked — `symmetricDifference` asks the argument for nothing but its
//! `keys`, where `union` asks for its members and `isSubsetOf` for its `has`.
//! That difference is in `set.rs`, beside the method it belongs to.

use super::with_current;
use crate::value::Value;

/// The members of a set, in insertion order.
pub(super) fn members(collection: u64) -> Vec<u64> {
    with_current(|context| {
        Value(collection)
            .as_slot()
            .and_then(|cell| context.table_at(cell))
            .map(|table| table.keys().to_vec())
            .unwrap_or_default()
    })
}

/// Whether the receiver holds a value, by its own table.
pub(super) fn held_by(collection: u64, value: u64) -> bool {
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
pub(super) struct Other {
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
    pub(super) fn members(&self) -> Vec<u64> {
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
        // Driven by `next`, not spread as an iterable. `keys()` answers an
        // ITERATOR, and an iterator written by hand almost never declares a
        // `Symbol.iterator` of its own — `elements_of` went through `iterate`,
        // found none, and refused every set-like in the corpus with "the value
        // is not iterable".
        crate::entry::iterate::drained(iterator).unwrap_or_default()
    }

    /// Whether it holds a value.
    pub(super) fn holds(&self, value: u64) -> bool {
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
pub(super) fn other_of(this: u64, other: u64) -> Option<Other> {
    // The RECEIVER first, which is the order the specification states and one a
    // program can watch: every set operation begins with
    // `RequireInternalSlot(O, [[SetData]])`, so `Set.prototype.union.call({}, x)`
    // throws before `x`'s `size` getter runs at all.
    super::branded(this, super::Brand::Set)?;
    // The only question this crate can answer without running anything: is it an
    // object at all, and does it carry a table of its own. A string has a cell
    // and is not an object, and a set operation over one is the same refusal an
    // array gets.
    let table = with_current(|context| match Value(other).as_slot() {
        Some(cell) if crate::entry::primitive::is_object_in(context, other) => {
            Some(context.table_at(cell).map(|_| cell))
        }
        _ => None,
    });
    let read = read_record(other, table);
    if read.is_none() && !crate::entry::throw::in_flight() {
        crate::entry::throw::type_error(
            "a set operation takes a Set or a set-like object: one with a numeric \
             `size` and callable `has` and `keys`",
        );
    }
    read
}

/// `GetSetRecord` proper, with **no borrow held**.
///
/// `size`, `has` and `keys` are read in that order and through the crate's one
/// property read, so an ACCESSOR runs. That is not a nicety: `size` is a
/// prototype accessor on this engine's own `Set` — see `super::sized` — and a
/// set-like written by hand almost always spells it `get size()`, which is what
/// the fixture corpus does. Reading slots instead reported every such object as
/// size-less and refused `a.union(setLike)` outright.
///
/// `ToNumber` over `size`, which is what the specification runs. The previous
/// spelling required the property to already BE a number because it read under a
/// borrow and coercion is user code; splitting the borrow is what removes that
/// refusal rather than documenting it.
///
/// Rule 8 at every step: each of these four reads may run user code, and a
/// throw left behind means the record was never obtained — the `None` here is
/// the caller's signal to stop, and `other_of` does not turn it into a second,
/// wrong `TypeError`.
pub(super) fn read_record(other: u64, table: Option<Option<u32>>) -> Option<Other> {
    let table = table?;
    let size = match table {
        // A real Map or Set answers from its TABLE, which is also what the
        // getter itself does, so the two cannot come apart.
        Some(cell) => with_current(|context| context.table_at(cell).map(|t| t.len() as f64))?,
        None => {
            let raw = read_member(other, "size");
            if crate::entry::throw::in_flight() {
                return None;
            }
            let size = crate::entry::class_support::to_number(raw);
            if crate::entry::throw::in_flight() {
                return None;
            }
            size
        }
    };
    if size.is_nan() {
        return None;
    }
    // A NEGATIVE size is a `RangeError`, not a `TypeError`, and the
    // specification is explicit about which: `GetSetRecord` runs
    // `ToIntegerOrInfinity` and then refuses anything below zero by range. The
    // distinction is program-visible — a fixture catches one and prints the
    // constructor's name — and answering `TypeError` for both would have made
    // `{ size: -1 }` indistinguishable from `{}`.
    if size < 0.0 {
        crate::entry::throw::range_error("a set-like object's `size` cannot be negative");
        return None;
    }
    let has = callable_member(other, "has")?;
    let keys = callable_member(other, "keys")?;
    Some(Other {
        object: other,
        has,
        keys,
        table,
    })
}

/// One property of a value by name, **getters run**.
///
/// Through `objects::get_property` — the crate's one property read, and the
/// reason `super::super::promise::drain` reaches for it too: a `size` behind an
/// inherited getter or a proxy trap answers here the way it answers everywhere
/// else in the program. No borrow is held across it, which is what lets it run
/// user code at all.
pub(super) fn read_member(object: u64, name: &str) -> u64 {
    let key = with_current(|context| {
        let key = context.well_known(name);
        crate::entry::objects::machine_key(key).map(|key| key.index() as i64)
    });
    match key {
        Some(key) => crate::entry::objects::get_property(object, key),
        None => super::undefined(),
    }
}

/// One CALLABLE member of an object, by name, **getters run**.
pub(super) fn callable_member(object: u64, name: &str) -> Option<u64> {
    let found = read_member(object, name);
    if crate::entry::throw::in_flight() {
        return None;
    }
    with_current(|context| {
        let slot = Value(found).as_slot()?;
        context.callable_at(slot)?;
        Some(found)
    })
}

/// The members of `source` that `other` does or does not hold.
///
/// One function for five operations, because they differ only in which answer
/// keeps a member — and five copies of "walk this table, ask that one" is where
/// they would come to disagree about a `NaN` member.
///
/// ROOTED: `other.holds` may be a call into user code, which allocates, and what
/// has been kept so far is named only by a `Vec` on the Rust heap.
pub(super) fn kept(source: u64, other: &Other, wanted: bool) -> Vec<u64> {
    let mut held = crate::entry::rooted::Rooted::new();
    for value in members(source) {
        if other.holds(value) == wanted {
            held.values().push(value);
        }
    }
    held.take()
}

/// A new `Set` holding each of `mine`, then each of `theirs` TOGGLED into it:
/// present means remove, absent means append.
///
/// Written as a toggle over one table rather than as two filtered lists,
/// because two lists need a membership test for the argument's members and the
/// only ones available are its `has` — which the specification does not call
/// here — or a second equality written beside [`super::table::Table`]'s. The
/// table is the one place that decides what a duplicate is, and asking it twice
/// is cheaper than answering the question a second way.
pub(super) fn toggled(mine: Vec<u64>, theirs: Vec<u64>) -> u64 {
    // Both lists are values named only by a `Vec` on the Rust heap, and
    // `fresh` allocates — rule 10 of the crate's README.
    let mine = crate::entry::rooted::Rooted::with(mine);
    let theirs = crate::entry::rooted::Rooted::with(theirs);
    with_current(|context| {
        let made = super::fresh(context, "Set");
        let Some(cell) = Value(made).as_slot() else {
            return made;
        };
        if let Some(mut table) = super::taken(context, cell) {
            for value in mine.as_slice() {
                let value = super::table::canonical(*value);
                table.set(context, value, value);
            }
            for value in theirs.as_slice() {
                let value = super::table::canonical(*value);
                match table.has(context, value) {
                    true => {
                        table.remove(context, value);
                    }
                    false => table.set(context, value, value),
                }
            }
            super::restore_sized(context, cell, table);
        }
        made
    })
}

/// A new `Set` over these values, duplicates dropped.
pub(super) fn assembled(values: Vec<u64>) -> u64 {
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
