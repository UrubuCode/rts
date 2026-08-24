//! The one entry point the machine names itself.
//!
//! # Why this is not a `RuntimeOp`
//!
//! Every other entry point in this crate is the LANGUAGE asking for something:
//! `rts-codegen` decides that `a + b` needs a call and names the symbol. This
//! one is the MACHINE asking. `Inst::Alloc` lowers to a call to `rts_alloc`
//! whatever language is being compiled, because asking a heap for space is the
//! one memory operation that is not arithmetic:
//!
//! > Reading and writing a field is arithmetic and lands here; asking a heap for
//! > space is a runtime entry point, and a runtime entry point is something to
//! > declare rather than something to emit.
//!
//! So the name is not ours to choose. `RtEntry::Alloc` says `rts_alloc`, without
//! the `__rts_` the language's own operations carry, and the attribute is given
//! that name explicitly rather than deriving one that would not match.
//!
//! # What the descriptor this emits is not
//!
//! Authoritative. `RtEntry::Alloc` states the signature —
//! `(I64, I64) -> Ref(Opaque)` — and that is what compiled code is built
//! against. The attribute derives `(Tagged, Tagged) -> Tagged` from the Rust
//! types, which agrees about the machine words and not about what they mean.
//!
//! Saying so beats inventing a Rust type whose only purpose is to make a derived
//! descriptor match a descriptor nobody reads: nothing consults this one,
//! because for a machine entry the machine's own table is the contract.

use super::{Context, with_current};

/// Asks the region for a cell, collecting once and retrying if it is full.
///
/// # The single place every allocation-failure call site reaches for
///
/// This crate has SEVEN places that call `context.region.alloc` and act on a
/// full region — this file's own [`alloc`], plus one each in `objects.rs`,
/// `functions.rs` (two), `native.rs` (two) and `array.rs` and `modules.rs`.
/// Before this existed, only [`alloc`] — the one entry point the MACHINE
/// itself calls — ever collected; the other six went straight to
/// [`heap_exhausted`] the moment the region reported full, which is why a
/// benchmark allocating three million short-lived objects, one at a time
/// through `new`, exhausted the region on its first run even with a working
/// collector sitting right next to the call: `new` reaches the region through
/// `functions.rs`, never through `rts_alloc`. Routing every site through this
/// one function is what makes a full region try to make room before it gives
/// up, wherever the language is the one asking.
///
/// # Mind the borrow
///
/// Every caller already holds the `&mut Context` `with_current` handed its own
/// closure — see `entry::current`'s own documentation for the `RefCell` that
/// backs it. A collection needs that same borrow, and asking `with_current`
/// for it again from inside would be a second `borrow_mut` on a `RefCell`
/// already borrowed, which panics inside an `extern "C"` frame — an abort, not
/// a catchable error, and precisely the trap this crate's README names for a
/// native that reaches for the ambient context twice. So this takes `&mut
/// Context` directly, on the borrow the caller already has, and calls
/// `collect_cycle::collect` the same way — never through `with_current`.
pub(super) fn alloc_after_collecting(context: &mut Context, size: u32, ty: u32) -> Option<u32> {
    if let Some(cell) = context.region.alloc(size, ty) {
        return Some(cell);
    }

    // A local right here approximates this thread's current stack pointer —
    // see `collect_cycle::collect`'s own documentation for why that is sound.
    // Taken in THIS frame rather than the caller's or the collector's, so it
    // is a fixed, known-correct distance from `stack_high` regardless of how
    // deep the call that ran out of room was nested.
    let anchor = 0u64;
    let stack_low = &anchor as *const u64 as usize;
    // Captured HERE and not inside the collector: this frame is the one that
    // took `stack_low`, so whatever its own prologue saved sits above that
    // address and the stack walk finds it. The collector's prologue saves
    // below it, where the walk does not reach — see `registers.rs`.
    let registers = super::registers::callee_saved();
    super::collect_cycle::collect(context, stack_low, &registers);

    if let Some(cell) = context.region.alloc(size, ty) {
        return Some(cell);
    }
    grow_and_retry(context, |region| region.alloc(size, ty))
}

/// Raises the region's bound until the allocation fits, or the reservation is
/// spent.
///
/// # Why growing comes AFTER a collection and never before
///
/// Because the two answers to a full region are not interchangeable. A
/// collection costs a cycle and gives the memory back; growth costs memory and
/// never gives it back — the bound only ever rises, since lowering it would
/// abandon cells references still name. Asking the collector first is what
/// keeps a program whose garbage is collectable — the common one — at the
/// working set it actually has, instead of at the high-water mark of its
/// allocation rate. The loop's own shape says the same thing: it is reached
/// only when a full cycle reclaimed too little.
///
/// A loop rather than one doubling, because one allocation can need more than
/// one step: a spanning object wants a RUN of cells, and a single doubling can
/// leave the bound past `next` without leaving that run contiguous.
fn grow_and_retry(
    context: &mut Context,
    mut attempt: impl FnMut(&mut crate::heap::Region) -> Option<u32>,
) -> Option<u32> {
    while context.region.grow() {
        if let Some(cell) = attempt(&mut context.region) {
            return Some(cell);
        }
    }
    None
}

/// The same, ending the program when a collection did not make enough room.
///
/// What every call site that has no way to answer "no" reaches for — see
/// [`heap_exhausted`] for why that is a report rather than a value the program
/// gets back.
pub(super) fn alloc_or_die(context: &mut Context, size: u32, ty: u32) -> u32 {
    match alloc_after_collecting(context, size, ty) {
        Some(cell) => cell,
        None => heap_exhausted(context),
    }
}

/// The same for an object that spans several cells, dying if it cannot fit.
///
/// Separate from [`alloc_or_die`] because the region refuses an oversized
/// object through `alloc` on purpose — a caller that spans has to say so — and
/// because the retry after a collection is the part worth not duplicating: a
/// spill block that answered `None` and was silently dropped would lose a
/// property WRITE, which is a wrong answer rather than a slow one.
pub(super) fn alloc_spanning_or_die(context: &mut Context, size: u32, ty: u32) -> u32 {
    if let Some(cell) = context.region.alloc_spanning(size, ty) {
        return cell;
    }
    let anchor = 0u64;
    let stack_low = &anchor as *const u64 as usize;
    // Captured HERE and not inside the collector: this frame is the one that
    // took `stack_low`, so whatever its own prologue saved sits above that
    // address and the stack walk finds it. The collector's prologue saves
    // below it, where the walk does not reach — see `registers.rs`.
    let registers = super::registers::callee_saved();
    super::collect_cycle::collect(context, stack_low, &registers);
    if let Some(cell) = context.region.alloc_spanning(size, ty) {
        return cell;
    }
    match grow_and_retry(context, |region| region.alloc_spanning(size, ty)) {
        Some(cell) => cell,
        None => heap_exhausted(context),
    }
}

/// Asks the heap for a cell.
///
/// The one entry point the MACHINE calls rather than the language — see this
/// module's own documentation. Its failure path is [`alloc_or_die`], the same
/// one every other allocation site in this crate now shares.
#[rtse::entry("rts_alloc")]
pub fn alloc(size: i64, ty: i64) -> u64 {
    with_current(|context| {
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let ty = u32::try_from(ty).unwrap_or(u32::MAX);
        // Was `unwrap_or(0)` — cell ZERO, which is a real object belonging to
        // somebody else. Every other allocation in this crate carried a
        // comment saying that was the thing it was avoiding by answering
        // `undefined` instead, and this one did it.
        u64::from(alloc_or_die(context, size, ty))
    })
}

/// Ends the program because the heap is full.
///
/// # Why this is not a value the program gets back
///
/// It was. Every allocation answered `undefined` when the region had no room,
/// with a comment at each saying that was "less wrong than handing back cell
/// zero" — which is true, and was measuring the wrong pair. The comparison that
/// matters is against **saying so**, and against that, `undefined` loses badly:
/// the program carries on, adds `undefined` to a number, and answers `NaN`.
///
/// This was not hypothetical and it was not caught by reading. A benchmark in
/// this repository allocated forty thousand objects in a region that holds
/// sixty-five thousand cells, ran out, and reported a beautiful two-threads-for-
/// free number — for a program whose answer was the canonical `NaN` and whose
/// timing was of the failure path. It looked exactly like a measurement. That is
/// the honesty floor's own example, produced by this crate's own behaviour.
///
/// # Why exit and not throw
///
/// A `RangeError` is what the language would raise, and `entry::throw` records
/// why it cannot be raised where a handler could catch it: that needs an
/// exception table and a personality routine. So the choice is between a value
/// nobody can act on and a report, and the report is what the same file already
/// chose for an uncaught exception.
///
/// `exit`, not `abort`: this is a program ending because of something the
/// program did — it asked for more memory than the region has — and a core dump
/// describes the engine rather than the fault.
///
/// # What still removes this
///
/// [`alloc`] collects, and then GROWS, before reaching here — so this now runs
/// only when a full cycle reclaimed too little AND the region's whole
/// reservation is spent. The reservation is the ceiling because the base is an
/// immediate in the compiled code and therefore cannot move; `heap::Region`
/// carries the reasoning and what was rejected.
///
/// It reports the reservation rather than the current bound, because the
/// current bound is by then equal to it and the reservation is the number a
/// reader can act on.
pub(super) fn heap_exhausted(context: &Context) -> ! {
    eprintln!(
        "rts: heap exhausted — the region grew to its whole reservation of {} \
         cells and all of them are in use even after a collection.\n     \
         Nothing left to reclaim, and the program cannot continue.\n     What \
         would have happened instead is `undefined` from the allocation, which \
         computes a wrong answer quietly.",
        context.region.reserved()
    );
    std::process::exit(1);
}
