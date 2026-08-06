//! What a cell is, besides what fits in it.
//!
//! # Why anything lives beside a cell
//!
//! A cell is sixty-four bytes: one word of header and seven of fields. Those
//! seven are what a program's own properties get, and every word spent on
//! something else is spent on **every object**, including the overwhelming
//! majority that are not the thing being recorded.
//!
//! So four facts are kept beside the cells rather than inside them, each sparse
//! and each keyed by the CELL a reference names — see [`Aside::in_region`] for
//! why that is the cell and not the reference:
//!
//! | what | why it is not in the cell |
//! |---|---|
//! | a callable's code and environment | JavaScript must not be able to write a code address |
//! | an array's elements | any length, where a cell is fixed |
//! | properties past the seventh | there is no eighth slot |
//! | a prototype | almost nothing reads it, and everything would pay |
//!
//! # Why one type and not four hand-rolled tables
//!
//! Because they were four, and each carried its own `resize`-then-index pair
//! written out. That is a rule stated four times — *grow to fit, then read or
//! write at the index* — and the crate's own rule 7 asks what a second copy is
//! for.
//!
//! It also matters more than tidiness for one specific reason: **a moving
//! collector has to relocate every one of these**, because they are keyed by an
//! index it would change. Four tables is four places to remember; one type is
//! one place to teach. There is no collector, and this is the note the day
//! there is.
//!
//! # Why a `Vec` and not a map
//!
//! The key is a cell index, which is dense and small — the region hands them out
//! in order from zero. A hash of one would cost more than the load it replaces,
//! and the empty entries cost a word each for cells that never had the thing.
//!
//! Dense is a property that had to be RESTORED. When the heap gained regions a
//! reference became `(cell << selector_bits) | region`, and a table indexed by
//! the reference stopped being dense — four regions left three empty entries
//! between every used one. [`Aside::in_region`] is what puts the density back.

/// Data attached to region cells, kept sparse.
///
/// `None` is "this cell does not have one", which is why `T` is not itself
/// `Option`-shaped: a prototype of `null` is a real prototype, and it has to be
/// distinguishable from a cell that was never given one.
pub struct Aside<T> {
    entries: Vec<Option<T>>,
    /// How many low bits of a reference name the region rather than the cell.
    ///
    /// See [`Aside::in_region`] for why this is here rather than nowhere.
    selector_bits: u32,
}

impl<T> Default for Aside<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Aside<T> {
    /// Nothing attached to anything, in a heap of one region.
    pub fn new() -> Self {
        Self::in_region(0)
    }

    /// Nothing attached to anything, in a heap whose references carry a region.
    ///
    /// # Why the width is held here and not decomposed by the caller
    ///
    /// A reference is `(cell << selector_bits) | region`, so indexing this table
    /// by the reference leaves `1 << selector_bits - 1` empty entries between
    /// every used one: four regions meant four times the table for the same
    /// objects. That was the cost taken when the regions were wired, knowingly
    /// and with the alternative named — decomposing at every call site, which
    /// would put the encoding in a dozen places in `entry/` and is exactly the
    /// rule-written-twice this crate keeps refusing.
    ///
    /// This is the third option and it is better than both: **a context holds
    /// exactly one region**, so the width is a fact about the whole table rather
    /// than something a caller has to know. The table decomposes, once, where it
    /// already knows how big it is — and the twenty-six call sites still pass a
    /// reference and know nothing.
    ///
    /// The region's own bits are dropped rather than checked. A reference from
    /// another region cannot reach here: `Region::decompose` refuses it first,
    /// and the caller has no cell to ask about.
    pub fn in_region(selector_bits: u32) -> Self {
        Aside {
            entries: Vec::new(),
            selector_bits,
        }
    }

    /// The cell a reference names, without the region it came from.
    fn cell_of(&self, reference: u32) -> usize {
        (reference >> self.selector_bits) as usize
    }

    /// How many entries the table has grown to.
    ///
    /// Not a count of what is attached — the gaps below the highest cell are
    /// entries too. It exists because the whole point of [`Self::in_region`] is
    /// that this number tracks the CELLS rather than the references, and a
    /// property nothing can observe is a property nothing can test.
    pub fn reach(&self) -> usize {
        self.entries.len()
    }

    /// What is attached to a cell, if anything.
    pub fn get(&self, reference: u32) -> Option<&T> {
        self.entries.get(self.cell_of(reference))?.as_ref()
    }

    /// The same, to write through.
    pub fn get_mut(&mut self, reference: u32) -> Option<&mut T> {
        let cell = self.cell_of(reference);
        self.entries.get_mut(cell)?.as_mut()
    }

    /// Attaches something to a cell, growing to fit.
    pub fn set(&mut self, reference: u32, value: T) {
        let cell = self.cell_of(reference);
        if self.entries.len() <= cell {
            self.entries.resize_with(cell + 1, || None);
        }
        self.entries[cell] = Some(value);
    }
}

impl<T: Copy> Aside<T> {
    /// What is attached, for the ones small enough to hand back by value.
    ///
    /// Separate from [`Self::get`] rather than replacing it: a caller holding a
    /// `Vec` wants the reference, and a caller holding a pair of words wants
    /// the copy — and `Copy` is exactly the line between them.
    pub fn copied(&self, reference: u32) -> Option<T> {
        self.get(reference).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_nothing_was_attached_to_has_nothing() {
        let mut aside: Aside<u64> = Aside::new();
        aside.set(5, 1);
        assert_eq!(aside.copied(4), None, "a gap below the one that was set");
        assert_eq!(aside.copied(9), None, "past the end");
        assert_eq!(aside.copied(5), Some(1));
    }

    #[test]
    fn a_sharded_table_grows_by_cells_and_not_by_references() {
        // The fix this width exists for. Four regions means a reference is four
        // apart from the next cell's, so a table indexed by the reference held
        // four times the entries for the same objects.
        let mut sharded: Aside<u64> = Aside::in_region(2);
        for cell in 0..10u32 {
            // Region 3's reference for each cell, so the dropped bits are not
            // zero and a table that kept them would be wrong as well as large.
            sharded.set((cell << 2) | 3, u64::from(cell));
        }
        assert_eq!(sharded.reach(), 10, "one entry per cell, not per reference");
        assert_eq!(sharded.copied((9 << 2) | 3), Some(9));

        // And a lone region is unchanged, which is what keeps the
        // single-threaded path exactly what it was.
        let mut single: Aside<u64> = Aside::new();
        for cell in 0..10u32 {
            single.set(cell, u64::from(cell));
        }
        assert_eq!(single.reach(), 10);
    }

    #[test]
    fn the_region_bits_are_dropped_rather_than_kept() {
        // Two references differing only in the region name one cell here. That
        // is safe because a foreign reference never reaches this table —
        // `Region::decompose` refuses it first — and it is what makes the
        // shift a decomposition rather than a hash.
        let mut aside: Aside<u64> = Aside::in_region(2);
        aside.set(0b100, 1);
        assert_eq!(aside.copied(0b101), Some(1), "same cell, other region bits");
        assert_eq!(aside.copied(0b1000), None, "the next cell");
    }

    #[test]
    fn a_value_that_looks_like_absence_is_still_present() {
        // The reason `T` is not `Option`-shaped: a prototype of `null` encodes
        // as a word like any other, and "the chain ends here" has to be
        // distinguishable from "this cell was never given one".
        let mut aside: Aside<u64> = Aside::new();
        aside.set(0, 0);
        assert_eq!(aside.copied(0), Some(0));
        assert_eq!(aside.copied(1), None);
    }
}
