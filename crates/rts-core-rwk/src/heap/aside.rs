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
//! and each keyed by the region index a reference already carries:
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
//! The key is a region index, which is dense and small — the region hands them
//! out in order from zero. A hash of one would cost more than the load it
//! replaces, and the empty entries cost a word each for cells that never had
//! the thing.

/// Data attached to region cells, kept sparse.
///
/// `None` is "this cell does not have one", which is why `T` is not itself
/// `Option`-shaped: a prototype of `null` is a real prototype, and it has to be
/// distinguishable from a cell that was never given one.
pub struct Aside<T> {
    entries: Vec<Option<T>>,
}

impl<T> Default for Aside<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Aside<T> {
    /// Nothing attached to anything.
    pub fn new() -> Self {
        Aside {
            entries: Vec::new(),
        }
    }

    /// What is attached to a cell, if anything.
    pub fn get(&self, cell: u32) -> Option<&T> {
        self.entries.get(cell as usize)?.as_ref()
    }

    /// The same, to write through.
    pub fn get_mut(&mut self, cell: u32) -> Option<&mut T> {
        self.entries.get_mut(cell as usize)?.as_mut()
    }

    /// Attaches something to a cell, growing to fit.
    pub fn set(&mut self, cell: u32, value: T) {
        if self.entries.len() <= cell as usize {
            self.entries.resize_with(cell as usize + 1, || None);
        }
        self.entries[cell as usize] = Some(value);
    }
}

impl<T: Copy> Aside<T> {
    /// What is attached, for the ones small enough to hand back by value.
    ///
    /// Separate from [`Self::get`] rather than replacing it: a caller holding a
    /// `Vec` wants the reference, and a caller holding a pair of words wants
    /// the copy — and `Copy` is exactly the line between them.
    pub fn copied(&self, cell: u32) -> Option<T> {
        self.get(cell).copied()
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
