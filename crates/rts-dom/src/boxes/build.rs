//! Building the tree: one downward pass over the DOM.
//!
//! In this lot it is close to a MIRROR: one box per element, in document
//! order, plus the ONE family of anonymous box this lot adds — the CSS 2.1
//! §9.2.1.1 split of an inline box around a block-level child. No table
//! fixups, no generated boxes, and a text node still generates no box — its
//! style is the enclosing inline's, and what lays it out is still
//! `collect_runs`, which inherits those properties through parameters
//! threaded down the descent.
//!
//! Outside the block-in-inline case, behaviour is identical BY CONSTRUCTION,
//! which is what this lot's ruler demands: zero lost **and zero gained**.
//! Nothing in `layout` consumes this tree yet, so the split has no effect on
//! any answer today — it is exercised by `boxes/tests.rs` alone.
//!
//! The rules that will live here — the table fixups, generated boxes — are in
//! section 4 of `docs/ui/html-engine/box-tree.md`. Neither enters in this lot,
//! and each has its own (BT-4, BT-5).

use super::{BoxId, BoxTree};
use crate::dom::{Dom, NodeIdx, NodeKind};
use crate::style::DisplayKind;

/// Builds the tree: one box per element that has a computed style, with an
/// inline box split around a block-level child (CSS 2.1 §9.2.1.1).
///
/// An element with no computed style generates no box. Today that is what the
/// cascade refuses, and it is the same answer layout already gives —
/// `layout_block` asks for the style and gives up without it. It is not a new
/// decision taken by this module.
pub fn build_mirror(dom: &Dom) -> BoxTree {
    // The generation counts BUILDS, not revisions: the tree is memoised by
    // `(revision, style_epoch)`, so a style-only change yields a new tree at the
    // same revision — and an id from the old one would pass the check and read
    // the wrong arena. A build counter has no such hole.
    let mut tree = BoxTree::with_generation(dom.next_box_generation());
    // The document root is not an element and generates no box; entry is
    // through its children, the way `layout_document` does it.
    let roots: Vec<NodeIdx> = dom.node(dom.root).children.clone();
    for node in roots {
        descend(dom, node, None, &mut tree);
    }
    tree
}

fn descend(dom: &Dom, node: NodeIdx, parent: Option<BoxId>, tree: &mut BoxTree) {
    // A TEXT node gets a box, and it inherits the style of the element that
    // encloses it — text has no style of its own, `computed_style_idx` answers
    // `None` for one. Without a box, text could not appear in a tree traversal
    // at all, and the layout would have to keep walking the DOM to find it:
    // that is why the child loop still takes its ORDER from the DOM today.
    //
    // Whitespace that collapses away is NOT filtered here. Which whitespace
    // survives is a question about `white-space` and about the neighbours in a
    // line, and `quebra.rs` owns it — deciding it twice, once here on the tree
    // and once there on the runs, is the second-truth failure this module was
    // built to avoid.
    if let NodeKind::Text(_) = &dom.node(node).kind {
        let (Some(p), Some(source)) = (parent, tree_style_source(tree, parent)) else {
            return;
        };
        tree.push_text(node, source, p);
        return;
    }
    // An element the cascade refuses generates no box. The style itself is not
    // read here — `context.rs` asks for it fresh when someone needs it — so
    // this is a presence test and nothing more.
    if dom.computed_style_idx(node).is_none() {
        return;
    }
    let id = tree.push_element(node, parent);
    // `children` is cloned because `descend` re-borrows `dom` immutably while
    // mutating the tree. The cost is one allocation per element with children;
    // the alternative — an index and re-reading `dom.node(node).children` on
    // every step — trades the allocation for an arena hit per child. If this
    // shows up in a profile, it is here. Measure first, which is the house rule.
    let children: Vec<NodeIdx> = dom.node(node).children.clone();

    // The split applies to an actual inline BOX and to nothing else: inline-
    // level to its siblings, flow inside, and not establishing a context of its
    // own. `inline-block` and `inline-flex` pass the first test and fail the
    // other two, which is right — a block-level child inside them is ordinary
    // content, not a reason to split.
    //
    // This used to ask `!is_block_level`, which answered the question by
    // accident: that function routes to `layout_block`, and an `inline-block`
    // routes there, so it fell out of the criterion for a reason unrelated to
    // what the criterion means. `context.rs` carries the divergence in full.
    // Asking `effective_display() == Some(Inline)` was tried before that and
    // never fired at all — a plain `<span>` declares no display, inline being
    // the tag default, so the test was against `None` every time.
    let fc = crate::boxes::context::element_formatting_context(dom, node);
    let e_caixa_inline = fc.is_inline_level()
        && fc.inner == crate::boxes::InnerDisplay::Flow
        && !fc.independent;
    let is_split_inline =
        e_caixa_inline && children.iter().any(|&c| is_block_level_child(dom, c));

    if is_split_inline {
        split_around_block_children(dom, &children, id, node, tree);
    } else {
        for child in children {
            descend(dom, child, Some(id), tree);
        }
    }
}

/// `true` for an ELEMENT child that is block-level to its siblings — the one
/// question the split needs about a child, asked through
/// `element_formatting_context` so that "what is this to its siblings" has a
/// single answer in this crate. A non-element is never block-level: a text node
/// stays in the inline run, and a comment generates no box at all.
///
/// `display: none` is excluded because it generates no box: a child that does
/// not exist cannot split anything, and counting it would produce an anonymous
/// box around nothing.
fn is_block_level_child(dom: &Dom, node: NodeIdx) -> bool {
    if !matches!(&dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    let declared = dom
        .computed_style_idx(node)
        .and_then(|css| css.effective_display());
    if declared == Some(DisplayKind::None) {
        return false;
    }
    crate::boxes::context::element_formatting_context(dom, node).is_block_level()
}

/// Splits an inline box's children around each run of block-level ones (CSS
/// 2.1 §9.2.1.1): a run of inline-level content becomes one anonymous block
/// box, and a block-level child becomes a sibling of that box rather than a
/// descendant of it. Both land as children of `parent_box` — the inline's own
/// box — which is the local reading of "the block-level child becomes a
/// sibling of the anonymous boxes" this lot takes: the fixup does not bubble
/// past the inline's own container in BT-1.
///
/// A run with nothing in it produces no anonymous box: a block-level child as
/// the first or last child, or two of them in a row, leaves no inline content
/// on that side to enclose, and the browsers this engine measures against do
/// not materialise an empty one either.
fn split_around_block_children(
    dom: &Dom,
    children: &[NodeIdx],
    parent_box: BoxId,
    inline_node: NodeIdx,
    tree: &mut BoxTree,
) {
    let mut run: Vec<NodeIdx> = Vec::new();
    for &child in children {
        if is_block_level_child(dom, child) {
            flush_inline_run(dom, &mut run, parent_box, inline_node, tree);
            descend(dom, child, Some(parent_box), tree);
        } else {
            run.push(child);
        }
    }
    flush_inline_run(dom, &mut run, parent_box, inline_node, tree);
}

/// Wraps one accumulated run of inline-level children in a fresh anonymous
/// block box, or does nothing when the run is empty. The anonymous box
/// inherits the style of `inline_node` — the CSS rule that an anonymous box has no
/// declarations of its own, which `push_anonymous` already encodes.
fn flush_inline_run(
    dom: &Dom,
    run: &mut Vec<NodeIdx>,
    parent_box: BoxId,
    inline_node: NodeIdx,
    tree: &mut BoxTree,
) {
    if run.is_empty() {
        return;
    }
    // `inherits_from` e o proprio inline que se partiu: a caixa anonima nao tem
    // declaracoes suas e toma as propriedades herdadas do elemento que a gerou.
    let anon = tree.push_anonymous(inline_node, parent_box);
    for &child in run.iter() {
        descend(dom, child, Some(anon), tree);
    }
    run.clear();
}

/// The node whose style a child box inherits: the style source of the parent
/// box. For an element parent that is the element; for an anonymous box it is
/// the inline that was split, which is the same answer the CSS rules give.
fn tree_style_source(tree: &BoxTree, parent: Option<BoxId>) -> Option<NodeIdx> {
    parent.map(|p| tree.style_source(p))
}
