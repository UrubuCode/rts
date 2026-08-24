//! Where an array's elements are, and how many — the pair a bounded load needs.
//!
//! # Why these two are together and apart from `array.rs`
//!
//! They answer one question in two halves: `rts_cranelift::ir::inst::Inst::ElementLoad`
//! takes a base, an index and a length, and a client that has the first without
//! the third cannot emit one. Splitting them across files is how a caller ends
//! up with a base it cannot use.
//!
//! Apart from `array.rs` because that file is 721 lines against this crate's
//! 500-line ceiling. Rule 6 says new code lands in a small focused module rather
//! than being appended to one that is over, and moving the base's half along
//! with the count keeps the pair legible instead of straddling the boundary.

use super::with_current;
use crate::value::Value;

/// Where an array's elements START, as a machine address.
///
/// # What the caller is taking on
///
/// That the run does not MOVE while it holds this. The elements are a `Vec`,
/// and pushing to one reallocates — so an address handed out here is good only
/// for as long as nothing grows that array.
///
/// The one caller is `rts-codegen`'s `for-of` desugaring, and it is safe there
/// for a reason nothing else can borrow: the array is the copy `iterate` just
/// made, no program can name it, and the loop only reads. `iterate` copies
/// deliberately — its own documentation says a body that pushes to the original
/// must not walk its own additions — and that same copy is what makes the
/// address stable.
///
/// # That caller does not fire, and this says so rather than implying otherwise
///
/// `foreach.rs` hoists only when the loop's bound is a proven double, and the
/// bound is a property read, which that layer always answers generically. The
/// condition is unsatisfiable by construction, so this is never called and
/// `Inst::ElementLoad` is never emitted — `rts ir` over 59 files (the benches
/// and every `array_*`/`for_of*` test), 2026-08-23: **zero**.
///
/// Not a producer-less structure, which rule 9 would forbid: the producer is
/// written and refused by one predicate. It needs a PROVEN `length`, and until
/// then every `for-of` pays [`element_at`] per element.
///
/// **Do not price that gap by differencing a `for-of` that reads its binding
/// against one that ignores it.** The binding is pushed unconditionally, so the
/// call is in both arms and cancels; the difference measures an unbox, not a
/// load.
///
/// Answers `0` for anything that is not an array, which the caller must treat
/// as "no run": zero elements, so a bounded read of it is refused by its own
/// bound before the address is ever used.
#[rtse::entry]
pub fn elements_base(array: u64) -> i64 {
    with_current(|context| {
        let Some(cell) = Value(array).as_slot() else {
            return 0;
        };
        match context.elements_at(cell) {
            Some(elements) => elements.as_ptr() as i64,
            None => 0,
        }
    })
}

/// How many elements an array's run holds.
///
/// # Why this exists rather than converting the loop's own bound
///
/// [`Inst::ElementLoad`] takes the count as a **proven** `I32`, and the bound a
/// `for-of` already has is `enumerated.length` — a property read, which
/// `rts-codegen` answers generically. Turning that into a proven integer means
/// narrowing, and narrowing is only reachable through a guard (machine rule 11),
/// whose failure path would need a second copy of the whole loop body.
///
/// So the count is not narrowed; it is **asked for** in the form the instruction
/// takes. Machine rule 10 is the same argument from the other side: an operation
/// does not accept both a proven and a generic operand, so the way to get a
/// proven one is a separate operation with its own name and its own cost.
///
/// # Why it is not folded into [`elements_base`]
///
/// One `extern "C"` return. Two crossings for the two halves, once per loop
/// rather than once per element, is what the whole hoist is trading against —
/// and a pair of entry points is what `RuntimeOp::ElementsBase`'s own
/// documentation already assumes a caller will need.
///
/// # What the caller is taking on
///
/// The same contract [`elements_base`] states, and it is the same array: the
/// count is good only while nothing grows the run. `for-of` walks the copy
/// `iterate` made, which no program can name.
///
/// Answers `0` for anything that is not an array — the same "no run" the base
/// answers with, and a bounded load of zero words is refused by its own bound.
#[rtse::entry]
pub fn elements_count(array: u64) -> i32 {
    with_current(|context| {
        let Some(cell) = Value(array).as_slot() else {
            return 0;
        };
        match context.elements_at(cell) {
            // Saturating rather than wrapping: a run longer than `i32::MAX` is
            // not something this region can hold, and a negative count would be
            // a bound the machine reads as enormous.
            Some(elements) => i32::try_from(elements.len()).unwrap_or(i32::MAX),
            None => 0,
        }
    })
}
