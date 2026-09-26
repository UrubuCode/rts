//! Region membership for a client that decided its regions before emitting.
//!
//! Apart from `builder.rs` because that file is past this crate's ceiling of 1000 lines,
//! and README rule 5 says new code lands in a small focused module rather than being
//! appended to one already too large.

use super::FuncBuilder;
use crate::unwind::RegionId;

impl FuncBuilder<'_> {
    /// From here on, a block made while no region is open belongs to the region of
    /// the block being built.
    ///
    /// # Who this is for, and why it is a mode rather than a parameter
    ///
    /// A client TRANSLATING a representation whose regions are already decided -- a
    /// mid-level IR that places every one of its blocks in a region before any
    /// instruction is emitted -- opens and closes each region once, while it creates
    /// those blocks, and then emits instructions with no region open. Every block it
    /// makes after that is a continuation of the code it is emitting: the two halves
    /// of a cached read, the path past a raising call. Each belongs where the block
    /// being built is, and under the open-region rule each would belong to NOTHING --
    /// which is the `catch` that reads correctly and never runs that
    /// [`FuncBuilder::create_block`] describes.
    ///
    /// A parameter on `create_block` would be rule 8's failure exactly: every site
    /// that makes a block would have to remember to pass it. So it is derived, from
    /// the one fact that decides it, and a client turns it on once.
    ///
    /// Not the default, because a client that builds regions AS it emits -- opening
    /// one, emitting the body, closing it, then making the block after it -- would put
    /// that after-block inside the region it just left.
    pub fn inherit_block_regions(&mut self) {
        self.inherit_block_regions = true;
    }

    /// Which region protects the block being built, if any.
    ///
    /// For a client that keeps something per region -- a shared block a raise is
    /// re-issued from, say -- and must not share it across two regions, since where a
    /// throw lands is decided by the region its block is in.
    pub fn current_region(&self) -> Option<RegionId> {
        self.func.region_of(self.block)
    }
}
