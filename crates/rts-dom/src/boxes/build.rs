//! Building the tree: one downward pass over the DOM.
//!
//! In this lot it is a MIRROR and nothing else: one box per element, in
//! document order. No anonymous boxes, no generated boxes, and a text node
//! generates no box — its style is the enclosing inline's, and what lays it out
//! is still `collect_runs`, which inherits those properties through parameters
//! threaded down the descent.
//!
//! That is what makes behaviour identical BY CONSTRUCTION rather than by luck,
//! which is what this lot's ruler demands: zero lost **and zero gained**.
//!
//! The rules that will live here — blockification, anonymous block and table
//! boxes, generated boxes — are in section 4 of
//! `docs/ui/html-engine/box-tree.md`. **None of them enters in this lot**, and
//! each has its own (BT-3, BT-4, BT-5).

use super::{BoxId, BoxTree};
use crate::dom::{Dom, NodeIdx, NodeKind};

/// Builds the mirror: one box per element that has a computed style.
///
/// An element with no computed style generates no box. Today that is what the
/// cascade refuses, and it is the same answer layout already gives —
/// `layout_block` asks for the style and gives up without it. It is not a new
/// decision taken by this module.
pub fn build_mirror(dom: &Dom) -> BoxTree {
    let mut tree = BoxTree::default();
    // The document root is not an element and generates no box; entry is
    // through its children, the way `layout_document` does it.
    let roots: Vec<NodeIdx> = dom.node(dom.root).children.clone();
    for node in roots {
        descend(dom, node, None, &mut tree);
    }
    tree
}

fn descend(dom: &Dom, node: NodeIdx, parent: Option<BoxId>, tree: &mut BoxTree) {
    let NodeKind::Element { .. } = &dom.node(node).kind else {
        return;
    };
    let Some(style) = dom.computed_style_idx(node) else {
        return;
    };
    let id = tree.push_element(node, style, parent);
    // `children` is cloned because `descend` re-borrows `dom` immutably while
    // mutating the tree. The cost is one allocation per element with children;
    // the alternative — an index and re-reading `dom.node(node).children` on
    // every step — trades the allocation for an arena hit per child. If this
    // shows up in a profile, it is here. Measure first, which is the house rule.
    let children: Vec<NodeIdx> = dom.node(node).children.clone();
    for child in children {
        descend(dom, child, Some(id), tree);
    }
}
