//! `Map`, `Set`, `WeakMap`, `WeakSet` — collections keyed by a value.
//!
//! # Why the entries live beside the cell
//!
//! A collection is an **object**: `m.tag = 9` has to work, `class Mine extends
//! Map {}` has to reach `Mine.prototype`, and a program can hang whatever it
//! likes on one. So its cell carries an ordinary shape like any other object's,
//! and what it additionally *is* goes in an [`crate::heap::Aside`] keyed by
//! region index — the same decision `Context::array_elements` records, and for
//! the reason recorded there: arrays once had a reserved layout, which made
//! `a.tag = 9` a silent no-op and `a.tag` read `undefined`. A wrong program that
//! runs is worse than a refusal.
//!
//! # The collector does not know about this table
//!
//! Nothing calls `collect::mark` today, so nothing collects, and an `Aside` is
//! invisible to a tracing collector in any case. Holding keys and values here is
//! therefore safe **today** and is exactly the bet `Context::arrays` already
//! makes for array elements. The day there is a collector it has to learn about
//! this table, or every key and value in every live map is garbage the moment
//! the program's last other reference to it drops. That is the note, not a
//! mechanism: inventing a tracing hook with no collector to call it would be a
//! second design for the collector to disagree with.
//!
//! # `WeakMap` and `WeakSet` hold their keys STRONGLY
//!
//! They are the same table with identity lookup and no iteration, and nothing
//! about them is weak. Making them weak needs a reference that can be observed
//! to have died, which is the `(slot, generation)` pair `PLAN.md` phase C1
//! describes — a handle whose generation no longer matches is a key that is
//! gone. Faking it — clearing entries on some other event, or answering `has`
//! false for something still present — would be a wrong answer where the
//! present one is merely a leak, and a leak is what every collection here is
//! until there is a collector.
//!
//! # What the iteration methods answer, and what they do not
//!
//! `keys()`, `values()` and `entries()` each answer an **array**. A real
//! iterator object — one with `next()` — is still absent, and
//! [`super::iterate`] records why materialising is the shape this engine
//! walks. So `m.keys().next()` is not a function.
//!
//! What a collection IS iterable by is answered by [`iterated`]: `for-of` over a
//! `Map` or a `Set` reaches the table directly rather than through a declared
//! `Symbol.iterator`. That is not a shortcut around the protocol — the protocol
//! is there, [`super::iterate::iterate`] dispatches on it, and a program may
//! declare its own. It is that a built-in whose elements the runtime already
//! holds has nothing to gain from being asked for them through two calls per
//! element into itself.

mod map;
mod set;
mod table;
mod weak;

pub(in crate::entry) use map::register_map;
pub(in crate::entry) use set::register_set;
pub(in crate::entry) use table::Table;
pub(in crate::entry) use weak::{register_weak_map, register_weak_set};

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

impl Context {
    /// The entries a cell holds, if it is one of these collections.
    pub(super) fn table_at(&self, cell: u32) -> Option<&Table> {
        self.collections.get(cell)
    }
}

/// The table off a cell, leaving an empty one behind.
///
/// # Why it is moved out rather than borrowed
///
/// Every mutation needs the context too — hashing a string key reads its text,
/// and comparing two reads both — and `context.collections.get_mut(cell)` holds
/// the context mutably for as long as the table is in hand. Moving the five
/// vectors out is five pointer copies and leaves the context free, where the
/// alternative is a second borrow the compiler refuses.
///
/// Nothing calls out to JavaScript while the table is out, which is what makes
/// the gap invisible: a re-entrant `m.set` during a `m.set` would find an empty
/// map, and the only way to get one is a callback this module never runs from
/// here.
pub(super) fn taken(context: &mut Context, cell: u32) -> Option<Table> {
    Some(std::mem::take(context.collections.get_mut(cell)?))
}

/// Puts one back.
pub(super) fn restore(context: &mut Context, cell: u32, table: Table) {
    context.collections.set(cell, table);
}

/// Puts one back and writes `size` to match.
///
/// # Why `size` is a real property and not an answer the runtime invents
///
/// The reason [`super::array::set_length`] records, which was learned the hard
/// way there: compiled code reading a property that is in the layout never asks
/// the runtime at all — it emits `cached_get` and finds the stored slot — so a
/// value only the slow path knows about is one the fast path disagrees with the
/// moment it starts working.
///
/// The divergence that leaves, named: `size` is an own **data** property where
/// the language makes it an accessor on the prototype, so `m.size = 5` stores a
/// number and the next mutation overwrites it, rather than being refused. The
/// same trade `length` on an array already makes.
pub(super) fn restore_sized(context: &mut Context, cell: u32, table: Table) {
    let size = table.len();
    restore(context, cell, table);
    let key = context.well_known("size");
    let value = Value::from_f64(size as f64).bits();
    super::objects::put(context, cell, key, value);
}

/// A fresh instance of one of these classes, empty.
///
/// The prototype comes from the class's own registration rather than from a
/// field on the context, so `new Set([…]).union(other)` answers something with
/// `Set.prototype` on it — and so the methods this module installs are the ones
/// the result answers to.
pub(super) fn fresh(context: &mut Context, class: &'static str) -> u64 {
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = super::class_support::prototype(context, class) {
        context.set_prototype(cell, prototype);
    }
    context.collections.set(cell, Table::default());
    Value::from_slot(cell).bits()
}

/// The object a constructor writes into: the one `new` made, or one made here.
///
/// # Why a plain call is not refused
///
/// `Map()` without `new` is a `TypeError` in the language, and raising one here
/// would end the program: [`super::throw`] cannot find a handler in a caller.
/// The same tolerance `Error("x")` without `new` settles on there, and it fails where
/// the program uses the result rather than at an arbitrary later point.
pub(super) fn built(context: &mut Context, this: u64, class: &'static str) -> Option<u32> {
    let cell = match Value(this).as_slot() {
        Some(cell) => cell,
        None => Value(fresh(context, class)).as_slot()?,
    };
    context.collections.set(cell, Table::default());
    Some(cell)
}

/// `undefined`, from outside a borrow.
pub(super) fn undefined() -> u64 {
    with_current(|context| undefined_of(context))
}

/// The entries of a collection, snapshotted.
///
/// A copy rather than a cursor, and that is what makes `forEach` safe to write:
/// the callback runs with no borrow held and may mutate the map it is walking.
/// The divergence, named: the specification says a `forEach` callback sees
/// entries added during the walk, and this one does not. The same snapshotting
/// trade [`super::array::own_keys`] records, and in the same direction — the one
/// a program notices least.
pub(super) fn entries_of(collection: u64) -> Vec<(u64, u64)> {
    with_current(|context| {
        Value(collection)
            .as_slot()
            .and_then(|cell| context.table_at(cell))
            .map(Table::entries)
            .unwrap_or_default()
    })
}

/// The elements an iterable yields.
///
/// Everything [`super::iterate::iterate`] walks: an array, a string, another
/// collection, or an object declaring `Symbol.iterator`. Anything else yields
/// nothing rather than throwing, which is the same stated gap `for-of` over such
/// a value has.
pub(super) fn elements_of(iterable: u64) -> Vec<u64> {
    let array = super::iterate::iterate(iterable);
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    })
}

/// The `[key, value]` pairs an iterable of two-element arrays yields.
///
/// An entry that is not an array contributes `undefined => undefined`, where the
/// language throws — the same tolerance every other refusal here settles on.
pub(super) fn pairs_of(iterable: u64) -> Vec<(u64, u64)> {
    let entries = elements_of(iterable);
    with_current(|context| {
        let absent = undefined_of(context);
        entries
            .into_iter()
            .map(|entry| {
                let held = Value(entry)
                    .as_slot()
                    .and_then(|cell| context.elements_at(cell));
                match held {
                    Some(pair) => (
                        pair.first().copied().unwrap_or(absent),
                        pair.get(1).copied().unwrap_or(absent),
                    ),
                    None => (absent, absent),
                }
            })
            .collect()
    })
}

/// What a `for-of` over a collection yields.
///
/// Two answers rather than one, because a `Map` yields a `[key, value]` array
/// per entry and building one needs the context mutably — which the borrow that
/// read the table is holding. The caller decides where that happens.
pub(in crate::entry) enum Iterated {
    /// A `Set`: its members, once each, in insertion order.
    Members(Vec<u64>),
    /// A `Map`: its entries, still as pairs.
    Pairs(Vec<(u64, u64)>),
}

/// How a cell iterates, if it is a collection that iterates at all.
///
/// # Why the prototype decides rather than the table
///
/// All four classes share one [`Table`] — [`set`] records why — so the table
/// cannot say which class made it, and `WeakMap`/`WeakSet` are not iterable at
/// all. A `Set` additionally stores each member as both key and value, so
/// "keys equal values" is true of `new Map([[1, 1]])` as well.
///
/// So the chain is walked and compared against the prototypes the registrations
/// recorded. A subclass answers, which is what makes `class Mine extends Set {}`
/// iterate; an object that merely got `Map.prototype` hung on it and has no
/// table answers `None`, because the table is asked for first.
pub(in crate::entry) fn iterated(context: &Context, cell: u32) -> Option<Iterated> {
    let table = context.table_at(cell)?;
    if inherits(context, cell, "Set") {
        return Some(Iterated::Members(table.keys().to_vec()));
    }
    if inherits(context, cell, "Map") {
        return Some(Iterated::Pairs(table.entries()));
    }
    None
}

/// Whether a class's prototype is anywhere on a cell's chain.
///
/// Bounded by the number of cells, because a chain a program built with
/// `Object.setPrototypeOf` can be a cycle and this must answer rather than hang.
fn inherits(context: &Context, cell: u32, class: &'static str) -> bool {
    let Some(wanted) = super::class_support::prototype(context, class) else {
        return false;
    };
    let mut at = context.prototype_at(cell);
    let mut steps = 0;
    while let Some(link) = at {
        if link == wanted {
            return true;
        }
        steps += 1;
        if steps > MAX_CHAIN {
            return false;
        }
        at = Value(link).as_slot().and_then(|cell| context.prototype_at(cell));
    }
    false
}

/// How far a prototype chain is walked before it is treated as a cycle.
///
/// A real chain is a handful of links; the language's own limit is the heap.
const MAX_CHAIN: usize = 1024;

/// An array holding these values.
pub(super) fn array_of(values: Vec<u64>) -> u64 {
    let array = super::array::array_new(values.len() as i64);
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = values;
        }
        array
    })
}

/// An array of two-element arrays, which is what `entries()` answers.
pub(super) fn pairs_array(pairs: Vec<(u64, u64)>) -> u64 {
    let inner: Vec<u64> = pairs
        .into_iter()
        .map(|(key, value)| array_of(vec![key, value]))
        .collect();
    array_of(inner)
}

/// Appends to an array that already exists.
pub(super) fn append(array: u64, value: u64) -> u64 {
    super::iterate::array_append(array, value)
}

/// Calls user code, with **no borrow held**.
///
/// The rule this whole module is written around: `with_current` takes a
/// `RefCell` borrow for the length of its body, a second borrow panics, and an
/// `extern "C"` frame cannot unwind — so the process aborts rather than failing
/// a test. Every caller of this collects what it needs first, drops the borrow,
/// and re-takes one to store the answer.
pub(super) fn invoke(callback: u64, receiver: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let absent = undefined();
    super::functions::call(callback, receiver, a0, a1, a2, absent)
}
