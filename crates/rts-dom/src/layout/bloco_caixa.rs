//! THE BLOCK PATH THAT ACCEPTS A BOX WITH NO NODE.
//!
//! Everything else in the block flow is reached as `(dom, NodeIdx)`:
//! `layout_block`, `layout_block_reusing`, `escaped_margins_for_box`, the
//! fragment key and `layout_inline_flow` all ask for a node, and most of them
//! ask only in order to reach the STYLE. An anonymous box has no node, so until
//! this module existed the flow could not lay one out at all — it dissolved it
//! into its children instead, which was correct arithmetic and the wrong tree.
//!
//! ## Why this is a NEW function and not `layout_block` with a `BoxId`
//!
//! Because an anonymous box needs almost nothing of what `layout_block` does.
//! CSS 2.1 §9.2.1.1: an anonymous box **has no declarations of its own** and
//! takes only the inherited properties of the box that encloses it. So there is
//! no `width` to resolve, no `margin`, `padding` or `border` to lay out, no
//! background or box-shadow to paint, no `float`, no `clear`, no `position`, no
//! `overflow`, no `::before`, no `::after`, no `list-style` marker and no
//! `display` other than block-flow. What remains of those 1 400 lines is: take
//! the container's content box, and stack the children in it.
//!
//! Running an anonymous box through `layout_block` would therefore mean teaching
//! every one of those steps to answer "nothing" for a box with no node — the 175
//! style reads of invariant I6 in `docs/ui/html-engine/box-tree.md` — to arrive
//! at this same result.
//!
//! ## What it does NOT do, said plainly
//!
//! - **No fragment cache.** `FragmentKey` is keyed by `NodeIdx` and deliberately
//!   so (§10 of `box-tree.md`: a cached fragment outlives the tree that produced
//!   it, and a `BoxId` inside one would name a slot in an arena that has been
//!   rebuilt). An anonymous box has no key to be cached under, so it is laid out
//!   every pass. That is the same cost the expansion had.
//! - **No geometry entry.** `record_node_rect` is indexed by node and this box
//!   has none. Nothing asks for the rectangle of an anonymous box — it is not
//!   reachable from `getBoundingClientRect`, which is the whole of the public
//!   surface. It becomes a question when geometry moves to `BoxId`, which is the
//!   rest of BT-1.
//! - **No stacking context and no relative offset.** Same reason: both walk the
//!   DOM from a node (invariant I2).

use super::*;
use crate::boxes::{BoxId, BoxKind, BoxTree};

/// Lays an ANONYMOUS block box out at `(x, y)` across `content_w`, and answers
/// the height it took.
///
/// It is the block box CSS 2.1 §9.2.1.1 says encloses a run of line boxes when
/// an inline is split around a block-level child. Its content box IS the box —
/// no margin, no border, no padding — so `(x, y, content_w)` go straight down.
///
/// `css_pai` is the style of the box that CONTAINS this one, used only when the
/// tree cannot answer. It is the same value in every reachable case today: the
/// parent of an anonymous box is the box of the very container it inherits from.
/// Keeping the parameter rather than an `unwrap_or_default()` is the difference
/// between falling back to the right style and falling back to a 16px black
/// default — which would change the font of the text inside and answer plausibly
/// while being wrong.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_caixa_anonima(
    dom: &Dom,
    tree: &BoxTree,
    anonima: BoxId,
    x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    css_pai: &ComputedStyle,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    debug_assert!(
        matches!(tree.kind(anonima), BoxKind::Anonymous { .. }),
        "{anonima:?} nao e anonima: este caminho existe para a caixa SEM no, e uma \
         caixa de elemento tem de ir por `layout_block`, que lhe resolve a caixa"
    );
    // The style is asked of the TREE and of the document through it, never
    // captured: `style_source` answers the element this box inherits from, and
    // `style` asks `computed_style_idx` fresh. A copy taken when the tree was
    // built would be one frame behind for the whole of an animation — the tree is
    // memoised without `anim_epoch` on purpose (see `BoxKind`).
    let da_arvore = tree.style(dom, anonima);
    let css: &ComputedStyle = da_arvore.as_deref().unwrap_or(css_pai);
    // `style_source` and not a node of its own. It is the CONTAINER, and it is
    // the right answer for the four questions `layout_children_vertical` still
    // asks of a node: the DOM child list the window of `sequencia` is cut from,
    // the owner of the inline flow (`dono`), and the two sibling questions
    // (`em_contexto_inline`, `whitespace_is_inline_separator`) which look a child
    // up among the container's DOM children — where the children of this run
    // genuinely are.
    //
    // The two that would be WRONG with it — `::before`/`::after` and the
    // clearfix, which belong to the container and not to each of its runs — are
    // refused inside `layout_children_vertical`, which knows the box is anonymous
    // because the box says so. They are refused there rather than here because
    // that is where they are emitted.
    layout_children_vertical(
        dom,
        tree.style_source(anonima),
        Some(anonima),
        x,
        y,
        content_w,
        avail_h,
        css,
        font_size,
        bfc,
        ctx,
        list,
    )
}
