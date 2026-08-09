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
//!
//! # Several regions, and what a reference becomes
//!
//! One base address cannot serve two threads, because the base is an
//! **immediate** in the compiled code — there is one number, in the
//! instructions, and a second thread allocating against it would be allocating
//! in the first thread's memory. `rts_cranelift::mem::Addressing::Sharded` is
//! the machine's answer and it was written before anything used it: the
//! reference carries which region it belongs to, and the base is *loaded* from a
//! table the host writes once.
//!
//! So a reference is composed:
//!
//! ```text
//! reference = (cell << selector_bits) | region_index
//! ```
//!
//! and this module composes and decomposes it. That placement is the whole
//! design. [`Region`] has exactly five accessors that take a cell — [`alloc`],
//! [`field`], [`set_field`], [`set_type`], [`type_of`] — and every caller in the
//! crate reaches them through `context.region`. Teaching those five the encoding
//! means no call site learns it, and `Value::from_slot`/`as_slot` keep carrying
//! whatever number the region hands out without knowing it has parts.
//!
//! The rejected alternative was decomposing at each call site, or handing the
//! runtime a `(region, cell)` pair. Both put the encoding in a dozen places, and
//! the encoding is exactly the thing the compiler and the runtime have to agree
//! about bit for bit.
//!
//! [`alloc`]: Region::alloc
//! [`field`]: Region::field
//! [`set_field`]: Region::set_field
//! [`set_type`]: Region::set_type
//! [`type_of`]: Region::type_of
//!
//! # What a region per thread does NOT give you
//!
//! It gives each thread a heap of its own. It does **not** make a value shared
//! between threads safe, and nothing here pretends otherwise:
//!
//! - There is no `Local` versus `Shared` distinction on a region, so nothing can
//!   even state that an object escaped its thread.
//! - The write barrier records a store that crosses a region and nothing reads
//!   what it records, because the collector that would is the next piece rather
//!   than this one. See the write barrier in `entry/barrier.rs`.
//! - There is no collector, so there is nothing that would have to be told.
//!
//! Publishing a value to another thread is therefore **not expressible**: a
//! reference composed for region 3 decodes to nothing in region 5, so the read
//! answers absent rather than reading the wrong memory — which is a refusal, not
//! protection. A program that arranges to share anyway is protected by nothing.

mod span;

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
    /// Which region this is, and what goes in the low bits of its references.
    index: u32,
    /// How many low bits that takes.
    ///
    /// Zero for a lone region, which makes composition the identity — that is
    /// what keeps every reference in a single-region program bit-for-bit what it
    /// was before this encoding existed.
    selector_bits: u32,
}

impl Region {
    /// A region with room for `cells` objects.
    ///
    /// Fixed at construction and never grown, which is a limitation rather than
    /// a decision: growing moves the base, and every reference compiled code
    /// holds was turned into an address against the old one. Growing a region is
    /// the collector's business — it is what "compacting by requirement rather
    /// than by preference" means — and there is no collector yet.
    ///
    /// The lone region of a single-region heap: index 0, selector width 0. Its
    /// references are cell numbers, unshifted, which is why every existing
    /// caller and every existing test is unaffected by the composition above.
    pub fn with_capacity(cells: u32) -> Self {
        Region::sharded(cells, 0, 0)
    }

    /// One region of several, numbered `index` out of `1 << selector_bits`.
    ///
    /// Separate from [`Self::with_capacity`] rather than a defaulted parameter,
    /// because the single-region case is the one that must stay free: a caller
    /// that never asks for shards must not be able to accidentally pay for them.
    pub fn sharded(cells: u32, index: u32, selector_bits: u32) -> Self {
        let words = (cells as usize) * (STRIDE as usize / SLOT_BYTES as usize);
        Region {
            words: vec![0; words],
            next: 0,
            capacity: cells,
            index,
            selector_bits,
        }
    }

    /// Which region this is.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// How many low bits of a reference name the region.
    pub fn selector_bits(&self) -> u32 {
        self.selector_bits
    }

    /// Which region a reference belongs to.
    ///
    /// Answers about a reference this region did not hand out, which is the
    /// whole reason it is not [`Self::decompose`]: that one refuses a foreign
    /// reference, and the write barrier's question is precisely *is this one
    /// foreign*.
    pub fn region_of(&self, reference: u32) -> u32 {
        reference & self.selector_mask()
    }

    /// The reference naming a cell of this region.
    ///
    /// `None` when the shift would push the cell number out of a reference.
    /// Refused rather than wrapped, for the reason an oversized allocation is:
    /// a wrapped reference names a real cell, so it is a wrong answer that looks
    /// like a right one.
    fn compose(&self, cell: u32) -> Option<u32> {
        if self.selector_bits >= u32::BITS || cell > (u32::MAX >> self.selector_bits) {
            return None;
        }
        Some((cell << self.selector_bits) | self.index)
    }

    /// The low bits a reference spends naming its region.
    fn selector_mask(&self) -> u32 {
        ((1u64 << self.selector_bits) - 1) as u32
    }

    /// The cell a reference names, when it is one of this region's.
    ///
    /// `None` for a reference belonging to another region. That is what stops a
    /// value that reached the wrong thread from reading this thread's memory at
    /// the wrong offset: it decodes to nothing and every accessor answers absent.
    /// It is a refusal, not safety — see this module's opening note.
    fn decompose(&self, reference: u32) -> Option<u32> {
        if reference & self.selector_mask() != self.index {
            return None;
        }
        Some(reference >> self.selector_bits)
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

    /// How many cells the region has room for.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// How many cells have been handed out.
    pub fn used(&self) -> u32 {
        self.next
    }

    /// Takes a cell for an object of `size` bytes and type `ty`.
    ///
    /// Returns the **composed reference**, which is what a value carries: the
    /// cell number shifted past the region selector, with this region's number
    /// in the low bits. For a lone region that is the cell number itself.
    ///
    /// `None` when the region is full or the object does not fit a cell —
    /// refused rather than truncated, because an object missing its last field
    /// is a wrong answer that looks like a right one.
    pub fn alloc(&mut self, size: u32, ty: u32) -> Option<u32> {
        if size > STRIDE || self.next >= self.capacity {
            return None;
        }
        let index = self.next;
        let reference = self.compose(index)?;
        self.next += 1;

        // The header is one word and it is the type. The collector reads it
        // without knowing what the object is, which is the whole reason it is
        // the first thing in the cell.
        let at = self.word_of(index);
        self.words[at] = u64::from(ty);

        // The fields are zeroed by construction and stay that way: a cell handed
        // out twice would otherwise carry the previous object's values, and
        // there is no collector yet to have made that impossible.
        Some(reference)
    }

    /// Reads a field of a cell, for a runtime that needs to look at one.
    ///
    /// Compiled code does not use this — it computes the address and loads. This
    /// is for the runtime's own reads, and it exists so nothing else has to
    /// know how a cell is laid out.
    pub fn field(&self, reference: u32, slot: u32) -> Option<u64> {
        if slot >= INLINE_SLOTS {
            return None;
        }
        let index = self.decompose(reference)?;
        self.words
            .get(self.word_of(index) + 1 + slot as usize)
            .copied()
    }

    /// Writes a field of a cell.
    pub fn set_field(&mut self, reference: u32, slot: u32, value: u64) -> Option<()> {
        let index = self.decompose(reference)?;
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
    pub fn set_type(&mut self, reference: u32, ty: u32) -> Option<()> {
        let index = self.decompose(reference)?;
        if index >= self.next {
            return None;
        }
        let at = self.word_of(index);
        *self.words.get_mut(at)? = u64::from(ty);
        Some(())
    }

    /// The type a cell's header holds.
    pub fn type_of(&self, reference: u32) -> Option<u32> {
        let index = self.decompose(reference)?;
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

    #[test]
    fn a_lone_region_hands_out_the_cell_number_itself() {
        // The property that keeps single-region programs unchanged: with no
        // selector there is nothing to shift, so a reference is what it always
        // was and no existing compiled code sees a different number.
        let mut region = Region::with_capacity(4);
        assert_eq!(region.selector_bits(), 0);
        assert_eq!(region.alloc(16, 1), Some(0));
        assert_eq!(region.alloc(16, 1), Some(1));
    }
}
