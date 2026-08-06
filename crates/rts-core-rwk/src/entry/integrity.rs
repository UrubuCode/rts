//! Objects that refuse to be changed, and the one thing that makes it stick.
//!
//! # The table alone would be a lie
//!
//! Compiled code does not ask the runtime to store a property. `cached_set`
//! compares the object's type against the one the site remembers and, when they
//! match, writes at the offset it remembers — no call, no question. So a flag
//! consulted only by the slow path freezes an object against every site that has
//! not run yet and against none of the ones that have, which is a freeze that
//! works until the loop warms up.
//!
//! Two things together make it real, and neither is enough alone:
//!
//! 1. **Freezing gives the cell a new type**, with the same layout. Every site
//!    that remembered the old one stops recognising the object and has to ask.
//!    That is what a type number is already for — a shape a site does not
//!    recognise is the mechanism, not a new one.
//! 2. **A store asks a different resolver.** `rts_cache_resolve_store` answers
//!    negative for a frozen cell where the read resolver still answers an
//!    offset, so the site cannot re-cache its way back to writing. The machine
//!    grew `RtEntry::CacheResolveStore` for this, and its documentation says why
//!    it is a second entry point rather than a flag.
//!
//! # Why the new type is a duplicate rather than a marker
//!
//! Because everything else must keep working. Reads resolve through
//! `shape_of_type`, which is pointed at the SAME shape, so a frozen object's
//! properties are at the same offsets and read at the same speed after one miss.
//! A type with no shape would have made every read of a frozen object answer
//! `undefined`, which is a wrong program that runs.
//!
//! # What is still not per-property
//!
//! `Object.defineProperty(o, "x", {writable: false})` is not this: a shape holds
//! a key, a slot and a representation, and nothing else, so there is nowhere to
//! record a flag for ONE property. Integrity here is a fact about the whole
//! object, which is exactly what `freeze`, `seal` and `preventExtensions` are —
//! and it is why those three landed and the per-property descriptor fields did
//! not.

use super::{Context, with_current};
use crate::value::Value;

/// How much a cell refuses.
///
/// Ordered by strictness, and compared that way: every level refuses what the
/// one before it refuses. An object with no entry here refuses nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(in crate::entry) enum Integrity {
    /// `Object.preventExtensions` — no new properties. The ones present may
    /// still be written and deleted.
    Closed = 0,
    /// `Object.seal` — and none removed either.
    Sealed = 1,
    /// `Object.freeze` — and none written.
    Frozen = 2,
}

impl Context {
    /// How much a cell refuses, if anything.
    pub(in crate::entry) fn integrity_at(&self, cell: u32) -> Option<Integrity> {
        self.integrity.copied(cell)
    }
}

/// Whether a store to this cell is refused.
///
/// Read by [`super::objects::put`], which is the one funnel every named and
/// computed write passes through — so this is asked once rather than at each
/// spelling of an assignment.
pub(in crate::entry) fn refuses_write(context: &Context, cell: u32) -> bool {
    context.integrity_at(cell) == Some(Integrity::Frozen)
}

/// Whether a NEW property on this cell is refused.
///
/// Every level refuses it: that is what all three of them have in common, and
/// the only thing `preventExtensions` says.
pub(in crate::entry) fn refuses_growth(context: &Context, cell: u32) -> bool {
    context.integrity_at(cell).is_some()
}

/// Whether removing a property from this cell is refused.
pub(in crate::entry) fn refuses_removal(context: &Context, cell: u32) -> bool {
    context.integrity_at(cell) >= Some(Integrity::Sealed)
}

/// Applies a level, keeping the strictest one the object has been given.
///
/// Strictest rather than latest, because the operations are one-way in the
/// language: `Object.preventExtensions` on a frozen object must not thaw it.
pub(in crate::entry) fn restrict(object: u64, level: Integrity) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            // A primitive is already unchangeable, and the language answers it
            // unchanged rather than throwing.
            return;
        };
        let reached = context.integrity_at(cell).map_or(level, |held| held.max(level));
        context.integrity.set(cell, reached);
        if reached == Integrity::Frozen {
            retype(context, cell);
        }
    });
    object
}

/// Gives a cell a fresh type with the layout it already had.
///
/// The whole point is that the NUMBER differs: every inline cache compares it,
/// so a cell that changed type is a cell every warmed-up site has to ask about
/// again. Nothing else about the object moves — same shape, same slots, same
/// offsets — which is why reads survive it.
///
/// A cell whose type has no shape (a string, a callable) is left alone: there is
/// no layout to duplicate, and nothing writes properties into one anyway.
fn retype(context: &mut Context, cell: u32) {
    let Some(ty) = context.region.type_of(cell) else {
        return;
    };
    let Some(shape) = context.shape_of(ty) else {
        return;
    };
    let fields: Vec<_> = context
        .shapes
        .properties(shape)
        .into_iter()
        .map(|(_, repr)| repr)
        .collect();
    let fresh = context.types.declare(&fields);
    context.record_shape(fresh, shape);
    context.region.set_type(cell, fresh.index() as u32);
}

/// Whether a cell answers `Object.isFrozen`.
///
/// An object with no properties at all is frozen as soon as it is closed, which
/// the specification says and which is not an edge case: `Object.freeze({})` and
/// `Object.preventExtensions({})` are indistinguishable afterwards, because
/// there is nothing left to write.
pub(in crate::entry) fn is_frozen(context: &mut Context, cell: u32) -> bool {
    match context.integrity_at(cell) {
        None => false,
        Some(Integrity::Frozen) => true,
        Some(_) => own_count(context, cell) == 0,
    }
}

/// Whether a cell answers `Object.isSealed`, by the same reasoning.
pub(in crate::entry) fn is_sealed(context: &mut Context, cell: u32) -> bool {
    match context.integrity_at(cell) {
        None => false,
        Some(Integrity::Closed) => own_count(context, cell) == 0,
        Some(_) => true,
    }
}

/// How many own properties a cell has, elements included.
///
/// Through the shape and the element table rather than through `own_keys`,
/// because that one allocates an array of interned strings to answer a question
/// about a count.
fn own_count(context: &mut Context, cell: u32) -> usize {
    let elements = context.elements_at(cell).map_or(0, Vec::len);
    let properties = context
        .region
        .type_of(cell)
        .and_then(|ty| context.shape_of(ty))
        .map_or(0, |shape| context.shapes.properties(shape).len());
    elements + properties
}
