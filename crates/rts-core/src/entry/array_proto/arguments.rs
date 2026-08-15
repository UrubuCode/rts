//! What a call actually carried, for a native that takes any number of them.
//!
//! # Why this is here rather than beside `functions::rest_arguments`
//!
//! That entry point answers the same question for a **compiled** rest parameter,
//! and it answers it as an array — a region cell, in a region nothing collects.
//! A native folding over its arguments wants a `Vec` and nothing else, so
//! `Math.max(a, b)` in a loop must not spend a cell an iteration. The question is
//! shared; the shape of the answer is not, and that is the whole difference.
//!
//! It is in this folder because the four-slot version of it was, as `args` —
//! `push`, `unshift`, `concat`, `Array.of` and `Array()` are five natives that
//! already had to drop the convention's padding — and growing that function to
//! see past the four slots is what fixed all five at once. `Math` and
//! `Function.prototype` reach it here for the same reason they would have reached
//! a second copy: there is one rule about what a call carried, so there is one
//! function that states it.

use super::super::Context;
use super::super::string::absent;
use crate::value::Value;

/// The arguments a call actually carried, from `from` on.
///
/// Past four the caller built the vector and the runtime is holding it, which is
/// what makes `a.push(1, 2, 3, 4, 5)` push five and `Math.max(1, 5, 3, 8, 2)`
/// answer 8. Below that there is no vector, and trailing `undefined` is dropped
/// because the convention pads missing arguments with it and a native cannot tell
/// padding from an argument a program wrote. The divergence, named:
/// `a.push(undefined)` pushes nothing.
///
/// `from` is how a method whose first slot is spent on something else — the
/// receiver `Function.prototype.call` takes — skips it in the vector and in the
/// slots with one rule rather than two.
///
/// Takes the context rather than borrowing one, because every caller is already
/// inside `with_current` and a second borrow is the re-entry this folder's split
/// exists to make impossible.
pub fn arguments_at(context: &Context, from: usize, given: [u64; 4]) -> Vec<u64> {
    let spilled = context
        .pending_arguments
        .last()
        .copied()
        .and_then(|vector| Value(vector).as_slot())
        .and_then(|cell| context.elements_at(cell));
    if let Some(elements) = spilled {
        return elements.iter().skip(from).copied().collect();
    }
    // What the SITE said it wrote, when it said. The doc above described the
    // guess below as the only available answer and named its divergence —
    // `a.push(undefined)` pushes nothing — and the guess is no longer the only
    // one: `call_counted` carries the number, so the four slots can be cut at
    // the right place instead of at the last non-`undefined`.
    if let Some(count) = context.pending_counts.last().copied().flatten() {
        let count = count.min(given.len());
        return given[from.min(count)..count].to_vec();
    }
    let mut given = given.to_vec();
    while given.last().is_some_and(|last| absent(context, *last)) {
        given.pop();
    }
    given.into_iter().skip(from).collect()
}
