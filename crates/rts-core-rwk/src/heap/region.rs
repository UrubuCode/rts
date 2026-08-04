//! The heap compiled code addresses with arithmetic.
//!
//! # Why this exists beside the slab
//!
//! [`crate::heap::Slab`] holds Rust values and hands out indices, which is what
//! a runtime written in Rust wants. Compiled code wants something else, and the
//! machine says exactly what:
//!
//! > One contiguous region: the address is a base plus a scaled index. Two
//! > instructions. This is what regional placement buys.
//!
//! A `Slab<Cell>` cannot be addressed that way. Its elements are Rust enums
//! holding `Vec`s, so there is no stride and the payload of a reference names a
//! position in a `Vec` rather than a place in memory. Every property access has
//! to become a call, which is what it was, and measured at 94.8 ns against a
//! design whose answer is a compare, a branch and a load.
//!
//! So this is a region: one allocation, fixed-stride cells, each a header
//! followed by inline slots. `base + index × stride` reaches one, which is the
//! arithmetic `lower::memory::address_of` emits.
//!
//! # Why the stride is fixed, and what that costs
//!
//! Because the addressing is `base + index × stride`. A variable-size heap needs
//! a reference that is an address, and an address is what this design refuses to
//! put in a value — an index is what makes conservative scanning safe and a
//! moving collector possible.
//!
//! What it costs is an object that does not fit. That is a real case and it has
//! a known answer — an overflow indirection, which a prior measurement in this
//! repository put at 0.25 ns — and it exists now, in `entry::objects`: a
//! property past the seventh goes to a spill beside the cell, so the region
//! keeps knowing only about the seven it holds.
//!
//! It was **not** implemented for a while, and the gap was not visible the way
//! this paragraph used to claim. Refusing the write while the read answered
//! `undefined` is precisely "a silently wrong object" — the refusal is only
//! visible if something reports it, and nothing did.
//!
//! An **allocation** that does not fit is still refused rather than truncated.

use rts_cranelift::mem::{HeaderLayout, SLOT_BYTES};

/// How many inline slots a cell holds.
///
/// Seven, so that a cell is 64 bytes: one word of header and seven of fields.
///
/// The reason is alignment rather than a measurement of object sizes, and the
/// difference matters. 64 bytes is a cache line on every target this runs on,
/// so an object never straddles two of them — reading any field of an object
/// touches one line. How many properties a typical object has is a different
/// question, it has not been measured here, and the number that answers it may
/// well not be seven.
pub const INLINE_SLOTS: u32 = 7;

/// How far apart consecutive cells are.
pub const STRIDE: u32 = HeaderLayout::BYTES + INLINE_SLOTS * SLOT_BYTES;

/// A contiguous region of fixed-stride cells.
///
/// # Why it owns a `Vec<u64>` rather than a `Vec<u8>`
///
/// Alignment. A byte vector is aligned to one byte, and every field in a cell is
/// a machine word — so the region would be handing out addresses that a load
/// cannot use. A `u64` vector is aligned to eight, which is what correctness
/// needs.
///
/// Sixty-four-byte alignment, which would put every cell at the start of a cache
/// line rather than merely inside one, is **not** arranged. It would need an
/// over-allocation and an offset, and whether it is worth that has not been
/// measured.
pub struct Region {
    words: Vec<u64>,
    next: u32,
    capacity: u32,
}

impl Region {
    /// A region with room for `cells` objects.
    ///
    /// Fixed at construction and never grown, which is a limitation rather than
    /// a decision: growing moves the base, and every reference compiled code
    /// holds was turned into an address against the old one. Growing a region is
    /// the collector's business — it is what "compacting by requirement rather
    /// than by preference" means — and there is no collector yet.
    pub fn with_capacity(cells: u32) -> Self {
        let words = (cells as usize) * (STRIDE as usize / SLOT_BYTES as usize);
        Region {
            words: vec![0; words],
            next: 0,
            capacity: cells,
        }
    }

    /// Where the region starts.
    ///
    /// What `RegionBase::Immediate` carries. Valid for as long as this `Region`
    /// is alive and not moved — which is why a host holds it for the life of a
    /// compiled program rather than handing the address out and dropping it.
    pub fn base(&self) -> u64 {
        self.words.as_ptr() as u64
    }

    /// How far apart consecutive cells are.
    pub fn stride(&self) -> u32 {
        STRIDE
    }

    /// How many cells have been handed out.
    pub fn used(&self) -> u32 {
        self.next
    }

    /// Takes a cell for an object of `size` bytes and type `ty`.
    ///
    /// Returns the **index**, which is what a reference carries. `None` when the
    /// region is full or the object does not fit a cell — refused rather than
    /// truncated, because an object missing its last field is a wrong answer
    /// that looks like a right one.
    pub fn alloc(&mut self, size: u32, ty: u32) -> Option<u32> {
        if size > STRIDE || self.next >= self.capacity {
            return None;
        }
        let index = self.next;
        self.next += 1;

        // The header is one word and it is the type. The collector reads it
        // without knowing what the object is, which is the whole reason it is
        // the first thing in the cell.
        let at = self.word_of(index);
        self.words[at] = u64::from(ty);

        // The fields are zeroed by construction and stay that way: a cell handed
        // out twice would otherwise carry the previous object's values, and
        // there is no collector yet to have made that impossible.
        Some(index)
    }

    /// Reads a field of a cell, for a runtime that needs to look at one.
    ///
    /// Compiled code does not use this — it computes the address and loads. This
    /// is for the runtime's own reads, and it exists so nothing else has to
    /// know how a cell is laid out.
    pub fn field(&self, index: u32, slot: u32) -> Option<u64> {
        if slot >= INLINE_SLOTS {
            return None;
        }
        self.words.get(self.word_of(index) + 1 + slot as usize).copied()
    }

    /// Writes a field of a cell.
    pub fn set_field(&mut self, index: u32, slot: u32, value: u64) -> Option<()> {
        if slot >= INLINE_SLOTS || index >= self.next {
            return None;
        }
        let at = self.word_of(index) + 1 + slot as usize;
        *self.words.get_mut(at)? = value;
        Some(())
    }

    /// Records a new type for a cell.
    ///
    /// What a property addition does: the object changed what it IS, and the
    /// header is where that is written. Nothing else in the cell moves — a
    /// transition only ever appends, so the fields already there keep their
    /// offsets.
    pub fn set_type(&mut self, index: u32, ty: u32) -> Option<()> {
        if index >= self.next {
            return None;
        }
        let at = self.word_of(index);
        *self.words.get_mut(at)? = u64::from(ty);
        Some(())
    }

    /// The type a cell's header holds.
    pub fn type_of(&self, index: u32) -> Option<u32> {
        self.words.get(self.word_of(index)).map(|word| *word as u32)
    }

    /// Which word a cell starts at.
    fn word_of(&self, index: u32) -> usize {
        (index as usize) * (STRIDE as usize / SLOT_BYTES as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_a_cache_line() {
        // The reason for seven slots rather than any other number. Stated as a
        // test because the constant and the reason are in different places, and
        // changing one without the other is how a comment starts lying.
        assert_eq!(STRIDE, 64);
    }

    #[test]
    fn the_base_is_word_aligned_because_every_field_is_a_word() {
        // What a byte vector would not give, and what a load needs.
        let region = Region::with_capacity(4);
        assert_eq!(region.base() % u64::from(SLOT_BYTES), 0);
    }

    #[test]
    fn consecutive_cells_are_one_stride_apart() {
        // The arithmetic the machine emits, checked against what this hands out:
        // `base + index * stride` must reach the cell `alloc` returned.
        let mut region = Region::with_capacity(4);
        let first = region.alloc(16, 1).expect("fits");
        let second = region.alloc(16, 2).expect("fits");
        assert_eq!(second - first, 1);
        assert_eq!(region.type_of(first), Some(1));
        assert_eq!(region.type_of(second), Some(2));
    }

    #[test]
    fn an_object_too_large_for_a_cell_is_refused_rather_than_truncated() {
        // The gap this region has, made visible. An object missing its last
        // field is a wrong answer that looks like a right one.
        let mut region = Region::with_capacity(4);
        assert_eq!(region.alloc(STRIDE + 8, 1), None);
    }

    #[test]
    fn a_full_region_refuses_rather_than_overwriting() {
        let mut region = Region::with_capacity(2);
        assert!(region.alloc(16, 1).is_some());
        assert!(region.alloc(16, 1).is_some());
        assert_eq!(region.alloc(16, 1), None);
    }

    #[test]
    fn a_field_written_is_the_field_read_and_the_neighbour_is_untouched() {
        let mut region = Region::with_capacity(2);
        let a = region.alloc(64, 1).expect("fits");
        let b = region.alloc(64, 1).expect("fits");
        region.set_field(a, 0, 111).expect("slot exists");
        region.set_field(b, 0, 222).expect("slot exists");
        assert_eq!(region.field(a, 0), Some(111));
        assert_eq!(region.field(b, 0), Some(222));
        // The header of the next cell must not be what the previous one's last
        // slot wrote — which is what an off-by-one stride would produce.
        assert_eq!(region.type_of(b), Some(1));
    }

    #[test]
    fn a_slot_past_the_inline_ones_is_refused() {
        // Where the overflow indirection will go. Refusing says the gap is here
        // rather than letting a write land in the next object.
        let mut region = Region::with_capacity(2);
        let cell = region.alloc(64, 1).expect("fits");
        assert_eq!(region.set_field(cell, INLINE_SLOTS, 1), None);
    }
}
