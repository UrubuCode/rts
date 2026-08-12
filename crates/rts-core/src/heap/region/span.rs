//! An object that does not fit one cell, and lives in several consecutive ones.
//!
//! A cell holds seven words and that is the ceiling `Region::alloc` enforces.
//! One kind of object genuinely exceeds it: a generator's parked frame, laid out
//! by `rts_cranelift::frame::resumable_form` as an ordinary aggregate — label,
//! resumed value, parameters, returns and every value that outlives a
//! suspension. Six parameters and a return is already nine words.
//!
//! Nothing about the addressing changes, which is the whole reason this is the
//! answer rather than a second heap: a region's cells are consecutive words of
//! ONE allocation, so an object that runs past a cell boundary is still
//! contiguous, and `base + reference × stride` still reaches its first word.
//! Both the allocation and the access stay O(1).
//!
//! Split out of the region's own file rather than added to it because that file
//! reached the crate's 500-line ceiling, and this is a cohesive piece: the
//! allocation that spans, and the two accessors that know an object may.

use super::{Region, STRIDE};

impl Region {
    /// Takes CONSECUTIVE cells for an object of `size` bytes and type `ty`.
    ///
    /// The answer is the reference of the FIRST cell, which is what makes this
    /// work at all: a region's cells are consecutive words of one allocation, so
    /// `size` bytes from the start of a cell are contiguous whether or not they
    /// stop at its end. Compiled code computes `base + reference × stride` and
    /// then adds a byte offset — arithmetic that never asked how big the object
    /// is — so an object spanning three cells is addressed by exactly the two
    /// instructions one cell costs.
    ///
    /// Both this and the access are O(1): the allocation is a bump of `cells`
    /// rather than a search, and a field is still one offset from one base.
    ///
    /// # Why this exists beside [`Self::alloc`], rather than replacing it
    ///
    /// A spanning object hides the cells it covers from anything that walks the
    /// region by index — the following cells have no header of their own, and
    /// the collector that will need to know is not written. `alloc` keeps
    /// refusing an oversized object so that the one caller that has a reason to
    /// span has to say so, and everything else keeps the property that one
    /// reference is one cell.
    ///
    /// The reason is a generator's frame: `rts_cranelift::frame::resumable_form`
    /// lays the parked frame out as an ordinary aggregate, and six parameters
    /// and a return already need nine words against a cell's seven. The rejected
    /// alternatives were a region of its own — not expressible, since a compiled
    /// program has ONE addressing and therefore one stride — and an out-of-line
    /// spill, which the rewrite cannot use because it reaches its fields by byte
    /// offset from the frame's address.
    ///
    /// `None` when the region has not that many cells left. The header of the
    /// first cell is written; the rest are zero, as [`Self::alloc`] leaves them.
    pub fn alloc_spanning(&mut self, size: u32, ty: u32) -> Option<u32> {
        let cells = size.div_ceil(STRIDE);
        if cells == 0 || cells > self.capacity - self.next {
            return None;
        }
        let index = self.next;
        let reference = self.compose(index)?;
        self.next += cells;

        let at = self.word_of(index);
        self.words[at] = u64::from(ty);

        // Every cell after the first has no header of its own — a sweep must
        // not mistake one for an abandoned ordinary object. See
        // `Region::spanned_interior`'s own documentation for what reading one
        // as an object would do to a live frame.
        for offset in 1..cells {
            self.mark_spanned_interior(index + offset);
        }

        Some(reference)
    }

    /// Reads a field of an object that may span several cells.
    ///
    /// `slots` is how many fields the object has, and it comes from the layout
    /// its allocator sized it with — the region does not remember, because
    /// remembering would be a per-cell span written into every cell to serve the
    /// one kind of object that spans.
    ///
    /// Refuses `slot >= slots` rather than reading, which is what keeps this
    /// from reaching into the object allocated after it.
    pub fn spanning_field(&self, reference: u32, slot: u32, slots: u32) -> Option<u64> {
        let at = self.spanning_word(reference, slot, slots)?;
        self.words.get(at).copied()
    }

    /// Writes a field of an object that may span several cells.
    pub fn set_spanning_field(
        &mut self,
        reference: u32,
        slot: u32,
        slots: u32,
        value: u64,
    ) -> Option<()> {
        let at = self.spanning_word(reference, slot, slots)?;
        *self.words.get_mut(at)? = value;
        Some(())
    }

    /// Which word a field of a spanning object is, when it is one of its own.
    ///
    /// The same arithmetic [`Self::field`] does, without the seven-slot ceiling:
    /// a field is one word past the header of the FIRST cell, and the cells after
    /// it hold nothing else, so the count continues straight through them.
    fn spanning_word(&self, reference: u32, slot: u32, slots: u32) -> Option<usize> {
        if slot >= slots {
            return None;
        }
        let index = self.decompose(reference)?;
        if index >= self.next {
            return None;
        }
        Some(self.word_of(index) + 1 + slot as usize)
    }

    /// Returns every cell a spanning object covers, not only its first.
    ///
    /// [`Region::free`] cannot do this: it reads ONE header and threads ONE
    /// cell onto the free list, so calling it on a spanning object leaks every
    /// cell after the first — permanently, because the interior flag stays set
    /// and nothing ever walks them again. `size` is the same figure the
    /// allocation was given, and it is the caller's because the region does not
    /// remember how wide an object is.
    ///
    /// Each interior cell is freed DIRECTLY rather than through `free`: it has
    /// no header of its own, so the double-free check would be reading a data
    /// word and could refuse a live cell that happened to hold the marker.
    pub fn free_spanning(&mut self, reference: u32, size: u32) -> bool {
        let Some(index) = self.decompose(reference) else {
            return false;
        };
        let cells = size.div_ceil(STRIDE);
        if cells == 0 || index >= self.next {
            return false;
        }
        let at = self.word_of(index);
        if self.words[at] == super::FREE_MARKER {
            return false; // already free
        }

        for offset in 0..cells {
            let cell = index + offset;
            if cell >= self.next {
                break;
            }
            let word = self.word_of(cell);
            let link = match self.free_head {
                Some(next) => u64::from(next),
                None => super::NO_NEXT,
            };
            self.words[word] = super::FREE_MARKER;
            self.words[word + 1] = link;
            self.free_head = Some(cell);
            if let Some(flag) = self.spanned_interior.get_mut(cell as usize) {
                *flag = false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spanning_object_takes_the_cells_it_needs_and_the_next_one_starts_after() {
        // What makes the addressing work unchanged: the reference is the first
        // cell's, and the object after it begins past the last one it covers.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2 + 8, 5).expect("room");
        let after = region.alloc(16, 6).expect("room");
        assert_eq!(wide, 0);
        assert_eq!(after, 3, "two cells plus a word of a third were taken");
        assert_eq!(region.type_of(wide), Some(5));
    }

    #[test]
    fn a_field_past_the_seventh_of_a_spanning_object_is_its_own() {
        // The slot the cell form refuses, and the reason this pair of accessors
        // exists: it continues straight through the cells the object covers.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2, 1).expect("room");
        let after = region.alloc(16, 2).expect("room");

        region
            .set_spanning_field(wide, 9, 15, 999)
            .expect("its own field");
        assert_eq!(region.spanning_field(wide, 9, 15), Some(999));
        assert_eq!(
            region.type_of(after),
            Some(2),
            "a write inside the object must not have reached the one after it"
        );
    }

    #[test]
    fn a_slot_past_what_the_layout_says_is_refused() {
        // The region does not remember how wide a spanning object is, so the
        // bound is the caller's layout — and it is enforced rather than trusted.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2, 1).expect("room");
        assert_eq!(region.set_spanning_field(wide, 15, 15, 1), None);
        assert_eq!(region.spanning_field(wide, 15, 15), None);
    }

    #[test]
    fn a_region_without_room_for_every_cell_refuses_all_of_them() {
        // Refused rather than handing back a first cell whose later fields fall
        // outside the region: that write would land in nothing.
        let mut region = Region::with_capacity(2);
        assert_eq!(region.alloc_spanning(STRIDE * 3, 1), None);
        assert!(
            region.alloc(16, 1).is_some(),
            "a refusal takes no cells with it"
        );
    }
}
