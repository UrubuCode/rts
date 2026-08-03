//! Deciding what happens when a value is thrown.
//!
//! The search is not a run-time walk over a data structure when the region
//! structure is known: a throw whose tag is known and whose region is known has
//! exactly one answer, and computing it here means the emitted code contains the
//! answer rather than the search.

use super::region::{RegionId, RegionTree, Tag};
use crate::ir::BlockId;

/// What a throw does.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UnwindPlan {
    /// Cleanup blocks to run, innermost first.
    ///
    /// Every region between the throw and the handler contributes its cleanup,
    /// including the region the handler itself is in — leaving that one out is
    /// the classic mistake, and it leaks exactly the resources the innermost
    /// scope was responsible for.
    pub cleanups: Vec<BlockId>,
    /// Where control ends up, when a handler in this function matched.
    pub handler: Option<BlockId>,
}

impl UnwindPlan {
    /// Whether the value leaves this function.
    ///
    /// The cleanups still run; the caller resumes the search from its own
    /// innermost region. An escaping throw is not a failure to plan, it is the
    /// plan.
    pub fn escapes(&self) -> bool {
        self.handler.is_none()
    }
}

/// Computes what a throw of `tag` from inside `from` does.
///
/// Regions are searched outward. The first handler whose tag matches wins, and
/// every region entered on the way out contributes its cleanup first.
pub fn plan_unwind(tree: &RegionTree, from: RegionId, tag: Tag) -> UnwindPlan {
    let mut cleanups = Vec::new();

    for (_, region) in tree.enclosing(from) {
        if let Some(cleanup) = region.cleanup {
            cleanups.push(cleanup);
        }
        if let Some(handler) = region.handlers.iter().find(|h| h.tag == tag) {
            return UnwindPlan {
                cleanups,
                handler: Some(handler.block),
            };
        }
    }

    UnwindPlan {
        cleanups,
        handler: None,
    }
}

/// Computes what leaving `from` normally does, when it must run cleanup.
///
/// Returning out of a protected region is not a throw, but it owes the same
/// cleanup — a scope that only unwinds correctly when something goes wrong is a
/// scope that leaks on the path taken most of the time.
pub fn plan_normal_exit(tree: &RegionTree, from: RegionId) -> Vec<BlockId> {
    tree.enclosing(from)
        .filter_map(|(_, region)| region.cleanup)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::region::Handler;
    use super::*;

    /// Real blocks from a real function.
    ///
    /// A handle is only meaningful against the function that issued it, so the
    /// tests obtain them the way a client does rather than through a constructor
    /// that exists only to serve tests.
    fn blocks(count: usize) -> Vec<BlockId> {
        let mut func = crate::ir::Function::new(crate::ir::Signature::default());
        (0..count).map(|_| func.push_block()).collect()
    }

    #[test]
    fn the_innermost_matching_handler_wins() {
        let b = blocks(4);
        let mut tree = RegionTree::new();
        let outer = tree.declare(
            None,
            vec![Handler {
                tag: Tag(1),
                block: b[0],
            }],
            None,
        );
        let inner = tree.declare(
            Some(outer),
            vec![Handler {
                tag: Tag(1),
                block: b[1],
            }],
            None,
        );

        assert_eq!(plan_unwind(&tree, inner, Tag(1)).handler, Some(b[1]));
    }

    #[test]
    fn an_unmatched_tag_is_offered_outward() {
        let b = blocks(4);
        let mut tree = RegionTree::new();
        let outer = tree.declare(
            None,
            vec![Handler {
                tag: Tag(7),
                block: b[0],
            }],
            None,
        );
        let inner = tree.declare(
            Some(outer),
            vec![Handler {
                tag: Tag(1),
                block: b[1],
            }],
            None,
        );

        assert_eq!(plan_unwind(&tree, inner, Tag(7)).handler, Some(b[0]));
    }

    #[test]
    fn cleanup_runs_innermost_first_including_the_handlers_own_region() {
        let b = blocks(4);
        let mut tree = RegionTree::new();
        let outer = tree.declare(
            None,
            vec![Handler {
                tag: Tag(1),
                block: b[0],
            }],
            Some(b[1]),
        );
        let inner = tree.declare(Some(outer), vec![], Some(b[2]));

        assert_eq!(
            plan_unwind(&tree, inner, Tag(1)).cleanups,
            vec![b[2], b[1]],
            "the handler's own region cleans up too; omitting it leaks that scope"
        );
    }

    #[test]
    fn a_throw_nothing_handles_escapes_but_still_cleans_up() {
        let b = blocks(1);
        let mut tree = RegionTree::new();
        let region = tree.declare(None, vec![], Some(b[0]));

        let plan = plan_unwind(&tree, region, Tag(9));
        assert!(plan.escapes());
        assert_eq!(plan.cleanups, vec![b[0]]);
    }

    #[test]
    fn returning_out_of_a_region_owes_the_same_cleanup() {
        let b = blocks(2);
        let mut tree = RegionTree::new();
        let outer = tree.declare(None, vec![], Some(b[0]));
        let inner = tree.declare(Some(outer), vec![], Some(b[1]));

        assert_eq!(plan_normal_exit(&tree, inner), vec![b[1], b[0]]);
    }
}
