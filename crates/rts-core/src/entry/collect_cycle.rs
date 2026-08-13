//! Phase 3: making a collection actually free something.
//!
//! # What the two phases before this hand over
//!
//! [`super::roots::context_roots`] plus [`super::roots::scan_stack`] answer
//! *what is a root*; [`super::trace::mark`] answers *what is reachable from
//! them*. Neither frees anything — see their own module documentation. This is
//! where a cycle stops being an exercise: [`collect`] runs both, then
//! [`sweep`] gives back every cell the marker did not reach, through
//! [`crate::heap::Region::free`], clearing every side table [`crate::heap::Aside::remove`]
//! knows about and every `Slab`-held payload the region does not.
//!
//! # Why this is not `crate::collect::sweep`
//!
//! That function frees a whole `Slab<T>` entry in one call, because a
//! homogeneous `Slab<T: Trace>` is what it sweeps. A doomed cell here is not
//! one entry in one table — it is a cell whose death has to be told to
//! however many of the fifteen side tables in [`super::Context`] have
//! something keyed by it, plus whichever `Slab` those tables in turn point
//! into. `crate::collect::sweep` cannot express that any more than
//! `crate::collect::mark` could, for the same reason [`super::trace`] gives
//! for not calling it.
//!
//! # The table list, and how a forgotten one would be caught
//!
//! [`release`] below enumerates every `Aside` and every `Slab` [`super::Context`]
//! declares, by reading its field list rather than from memory — the same
//! discipline [`super::roots::context_roots`]'s own documentation names. That
//! reading is manual and nothing enforces it stays exhaustive: a new `Aside`
//! field added to `Context` without a matching line here compiles clean and
//! leaks silently — the next occupant of that cell index would not read the
//! old value (each table still refuses a `None` cell honestly), but the old
//! entry sits there forever, unreachable and unreclaimed. That is a leak, not
//! corruption, because `Region::free` only ever hands a freed cell's INDEX
//! back out — a stale `Aside` entry is simply never looked at again once nothing
//! composes that index into a live reference for it. Nothing here makes a
//! missing line loud. The honest fix is exhaustiveness: `Context`'s own fields
//! could be walked by a macro or a test that lists them once and fails to
//! compile — or fails a test — when the two lists diverge, mirroring what this
//! crate's `#[deny(dead_code)]` already does for a producer with no caller.
//! That is not built here; this paragraph is the proposal in place of it.
//!
//! # Why a generator's frame is not swept the same way
//!
//! Its cell is ordinary — allocated by [`super::native::plain`] and swept like
//! any other object below, through the `generators` table. What it POINTS at,
//! [`super::generator::State`]'s `frame`, is a SPANNING allocation:
//! `Region::alloc_spanning` gives it several consecutive cells with no header
//! on any but the first. [`crate::heap::Region::live_refs`] already excludes the
//! trailing ones from what a sweep ever considers — see its own documentation
//! for why walking them as ordinary cells would corrupt a live frame — so this
//! module frees only the frame's own first cell, through the same
//! `Region::free` every other cell goes through. The cells after it are not
//! reclaimed: `Region::free` threads exactly one cell into the free list, and
//! teaching it to free a run is a distinct, larger change nothing here needs
//! yet. A dead generator therefore leaks the tail of its frame — documented
//! rather than silently accepted, and strictly better than the alternative of
//! writing a free-list link into a live frame's own field.

use crate::collect::Marks;
use crate::heap::Slot;

use super::Context;

/// Runs one full mark-and-sweep cycle and answers how many cells were freed.
///
/// `stack_low` is this thread's own current stack pointer, approximately —
/// the address of a local variable at the call site is what
/// [`super::alloc::alloc`] passes, which is sound for the reason
/// [`super::roots::scan_stack`] gives: retaining a dead reference one cycle
/// longer costs nothing, and a `low` a few words below the true `rsp` only
/// widens that margin.
///
/// Does nothing — and answers `0` — when [`Context::stack_high`] is absent.
/// That is not a smaller collection, it is the refusal this crate's own
/// honesty floor asks for: a cycle that cannot see the stack half of the root
/// set would free something the stack alone still names, which is a wrong
/// answer wearing a collection's clothes.
pub fn collect(context: &mut Context, stack_low: usize) -> usize {
    let Some(stack_high) = context.stack_high else {
        if std::env::var_os("RTS_GC_DEBUG").is_some() {
            eprintln!("rts-gc REFUSED: no stack bound installed");
        }
        return 0;
    };

    let mut roots = super::roots::context_roots(context);
    if stack_high > stack_low {
        // SAFETY: `stack_low` is a real address on this thread's own stack —
        // the caller took it from a local just before this call — and
        // `stack_high` came from the host's own OS call for THIS thread,
        // installed before the program that is now allocating ever ran. Both
        // halves of `scan_stack`'s contract.
        roots.extend(unsafe { super::roots::scan_stack(stack_low, stack_high) });
    }

    let stack_roots = roots.len();
    let marks = super::trace::mark(context, &roots);
    let freed = sweep(context, &marks);
    if std::env::var_os("RTS_GC_DEBUG").is_some() {
        eprintln!(
            "rts-gc roots {stack_roots} live {} freed {freed}",
            context.region.live_refs().len()
        );
    }
    freed
}

/// Frees every live cell the marker did not reach.
fn sweep(context: &mut Context, marks: &Marks) -> usize {
    let doomed: Vec<u32> = context
        .region
        .live_refs()
        .into_iter()
        .filter(|&cell| !marks.is_marked(Slot(cell)))
        .collect();

    for cell in &doomed {
        release(context, *cell);
    }
    doomed.len()
}

/// Clears every side table a cell might be named in, frees whatever `Slab`
/// payload it owned, and gives the cell itself back.
///
/// # Order
///
/// Every `Slab`/`Aside` read or removed BEFORE `Region::free` — freeing the
/// cell first would still leave every table below correct (they are keyed by
/// index, not by the header `free` overwrites), but reading `region.type_of`
/// and `region.field` after the free would be reading a cell already spliced
/// into the free list, which is exactly the kind of "still looks like an
/// object" trap this crate's rules keep naming. Doing the reads first removes
/// the question.
fn release(context: &mut Context, cell: u32) {
    // Every watch on this cell, cleared before the cell goes back — a watcher
    // reading a reused cell would read somebody else's object and every
    // question would answer plausibly. See `super::weak`.
    super::weak::clear_freed(context, cell);
    // A text cell's payload is a RAW slab index sitting in field 0, not an
    // encoded `Value` — `trace.rs`'s own documentation flags this as the one
    // thing the marker cannot follow, and says the sweep must reach it
    // deliberately. This is that reach.
    if context.region.type_of(cell) == Some(context.text_type.index() as u32)
        && let Some(slot) = context.region.field(cell, 0)
    {
        context.cells.free(Slot(slot as u32));
    }

    // The overflow is region cells now, and it spans — so every cell it covers
    // comes back, not only the one its reference names.
    if let Some((block, slots)) = context.spill_of.remove(cell) {
        context.region.free_spanning(block, (slots + 1) * 8);
    }
    if let Some(elements) = context.array_elements.remove(cell) {
        context.arrays.free(elements);
    }
    if let Some(buffer) = context.buffer_of.remove(cell) {
        context.buffers.free(buffer);
    }

    // Every remaining `Aside` in `Context`, by its field list — see this
    // module's own documentation for what keeps this exhaustive and what does
    // not.
    context.prototypes.remove(cell);
    // Keyed by the cell that WAS a prototype, so reclaiming it drops the numbers
    // minted against it — which is what stops the free list handing the index
    // back and an unrelated object inheriting a stale discrimination.
    context.proto_types.remove(cell);
    context.callables.remove(cell);
    context.proxies.remove(cell);
    context.cursors.remove(cell);
    context.bound.remove(cell);
    context.views.remove(cell);
    context.collections.remove(cell);
    context.generators.remove(cell);
    context.regexes.remove(cell);
    context.accessors.remove(cell);
    context.integrity.remove(cell);
    context.attributes.remove(cell);
    context.derived.remove(cell);
    context.class_constructors.remove(cell);
    context.boxed.remove(cell);
    // The word a client attached. Dropped with the cell and nothing is called —
    // `super::foreign` says so where an addon author will read it.
    context.foreign.remove(cell);
    // Whatever asked to be told about this death, moved to the run queue. NOT
    // called: this runs with the borrow held, and a finalizer calls out. See
    // `super::finalize`.
    super::finalize::queue_freed(context, cell);

    context.region.free(cell);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Kinds, Singletons, Value};

    fn empty_context() -> Context {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        };
        Context::new(singletons, Kinds::in_declaration_order())
    }

    fn plain(context: &mut Context) -> u32 {
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
        context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room")
    }

    /// Runs [`collect`] against an EMPTY, controlled stack range rather than
    /// this thread's real one.
    ///
    /// The real stack holds whatever the test harness left on it — return
    /// addresses, saved registers, a prior test's locals — and a stray word
    /// among them can coincidentally decode as a reference to a cell this
    /// test allocated, which is sound (conservative retention, never a wrong
    /// free) but makes a fixed-count assertion flaky. `roots.rs`'s own tests
    /// use the same fixed, zeroed buffer for exactly this reason.
    fn collect_over_empty_stack(context: &mut Context) -> usize {
        let buffer = [0u64; 4];
        let low = buffer.as_ptr() as usize;
        let high = low + core::mem::size_of_val(&buffer);
        context.stack_high = Some(high);
        collect(context, low)
    }

    #[test]
    fn without_a_stack_high_nothing_is_freed() {
        let mut context = empty_context();
        let orphan = plain(&mut context);
        let buffer = [0u64; 4];
        let low = buffer.as_ptr() as usize;

        assert_eq!(context.stack_high, None);
        assert_eq!(collect(&mut context, low), 0, "the refusal, not a smaller run");
        assert!(context.region.type_of(orphan).is_some(), "nothing touched it");
    }

    #[test]
    fn an_orphan_is_freed_and_a_root_survives() {
        let mut context = empty_context();
        let root = plain(&mut context);
        let orphan = plain(&mut context);
        context.globals = Some(root);

        let freed = collect_over_empty_stack(&mut context);
        assert_eq!(freed, 1, "only the orphan");
        let live: Vec<u32> = context.region.live_refs();
        assert!(live.contains(&root), "the root survives");
        assert!(
            !live.contains(&orphan),
            "freed — `Region::type_of` alone cannot tell, since it reads a \
             freed header's raw bits rather than checking the marker"
        );
    }

    #[test]
    fn a_value_held_from_outside_the_heap_survives_and_stops_when_released() {
        // The whole reason `entry::external` exists. Nothing on this thread's
        // stack names the cell — the buffer above is zeroed — and no `Context`
        // field points at it, so before that module the collector was RIGHT to
        // free it, and an N-API addon holding it would have been left with a
        // handle to a reused cell.
        let mut context = empty_context();
        let kept = plain(&mut context);
        let held = super::super::external::hold(&mut context, Value::from_slot(kept).bits());

        assert_eq!(collect_over_empty_stack(&mut context), 0, "nothing to free");
        assert!(
            context.region.live_refs().contains(&kept),
            "an externally held value is a root"
        );

        super::super::external::release(&mut context, held);
        assert_eq!(
            collect_over_empty_stack(&mut context),
            1,
            "released, and now nothing names it"
        );
        assert!(!context.region.live_refs().contains(&kept));
    }

    #[test]
    fn a_watch_is_cleared_by_the_collection_that_frees_what_it_watched() {
        // `entry::weak`'s whole claim, against a real cycle rather than against
        // `clear_freed` called by hand. A watch that survived would hand its
        // holder a word naming a cell the region is free to hand to somebody
        // else — plausible answers to every question, which is the failure mode
        // that module exists to remove.
        let mut context = empty_context();
        let doomed = plain(&mut context);
        let watch = super::super::weak::watch(&mut context, Value::from_slot(doomed).bits())
            .expect("a cell is watchable");

        assert!(matches!(
            super::super::weak::peek(&context, watch),
            Some(Some(_))
        ));
        assert_eq!(collect_over_empty_stack(&mut context), 1, "nothing named it");
        assert_eq!(
            super::super::weak::peek(&context, watch),
            Some(None),
            "the watch remains and answers that its value is gone"
        );
    }

    #[test]
    fn a_watch_does_not_keep_its_value_alive() {
        // The difference from `entry::external`, and the reason both exist. If
        // watching rooted, this collection would free nothing.
        let mut context = empty_context();
        let cell = plain(&mut context);
        let _ = super::super::weak::watch(&mut context, Value::from_slot(cell).bits());
        assert_eq!(
            collect_over_empty_stack(&mut context),
            1,
            "a watch is not a root"
        );
    }

    #[test]
    fn a_collection_queues_a_finalizer_and_does_not_call_it() {
        // The separation `entry::finalize` exists for. Calling from here would
        // be calling out with the borrow held, which this crate aborts on — so
        // the assertion is deliberately about the QUEUE rather than about the
        // callback having run.
        extern "C" fn never(_: usize, _: usize) {
            unreachable!("the sweep must not call a finalizer");
        }

        let mut context = empty_context();
        let doomed = plain(&mut context);
        super::super::finalize::on_death(
            &mut context,
            Value::from_slot(doomed).bits(),
            super::super::finalize::Pending {
                code: never,
                data: 0,
                hint: 0,
            },
        )
        .expect("a cell is registrable");

        assert_eq!(collect_over_empty_stack(&mut context), 1);
        assert_eq!(
            context.dying.len(),
            1,
            "queued for the next drain, not called during the sweep"
        );
        assert!(
            context.deaths.is_empty(),
            "and no longer waiting — a death happens once"
        );
    }

    #[test]
    fn a_freed_text_cells_slab_slot_is_reclaimed() {
        let mut context = empty_context();
        let text = context.intern_value(crate::text::Str::from_str("gone"));
        let _cell = text.as_slot().expect("a text cell");
        let before = context.cells.len();

        let freed = collect_over_empty_stack(&mut context);

        assert_eq!(freed, 1);
        assert_eq!(
            context.cells.len(),
            before - 1,
            "the raw slab slot the header pointed at, not just the cell"
        );
    }

    #[test]
    fn a_reachable_aside_entry_survives_and_an_unreachable_one_is_cleared() {
        let mut context = empty_context();
        let kept = plain(&mut context);
        let dropped = plain(&mut context);
        context.set_prototype(kept, Value::from_slot(0).bits());
        context.set_prototype(dropped, Value::from_slot(0).bits());
        context.globals = Some(kept);

        collect_over_empty_stack(&mut context);

        assert!(context.prototype_at(kept).is_some());
        assert!(
            context.prototype_at(dropped).is_none(),
            "the aside entry a stranger reusing this cell would otherwise inherit"
        );
    }
}
