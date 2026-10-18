//! Regions created at run time, and the table that finds them.
//!
//! [`super::Regions`] is a set fixed before compiling: its count is a selector
//! width baked into every address the code computes, and its [`super::BaseTable`]
//! is filled once and never grown. That is right for threads and wrong for a
//! module loaded while the program runs, which has to get a region of its own
//! without touching code already placed.
//!
//! # The shape, and why it costs the base path nothing
//!
//! Region 0 is the BASE region (single, inline allocation, owned by the context
//! exactly as before). This registry never holds it as a [`Region`]; it only
//! records its base address in slot 0 of the table so an indexed read works for
//! every id. Regions 1, 2, ... are appended here, each a lone region (selector
//! width 0: its own references are plain cell numbers) because the id that tells
//! them apart travels in the foreign reference (`Value::from_foreign`), not in
//! the low bits of a 32-bit reference.
//!
//! The table is **fixed capacity, allocated once**: appending writes one slot and
//! never reallocates, so its address -- the only thing code compiled against it
//! could have baked -- is stable for the registry's life. Capacity, not length, is
//! what is fixed; a slot past the length reads zero. Code that wants region `id`
//! loads `table + id * 8`, the same indexed read `Addressing::Sharded` does.
//!
//! Nothing here is built for a program that loads nothing: a single-region run
//! never constructs a registry.

use super::Region;
use crate::value::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Slots a registry holds unless told otherwise.
pub const DEFAULT_REGISTRY_CAPACITY: u32 = 256;

/// The base region's id; everything above it is appended here.
pub const BASE_REGION: u32 = 0;

/// Regions appended at run time, resolvable by id.
pub struct RegionRegistry {
    /// `bases[id]` is where region `id` starts. Atomic so an append can publish
    /// a slot while compiled code on another thread reads the table; same size
    /// and alignment as `u64`, which is what that code loads.
    bases: Box<[AtomicU64]>,
    /// Regions `1..`, so region `id` is `regions[id - 1]`.
    regions: Vec<Region>,
}

impl RegionRegistry {
    /// A registry whose slot 0 names the base region at `base`, with room for
    /// `capacity` regions in all (the base included).
    ///
    /// `capacity` is clamped to what a foreign reference can name.
    pub fn new(base: u64, capacity: u32) -> Self {
        let capacity = capacity.clamp(1, 0x1_0000) as usize;
        let bases: Box<[AtomicU64]> = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        bases[BASE_REGION as usize].store(base, Ordering::Release);
        RegionRegistry {
            bases,
            regions: Vec::new(),
        }
    }

    /// Creates a region of `cells` (growable to `reserved`), appends it and
    /// returns its id: 1 for the first, then 2, ...
    ///
    /// `None` when the table is full. O(1): one allocation, one slot write, one
    /// push.
    pub fn create(&mut self, cells: u32, reserved: u32) -> Option<u32> {
        let id = self.regions.len() as u32 + 1;
        if id as usize >= self.bases.len() {
            return None;
        }
        // A lone region: its own references are bare cell numbers. The id is
        // carried by the foreign reference that wraps them, so `index` stays 0
        // and `decompose` accepts the cells this region hands out.
        let region = Region::with_reservation(cells, reserved);
        self.bases[id as usize].store(region.base(), Ordering::Release);
        self.regions.push(region);
        Some(id)
    }

    /// The region with this id. `None` for the base (not held here) and for an
    /// id never created. O(1).
    pub fn region(&self, id: u32) -> Option<&Region> {
        self.regions.get((id as usize).checked_sub(1)?)
    }

    /// [`Self::region`] for allocating into it.
    pub fn region_mut(&mut self, id: u32) -> Option<&mut Region> {
        self.regions.get_mut((id as usize).checked_sub(1)?)
    }

    /// The region and cell a foreign reference names.
    pub fn resolve(&self, value: Value) -> Option<(&Region, u32)> {
        let (id, cell) = value.as_foreign()?;
        Some((self.region(id)?, cell))
    }

    /// Where region `id` starts, base included. O(1).
    pub fn base_of(&self, id: u32) -> Option<u64> {
        if id as usize > self.regions.len() {
            return None;
        }
        Some(self.bases[id as usize].load(Ordering::Acquire))
    }

    /// How many regions exist, the base included.
    pub fn len(&self) -> u32 {
        self.regions.len() as u32 + 1
    }

    /// Never empty: the base is always there.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Where the table starts: stable for the registry's life, because it is
    /// allocated once at full capacity and appending only writes a slot.
    pub fn table_address(&self) -> u64 {
        self.bases.as_ptr() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_created_at_run_time_is_resolved_by_id_and_the_base_is_untouched() {
        let base = Region::with_capacity(8);
        let base_address = base.base();
        let mut registry = RegionRegistry::new(base_address, 8);
        let table = registry.table_address();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.base_of(0), Some(base_address));
        assert!(registry.region(0).is_none(), "the base is not held here");

        let first = registry.create(8, 64).expect("room");
        let second = registry.create(8, 64).expect("room");
        assert_eq!((first, second), (1, 2));
        assert_eq!(registry.len(), 3);
        assert_eq!(registry.table_address(), table, "appending must not move the table");

        // Slot 0 still the base; slots 1, 2 are the new regions' bases.
        assert_eq!(registry.base_of(0), Some(base_address));
        let listed = unsafe { std::slice::from_raw_parts(table as *const u64, 3) };
        assert_eq!(listed[0], base_address);
        assert_eq!(listed[1], registry.region(1).unwrap().base());
        assert_eq!(listed[2], registry.region(2).unwrap().base());
        assert_ne!(listed[1], listed[2]);
        assert_eq!(registry.base_of(3), None);

        // Allocate in region 2 and resolve a foreign ref back to the cell.
        let cell = registry.region_mut(2).unwrap().alloc(16, 7).expect("fits");
        let foreign = Value::from_foreign(2, cell).expect("fits");
        let (region, resolved) = registry.resolve(foreign).expect("region 2 exists");
        assert_eq!(region.base(), listed[2]);
        assert_eq!(resolved, cell);
        assert_eq!(region.type_of(resolved), Some(7));

        // Not resolvable: base ref, unknown id, plain slot.
        assert!(registry.resolve(Value::from_foreign(3, 0).unwrap()).is_none());
        assert!(registry.resolve(Value::from_slot(1)).is_none());

        // The base region is bit-identical: same address, still empty/lone.
        assert_eq!(base.base(), base_address);
        assert_eq!(base.index(), 0);
        assert_eq!(base.selector_bits(), 0);
    }

    #[test]
    fn a_full_table_refuses_rather_than_reallocating() {
        let mut registry = RegionRegistry::new(0x1000, 2);
        assert_eq!(registry.create(4, 8), Some(1));
        assert_eq!(registry.create(4, 8), None);
        assert_eq!(registry.len(), 2);
    }
}
