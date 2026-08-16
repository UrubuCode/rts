//! Where things live.
//!
//! A slot table, addressed by index. The machine's 48-bit payload is a slot
//! index rather than an address, and every property that follows from that —
//! conservative scanning being safe, growth invalidating nothing, a moving
//! collector being possible later — is a consequence of the indirection rather
//! than of anything clever.
//!
//! # What several regions cost the tables beside them
//!
//! [`Aside`] is a `Vec<Option<T>>` indexed by the number a reference carries.
//! With one region that number is the cell, and the vector is dense. With
//! `1 << selector_bits` regions it is `(cell << selector_bits) | index`, so the
//! entries a single thread touches are `1 << selector_bits` apart: **four
//! regions means four times the table for the same objects**, and the words in
//! between are `None` forever.
//!
//! It still works — `Aside::set` grows to fit — and nothing that uses one has to
//! change, which is why this is the trade taken rather than the problem solved.
//!
//! The alternative rejected was decomposing the reference at every `Aside`
//! access, handing it `(region, cell)` and keeping a dense table per region.
//! That is strictly better on memory and strictly worse everywhere else: the
//! encoding would move from [`Region`]'s five accessors out to a dozen call
//! sites in [`crate::entry`], which is precisely the property that kept this
//! change to one module. An encoding restated in a dozen places is an encoding
//! that will be restated differently.
//!
//! What would fix it properly is the thing that is absent anyway: a `Context`
//! per thread already owns exactly one region, so its asides only ever want that
//! region's cells. Giving `Aside` the owning region's index and selector width —
//! the same two numbers [`Region`] now carries — makes it dense again with the
//! decomposition still in one place. That is a change to [`Aside`]'s
//! constructor, which every field of `Context` calls, and it is worth making
//! when a thread-per-region run is more than a test.

//! # Phase 1: reclaiming a cell, and what is deliberately not here
//!
//! [`Region::free`] and [`Aside::remove`] are the two primitives a collector
//! needs: give a cell back to the free list, and clear whatever side table
//! still names it. Nothing here calls them together.
//!
//! A "sweep a cell" helper that walks every `Aside` a `Context` owns and calls
//! `remove` on each was considered and refused. `heap` does not know how many
//! side tables exist, what they are keyed by beyond a cell reference, or in
//! what order clearing them is safe — that is `Context`'s inventory, in
//! `entry/mod.rs`, and it is the crate's own rule 2 (ask what already answers
//! this) pointing the other way: a registration point built here ahead of a
//! caller is a guess at a shape `entry`'s eighteen tables have not been asked
//! to fit yet. Phase 2, which wires an actual collection cycle, is where that
//! caller exists and where the real shape of "sweep a cell" will be visible
//! rather than guessed.

mod aside;
mod region;
mod slab;

pub use aside::Aside;
pub use region::{GROWTH_CEILING, INLINE_SLOTS, Region, STRIDE};
pub use slab::{Handle, Slab, Slot, Stale};

/// Where compiled code reads each region's base from.
///
/// # Why this is held rather than built per run
///
/// Its **address** is what `Addressing::Sharded` carries, and that address is an
/// immediate in the compiled code exactly as a single region's base is. So the
/// table is not data a run assembles and drops: it is part of the program, and
/// it has to outlive every run of it.
///
/// If it moved, every address every thread computed would be read from whatever
/// now occupies the old allocation — an arbitrary number used as a base, which
/// is a wild store rather than a crash. So the `Vec` is filled once at
/// construction and never pushed to again: reallocation is the only thing that
/// moves a `Vec`'s buffer, and not growing it is what makes the address stable.
///
/// Note what is *not* pinned and does not need to be: the [`Region`] values
/// themselves may be moved — into a thread, into a `Context` — because a
/// region's base points into its own word allocation, which stays where it is
/// when the `Region` struct is moved.
pub struct BaseTable {
    bases: Vec<u64>,
}

impl BaseTable {
    /// Where the table starts, which is what the compiled code is given.
    pub fn address(&self) -> u64 {
        self.bases.as_ptr() as u64
    }

    /// How many regions it lists.
    pub fn len(&self) -> u32 {
        self.bases.len() as u32
    }

    /// Whether it lists none.
    pub fn is_empty(&self) -> bool {
        self.bases.is_empty()
    }
}

/// A heap of several regions, and the table that finds them.
///
/// The count is decided **before compiling** and cannot change afterwards: the
/// selector width is baked into every address computation in the program, so a
/// heap with more regions than the code was compiled for has references whose
/// low bits mean something else.
pub struct Regions {
    table: BaseTable,
    regions: Vec<Region>,
}

impl Regions {
    /// `count` regions of `cells` each.
    ///
    /// `None` when the count is not a power of two — a selector is a mask and a
    /// mask cannot express any other count, which is the same refusal
    /// `rts_cranelift::mem::RegionBases::sharded` makes and for the same reason
    /// — or when a cell number would not survive being shifted past the
    /// selector.
    pub fn new(count: u32, cells: u32) -> Option<Self> {
        if !count.is_power_of_two() {
            return None;
        }
        let selector_bits = count.trailing_zeros();
        if cells == 0 || cells - 1 > (u32::MAX >> selector_bits) {
            return None;
        }
        let regions: Vec<Region> = (0..count)
            .map(|index| Region::sharded(cells, index, selector_bits))
            .collect();
        // Filled once, here, and never grown. See [`BaseTable`] for what growing
        // it would do to code already compiled against its address.
        let bases: Vec<u64> = regions.iter().map(Region::base).collect();
        Some(Regions {
            table: BaseTable { bases },
            regions,
        })
    }

    /// Where the bases are listed.
    pub fn table(&self) -> &BaseTable {
        &self.table
    }

    /// How far apart consecutive cells are, in every region.
    pub fn stride(&self) -> u32 {
        STRIDE
    }

    /// The table and the regions, separately.
    ///
    /// Split rather than lent because they have different destinations: the
    /// regions go to the threads that allocate in them, one each, and the table
    /// stays with the program whose code reads it. A caller that dropped the
    /// table and kept the regions would have a heap the code cannot find.
    pub fn into_parts(self) -> (BaseTable, Vec<Region>) {
        (self.table, self.regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_regions_cannot_hand_out_the_same_reference() {
        // The property the whole encoding exists for. Two threads allocating
        // their first cell must not both be told "0", because that number is
        // what a value carries and what an `Aside` is keyed by.
        let (_table, mut regions) = Regions::new(4, 16).expect("a power of two").into_parts();
        let handed: Vec<u32> = regions
            .iter_mut()
            .map(|region| region.alloc(16, 1).expect("fits"))
            .collect();
        assert_eq!(handed, vec![0, 1, 2, 3], "the region is in the low bits");

        let mut all = handed.clone();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), handed.len(), "no two regions agree on a name");
    }

    #[test]
    fn a_reference_composed_elsewhere_reads_as_absent_rather_than_as_memory() {
        // Not safety — it is a refusal, and `region` says so. What it rules out
        // is the worse failure: decoding another region's reference against this
        // region's words and reading a real cell at the wrong place.
        let (_table, mut regions) = Regions::new(2, 8).expect("a power of two").into_parts();
        let theirs = regions[1].alloc(16, 7).expect("fits");
        assert_eq!(regions[1].type_of(theirs), Some(7));
        assert_eq!(regions[0].type_of(theirs), None);
        assert_eq!(regions[0].set_type(theirs, 9), None);
        assert_eq!(regions[0].field(theirs, 0), None);
    }

    #[test]
    fn the_table_lists_where_each_region_actually_starts() {
        // What compiled code loads. A table listing anything else is a wild
        // store, and it is written once precisely because nothing checks it.
        let regions = Regions::new(4, 8).expect("a power of two");
        let address = regions.table().address();
        assert_eq!(regions.table().len(), 4);
        let (table, regions) = regions.into_parts();
        assert_eq!(
            table.address(),
            address,
            "moving the holder must not move the table the code was given"
        );
        // SAFETY: the table is four `u64`s and it is alive for this borrow.
        let listed = unsafe { std::slice::from_raw_parts(table.address() as *const u64, 4) };
        for (index, region) in regions.iter().enumerate() {
            assert_eq!(listed[index], region.base());
        }
    }

    #[test]
    fn a_region_count_that_is_not_a_power_of_two_is_refused() {
        // The same refusal `RegionBases::sharded` makes, stated on this side
        // too: a selector is a mask, and rounding would give a caller a count
        // it did not ask for.
        assert!(Regions::new(3, 8).is_none());
        assert!(Regions::new(4, 8).is_some());
    }
}
