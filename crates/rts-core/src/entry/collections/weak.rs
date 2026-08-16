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
use crate::entry::{primitive, throw};
use crate::value::Value;

/// What a weak collection says when handed a key it cannot hold.
///
/// One string for four entry points, because it is one rule — the same reason
/// [`write`] is one function.
const NOT_A_KEY: &str =
    "a WeakMap key and a WeakSet member must be an object or an unregistered symbol";

/// `WeakMap`.
#[rtse::class("WeakMap", tag)]
impl WeakMap {
    /// `new WeakMap(iterable?)` — an array of `[key, value]` pairs.
    ///
    /// A pair whose key is not an object stops the walk and raises, as `set`
    /// does: the entries before it are already written, which is what the
    /// language's own loop leaves behind too.
    #[construct]
    /// Arity 0, not 1: the specification pins `Map.length` at zero because
    /// the iterable is optional in the way `length` counts.
    #[arity(0)]
    fn build(this: u64, iterable: u64) -> u64 {
        if !super::requires_new(this, "WeakMap") {
            return super::undefined();
        }
        let pairs = match super::nothing_to_fill_from(iterable) {
            true => Vec::new(),
            false => super::pairs_of(iterable),
        };
        let (made, refused) = with_current(|context| {
            let Some(cell) = super::built(context, this, "WeakMap") else {
                return (undefined_of(context), false);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return (undefined_of(context), false);
            };
            let mut refused = false;
            for (key, value) in pairs {
                if !write(context, &mut table, key, value) {
                    refused = true;
                    break;
                }
            }
            super::restore(context, cell, table);
            (Value::from_slot(cell).bits(), refused)
        });
        if refused {
            throw::type_error(NOT_A_KEY);
        }
        made
    }

    /// `w.get(k)`.
    fn get(this: u64, key: u64) -> u64 {
        let Some(cell) = super::branded(this, super::Brand::WeakMap) else {
            return super::undefined();
        };
        with_current(|context| {
            let absent = undefined_of(context);
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
    /// A primitive key is a `TypeError`, which is what the language says and
    /// what this could not do until a native could raise one a `catch` sees. It
    /// was a silent refusal, and the silence was the worse half: a program that
    /// wrote `wm.set(id, x)` with a numeric id got a map that never held
    /// anything and never said so.
    ///
    /// The divergence that remains, named: ES2023 also admits an unregistered
    /// SYMBOL as a weak key, and this does not. A symbol here is not a cell, so
    /// admitting it means a second notion of identity beside the one
    /// [`super::Table::identical`] has, which is a change about identity rather
    /// than about this refusal.
    fn set(this: u64, key: u64, value: u64) -> u64 {
        // The receiver's brand BEFORE the key's kind, which is the order
        // `RequireInternalSlot` puts it in.
        let Some(cell) = super::branded(this, super::Brand::WeakMap) else {
            return super::undefined();
        };
        if !keyed(key) {
            throw::type_error(NOT_A_KEY);
            return this;
        }
        with_current(|context| {
            if let Some(mut table) = super::taken(context, cell) {
                write(context, &mut table, key, value);
                super::restore(context, cell, table);
            }
            this
        })
    }

    /// `w.has(k)`.
    fn has(this: u64, key: u64) -> bool {
        if super::branded(this, super::Brand::WeakMap).is_none() {
            return false;
        }
        with_current(|context| found(context, this, key).is_some())
    }

    /// `w.delete(k)`.
    #[js("delete")]
    fn remove(this: u64, key: u64) -> bool {
        if super::branded(this, super::Brand::WeakMap).is_none() {
            return false;
        }
        with_current(|context| dropped(context, this, key))
    }
}

/// `WeakSet`.
#[rtse::class("WeakSet", tag)]
impl WeakSet {
    /// `new WeakSet(iterable?)` — a member that is not an object raises, as in
    /// `WeakMap`'s constructor.
    #[construct]
    /// Arity 0, not 1: the specification pins `Map.length` at zero because
    /// the iterable is optional in the way `length` counts.
    #[arity(0)]
    fn build(this: u64, iterable: u64) -> u64 {
        if !super::requires_new(this, "WeakSet") {
            return super::undefined();
        }
        let values = match super::nothing_to_fill_from(iterable) {
            true => Vec::new(),
            false => super::elements_of(iterable),
        };
        let (made, refused) = with_current(|context| {
            let Some(cell) = super::built(context, this, "WeakSet") else {
                return (undefined_of(context), false);
            };
            let Some(mut table) = super::taken(context, cell) else {
                return (undefined_of(context), false);
            };
            let mut refused = false;
            for value in values {
                if !write(context, &mut table, value, value) {
                    refused = true;
                    break;
                }
            }
            super::restore(context, cell, table);
            (Value::from_slot(cell).bits(), refused)
        });
        if refused {
            throw::type_error(NOT_A_KEY);
        }
        made
    }

    /// `w.add(v)` — the set. A primitive is a `TypeError`, as in `WeakMap.set`.
    fn add(this: u64, value: u64) -> u64 {
        let Some(cell) = super::branded(this, super::Brand::WeakSet) else {
            return super::undefined();
        };
        if !keyed(value) {
            throw::type_error(NOT_A_KEY);
            return this;
        }
        with_current(|context| {
            if let Some(mut table) = super::taken(context, cell) {
                write(context, &mut table, value, value);
                super::restore(context, cell, table);
            }
            this
        })
    }

    /// `w.has(v)`.
    fn has(this: u64, value: u64) -> bool {
        if super::branded(this, super::Brand::WeakSet).is_none() {
            return false;
        }
        with_current(|context| found(context, this, value).is_some())
    }

    /// `w.delete(v)`.
    #[js("delete")]
    fn remove(this: u64, value: u64) -> bool {
        if super::branded(this, super::Brand::WeakSet).is_none() {
            return false;
        }
        with_current(|context| dropped(context, this, value))
    }
}

/// Writes an entry, if the key is something a weak collection may hold.
///
/// One function for both classes and for the constructor as well, because "a
/// weak key is an object" is one rule — and this crate keeps refusing to write
/// a rule twice, on the grounds that the second copy is where the two come to
/// disagree.
///
/// Answers whether it wrote, so a caller can raise. The test was `as_slot`,
/// which is every REFERENCE — and a string is one: `wm.set("x", 1)` was
/// therefore stored rather than refused, keyed by text under a collection whose
/// whole contract is identity.
fn write(context: &Context, table: &mut super::Table, key: u64, value: u64) -> bool {
    if !holdable_in(context, key) {
        return false;
    }
    match table.identical(context, key) {
        Some(at) => table.set_value_at(at, value),
        None => table.push_unindexed(key, value),
    }
    true
}

/// Whether a value may key a weak collection, from outside a borrow.
///
/// # Why the raise is not inside [`write`]
///
/// `throw::type_error` CONSTRUCTS the program's own `TypeError`, which allocates
/// and runs a constructor — so it cannot happen under the borrow every mutation
/// here holds. The question is asked first and the answer is raised after, which
/// is the shape rule 8 of `crates/rts-core/README.md` forces on every native
/// that has to report something.
fn keyed(key: u64) -> bool {
    with_current(|context| holdable_in(context, key))
}

/// Whether a value is something a weak collection may hold weakly.
///
/// An object, or a symbol the global registry does NOT hold — which is ES2023's
/// rule and the one this file's `set` used to name as a stated divergence:
/// "a symbol here is not a cell, so admitting it means a second notion of
/// identity". It does not. [`super::Table::identical`] compares with
/// `strict_equals`, and two symbols are strictly equal exactly when they are the
/// same symbol, so identity was already answered for them; what was missing was
/// only the admission.
///
/// A REGISTERED symbol stays refused, and that refusal is the language's own
/// reasoning rather than an implementation limit: `Symbol.for("k")` is reachable
/// from the registry for the whole life of the program, so it can never die, so
/// a weak reference to one could never be observed to break. Shared by
/// `WeakMap`, `WeakSet`, `WeakRef` and `FinalizationRegistry` because it is one
/// rule — the same reason [`write`] is one function.
pub(super) fn holdable_in(context: &Context, value: u64) -> bool {
    if primitive::is_object_in(context, value) {
        return true;
    }
    crate::entry::symbol::is_symbol(context, value)
        && !crate::entry::symbol::is_registered(context, value)
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
