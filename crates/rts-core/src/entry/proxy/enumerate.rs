//! `for`-`in` over a chain with a proxy in it: which traps run, and when.
//!
//! # The order, and why it is two phases
//!
//! `EnumerateObjectProperties` leaves the mechanism to the engine but pins what
//! it must call: `[[OwnPropertyKeys]]` and `[[GetPrototypeOf]]` per level, and
//! `[[GetOwnProperty]]` to decide whether a key is enumerable. Every engine a
//! program is compared with collects first — `ownKeys` then `getPrototypeOf`,
//! level by level — and asks the descriptor LAZILY, when the loop reaches the
//! key. So a handler logging its traps sees
//! `ownKeys,getPrototypeOf,getOwnPropertyDescriptor:a,…`, and no `has` at all.
//!
//! This engine asked the descriptors while collecting, then the prototype, and
//! then — through the per-pass "was it deleted?" guard the compiler emits —
//! the `has` trap once per visited key:
//! `ownKeys,gopd:a,gopd:b,getPrototypeOf,has:a,has:b`. Same keys, a different
//! program as far as the handler can tell.
//!
//! So the snapshot ([`level_keys`]) asks only `ownKeys`, and the per-pass guard
//! ([`still_enumerable`]) is `HasEnumerableProperty` — a descriptor on each
//! proxy level, walking up through `getPrototypeOf` when a level does not own
//! the key. The guard was already the right place for the question: it runs
//! when the key is reached, which is when the key's enumerability is read.

use super::{describe, is_proxy, prototype_of};
use crate::entry::{Context, chain, objects, primitives, throw, with_current};
use crate::object::Key;
use crate::value::Value;

/// One proxy level of a `for`-`in` snapshot: every STRING key its `ownKeys`
/// reports, enumerable or not — enumerability is decided per pass.
///
/// `None` when the object is not a proxy, which tells the walk to read the cell
/// as it always did. Empty when the trap threw; the caller asks
/// `throw::in_flight()`.
pub(in crate::entry) fn level_keys(object: u64) -> Option<Vec<Key>> {
    let listed = super::own_keys(object)?;
    if throw::in_flight() {
        return Some(Vec::new());
    }
    let entries = with_current(|context| {
        Value(listed)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    });
    Some(with_current(|context| {
        let mut keys = Vec::with_capacity(entries.len());
        for entry in entries {
            // A symbol never reaches a `for`-`in`, in either spelling a trap
            // answers one: the symbol VALUE, or the reserved key text of a list
            // forwarded out of the target's shape.
            let symbol = crate::entry::symbol::is_symbol(context, entry)
                || crate::entry::text::to_text(context, Value(entry))
                    .is_some_and(|text| crate::entry::symbol::is_symbol_key(&text));
            if symbol {
                continue;
            }
            if let Some(key) = crate::entry::computed::property_key(context, Value(entry)) {
                keys.push(key);
            }
        }
        keys
    }))
}

/// `HasEnumerableProperty(object, key)` for a chain with a proxy on it.
///
/// `None` when no level of the chain is a proxy, which leaves the ordinary
/// guard — a `HasProperty` walk that runs nothing — exactly as it was, so a
/// program with no proxy in its chains pays one walk of cells here.
///
/// An ORDINARY level that owns the key answers `true` without reading its
/// enumerability, because the snapshot only took that level's enumerable keys
/// and the guard's job there is the deleted-key question.
pub(in crate::entry) fn still_enumerable(key: u64, object: u64) -> Option<bool> {
    let named = with_current(|context| {
        if !context.any_proxy() {
            return None;
        }
        let cell = Value(object).as_slot()?;
        if !chain_holds_proxy(context, cell) {
            return None;
        }
        crate::entry::computed::property_key(context, Value(key))
    })?;
    let property = super::property_of(named);
    let mut level = object;
    for _ in 0..objects::CHAIN_LIMIT {
        if is_proxy(level) {
            let described = describe(level, named).unwrap_or_else(super::absent);
            if throw::in_flight() {
                return Some(false);
            }
            if described != super::absent() {
                let shown = with_current(|context| {
                    let field = context.well_known("enumerable");
                    Value(described)
                        .as_slot()
                        .and_then(|cell| objects::read_property(context, cell, field))
                        .map(|found| found.bits())
                });
                return Some(shown.is_some_and(|found| primitives::to_boolean(found)));
            }
            level = prototype_of(level).unwrap_or_else(super::absent);
        } else {
            let owned = crate::entry::object_global::describe_own(level, property);
            if owned != super::absent() {
                return Some(true);
            }
            level = chain::get_prototype(level);
        }
        if throw::in_flight() {
            return Some(false);
        }
        if !with_current(|context| objects::is_object(context, level)) {
            return Some(false);
        }
    }
    Some(false)
}

/// Whether any level from `cell` up is a proxy, read without calling anything.
///
/// The walk stops AT a proxy — what is above one is its handler's to say — so
/// this is a bounded read of cells, never a trap.
fn chain_holds_proxy(context: &mut Context, cell: u32) -> bool {
    let mut at = cell;
    for _ in 0..objects::CHAIN_LIMIT {
        if context.proxy_at(at).is_some() {
            return true;
        }
        match objects::inherited_from(context, at) {
            Some(next) => at = next,
            None => return false,
        }
    }
    false
}
