//! Making the machine's blocks in the order the region tree says.
//!
//! # Why the order is the whole of it
//!
//! `rts_cranelift::ir::FuncBuilder` places a block in a region when the block is MADE,
//! from the regions open at that moment -- rule 8 of that crate, because a client that
//! placed each block itself would forget one and the `catch` would read correctly and
//! never run. This crate has already placed every block in a region, so translating one
//! is a matter of making each machine block while its region is open: open a region,
//! make its blocks, descend into the regions inside it, close it.
//!
//! This refusal stood as `NeedsHandlerTag`, and `docs/engine/four-stages.md` recorded
//! that the name suggested a one-line fix it was not -- `lower` made every block before
//! any region was opened, so a region could only be refused.
//!
//! # The anchor, which is the one subtle part
//!
//! The machine's `open_region` also places the block being BUILT in the region it
//! opens, so that "the first statement of a `try`" is protected -- and this crate's own
//! builder does the same. So the block being built when a region opens has to be one
//! that belongs inside it. It is the region's first block, counting the regions inside
//! it: a `try` whose first statement is a `try` has its first block in the inner one,
//! and opening both around that one block leaves it in the inner, which is right. The
//! entry is already made, and a region whose first block it is simply switches to it.
//!
//! A region with no block at all, anywhere under it, protects nothing and is not opened:
//! opening it would place whatever block was last built inside it.

use std::collections::{BTreeMap, BTreeSet};

use rts_cranelift::ir::{BlockId as MachineBlock, FuncBuilder};

use super::{MachineOps, Unlowerable};
use crate::cfg::{BlockId, Func, Terminator};
use crate::region::RegionId;

/// Makes a machine block for every block but the entry, each inside its region.
pub(super) fn create_blocks(
    func: &Func,
    into: &mut FuncBuilder,
    ops: &mut impl MachineOps,
    blocks: &mut BTreeMap<BlockId, MachineBlock>,
) -> Result<(), Unlowerable> {
    let entry = into.current();
    let mut children: BTreeMap<Option<RegionId>, Vec<RegionId>> = BTreeMap::new();
    for (at, region) in func.regions.iter().enumerate() {
        children
            .entry(region.parent)
            .or_default()
            .push(RegionId(at as u32));
    }
    let mut walk = Walk {
        func,
        children: &children,
        blocks,
    };
    walk.level(into, ops, None)?;
    into.switch_to(entry);
    Ok(())
}

struct Walk<'a> {
    func: &'a Func,
    children: &'a BTreeMap<Option<RegionId>, Vec<RegionId>>,
    blocks: &'a mut BTreeMap<BlockId, MachineBlock>,
}

impl Walk<'_> {
    /// Makes the blocks directly in `region`, then each region directly inside it.
    ///
    /// Its own blocks FIRST, because a region inside this one has its handler and its
    /// cleanup here -- they are made before the region they serve is opened, which the
    /// machine requires: a handler inside its own region would catch its own throws.
    fn level(
        &mut self,
        into: &mut FuncBuilder,
        ops: &mut impl MachineOps,
        region: Option<RegionId>,
    ) -> Result<(), Unlowerable> {
        for block in self.func.block_ids() {
            if self.func.region_of(block) == region && !self.blocks.contains_key(&block) {
                let made = into.create_block();
                self.blocks.insert(block, made);
            }
        }
        let inside = self.children.get(&region).cloned().unwrap_or_default();
        for child in inside {
            self.descend(into, ops, child)?;
        }
        Ok(())
    }

    fn descend(
        &mut self,
        into: &mut FuncBuilder,
        ops: &mut impl MachineOps,
        region: RegionId,
    ) -> Result<(), Unlowerable> {
        let Some(anchor) = self.first_block_under(region) else {
            return Ok(());
        };
        let anchor = match self.blocks.get(&anchor) {
            Some(made) => *made,
            None => {
                let made = into.create_block();
                self.blocks.insert(anchor, made);
                made
            }
        };
        into.switch_to(anchor);

        let declared = self.func.region(region);
        let handlers = match declared.handler {
            Some(handler) => {
                let Some(tag) = ops.exception_tag() else {
                    return Err(Unlowerable::NeedsHandlerTag(region));
                };
                vec![rts_cranelift::unwind::Handler {
                    tag,
                    block: self.made(handler)?,
                }]
            }
            None => Vec::new(),
        };
        let cleanup = match declared.cleanup {
            Some(cleanup) => Some(self.made(cleanup)?),
            None => None,
        };
        into.open_region(handlers, cleanup);
        if let Some(block) = declared.resume_return {
            into.set_region_return(self.made(block)?);
        }
        self.level(into, ops, Some(region))?;
        into.close_region();
        Ok(())
    }

    /// A handler's or a cleanup's machine block, which its enclosing level made.
    fn made(&self, block: BlockId) -> Result<MachineBlock, Unlowerable> {
        self.blocks.get(&block).copied().ok_or_else(|| {
            Unlowerable::Machine(format!(
                "block {} serves a region it is not outside of; `verify` was not run",
                block.0
            ))
        })
    }

    /// The first block, in block order, of this region or of any region inside it.
    fn first_block_under(&self, region: RegionId) -> Option<BlockId> {
        self.func.block_ids().find(|block| {
            let mut at = self.func.region_of(*block);
            while let Some(here) = at {
                if here == region {
                    return true;
                }
                at = self.func.region(here).parent;
            }
            false
        })
    }
}

/// The blocks every cleanup is made of: reachable from a region's cleanup without
/// passing the block that finishes it.
///
/// Derived here from the graph rather than declared, for the reason the machine derives
/// its own copy of the same set: a list a client kept could disagree with the piece
/// control actually reaches.
pub(super) fn cleanup_blocks(func: &Func) -> BTreeSet<BlockId> {
    let mut inside = BTreeSet::new();
    let mut pending: Vec<BlockId> = func
        .regions
        .iter()
        .filter_map(|held| held.cleanup)
        .collect();
    while let Some(block) = pending.pop() {
        if !inside.insert(block) {
            continue;
        }
        match &func.block(block).terminator {
            Some(Terminator::CleanupDone) | None => {}
            Some(end) => pending.extend(end.successors()),
        }
    }
    inside
}
