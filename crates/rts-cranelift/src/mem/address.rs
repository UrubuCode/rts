//! Turning a reference into an address.
//!
//! This is the measured lever. Computing an address from a reference with
//! instructions, rather than calling out to do it, was worth more than any
//! change to the value representation — and collapsing the region routing to a
//! single base was worth more again.
//!
//! Both forms are here because both are honest answers to different situations,
//! and the choice between them is a property of the heap rather than of a call
//! site. A call site does not get to pick.

/// How a reference becomes an address.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Addressing {
    /// One contiguous region: the address is a base plus a scaled index.
    ///
    /// Two instructions. This is what regional placement buys, and the reason
    /// the collector is compacting by requirement rather than by preference: the
    /// arithmetic is only valid while a region's objects are contiguous.
    Single {
        /// Where the region starts.
        base: RegionBase,
        /// How far apart consecutive slots are.
        stride: u32,
    },

    /// Several regions, selected by the low bits of the reference.
    ///
    /// One more load and one more mask than the single form. It exists because a
    /// heap shared by several threads cannot be one contiguous region without
    /// synchronizing every allocation, which is the cost regions were introduced
    /// to avoid. Paying two instructions to avoid a lock is the trade.
    Sharded {
        /// Where each region's bases are listed.
        table: RegionBase,
        /// How many low bits select the region.
        selector_bits: u32,
        /// How far apart consecutive slots are.
        stride: u32,
    },
}

/// Where a base address is read from.
///
/// Named rather than a raw number because the two destinations answer it
/// differently — compiled-into-memory code can be given the address as a
/// constant, while an object file must reference a symbol the linker resolves —
/// and that difference must not reach a call site.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RegionBase {
    /// The address is known while compiling.
    Immediate(u64),
    /// The address lives in a named cell, read at run time.
    Symbol(String),
}

/// Region bases for a heap, in the form lowering needs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RegionBases {
    addressing: Addressing,
}

impl RegionBases {
    /// A heap that is one contiguous region.
    pub fn single(base: RegionBase, stride: u32) -> Self {
        Self {
            addressing: Addressing::Single { base, stride },
        }
    }

    /// A heap split into regions selected by the low bits of a reference.
    ///
    /// The count must be a power of two, because selecting a region is a mask
    /// and a mask cannot express any other count. Rejecting it here rather than
    /// rounding keeps a caller from believing it got the count it asked for.
    pub fn sharded(table: RegionBase, region_count: u32, stride: u32) -> Option<Self> {
        if !region_count.is_power_of_two() {
            return None;
        }
        Some(Self {
            addressing: Addressing::Sharded {
                table,
                selector_bits: region_count.trailing_zeros(),
                stride,
            },
        })
    }

    /// How a reference becomes an address in this heap.
    pub fn addressing(&self) -> &Addressing {
        &self.addressing
    }

    /// How far apart consecutive slots are.
    pub fn stride(&self) -> u32 {
        match &self.addressing {
            Addressing::Single { stride, .. } | Addressing::Sharded { stride, .. } => *stride,
        }
    }

    /// How many instructions an address computation costs, before the access.
    ///
    /// Reported because the difference between the two forms is the whole reason
    /// both exist, and a number a caller can read beats a claim it has to trust.
    pub fn address_instructions(&self) -> u32 {
        match &self.addressing {
            // Scale the index, add the base.
            Addressing::Single { .. } => 2,
            // Mask out the region, load its base, scale the rest, add.
            Addressing::Sharded { .. } => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_region_costs_less_to_address_than_several() {
        let single = RegionBases::single(RegionBase::Immediate(0x1000), 8);
        let sharded =
            RegionBases::sharded(RegionBase::Symbol("bases".into()), 32, 8).expect("power of two");

        assert!(
            single.address_instructions() < sharded.address_instructions(),
            "this difference is the empirical case for regional placement"
        );
    }

    #[test]
    fn a_region_count_that_is_not_a_power_of_two_is_refused() {
        assert!(RegionBases::sharded(RegionBase::Immediate(0), 30, 8).is_none());
        assert!(RegionBases::sharded(RegionBase::Immediate(0), 32, 8).is_some());
    }

    #[test]
    fn the_selector_is_as_wide_as_the_region_count_needs() {
        let bases = RegionBases::sharded(RegionBase::Immediate(0), 32, 8).expect("power of two");
        match bases.addressing() {
            Addressing::Sharded { selector_bits, .. } => assert_eq!(*selector_bits, 5),
            other => panic!("expected several regions, got {other:?}"),
        }
    }
}
