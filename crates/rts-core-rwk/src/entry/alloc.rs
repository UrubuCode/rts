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

/// Asks the heap for a cell.
///
/// # What it returns when the heap is full
///
/// Zero, which is a reference to cell zero — a real object, and the wrong one.
/// That is a **known defect** and it is here rather than hidden because the
/// alternative today is worse: the signature returns a reference and has no way
/// to say "no", and a compiled program has no handler to send a failure to.
///
/// It goes when there is a collector to ask first, or protected regions to throw
/// through. Until then a program that exhausts its region is wrong, and this
/// records where.
#[rtse::entry("rts_alloc")]
pub fn alloc(size: i64, ty: i64) -> u64 {
    with_current(|context| {
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let ty = u32::try_from(ty).unwrap_or(u32::MAX);
        match context.region.alloc(size, ty) {
            Some(cell) => u64::from(cell),
            // Was `unwrap_or(0)` — cell ZERO, which is a real object belonging
            // to somebody else. Every other allocation in this crate carried a
            // comment saying that was the thing it was avoiding by answering
            // `undefined` instead, and this one did it.
            None => heap_exhausted(context),
        }
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
/// # What removes this
///
/// A collector. There is none, so a full region is genuinely the end: nothing
/// can be reclaimed and no larger region can be moved to, because the base is an
/// immediate in the compiled code. Growing is the collector's business, which is
/// what "compacting by requirement rather than by preference" means.
pub(super) fn heap_exhausted(context: &Context) -> ! {
    eprintln!(
        "rts: heap exhausted — the region holds {} cells and all of them are in \
         use.\n     There is no collector yet, so nothing can be reclaimed and \
         the program cannot continue.\n     What would have happened instead is \
         `undefined` from the allocation, which computes a wrong answer quietly.",
        context.region.capacity()
    );
    std::process::exit(1);
}
