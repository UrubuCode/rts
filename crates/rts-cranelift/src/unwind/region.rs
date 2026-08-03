//! Protected regions and the tree they form.
//!
//! A region is a span of a function with handlers attached and, optionally,
//! cleanup that runs whether or not anything was thrown through it. Regions
//! nest, and nesting is the search order: an exception that no handler in the
//! innermost region matches is offered to its parent, and so on until the
//! function is exhausted and it becomes the caller's problem.

use crate::ir::BlockId;

/// Identifies a protected region within a function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct RegionId(pub(crate) u32);

impl RegionId {
    /// The tree index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a thrown value is tagged with.
///
/// Opaque. This layer compares tags for equality and does nothing else with
/// them: what a tag means, and whether a given thrown value matches one, is
/// decided by whoever threw it. A machine layer that interpreted tags would be
/// deciding what may be thrown, which is a question about a language.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Tag(pub u32);

/// A tag paired with the block that handles it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Handler {
    /// What this handler catches.
    pub tag: Tag,
    /// Where control goes when it does.
    ///
    /// Receives the thrown value as its first parameter, the same discipline a
    /// guard uses for the value it narrows: a handler that did not receive the
    /// value would have to find it somewhere else, and "somewhere else" is a
    /// side channel that outlives the frame it belongs to.
    pub block: BlockId,
}

/// A span with handlers and cleanup.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Region {
    /// The enclosing region, if this one is nested.
    pub parent: Option<RegionId>,
    /// Handlers attached here, in the order they are offered a thrown value.
    pub handlers: Vec<Handler>,
    /// Cleanup that runs when control leaves this region abnormally.
    ///
    /// Runs whether or not a handler here matched, and runs before any handler
    /// further out. Ordering matters: cleanup exists to undo what the region
    /// established, and undoing it after an outer handler has already run is
    /// indistinguishable from not undoing it at all.
    pub cleanup: Option<BlockId>,
}

/// Every region in one function.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RegionTree {
    regions: Vec<Region>,
}

impl RegionTree {
    /// A function with no protected regions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a region.
    ///
    /// A region's parent must already exist, which makes a cycle
    /// unrepresentable rather than something the search has to defend against.
    pub fn declare(
        &mut self,
        parent: Option<RegionId>,
        handlers: Vec<Handler>,
        cleanup: Option<BlockId>,
    ) -> RegionId {
        debug_assert!(
            parent.is_none_or(|p| p.index() < self.regions.len()),
            "a region's parent must be declared before it"
        );
        let id = RegionId(self.regions.len() as u32);
        self.regions.push(Region {
            parent,
            handlers,
            cleanup,
        });
        id
    }

    /// A declared region.
    pub fn get(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(id.index())
    }

    /// The region and every region enclosing it, innermost first.
    ///
    /// This is the search order, and it is finite because a parent is always
    /// declared before its children.
    pub fn enclosing(&self, id: RegionId) -> Enclosing<'_> {
        Enclosing {
            tree: self,
            next: Some(id),
        }
    }

    /// How many regions are declared.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Whether the function protects nothing.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// Walks outward from a region through its ancestors.
pub struct Enclosing<'a> {
    tree: &'a RegionTree,
    next: Option<RegionId>,
}

impl<'a> Iterator for Enclosing<'a> {
    type Item = (RegionId, &'a Region);

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        let region = self.tree.get(id)?;
        self.next = region.parent;
        Some((id, region))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosing_walks_outward_and_stops() {
        let mut tree = RegionTree::new();
        let outer = tree.declare(None, vec![], None);
        let inner = tree.declare(Some(outer), vec![], None);

        let walked: Vec<_> = tree.enclosing(inner).map(|(id, _)| id).collect();
        assert_eq!(walked, vec![inner, outer]);
    }
}
