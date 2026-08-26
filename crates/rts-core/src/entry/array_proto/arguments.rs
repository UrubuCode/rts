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
    with_arguments_at(context, from, given, <[u64]>::to_vec)
}

/// The arguments a call carried, OWNED, without allocating for the common count.
///
/// # Why this exists beside the other two
///
/// [`with_arguments_at`] hands over a slice and is the cheapest thing here, but
/// its slice may borrow the context — so a caller that must give the borrow back
/// before it can act cannot use it. Two such callers are the reason this type
/// exists, and neither is unusual:
///
/// - `push` appends through `elements_at_mut`, a MUTABLE borrow of the same
///   context a spilled slice comes from.
/// - `Math.max` coerces each value with `to_number`, which takes a borrow of its
///   own, and `math.rs` records that nesting them "is a panic on the re-entry".
///
/// Both used [`arguments_at`] and paid a heap allocation per call for it. This
/// holds the four-slot case in the value itself, so the overwhelmingly common
/// call allocates nothing and the spilled one behaves exactly as before.
///
/// # Why an enum rather than a buffer at each call site
///
/// Because "four slots, then a vector" is one rule about what a call carried,
/// and this module exists on the principle that such a rule is stated once. A
/// buffer written out at two call sites is that rule written twice, and the
/// second copy is where the two come to disagree about `from`.
pub enum Arguments {
    /// Held in the value: what the convention's four slots carried.
    Inline([u64; 4], usize),
    /// The call spilled past four, so the runtime's vector was copied out.
    Spilled(Vec<u64>),
}

impl Arguments {
    /// What the call carried, whichever way it is held.
    pub fn as_slice(&self) -> &[u64] {
        match self {
            Self::Inline(held, count) => &held[..*count],
            Self::Spilled(held) => held,
        }
    }
}

/// [`arguments_at`] without the allocation, for a caller that needs to own them.
pub fn arguments_owned_at(context: &Context, from: usize, given: [u64; 4]) -> Arguments {
    with_arguments_at(context, from, given, |args| {
        let mut inline = [0u64; 4];
        match args.len() <= inline.len() {
            true => {
                inline[..args.len()].copy_from_slice(args);
                Arguments::Inline(inline, args.len())
            }
            false => Arguments::Spilled(args.to_vec()),
        }
    })
}

/// The same, handed to `take` as a SLICE rather than materialised.
///
/// # Why this exists beside [`arguments_at`]
///
/// Because the four slots are already on this frame's stack and the spilled
/// vector is already in the context, so building a `Vec` allocates to hold what
/// is in front of both of them. That is one heap allocation per call to every
/// native that reads its arguments — `push`, `unshift`, `concat`, `Array.of`,
/// `Math.max`, `Function.prototype.call` — and `a.push(i)` pays it to carry a
/// single word whose count the SITE already declared.
///
/// Measured 2026-08-26, release: `a.push(i)` cost 179 ns against Node's 10, and
/// an indexed write of the same element cost 71 — so the native call path was
/// ~108 ns over doing the append directly.
///
/// [`arguments_at`] stays, as a wrapper: twenty-three callers want an owned
/// vector because they hand it onward or hold it across user code, and this file
/// exists on the principle that there is one rule about what a call carried and
/// therefore one function stating it. That function is now this one.
///
/// # Why a closure rather than an iterator or a returned slice
///
/// The two answers live in different places — one borrows the context, the other
/// borrows this frame's argument array — so no single lifetime describes both. A
/// closure lets each arm hand over what it already has.
pub fn with_arguments_at<R>(
    context: &Context,
    from: usize,
    given: [u64; 4],
    take: impl FnOnce(&[u64]) -> R,
) -> R {
    let spilled = context
        .pending_arguments
        .last()
        .copied()
        .and_then(|vector| Value(vector).as_slot())
        .and_then(|cell| context.elements_at(cell));
    if let Some(elements) = spilled {
        return take(&elements[from.min(elements.len())..]);
    }
    // What the SITE said it wrote, when it said. The doc above described the
    // guess below as the only available answer and named its divergence —
    // `a.push(undefined)` pushes nothing — and the guess is no longer the only
    // one: `call_counted` carries the number, so the four slots can be cut at
    // the right place instead of at the last non-`undefined`.
    if let Some(count) = context.pending_counts.last().copied().flatten() {
        let count = count.min(given.len());
        return take(&given[from.min(count)..count]);
    }
    // The trailing `undefined` is CUT rather than popped, which is the same
    // answer without owning anything: the old spelling built a `Vec` in order to
    // shorten it.
    let mut end = given.len();
    while end > 0 && absent(context, given[end - 1]) {
        end -= 1;
    }
    take(&given[from.min(end)..end])
}
