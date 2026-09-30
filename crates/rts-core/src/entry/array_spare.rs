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
//! # Why they are kept by WIDTH
//!
//! One pile did not work, and the number said so: with buffers of every width
//! in it, an array of two elements drew a buffer of one and had to grow it,
//! and drawing again on the next call drew another of one — `[i, i]` read 121
//! ns against 97 without the pile at all (release, 2026-09-30). A pile per
//! power of two, and a buffer BORN at that width where its pile is empty, so
//! that what the next array of that size draws fits without growing.
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
//! array of a million elements must get that back. By COUNT per pile, because
//! the buffers of one sweep are at most the cells of one region, and keeping
//! more than the next cycle can use is keeping them for nothing.
//!
//! # Why nothing here is a root
//!
//! A buffer is cleared before it is kept, so it holds no value, and it is
//! reached from no cell — the array it belonged to is gone.

use super::Context;

/// The widest buffer kept, in elements: the largest pile's width.
const WIDEST: usize = 16;
/// The piles: widths 1, 2, 4, 8 and 16.
pub(super) const PILES: usize = 5;
/// How many each pile keeps: the cells of the region a program starts with.
const KEPT: usize = 1 << 16;

/// Which pile a buffer for `len` elements is drawn from, and the width its
/// buffers are born at.
fn pile_for(len: usize) -> (usize, usize) {
    let width = len.max(1).next_power_of_two();
    (width.trailing_zeros() as usize, width)
}

/// A buffer holding `values`, from a dead array's where there is one.
pub(super) fn holding(context: &mut Context, values: &[u64]) -> Vec<u64> {
    if values.is_empty() || values.len() > WIDEST {
        return values.to_vec();
    }
    let (pile, width) = pile_for(values.len());
    let mut buffer = match context.spare_arrays[pile].pop() {
        Some(buffer) => buffer,
        None => Vec::with_capacity(width),
    };
    buffer.extend_from_slice(values);
    buffer
}

/// Keeps the buffer of an array that died, where it is one worth keeping: a
/// buffer at exactly a pile's width, so that what is drawn fits.
pub(super) fn keep(context: &mut Context, mut buffer: Vec<u64>) {
    let capacity = buffer.capacity();
    if capacity == 0 || capacity > WIDEST || !capacity.is_power_of_two() {
        return;
    }
    let (pile, _) = pile_for(capacity);
    if context.spare_arrays[pile].len() >= KEPT {
        return;
    }
    buffer.clear();
    context.spare_arrays[pile].push(buffer);
}
