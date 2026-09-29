//! The element buffers of dead arrays, kept for the arrays made next.
//!
//! # What this removes, measured
//!
//! `KEEP = [i, i]` cost 126 ns where `KEEP = []` cost 69 (release,
//! 2026-09-29): 57 ns for two elements, none of it the elements. An array's
//! elements are a `Vec<u64>` beside its cell, so a non-empty array is one trip
//! to the process allocator to be born and another to die, and an empty one is
//! neither — a `Vec` with nothing in it allocates nothing.
//!
//! The collector frees arrays a region at a time and the program makes them
//! one at a time, so every buffer a sweep gives back is one the next few
//! thousand arrays would have asked the allocator for. They are kept.
//!
//! # What this is instead of
//!
//! Swapping the process allocator, which `text/narrow.rs` records as measured
//! and refused, for a reason that holds here unchanged: a quicker allocation is
//! still an allocation. And holding short arrays INLINE, as that module holds
//! short strings — refused because an array grows: `push` on an inline array
//! would have to move it out, and the cell a compiled site indexes through
//! would change under it.
//!
//! # Why it is bounded twice
//!
//! By WIDTH, because a buffer kept is memory held: a program that made one
//! array of a million elements must get that back. By COUNT, because the
//! buffers of one sweep are at most the cells of one region, and keeping more
//! than the next cycle can use is keeping them for nothing.
//!
//! # Why nothing here is a root
//!
//! A buffer is cleared before it is kept, so it holds no value, and it is
//! reached from no cell — the array it belonged to is gone.

use super::Context;

/// The widest buffer kept, in elements.
const WIDEST: usize = 16;
/// How many are kept: the cells of the region a program starts with.
const KEPT: usize = 1 << 16;

/// A buffer holding `values`, from a dead array's where there is one.
pub(super) fn holding(context: &mut Context, values: &[u64]) -> Vec<u64> {
    if values.is_empty() || values.len() > WIDEST {
        return values.to_vec();
    }
    match context.spare_arrays.pop() {
        Some(mut buffer) => {
            buffer.extend_from_slice(values);
            buffer
        }
        None => {
            // Born at the full width, so that whichever array takes it next
            // fits without asking again.
            let mut buffer = Vec::with_capacity(WIDEST);
            buffer.extend_from_slice(values);
            buffer
        }
    }
}

/// Keeps the buffer of an array that died, where it is one worth keeping.
pub(super) fn keep(context: &mut Context, mut buffer: Vec<u64>) {
    if buffer.capacity() == 0
        || buffer.capacity() > WIDEST
        || context.spare_arrays.len() >= KEPT
    {
        return;
    }
    buffer.clear();
    context.spare_arrays.push(buffer);
}
