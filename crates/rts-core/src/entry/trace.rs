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
//! A live cell of the text type names its string's payload in
//! [`Context::cells`] — a [`crate::heap::Slab<Str>`] indexed by a RAW,
//! unencoded slot number written into the cell's first field
//! (`Context::intern_value`). That word is not a [`Value`] and this walk
//! correctly does not follow it as one — `Value::kind` on a small unencoded
//! integer answers `Float`, so it is silently and safely ignored rather than
//! mis-followed. What that means is this module's [`Marks`] answers only
//! which REGION cells are live; a live text cell's entry in `Context::cells`
//! is a second, smaller table phase 3 has to sweep on its own account by
//! reading `Context::text_at` for every live cell of the text type. Stated
//! here so it is a known boundary rather than a surprise the day something
//! sweeps a live string's bytes.

use crate::collect::Marks;
use crate::heap::{INLINE_SLOTS, Slot};
use crate::value::{Kind, Value};

use super::Context;

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
    for slot in 0..owned {
        if let Some(word) = context.region.field(cell, slot) {
            out.push(word);
        }
    }

    // 2. Properties past the fifteenth.
    //
    //    The spill is a REGION block and not a slab vector, so its slots are
    //    reached the way an inline slot is — and that is exactly why the block
    //    ITSELF has to be marked. The comment here used to say the opposite:
    //    that pushing it "would only keep it alive by itself", which was true
    //    of the `Vec` in a slab it replaced and false the moment it became a
    //    region cell. `alloc_spanning` marks a span's cells 1..n as interior
    //    and leaves the FIRST one ordinary, so `Region::live_refs` offers it to
    //    the sweep as an object of its own — and nothing marked it, so every
    //    collection freed the overflow of an object that was still alive. Its
    //    cells went back on the free list, an unrelated allocation took them,
    //    and the sixteenth property onward read somebody else's fields.
    //
    //    Pushed the same way a generator's parked frame is (step 9), for the
    //    same reason and through the same mechanism: this is the one place that
    //    knows the block exists, and it must be marked even on a cycle where
    //    none of its slots happens to hold a reference. `collect_cycle::release`
    //    still frees it explicitly with its owner — marking keeps the SWEEP off
    //    a block whose owner survived, which is a different question.
    if let Some((block, slots)) = context.spill_of.copied(cell) {
        out.push(Value::from_slot(block).bits());
        for slot in 0..slots {
            if let Some(word) = context.region.spanning_field(block, slot, slots) {
                out.push(word);
            }
        }
    }

    // 3. Array elements.
    if let Some(elements) = context.array_elements.copied(cell)
        && let Ok(words) = context.arrays.at(elements)
    {
        out.extend_from_slice(words);
    }

    // 4. A closure's environment. Never its code — `callables.0` is an
    //    address, not a value, and following it as one would hand the region
    //    a decompose of a number that is not a reference at all. The third
    //    member is the class-constructor flag, which is a boolean.

    if let Some((_, environment, _)) = context.callables.copied(cell) {
        out.push(environment);
    }

    // 5. A proxy's target and handler — never reachable through an inline
    //    slot, because a proxy has no own properties by design.
    if let Some((target, handler)) = context.proxies.copied(cell) {
        out.push(target);
        out.push(handler);
    }

    // 6. A bound function's receiver and partial argument list, and the
    //    function it calls.
    if let Some(bound) = context.bound.get(cell) {
        bound.trace(out);
    }

    // 7. A view's buffer. Stored as a bare cell index rather than an encoded
    //    `Value` (`buffers::View::buffer` — see its own module documentation
    //    for why a view never caches the derived `Slot`), so it is encoded
    //    here before joining the rest: `follow` only ever reads `Value`s.
    if let Some(view) = context.views.get(cell) {
        out.push(Value::from_slot(view.buffer).bits());
    }

    // 8. A Map/Set/WeakMap/WeakSet's keys AND values — see the module
    //    documentation for why the weak pair is traced identically.
    if let Some(table) = context.collections.get(cell) {
        table.trace(out);
    }

    // 8b. WHAT AN ITERATOR IS WALKING, and this table is the only path to it.
    //
    //     `Context::cursors` holds `(listed, at)`: the array an array or string
    //     iterator steps, or the Map/Set whose table a collection cursor walks.
    //     Both are encoded `Value`s and neither is reachable from the iterator
    //     by any other route — an iterator has no own property naming its
    //     source, exactly as a helper does not, which is why `helpers` is
    //     traced at 9b below.
    //
    //     It was MISSING, and the failure it produced was silence rather than a
    //     crash. `const it = [1,2,3][Symbol.iterator]()` leaves the array named
    //     by nothing else; the first collection reclaimed it, `stepped` then
    //     found no elements at the cell, and `next()` answered `{done: true}`.
    //     A `for`-`of` over such an iterator ENDS EARLY and reports nothing.
    //     Reproduced 2026-08-29 for an array, a Set and a Map at once.
    //
    //     `cursors` was also the one `Aside` holding a `Value` that neither this
    //     walk visited nor the closing comment accounted for — every other
    //     unvisited table (`regexes`, `integrity`, `attributes`, `derived`,
    //     `buffer_of`, `foreign`, `detached`, `proto_types`) holds no reference,
    //     and says so there.
    if let Some((listed, _)) = context.cursors.copied(cell) {
        out.push(listed);
    }

    // 9. A generator's parked frame. Its own cell reference is pushed
    //    directly rather than through `follow`: `alloc_spanning` never wrote
    //    it as an encoded `Value` anywhere a decode could find it, this is
    //    the one place that knows the frame exists at all, and the frame
    //    must be marked live even on a turn where every one of its fields
    //    happens to hold no reference. Its fields are walked separately,
    //    through `Region::spanning_field` rather than `Region::field`,
    //    because a frame is not bounded by `INLINE_SLOTS`.
    if let Some(state) = context.generators.get(cell) {
        out.push(Value::from_slot(state.frame_cell()).bits());
        state.trace(&context.region, out);
    }

    // 9b. An iterator helper's source, its callback, and the inner sequence a
    //     `flatMap` is in the middle of. None of the three is reachable through
    //     an own property — a helper has none — so this table is the only path
    //     to them while the helper is alive.
    if let Some(state) = context.helpers.get(cell) {
        state.trace(out);
    }

    // 10. What a cell inherits from.
    if let Some(prototype) = context.prototypes.copied(cell) {
        out.push(prototype);
    }

    // 11. A getter and a setter are callables; an ordinary cached property
    //     read never reaches this table (`cache_resolve` already answers
    //     negative for an accessor), so this is the only path that visits it.
    if let Some(list) = context.accessors.get(cell) {
        for (_, getter, setter, _) in list {
            out.extend(getter.iter().copied());
            out.extend(setter.iter().copied());
        }
    }

    // 12. A wrapper object's boxed primitive — `new String("x")`'s
    //     `[[StringData]]`, which is itself a text reference and the one
    //     `Aside<u64>` among these that is not obviously an object link.
    if let Some(primitive) = context.boxed.copied(cell) {
        out.push(primitive);
    }

    // What a cell's header records about it that is genuinely NOT a
    // reference, so its absence above is a decision rather than a gap:
    // `regexes` (a compiled pattern and its flags — `lastIndex` is an
    // ordinary property, already covered by the inline slots or a spill),
    // `integrity` (a freeze level) and `attributes` (writable/enumerable/
    // configurable flags, keyed by a property NUMBER rather than by
    // anything the heap allocated) hold no value of any kind. `derived` is a
    // boolean, and so is the class-constructor flag that `callables` now
    // carries as its third member. `buffer_of` only locates
    // `Context::buffers`, whose bytes are never references themselves — an
    // `ArrayBuffer`'s bytes are exactly that, bytes, and a live view already
    // names the buffer's CELL through step 7 above, not through this table.
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
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
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
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
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
        let block = context
            .region
            .alloc_spanning(16, 0)
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
