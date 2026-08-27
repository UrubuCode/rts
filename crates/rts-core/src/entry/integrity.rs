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
//! # Per-property, and per-object, in one file
//!
//! `Object.freeze` says "refuses" of every key at once; `writable: false` says
//! it of one. They are the same refusal at two granularities, so both are here
//! and every question is asked of the pair — see [`refuses_key_write`].
//!
//! An attribute is per OBJECT per property, never per layout: a shape is shared
//! by every object built the same way, and hiding `x` on one must not hide it on
//! all of them.

use super::{Context, with_current};
use crate::value::Value;
use rts_cranelift::shape::Key as ShapeKey;

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

/// What one property permits, when it does not permit everything.
///
/// # Why beside the cell rather than in the shape
///
/// Because an attribute is per OBJECT per property, not per layout:
/// `Object.defineProperty(o, "x", {enumerable: false})` says nothing about the
/// other objects that share `o`'s shape. Recording it in the tree would either
/// hide `x` on all of them or fork the shape — and forking on a fact the
/// compiler never emits a guard for buys nothing and costs every site that had
/// warmed up on the original.
///
/// So this is the accessor table's shape, for the accessor table's reason: what
/// is true of one cell's one key lives beside that cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::entry) struct Attributes {
    /// Whether a store lands.
    pub writable: bool,
    /// Whether `Object.keys` and `for-in` report it.
    pub enumerable: bool,
    /// Whether `delete` removes it.
    pub configurable: bool,
}

impl Default for Attributes {
    /// What a property a program wrote has: all three.
    ///
    /// Which is why only DEVIATIONS are recorded — an object whose properties
    /// were all written the ordinary way has no entry here at all, and the
    /// common case pays nothing.
    fn default() -> Self {
        Attributes {
            writable: true,
            enumerable: true,
            configurable: true,
        }
    }
}

impl Context {
    /// What one key of one cell permits.
    pub(in crate::entry) fn attributes_at(&self, cell: u32, key: ShapeKey) -> Attributes {
        self.attributes
            .get(cell)
            .and_then(|held| held.iter().find(|(at, _)| *at == key))
            .map_or_else(
                || self.implied_attributes(cell, key),
                |(_, attributes)| *attributes,
            )
    }

    /// What a key permits when nothing was recorded for it.
    ///
    /// [`Attributes::default`] for everything a program wrote, and one
    /// exception: an ARRAY's `length` is `{writable, !enumerable,
    /// !configurable}` for every array there is, without exception and from the
    /// moment it exists.
    ///
    /// # Why the exception is here and not a record per array
    ///
    /// It was a record per array — `array::set_length` called
    /// `integrity::set_attributes` on every array it built, which for a fresh
    /// cell is the one call in that function that genuinely allocates: a `Vec`
    /// for the cell's first attribute entry, plus the `Aside` growth to hold
    /// it. Measured 2026-08-26 by ablating the two halves of `set_length`
    /// independently in one binary: `[]` cost **136.6 ns**, of which the
    /// `objects::put` was 24 and **the attribute record was 84**. A four-element
    /// literal was 204.9 and an empty array without the record is 52.4 — below
    /// `new C()` at 76.
    ///
    /// The record was never carrying information: it is the same three flags
    /// for every array, derivable from the one fact the cell already stores —
    /// that `array_elements` names it. So this is not a second source of truth
    /// replacing a first; it is the first one, read where it lives instead of
    /// copied per cell.
    ///
    /// A program that *changes* it still gets a record, and the record still
    /// wins: `Object.defineProperty(a, "length", {writable: false})` reaches
    /// [`set_attributes`], and the `find` above answers before this does. That
    /// is also why `set_length` no longer reads `writable` back before writing
    /// it — there is nothing left to undo.
    ///
    /// The alternative was to keep the write and make it cheaper (size the
    /// `Vec` at one, or hold the flags in a bitfield beside the cell). Rejected:
    /// both keep a per-cell copy of a per-kind constant, so both still pay the
    /// `Aside` and both can still drift from the language.
    fn implied_attributes(&self, cell: u32, key: ShapeKey) -> Attributes {
        // The key compare first, and the cell lookup only when it matches: this
        // function is on the miss path of every property write in the program,
        // and `array_elements` is an `Aside` probe.
        if self.well_known_keys[super::LENGTH_KEY_AT] == Some(crate::object::Key::Name(key))
            && self.array_elements.copied(cell).is_some()
        {
            return Attributes {
                writable: true,
                enumerable: false,
                configurable: false,
            };
        }
        Attributes::default()
    }

    /// Whether this cell has an explicit descriptor for this key.
    pub(in crate::entry) fn has_attributes(&self, cell: u32, key: ShapeKey) -> bool {
        self.attributes
            .get(cell)
            .is_some_and(|held| held.iter().any(|(at, _)| *at == key))
    }
}

/// What a key permits once the OBJECT's own refusals are folded in.
///
/// The three questions below answer one each, and every caller that wants the
/// whole descriptor was asking all three in the same shape — `describe` to build
/// one, `define` to validate against one. Stated once so the two cannot come to
/// different conclusions about a sealed object's `configurable`, which is the
/// one flag integrity changes without the property's own record moving.
pub(in crate::entry) fn effective(context: &Context, cell: u32, key: ShapeKey) -> Attributes {
    Attributes {
        writable: !refuses_key_write(context, cell, key),
        enumerable: enumerable(context, cell, key),
        configurable: !refuses_key_removal(context, cell, key),
    }
}

/// Forgets what a key permits, back to what a written property has.
///
/// `defineProperty` needs it: a redefinition stores the new value through the
/// ordinary write path, and that path refuses a key recorded non-writable — so
/// a descriptor changing `{writable: false, value: 1}` into `{value: 2}` would
/// be refused by the record it is replacing. Clearing first and recording after
/// is the order `define` already had to use for the value; this is the same
/// order applied to a key that already had a record.
///
/// The alternative — a write that bypasses the refusal — was rejected because
/// it would be a second store path with a second answer to what a frozen
/// object is.
pub(in crate::entry) fn clear_attributes(context: &mut Context, cell: u32, key: ShapeKey) {
    let Some(held) = context.attributes.get(cell) else {
        return;
    };
    let kept: Vec<(ShapeKey, Attributes)> =
        held.iter().filter(|(at, _)| *at != key).copied().collect();
    context.attributes.set(cell, kept);
}

/// Records what a key permits, and makes a non-writable one stick.
///
/// The retype is the same mechanism `freeze` needs and for the same reason: a
/// site that had warmed up writes at a remembered offset without asking, so the
/// only way to stop it is to stop it recognising the object.
/// Records what SEVERAL keys of one cell permit, reaching the table once.
///
/// # Why a second entry point rather than a loop over [`set_attributes`]
///
/// Because the cost is the reach, not the write. `context.attributes` is an
/// `Aside`, a `Vec<Option<T>>` indexed by cell, and reaching a cell it has never
/// held anything for GROWS it — so a caller recording two keys on a fresh cell
/// paid that twice for one cell.
///
/// `closure_new` is the caller that made it worth having: it records `name` and
/// `length` on every callable, and `prototype` and `constructor` on every
/// constructible one, which is up to four reaches per closure on a path that
/// runs once per CLOSURE rather than once per function.
///
/// ONE `Attributes` for every key rather than a pair per key, and the keys are
/// taken as they already are — `crate::object::Key`, filtered here. Building a
/// `Vec` of `(key, attributes)` for the caller to hand over was the first
/// spelling and it was slower than the two calls it replaced, because that
/// collection allocated once per closure. Measured 2026-08-25: 282 ns against
/// 253. Every caller wants one set of attributes for a run of keys anyway.
///
/// `Vec::with_capacity(4)` and not `records.len()`, for the reason
/// [`set_attributes`] states below: `RawVec`'s first block for an element this
/// size is four, so asking for two would make a cell that later receives a
/// third reallocate where it used to have room.
pub(in crate::entry) fn set_attributes_many(
    context: &mut Context,
    cell: u32,
    keys: &[crate::object::Key],
    attributes: Attributes,
) {
    let named = keys.iter().filter_map(|key| match key {
        crate::object::Key::Name(named) => Some(*named),
        crate::object::Key::Index(_) => None,
    });
    match context.attributes.get_mut(cell) {
        Some(held) => {
            for key in named {
                match held.iter_mut().find(|(at, _)| *at == key) {
                    Some((_, existing)) => *existing = attributes,
                    None => held.push((key, attributes)),
                }
            }
        }
        None => {
            let mut fresh = Vec::with_capacity(4);
            fresh.extend(named.map(|key| (key, attributes)));
            context.attributes.set(cell, fresh);
        }
    }
}

pub(in crate::entry) fn set_attributes(
    context: &mut Context,
    cell: u32,
    key: ShapeKey,
    attributes: Attributes,
) {
    // Written THROUGH the entry rather than cloned out, edited and put back.
    // The old spelling — `get(cell).cloned().unwrap_or_default()` … `set(cell,
    // held)` — allocated a fresh `Vec` and freed the previous one on every
    // call, including the overwhelmingly common one where the cell already has
    // a record and only one field of it changes.
    //
    // It is called more than its name suggests: four times per `closure_new`
    // (`prototype`, `constructor`, `name`, `length`), once per array literal
    // through `array::set_length`, once per built-in method installed, and
    // twice per `Object.defineProperty` — which calls `clear_attributes` first,
    // and that one still rebuilds, because removing from the middle is what it
    // is for.
    match context.attributes.get_mut(cell) {
        Some(held) => match held.iter_mut().find(|(at, _)| *at == key) {
            Some((_, existing)) => *existing = attributes,
            None => held.push((key, attributes)),
        },
        // The first record for this cell, which is the one call that genuinely
        // has to allocate.
        //
        // `Vec::new()` then `push`, and NOT `vec![(key, attributes)]`, which is
        // what this said first. The macro sizes the buffer at exactly one;
        // `push` onto an empty `Vec` asks `RawVec` for its first block, which
        // for an element this size is four. The old spelling —
        // `unwrap_or_default()` then `push` — took the second path, so writing
        // the macro here would have made every cell that later receives a
        // SECOND attribute reallocate where it used to have room.
        //
        // Found by measuring rather than by reading: with the macro, `prop
        // instanceof` moved from 198.5 to 217.3 ns — 9.5%, with the two runs'
        // ranges not overlapping across five runs each — on a path that does no
        // attribute write at all, and in a program where `RTS_GC_DEBUG=1`
        // reports no collection. The class prototypes are built once at startup
        // and every method installed on them lands here; the reallocation moved
        // them, and the loop read the result.
        None => {
            let mut fresh = Vec::new();
            fresh.push((key, attributes));
            context.attributes.set(cell, fresh);
        }
    }
    if !attributes.writable {
        retype(context, cell);
    }
}

/// Whether a store to this cell is refused, whatever the key.
///
/// Read by [`super::objects::put`], which is the one funnel every named and
/// computed write passes through — so this is asked once rather than at each
/// spelling of an assignment.
pub(in crate::entry) fn refuses_write(context: &Context, cell: u32) -> bool {
    context.integrity_at(cell) == Some(Integrity::Frozen)
}

/// Whether a store to this key of this cell is refused.
///
/// The object's own answer OR the property's, because either alone is a way to
/// refuse: `Object.freeze` says it of every key at once and `writable: false`
/// says it of one.
pub(in crate::entry) fn refuses_key_write(context: &Context, cell: u32, key: ShapeKey) -> bool {
    refuses_write(context, cell) || !context.attributes_at(cell, key).writable
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

/// Whether removing this key of this cell is refused.
pub(in crate::entry) fn refuses_key_removal(context: &Context, cell: u32, key: ShapeKey) -> bool {
    refuses_removal(context, cell) || !context.attributes_at(cell, key).configurable
}

/// Whether this key of this cell is reported by an enumeration.
///
/// The one attribute integrity says nothing about: freezing an object does not
/// hide its properties, it stops them changing.
pub(in crate::entry) fn enumerable(context: &Context, cell: u32, key: ShapeKey) -> bool {
    context.attributes_at(cell, key).enumerable
}

/// Applies a level, keeping the strictest one the object has been given.
///
/// Strictest rather than latest, because the operations are one-way in the
/// language: `Object.preventExtensions` on a frozen object must not thaw it.
pub(in crate::entry) fn restrict(object: u64, level: Integrity) -> u64 {
    let refused = with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            // A primitive is already unchangeable, and the language answers it
            // unchanged rather than throwing.
            return false;
        };
        // A typed array's elements cannot be made non-configurable, so sealing
        // or freezing one that HAS elements is a refusal rather than a stronger
        // level. The language reports it as a throw because `SetIntegrityLevel`
        // defines every step past `[[PreventExtensions]]` as
        // `DefinePropertyOrThrow` — so the extension ban lands and the rest
        // does not, which is why `Closed` is recorded before the raise instead
        // of the level that was asked for.
        if level >= Integrity::Sealed && indexed_elements(context, cell) > 0 {
            context.integrity.set(cell, Integrity::Closed);
            return true;
        }
        let reached = context.integrity_at(cell).map_or(level, |held| held.max(level));
        context.integrity.set(cell, reached);
        if reached == Integrity::Frozen {
            retype(context, cell);
        }
        false
    });
    if refused {
        super::throw::type_error("Cannot redefine property: 0");
    }
    object
}

/// How many elements a cell holds that no shape records and no `delete` reaches.
///
/// A typed array's, and only a typed array's: a `DataView` is a view too and has
/// none — it decides a width per call rather than exposing indices — which is
/// why the kind is asked rather than the byte length.
fn indexed_elements(context: &Context, cell: u32) -> usize {
    match context.view_at(cell) {
        Some(view) if view.kind != super::buffers::element::Kind::Raw => view.count(),
        _ => 0,
    }
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
pub(in crate::entry) fn retype(context: &mut Context, cell: u32) {
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

#[cfg(test)]
mod tests {
    /// `LENGTH_KEY_AT` indexes `CACHED_KEYS` by a number, and
    /// [`super::Context::implied_attributes`] reads
    /// `well_known_keys[LENGTH_KEY_AT]` on the miss path of every property
    /// write. Reordering `CACHED_KEYS` would make it compare against
    /// `"prototype"` instead — so every array's `length` would enumerate and
    /// every function's `prototype` would not, with nothing failing to compile.
    #[test]
    fn length_is_first() {
        assert_eq!(crate::entry::CACHED_KEYS[crate::entry::LENGTH_KEY_AT], "length");
    }
}
