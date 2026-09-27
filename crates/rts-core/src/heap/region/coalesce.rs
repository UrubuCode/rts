//! Free space rebuilt from the headers after a collection, so that a wide
//! object can be born where narrow ones died.
//!
//! # The failure this ends
//!
//! [`Region::free`] threads a one-cell object onto the linked free list and
//! pushes a wide one onto `free_runs`, and the two never meet: a stretch of
//! ten consecutive dead one-cell objects is ten links and no run. So once the
//! bump space is spent, a spanning allocation can be satisfied only by a run
//! some earlier WIDE object gave back — and a program whose wide objects are
//! the first of their kind finds none, collects, finds none again, grows, and
//! from then on runs a whole collection per allocation.
//!
//! Measured 2026-09-26 on `bench/analytic.ts`'s shape, release: an object
//! literal of 15 or 16 properties cost **13 400 ns** after two rows of narrow
//! literals had spent the bump space, against 112 for one of 12 properties and
//! 450 for the same wide literal in a fresh heap. `RTS_GC_DEBUG=1` counted a
//! collection per allocation.
//!
//! # What this does instead
//!
//! When a spanning allocation finds no run that fits and no bump space, one
//! pass over the headers rebuilds both structures: every stretch of
//! consecutive free cells becomes one run, and a run of one goes on the linked
//! list. The ask is then repeated, and only if it still fails does the
//! collector run. A narrow allocation that finds the linked list empty may
//! split a run as well, which is what keeps a run from being hoarded for wide
//! objects that never come.
//!
//! # Why not after every sweep
//!
//! It was, and it cost every narrow allocation: with 96% of the heap dead, the
//! whole free space coalesces into a few runs and the linked list is nearly
//! empty, so every new cell was the next address of a run — a COLD cache line
//! each time, where the LIFO list hands back the cell freed most recently, which
//! is hot. Measured 2026-09-26, release: `s + "-"` went from 145 ns to 394 and
//! `String(i)` from 172 to 470, while the wide literal fell as intended. Doing
//! it on demand keeps the list's locality for the common allocation and pays
//! the pass only where a wide object would otherwise pay a collection.
//!
//! Why not coalesce in [`Region::free`] itself: a freed cell's neighbour is
//! already threaded into a singly-linked list, which cannot be unlinked in
//! place; and the sweep frees cells in address order anyway, so one pass at
//! the end sees every adjacency the frees produced.

use super::{FREE_MARKER, NO_NEXT, Region};

impl Region {
    /// Rebuilds the linked free list and the run list from the headers,
    /// coalescing every stretch of consecutive free cells into one run.
    ///
    /// Called by [`Region::alloc_spanning`] when no run fits. Cells a live wide
    /// object spans are skipped by their interior flag, since their first word
    /// is a field and not a header — it could equal [`FREE_MARKER`] by
    /// coincidence.
    pub(super) fn coalesce_free_space(&mut self) {
        self.free_head = None;
        self.free_runs.clear();
        let mut index = 0u32;
        while index < self.next {
            if !self.is_free_cell(index) {
                index += 1;
                continue;
            }
            let start = index;
            while index < self.next && self.is_free_cell(index) {
                index += 1;
            }
            let length = index - start;
            if length >= 2 {
                self.free_runs.push((start, length));
            } else {
                self.thread_single(start);
            }
        }
    }

    /// A one-cell allocation when the linked list is empty and the bump space is
    /// spent: the first cell of a run, the rest of the run kept.
    ///
    /// Without this a heap whose free space had all coalesced into runs would
    /// report itself full to a narrow allocation while holding thousands of
    /// free cells — the mirror image of the failure the module header names.
    pub(super) fn alloc_splitting_a_run(&mut self, ty: u32) -> Option<u32> {
        let start = self.take_free_run(1)?;
        let at = self.word_of(start);
        self.words[at] = super::header_word(ty, super::INLINE_SLOTS);
        for slot in 0..super::INLINE_SLOTS as usize {
            self.words[at + 1 + slot] = 0;
        }
        self.compose(start)
    }

    fn is_free_cell(&self, index: u32) -> bool {
        self.words[self.word_of(index)] == FREE_MARKER && !self.is_spanned_interior(index)
    }

    fn thread_single(&mut self, index: u32) {
        let word = self.word_of(index);
        let link = match self.free_head {
            Some(next) => u64::from(next),
            None => NO_NEXT,
        };
        self.words[word] = FREE_MARKER;
        self.words[word + 1] = link;
        self.free_head = Some(index);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Region, STRIDE};

    fn region(cells: u32) -> Region {
        Region::with_capacity(cells)
    }

    #[test]
    fn dead_narrow_neighbours_become_a_run_a_wide_object_can_take() {
        let mut region = region(8);
        let cells: Vec<u32> = (0..8).map(|_| region.alloc(STRIDE, 7).expect("room")).collect();
        // Free cells 2..6: four consecutive narrow objects die.
        for cell in &cells[2..6] {
            assert!(region.free(*cell));
        }
        // The bump space is spent and no wide object was ever freed, so the
        // spanning allocator coalesces the four dead neighbours into one run of
        // four on its own, and takes two of them.
        let wide = region
            .alloc_spanning(2 * STRIDE, 9)
            .expect("the four dead neighbours are one run of four");
        assert_eq!(region.type_of(wide), Some(9));
        // Two cells remain of the run, and a narrow allocation may split them.
        let narrow = region.alloc(STRIDE, 3).expect("a run is split for a narrow object");
        assert_eq!(region.type_of(narrow), Some(3));
        let last = region.alloc(STRIDE, 4).expect("and the last cell of it");
        assert_eq!(region.type_of(last), Some(4));
        assert_eq!(region.alloc(STRIDE, 5), None, "and then the region is genuinely full");
    }

    #[test]
    fn a_live_wide_object_is_not_read_as_free_cells() {
        let mut region = region(6);
        let wide = region.alloc_spanning(2 * STRIDE, 9).expect("room");
        let _a = region.alloc(STRIDE, 1).expect("room");
        let b = region.alloc(STRIDE, 1).expect("room");
        assert!(region.free(b));
        region.coalesce_free_space();
        assert_eq!(region.type_of(wide), Some(9), "the wide object survived the rebuild");
        assert!(region.alloc(STRIDE, 2).is_some(), "the one freed cell is still allocatable");
    }
}
