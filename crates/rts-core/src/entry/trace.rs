//! Phase 2a: the tracer — mark every cell reachable from a set of roots.
//!
//! # What this does not do
//!
//! It does not find the roots. [`super::roots::context_roots`] and
//! [`super::roots::scan_stack`] are phase 2b, a different agent's work, and
//! this module takes their answer as an argument rather than deriving it.
//! It does not sweep either — freeing what is not marked is phase 3, over
//! [`Context::region`] and every `Slab` this crate owns, and nothing here
//! calls `Region::free` or `Slab::free`.
//!
//! # Why [`crate::collect::mark`] is not called directly
//!
//! It was the first thing checked, per this crate's rule 2. It fits a
//! homogeneous graph: one `Slab<T: Trace>`, where an edge always lands back
//! in the SAME slab. That is not this heap's shape. The primary structure is
//! [`crate::heap::Region`] — raw words behind a header, addressed by an
//! arithmetic reference, not a `Slab` a `Trace` impl could be written against
//! — and an edge out of one cell can land in eleven *different* side tables
//! before it names another cell: [`Context::spills`] and [`Context::arrays`]
//! (through [`Context::spill_of`]/[`Context::array_elements`]),
//! [`Context::callables`], [`Context::proxies`], [`Context::bound`],
//! [`Context::views`], [`Context::collections`], [`Context::generators`] (and,
//! through it, a SPANNING allocation `Region::field` cannot even address),
//! [`Context::prototypes`], and [`Context::accessors`]. `collect::mark`'s
//! generic parameter is one `T`; this walk needs all of them at once for one
//! `cell: u32`, which is a shape `Slab<T: Trace>` cannot express no matter
//! what `Trace` is implemented for.
//!
//! What IS reused, because it fits exactly: [`crate::collect::Marks`], the
//! mark bitmap and its termination rule — a slot already marked is not
//! visited again, which is what stops a cycle and is unrelated to what the
//! slots hold. Rewriting it here would be the second copy this crate's rule 7
//! warns about. The worklist loop below is the same shape `collect::mark`
//! uses — pop, trace, push what is newly marked — stated again because the
//! thing being popped is a raw cell index into `Region` rather than a `Slot`
//! into one `Slab`, which is precisely the mismatch above.
//!
//! # Why no recursion
//!
//! The same reason `collect::mark`'s module documentation gives: an object
//! graph is as deep as a program makes it, and marking it by recursion
//! overflows the stack during a collection, the worst moment available. See
//! `a_deep_chain_does_not_overflow_the_stack` below for the pin.
//!
//! # A reference is only a reference if its tag says so
//!
//! Every word taken from a cell, a spill, an array element, a bound
//! function's argument list and every other table below goes through
//! [`Value::kind`] before it is followed. A float or a small integer that
//! happens to spell a plausible cell index is rejected by its tag — the same
//! discipline [`crate::collect::conservative_roots`] applies to a raw stack
//! word, applied here to a raw heap word for the same reason.
//!
//! # `WeakMap`/`WeakSet`, on purpose
//!
//! [`Context::collections`] is traced identically for all four collection
//! kinds. `entry::collections::weak` says, in its own documentation, that a
//! `WeakMap`/`WeakSet` holds its keys STRONGLY today — there is no
//! `(slot, generation)` mechanism yet to let a collector observe that a key
//! died — so tracing them exactly like `Map`/`Set` is the only answer that
//! does not free a key still reachable through the collection. `PLAN.md`
//! phase C1 is where that changes; this is the deliberate deferral until it
//! does.
//!
//! # What is found but out of this phase's return value
//!
//! **Most strings no longer have a payload outside their cell**, and this
//! paragraph described the state before `entry::text_cell`: a string whose
//! `Str` owns no buffer — an inline Latin-1 run, which is nearly every string
//! a program makes — carries that `Str` in its own slots 2 to 6, and slot 0
//! holds a sentinel instead of a slab index. For those there is no second
//! table, nothing to sweep separately, and the bytes go away with the cell.
//!
//! What remains outside is the rest: a spilled Latin-1 run and every UTF-16
//! string keep their `Str` in [`Context::cells`] — a
//! [`crate::heap::Slab<Str>`] indexed by a RAW, unencoded slot number in the
//! cell's first field. That word is not a [`Value`] and this walk correctly
//! does not follow it as one. So this module's [`Marks`] still answers only
//! which REGION cells are live, and the slab entry of a live string that has
//! one is still a second, smaller table — `entry::collect_cycle::release` is
//! what gives it back, which is also where the sentinel is checked.
//!
//! The slots holding an in-cell `Str` are declared [`Repr::Payload`], which is
//! what keeps them out of this walk AND out of the check below. Both halves
//! are needed and the second was learned the hard way — see the loop.

use crate::collect::Marks;
use crate::heap::{INLINE_SLOTS, Slot};
use crate::value::{Kind, Value};

use super::{Context, side_tables};

/// Marks every cell reachable from `roots`, and answers which cells are live.
///
/// `roots` is expected to be the union phase 2b already produced —
/// [`super::roots::context_roots`] plus whatever [`super::roots::scan_stack`]
/// found — filtered to encoded references before it ever reaches here. This
/// function does not re-filter them: a `Slot` that turns out to be freed or
/// out of range is not an error, the same tolerance `collect::mark` states for
/// exactly the reason a conservative scan produces roots like that by
/// construction.
pub fn mark(context: &Context, roots: &[Slot]) -> Marks {
    let mut marks = Marks::new();
    let mut worklist: Vec<u32> = Vec::new();

    for &root in roots {
        // A root that names no cell of this region is not a root. The stack scan
        // offers every word that DECODES as a reference, and a stale one decodes
        // to any index at all -- the mark set is sized by the highest index it is
        // given, so one such word made a cycle over 2 571 live cells spend 56 ms
        // in here against 0.3 (release, 2026-09-29), and which cycles paid moved
        // with what the stack last held. Only what lies OUTSIDE the region is
        // refused: a free cell inside it is marked as before, because refusing
        // more than the impossible is how a live reference gets lost.
        if context.region.type_of(root.0).is_none() {
            continue;
        }
        if marks.mark(root) {
            worklist.push(root.0);
        }
    }

    // Reused across every cell visited, the same way `collect::mark` reuses
    // one `outgoing` buffer: a collection is exactly the moment nothing
    // should be allocating per edge.
    let mut referenced: Vec<u64> = Vec::new();
    while let Some(cell) = worklist.pop() {
        referenced.clear();
        edges_of(context, cell, &mut referenced);
        for &word in &referenced {
            follow(word, &mut marks, &mut worklist);
        }
    }

    marks
}

/// Marks a candidate word, if it is a reference, and queues it to be walked.
///
/// The one place [`Value::kind`] is asked. Every table below calls back
/// through here rather than testing the tag itself, so there is exactly one
/// answer to "is this word a reference" — the same discipline
/// `crate::collect::conservative_roots` states for a stack word.
fn follow(word: u64, marks: &mut Marks, worklist: &mut Vec<u32>) {
    if let Kind::Reference(slot) = Value(word).kind() {
        let slot = slot as u32;
        if marks.mark(Slot(slot)) {
            worklist.push(slot);
        }
    }
}

/// Every word one cell holds that might name another cell.
///
/// Appends rather than returns, for the same reason [`crate::collect::Trace`]
/// does: the caller owns the buffer and clears it between cells, so a
/// collection allocates once rather than once per cell.
fn edges_of(context: &Context, cell: u32, out: &mut Vec<u64>) {
    // 1. Every slot the cell OWNS — fifteen for an ordinary one, more for an
    //    object the emitter sized to its shape. Walking a fixed fifteen would
    //    leave a wide object's later properties unmarked, and a collection
    //    would free what one of them still names.
    //
    //    One short of that for a cell that HAS an overflow: there the last slot
    //    holds the block's ADDRESS rather than a value, and following it would
    //    hand the region a decompose of an address — the same fault the
    //    float-that-looks-like-a-cell case exists to refuse. A generator's
    //    parked frame spans too and its last field IS a value, which is why
    //    the question is "does it have an overflow" and not "is it wide".
    let width = context.region.width_of(cell).unwrap_or(INLINE_SLOTS);
    let owned = if context.spill_of.copied(cell).is_some() {
        width.saturating_sub(1)
    } else {
        width
    };
    // 2. And of those, only the ones that CAN name a cell. The machine already
    //    answers that, and `gc::barrier_for` already asks it for the STORE
    //    side: a cell's type has an aggregate layout, every field of it carries
    //    a `Repr`, and `Repr::is_gc_relevant` is true for exactly `Tagged` and
    //    `Ref`. The read side did not ask — so a string pushed its slab index
    //    and its length, and every array pushed its `length`, three words the
    //    compiler had already declared are not references.
    //
    //    A slot the layout does NOT declare keeps being pushed, and that half
    //    is conservative on purpose: a cell is fifteen slots wide whatever its
    //    shape says, a retyped cell's tail can still hold what the old shape
    //    put there, and nothing here knows it is dead. Precise where it is
    //    declared, unchanged where it is not — which is also why this cannot
    //    free something a property still reads: a slot past the declared fields
    //    is a slot no property resolves to.
    //
    //    What it risks is a field DECLARED non-GC-relevant that holds a
    //    reference anyway — rule 10's own failure direction, a silent free
    //    rather than a crash. The debug assertion below is what refuses it,
    //    which is the shape `side_tables` uses for an arm that disagrees with
    //    its table.
    //
    //    This comment said `Region::set_field` carried that assertion too, and
    //    it does not — there is no such check at the write, because `set_field`
    //    has no type registry to ask. Corrected rather than left standing: a
    //    reader who believed it would think the write side was covered.
    let declared = context
        .region
        .type_of(cell)
        .and_then(|ty| context.declared_fields(ty));
    for slot in 0..owned {
        if !rts_cranelift::gc::traces_field(declared.and_then(|fields| fields.field(slot as usize)))
        {
            // Rule 7's verifier half, and it is EXACT rather than approximate:
            // `is_encoded` tests the box bits, so a raw slab index and a double
            // both answer `None` to `as_slot` while a real reference answers
            // `Some`. A field declared `I64` or `F64` holding one is the silent
            // free this whole arm risks, caught here — in the path that would
            // do the harm — on every debug run of every test.
            //
            // Asked only of a field that IS a value, which is a second question
            // the machine answers and not a let-out: a field declared
            // `Repr::Payload` holds some bytes of a larger thing, and a byte of
            // text is neither a reference nor not one. Asking anyway aborted on
            // an ordinary six-character string whose fifth and sixth characters
            // put the reference tag in the word's top half —
            // `gc::field_holds_a_value` is where that is decided, beside
            // `traces_field` so the two cannot drift.
            debug_assert!(
                !rts_cranelift::gc::field_holds_a_value(
                    declared.and_then(|fields| fields.field(slot as usize))
                ) || context
                    .region
                    .field(cell, slot)
                    .and_then(|word| Value(word).as_slot())
                    .is_none_or(|named| context.region.header_of(named).is_none()),
                "cell {cell} slot {slot} holds a reference, and its layout declares the field is not one"
            );
            continue;
        }
        if let Some(word) = context.region.field(cell, slot) {
            out.push(word);
        }
    }

    // 3. Everything attached to the cell from OUTSIDE it, as a total walk
    //    over every such table. `side_tables` is both the classification and
    //    the walk, in one module, because a table and the answer to "can it
    //    name a cell" are one decision and splitting them is what let the
    //    answer be prose.
    side_tables::edges(context, cell, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Kinds, Singletons};

    fn empty_context() -> Context {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        };
        Context::new(singletons, Kinds::in_declaration_order())
    }

    /// Writes a plain object with one property, an ordinary inline reference.
    fn object_pointing_at(context: &mut Context, target: u32) -> u32 {
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::Tagged]);
        let cell = context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room");
        context
            .region
            .set_field(cell, 0, Value::from_slot(target).bits())
            .expect("a slot exists");
        cell
    }

    fn plain(context: &mut Context) -> u32 {
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::Tagged]);
        context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room")
    }

    #[test]
    fn what_a_root_reaches_through_an_inline_slot_survives() {
        let mut context = empty_context();
        let leaf = plain(&mut context);
        let held = object_pointing_at(&mut context, leaf);
        let orphan = plain(&mut context);

        let marks = mark(&context, &[Slot(held)]);
        assert!(marks.is_marked(Slot(held)), "the root itself");
        assert!(marks.is_marked(Slot(leaf)), "reached through the slot");
        assert!(!marks.is_marked(Slot(orphan)), "nothing points at it");
    }

    #[test]
    fn a_root_outside_the_region_does_not_size_the_mark_set() {
        let mut context = empty_context();
        let held = plain(&mut context);
        let stale = Slot(u32::MAX - 7);

        let marks = mark(&context, &[stale, Slot(held)]);
        assert!(marks.is_marked(Slot(held)), "the real root is still a root");
        assert!(!marks.is_marked(stale), "a word that names no cell marks nothing");
    }

    #[test]
    fn a_cycle_terminates() {
        let mut context = empty_context();
        let a = plain(&mut context);
        let b = object_pointing_at(&mut context, a);
        context
            .region
            .set_field(a, 0, Value::from_slot(b).bits())
            .expect("a slot exists");

        // If this did not terminate, the test itself would hang rather than
        // fail — which is exactly the bug class the mark bit exists to
        // refuse: a slot already marked is not walked a second time.
        let marks = mark(&context, &[Slot(a)]);
        assert!(marks.is_marked(Slot(a)));
        assert!(marks.is_marked(Slot(b)));
    }

    #[test]
    fn a_deep_chain_does_not_overflow_the_stack() {
        let mut context = empty_context();
        let mut previous = plain(&mut context);
        let mut all = vec![previous];
        for _ in 0..20_000 {
            previous = object_pointing_at(&mut context, previous);
            all.push(previous);
        }

        let marks = mark(&context, &[Slot(previous)]);
        for cell in all {
            assert!(marks.is_marked(Slot(cell)), "every link of the chain");
        }
    }

    #[test]
    fn an_object_reachable_only_through_a_map_value_is_marked() {
        let mut context = empty_context();
        let value = plain(&mut context);
        let holder = plain(&mut context);
        let mut table = crate::entry::collections::Table::default();
        table.set(&context, Value::from_i32(1).bits(), Value::from_slot(value).bits());
        context.collections.set(holder, table);

        let marks = mark(&context, &[Slot(holder)]);
        assert!(
            marks.is_marked(Slot(value)),
            "nothing but the Map's own table names this cell"
        );
    }

    #[test]
    fn an_object_reachable_only_through_a_bound_arguments_list_is_marked() {
        let mut context = empty_context();
        let argument = plain(&mut context);
        let target = plain(&mut context);
        let bound_cell = plain(&mut context);
        context.bound.set(
            bound_cell,
            crate::entry::function_proto::Bound::for_test(
                Value::from_slot(target).bits(),
                Value::from_i32(0).bits(),
                vec![Value::from_slot(argument).bits()],
            ),
        );

        let marks = mark(&context, &[Slot(bound_cell)]);
        assert!(marks.is_marked(Slot(target)), "the function it calls");
        assert!(
            marks.is_marked(Slot(argument)),
            "an argument reachable only through the partial list"
        );
    }

    #[test]
    fn an_object_reachable_only_through_a_spill_is_marked() {
        let mut context = empty_context();
        let value = plain(&mut context);
        let holder = plain(&mut context);
        // A type that says its fields hold values, which is what the runtime's
        // own `spill_type` says. It was `0` — the reserved TEXT layout, whose
        // field zero is a slab index — so the block claimed to be a string
        // while holding a reference, and `edges_of`'s assertion says so now.
        let ty = context
            .types
            .declare(&[rts_cranelift::repr::Repr::Tagged])
            .index() as u32;
        let block = context
            .region
            .alloc_spanning(16, ty)
            .expect("room for the overflow block");
        context
            .region
            .set_spanning_field(block, 0, 1, Value::from_slot(value).bits());
        context.spill_of.set(holder, (block, 1));

        let marks = mark(&context, &[Slot(holder)]);
        assert!(
            marks.is_marked(Slot(value)),
            "the eighth property and beyond live in the spill, not a slot"
        );
    }

    #[test]
    fn a_float_whose_bits_spell_a_cell_index_is_not_followed() {
        let mut context = empty_context();
        let looks_like_a_cell = plain(&mut context);
        let holder = plain(&mut context);
        // The bits of a small slot index read as a denormal double — the same
        // hostile pattern `crate::collect`'s own test constructs.
        let disguised = f64::from_bits(u64::from(looks_like_a_cell));
        context
            .region
            .set_field(holder, 0, Value::from_f64(disguised).bits())
            .expect("a slot exists");

        let marks = mark(&context, &[Slot(holder)]);
        assert!(marks.is_marked(Slot(holder)), "the root itself");
        assert!(
            !marks.is_marked(Slot(looks_like_a_cell)),
            "it is not encoded, so it is not a reference — following it would \
             be exactly the bug `Value::kind` exists to make impossible"
        );
    }

    #[test]
    fn an_out_of_range_root_does_not_panic() {
        // A conservative root can name a freed or never-allocated slot by
        // construction; the walk must skip it rather than treat it as an
        // error, the same tolerance `collect::mark` states.
        let context = empty_context();
        let marks = mark(&context, &[Slot(9_999)]);
        assert!(marks.is_marked(Slot(9_999)), "recorded as a root regardless");
    }
}
