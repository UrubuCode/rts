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
    let id = tree.push_element(node, parent);
    // `children` is cloned because `descend` re-borrows `dom` immutably while
    // mutating the tree. The cost is one allocation per element with children;
    // the alternative — an index and re-reading `dom.node(node).children` on
    // every step — trades the allocation for an arena hit per child. If this
    // shows up in a profile, it is here. Measure first, which is the house rule.
    let children: Vec<NodeIdx> = dom.node(node).children.clone();

    // The split applies only to an actual inline box — `display: inline` —
    // and not to `inline-block`/`inline-flex`/…: those are inline-level on
    // the OUTSIDE but establish their own block-formatting context on the
    // inside, so a block-level child is ordinary content for them, not a
    // reason to split. Restricted to that one variant on purpose: a wider
    // test here would be a second definition of "inline box" beside
    // `is_inline_level`.
    // "E uma caixa inline?" nao se pergunta por `effective_display()`: essa
    // funcao responde ao display DECLARADO, e um `<span>` normal nao declara
    // nenhum — inline e o default da tag. Medido: `<span>` responde `None`, e
    // o criterio nunca disparava. `is_block_level` e a pergunta que o motor ja
    // faz em 46 sitios, e usa-la aqui evita uma segunda verdade sobre a
    // condicao que decide se um inline se parte.
    //
    // `inline-block` e companhia ficam de fora na mesma: sao inline-level por
    // FORA mas estabelecem contexto proprio por DENTRO, e por isso
    // `is_block_level` responde `true` a eles — que e o que queremos, porque
    // um filho de bloco la dentro e conteudo normal, nao motivo para partir.
    let e_caixa_inline = !crate::layout::caixa::is_block_level(dom, node);
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

/// `true` for an ELEMENT child whose `effective_display` is block-level —
/// the blockification `ComputedStyle` already computes (float, `position`),
/// consulted here rather than re-derived: a second criterion for "is this
/// block-level" would be a second truth about a question the style already
/// answers. A non-element (text, comment) is never block-level: it has no
/// style of its own to blockify, and it stays in the inline run.
fn is_block_level_child(dom: &Dom, node: NodeIdx) -> bool {
    if !matches!(&dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    dom.computed_style_idx(node).is_some_and(|style| {
        style
            .effective_display()
            .is_some_and(|d| d != DisplayKind::None && !d.is_inline_level())
    })
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
