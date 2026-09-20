//! The members of a shape that a walk can serialise off the layout alone, and
//! what is remembered about a shape so the next object of it need not ask.

use super::super::super::Context;

/// The properties of an object a shape walk alone can serialise, in order.
///
/// # Why this exists beside the general path
///
/// Because the general path reaches an object through the doors a JavaScript
/// PROGRAM uses, and for a plain object every one of them is a detour.
/// `own_keys` allocates a JavaScript array on the heap and a string cell per
/// key, the loop clones that array's elements into a Rust `Vec`, and each
/// member is then read by `get_indexed`, which walks the prototype chain by a
/// key it re-derives from the text. To serialise `{a:1,…,h:8}` — forty
/// characters — that is one heap array, eight key lookups by text and eight
/// chain walks.
///
/// Measured 2026-08-25, `target/release/rts.exe`: `Object.keys` of an
/// eight-property object costs 2 023 ns and `JSON.stringify` of the same object
/// 4 046 — so producing the key list is **half of stringify**, before a single
/// character is written.
///
/// # The four refusals, and none of them is caution
///
/// Each is a case where the general path does something this cannot see:
///
/// - a **proxy** answers `ownKeys` by running a handler, so it has no shape to
///   walk;
/// - an **accessor** must run its getter, which is observable, and its position
///   in the enumeration is ranked separately (`ranked_accessors`) rather than
///   living in the layout;
/// - **elements** come first in enumeration order and are not shape properties
///   at all, so a shape walk would silently drop them;
/// - a **non-enumerable** property is skipped by `Object.keys` and by this, and
///   answering that question per key is what the general path calls
///   `integrity::enumerable` for — asked here too, so the two agree.
///
/// # What it deliberately does NOT return
///
/// The values. Only keys, which are numbers, because a JavaScript reference
/// held in a Rust `Vec` is invisible to the collector — the hazard the general
/// path's `external::hold_current` exists for, and which cost 31 wrong results
/// per 300 000 calls before it did. Each member is read inside its own borrow,
/// one at a time, exactly as the general path reads it.
///
/// # Remembered by shape, for the length of one walk
///
/// A hundred rows of one shape asked all of this a hundred times: a `Vec`, and
/// per key an attribute lookup, the key's text, a symbol test and an index
/// test. Everything but the attributes is a fact about the SHAPE — a shape
/// never changes, the tree only grows — so [`Plan`] keeps the last answer and
/// a cell of the same shape with no attributes recorded takes it whole. One
/// entry per DEPTH and not a map: the rows of a document are adjacent, and a
/// row's own children would otherwise overwrite the row's answer between one
/// row and the next.
///
/// The answer is LENT, not shared: the caller takes the depth's plan out,
/// walks with it, and puts it back. It was an `Rc` for one build, and the clock
/// said what that cost — an allocation per object, so five nested objects of
/// five shapes went from 1 755 ns to 2 138.
pub(super) fn plain_properties(
    context: &mut Context,
    cell: u32,
    plan: &mut Option<Plan>,
) -> Option<Lent> {
    if context.proxy_at(cell).is_some() {
        return None;
    }
    if !context.ranked_accessors(cell).is_empty() {
        return None;
    }
    if context.elements_at(cell).is_some() {
        return None;
    }
    let ty = context.region.type_of(cell)?;
    let shape = context.shape_of(ty)?;
    // Only a cell with nothing recorded may take or leave a remembered answer:
    // `enumerable` below is then the same for every cell of the shape.
    let ordinary = !context.records_attributes(cell);
    if ordinary
        && let Some((known, keys, labelled)) = plan.as_mut()
        && *known == shape
    {
        // The SECOND cell of a shape is what pays for the labels, and the
        // first never does. Built eagerly they were two allocations a key for
        // every lone object ever serialised, and the clock said so: eight
        // properties went from 1 168 ns to 1 695 the day they were.
        if !*labelled {
            *keys = shape_keys(context, cell, shape, true);
            *labelled = true;
        }
        return keys.is_some().then_some(Lent::Planned);
    }
    let keys = shape_keys(context, cell, shape, false);
    if !ordinary {
        return keys.map(Lent::Own);
    }
    let usable = keys.is_some();
    *plan = Some((shape, keys, false));
    usable.then_some(Lent::Planned)
}

/// What one walk hands the next: the plans by depth, and two buffers kept for
/// their capacity. See [`super::super::Scratch`].
pub(in crate::entry::json) type Kept = (Vec<Option<Plan>>, Vec<u32>, Vec<u8>);

/// Where the members [`plain_properties`] answered are.
pub(super) enum Lent {
    /// In the plan the caller handed in.
    Planned,
    /// Here: the cell records attributes of its own, so its answer is about
    /// the cell and must not be left for the next one of its shape.
    Own(Keys),
}

/// One member of a shape, with everything about it that is the SHAPE's.
///
/// The slot is what `own_property` would find by hashing the key, and the label
/// is what `quoted` would produce by scanning its text — both asked per member
/// per object, and both the same for every object of the shape.
pub(in crate::entry::json) struct Member {
    pub(super) key: rts_cranelift::shape::Key,
    /// Where the value sits, WHILE the cell still has this shape. A `toJSON`
    /// further up the walk may delete a property of the holder, so the reader
    /// checks the shape before it trusts this — see [`Writer::plain`].
    pub(super) slot: u32,
    /// The key as a JSON string literal, quotes and escapes included. Empty
    /// until a shape repeats, and for a key with a unit above 255 — both are
    /// written the long way, from the interner's text.
    pub(super) label: Vec<u8>,
}

/// The members a shape walk serialises, or `None` where it must not be one.
pub(super) type Keys = Vec<Member>;

/// The last shape [`plain_properties`] answered for, and its answer — a refusal
/// included, which is as much a fact about the shape as a key list is. The flag
/// is whether the labels were built yet.
pub(in crate::entry::json) type Plan = (rts_cranelift::shape::ShapeId, Option<Keys>, bool);

pub(super) fn shape_keys(
    context: &mut Context,
    cell: u32,
    shape: rts_cranelift::shape::ShapeId,
    labelled: bool,
) -> Option<Keys> {
    let mut keys = Vec::new();
    for (key, _) in context.shapes.properties(shape) {
        if !super::super::super::integrity::enumerable(context, cell, key) {
            continue;
        }
        // By reference, and the borrow ends before `enumerable` needs the
        // context again — a clone here would be one per key per call, which is
        // the allocation this path exists to remove.
        let (symbol, indexed, label) = match context.interner.text(key) {
            Some(text) => (
                super::super::super::symbol::is_symbol_key(text),
                crate::object::as_array_index(text).is_some(),
                text.narrow().filter(|_| labelled).map_or_else(Vec::new, |bytes| {
                    let mut label = super::super::out::Out::new();
                    label.bytes(b"\"");
                    label.escaped(bytes);
                    label.bytes(b"\"");
                    label.narrow().to_vec()
                }),
            ),
            None => return None,
        };
        // A symbol-keyed property is not enumerated, and its key lives in a
        // RESERVED NAME SPACE rather than in a variant of its own. Asked through
        // the same predicate `key_texts` asks, which is the one place that
        // encoding is known — a second spelling of it here is how the two would
        // come to disagree about what a symbol looks like.
        //
        // Written after a first version tested `text().is_none()`, which is
        // wrong in the direction that ships: a symbol key HAS text, so the check
        // passed and `{ a: 1, [Symbol("s")]: 2 }` serialised as
        // `{"a":1,"@@sym:14":2}` — the engine's internal spelling, in valid
        // JSON, against node and bun answering `{"a":1}`.
        if symbol {
            continue;
        }
        // An ARRAY-INDEX key is refused rather than handled, because
        // enumeration puts those first and in ascending numeric order while a
        // shape holds them in insertion order. `array::ordered` is that rule and
        // this does not restate it: an object with one such key takes the
        // general path, which already applies it.
        if indexed {
            return None;
        }
        let slot = context.shapes.slot_of(shape, key)?;
        keys.push(Member { key, slot, label });
    }
    Some(keys)
}

/// A plain member's value: by SLOT while the cell is still the shape the plan
/// was made for, and by key the moment it is not — a hook earlier in the walk
/// may have deleted or added a property of the holder.
// `#[inline]`: called per member from the runs in `walk.rs`. One file kept it
// inlined for free; across the split the clock read +10% on an eight-member
// object until it was asked for.
#[inline]
pub(super) fn member_value(
    context: &mut Context,
    cell: u32,
    shape: Option<rts_cranelift::shape::ShapeId>,
    member: &Member,
) -> Option<u64> {
    if context.shape_of(context.region.type_of(cell)?) == shape {
        return super::super::super::objects::slot_value(context, cell, member.slot);
    }
    super::super::super::objects::own_property(context, cell, crate::object::Key::Name(member.key))
        .map(|found| found.bits())
}
