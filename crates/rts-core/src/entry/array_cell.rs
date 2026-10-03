//! An array's elements in the cell's OWN slots, for an array small enough.
//!
//! # Why this exists
//!
//! An array's elements live in a `Vec` inside the `arrays` [`Slab`], where an
//! object's fields live in the cell's own slots. That is two allocations
//! against one, and it is measured (release, one size per process, 2026-10-02):
//!
//! | elements | object literal | array literal |
//! |---:|---:|---:|
//! | 2 | 40.0 ns | 60.0 ns |
//! | 4 | 40.0 | 70.0 |
//! | 8 | 46.7 | 90.0 |
//! | 14 | 60.0 | 110.0 |
//!
//! So an array pays **+15 ns fixed** and **+2.5 ns per element** over an
//! object, and matching the object's curve is -37 % at four elements. The fixed
//! part is the `Slab` insert and the side-table write; the per-element part is
//! the `Vec`'s malloc and its hole fill.
//!
//! # The three invariants, and why each is safe here rather than merely checked
//!
//! **1. The collector traces these slots already, and this is not a new
//! promise.** `gc::traces_field` answers `true` for a slot the aggregate does
//! not declare, and its own doc says why: *"a word followed in error keeps
//! something alive one cycle longer, and a word NOT followed in error is a
//! use-after-free."* The array layout declares `length` and nothing else, so
//! every element slot answers `None` and is followed. **Nothing in `trace.rs`
//! or `side_tables/edges.rs` changes for this**, which is the only reason this
//! is not an instance of the class `docs/engine/lost-roots.md` describes.
//!
//! **2. A property can never be handed an element's slot, because an array
//! holding elements inline is spilled before it can reach a second shape.**
//! Property slots are assigned from zero upward and written straight into the
//! cell, so `a.foo = 1` on an inline array would otherwise overwrite an
//! element and nothing would notice — no crash, just an element that silently
//! became a property's value. [`spill_before_transition`] is called from
//! `objects::put`, which is the one place a property is added.
//!
//! **3. There is one source of truth for the count, and it is `length`.** A
//! second copy in a slot of its own was the first design and is rejected: two
//! words that must agree about one fact is what `docs/engine/one-form-per-
//! question.md` exists to refuse, and the failure would be an array whose
//! elements and whose `length` disagree — which is exactly the bug
//! `objects::put` already carries a comment about for `a.length = 1`.
//!
//! # What stays in the `Slab`
//!
//! Anything longer than [`CAPACITY`], anything a program MUTATES (the first
//! write spills, which is why none of `elements_at_mut`'s twenty-two callers
//! changed), and any array that has gained a property. A `Str` in its cell is
//! immutable for the same reason and by the same mechanism.

use super::Context;
use crate::heap::INLINE_SLOTS;
use crate::heap::Slot;
use crate::value::Value;

/// The [`Slot`] that means "the elements are in the cell, not in the `Slab`".
///
/// A sentinel rather than a new side table, and that is the whole reason this
/// change touches eight sites instead of the fifteen a table would: the brand
/// check `array_elements.copied(cell).is_some()` keeps answering "this is an
/// array", `side_tables::release` keeps removing the entry, and the `Slab`
/// lookup in `side_tables::edges` already guards with `if let Ok(..)` and so
/// skips a sentinel without being told about it. `text_cell::IN_CELL` is the
/// same device for the same reason.
pub(super) const IN_CELL: Slot = Slot(u32::MAX);

/// The first slot an element may occupy.
///
/// One, because slot zero is `length` — the single property the array layout
/// declares. Asserted rather than assumed: [`fits`] refuses an array whose
/// layout declares more than this, so a layout that grows a second property
/// stops using the cell instead of corrupting it.
pub(super) const FIRST_SLOT: u32 = 1;

/// How many elements fit in a cell.
pub(super) const CAPACITY: u32 = INLINE_SLOTS - FIRST_SLOT;

/// Whether `cell` can hold `count` elements inline.
///
/// # Why this checks everything [`at`] checks, and not merely the count
///
/// It is the PRECONDITION for [`at`] succeeding, so the two must test the same
/// things or the failure is silent in the worst direction: elements written
/// into the cell that [`at`] then declines to read answer an array that has
/// quietly lost its contents, with no crash and nothing freed.
///
/// That is not hypothetical — it is the bug this function was rewritten to
/// prevent. [`at`] derives the count from `length` through
/// `Context::array_length_slot`, which is `None` until the first array mints
/// the layout; the first version checked only the declared width and the count,
/// so the very first array of a program would have been placed inline and read
/// back empty.
///
/// The declared width is asked rather than assumed for the same reason: if
/// `length` ever stops being the only field the array layout declares, this
/// answers `false` and every array goes back to the `Slab` — slower, and
/// correct.
pub(super) fn fits(context: &Context, cell: u32, count: usize) -> bool {
    if count as u32 > CAPACITY {
        return false;
    }
    let Some(layout) = context.array_layout else {
        return false;
    };
    if context.array_length_slot.is_none() || context.region.type_of(cell) != Some(layout) {
        return false;
    }
    context
        .declared_fields(layout)
        .is_some_and(|declared| declared.fields.len() as u32 <= FIRST_SLOT)
}

/// Writes `elements` into the cell's own slots.
///
/// The caller has already checked [`fits`] and is responsible for marking the
/// cell `IN_CELL` and for writing `length`: this function only moves the words,
/// so that the order of those three is decided in one place in `array.rs`
/// rather than half here.
pub(super) fn place(context: &mut Context, cell: u32, elements: &[u64]) {
    debug_assert!(
        fits(context, cell, elements.len()),
        "the caller must check `fits` before placing elements in a cell",
    );
    for (at, held) in elements.iter().enumerate() {
        let slot = FIRST_SLOT + at as u32;
        context
            .region
            .set_field(cell, slot, *held)
            .expect("an element slot is within a cell the caller just allocated");
    }
}

/// The elements of a cell holding them inline, or `None` if it does not.
///
/// # Why the count comes from `length` and costs a load
///
/// Invariant 3: one source of truth. The alternative was a raw count in a slot
/// of its own, which reads faster and can disagree with `length`. An array
/// whose `length` says three while its cell holds four is a wrong answer that
/// no test asserts on, and this engine has already paid twice this session for
/// a fact stored in two places.
pub(super) fn at<'a>(context: &'a Context, cell: u32) -> Option<&'a [u64]> {
    if context.array_elements.copied(cell) != Some(IN_CELL) {
        return None;
    }
    let count = length_of(context, cell)?;
    let window = context.region.payload_window(cell, FIRST_SLOT, count)?;
    // SAFETY: `payload_window` has checked that `FIRST_SLOT + count` is within
    // the cell's width AND within the words that back it, and it answers a
    // pointer into those words. The borrow of `context` outlives the slice by
    // construction, and nothing in this module hands out a `&mut` to the same
    // words while it is held — a mutation spills to the `Slab` first.
    Some(unsafe { std::slice::from_raw_parts(window.cast_const(), count as usize) })
}

/// Moves a cell's inline elements into the `Slab`, so it can be mutated or
/// grown a property.
///
/// Answers whether anything moved, so that "was not inline" and "nothing to
/// do" are the same answer to a caller: both mean the elements are in the
/// `Slab` now.
///
/// The cell's element slots are left as they are rather than cleared. They hold
/// the same words the `Slab` now holds, so following them is correct; and
/// leaving them costs at most retaining a cell one cycle longer after the array
/// SHRINKS, which is rule 12's direction — a word followed in error is safe
/// and a word missed is not. Clearing them would be fourteen stores on a path
/// taken by every first write to an array.
pub(super) fn spill(context: &mut Context, cell: u32) -> bool {
    let Some(held) = at(context, cell).map(<[u64]>::to_vec) else {
        return false;
    };
    let store = context.arrays.insert(held).slot();
    context.array_elements.set(cell, store);
    true
}

/// The `length` property, as the element count.
///
/// `None` if the array is not at the layout whose slot is remembered, which is
/// the same guard `array::fresh_length` uses before writing through it — and
/// the same condition that makes the elements safe at all, since a cell that
/// has left the array layout has had a property written into the slots they
/// occupy.
fn length_of(context: &Context, cell: u32) -> Option<u32> {
    let slot = context.array_length_slot?;
    if context.region.type_of(cell) != context.array_layout {
        return None;
    }
    let word = context.region.field(cell, slot)?;
    let count = Value(word).as_f64()?;
    // A negative or fractional `length` cannot be an inline count, and
    // answering `None` sends the caller to the `Slab` rather than making a
    // slice of nonsense. `a.length = 1.5` is a `RangeError` long before here,
    // so this is a floor and not a path.
    if count < 0.0 || count.fract() != 0.0 || count > f64::from(CAPACITY) {
        return None;
    }
    Some(count as u32)
}
