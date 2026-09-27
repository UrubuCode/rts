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
//! - **No fragment cache.** The fragment cache is now keyed by `BoxId` plus the
//!   tree generation, but its invalidation and stitching still start from a DOM
//!   node. An anonymous box has no independent dirty root, so it is laid out on
//!   every pass until that boundary is made box-native too.
//! - **No public geometry entry.** Its internal rectangle is keyed by `BoxId`,
//!   but `Geometry` deliberately translates only boxes with a DOM node. An
//!   anonymous box therefore remains unreachable from `getBoundingClientRect`.
//! - **No stacking context and no relative offset.** Same reason: both walk the
//!   DOM from a node (invariant I2).

use super::*;
use crate::boxes::{AnonymousRole, BoxId, BoxKind, BoxTree};

/// Lays out ANY anonymous box at `(x, y)` and records its rect by `BoxId`,
/// answering the height it took. The one entry the block flow calls: it
/// dispatches on what the anonymous box IS.
///
/// A TABLE sizes itself (shrink-to-fit, `table/anonima.rs`) and records its
/// own rect; a BLOCK takes the content box as it is. The rect is kept by
/// `BoxId` because the box has no node: that keeps transforms and any later
/// per-box operation complete without inventing a DOM geometry.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_anonymous(
    dom: &Dom,
    tree: &BoxTree,
    anonymous: BoxId,
    x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_style: &ComputedStyle,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    if matches!(tree.kind(anonymous), BoxKind::Anonymous { role: AnonymousRole::Table, .. }) {
        return crate::table::layout_anonymous_table(dom, tree, anonymous, x, y, content_w, font_size, ctx, list).1;
    }
    let h = layout_anonymous_box(dom, tree, anonymous, x, y, content_w, avail_h, parent_style, font_size, bfc, ctx, list);
    record_box_rect(list, anonymous, Rect::new(x, y, content_w, h));
    h
}

/// Lays an ANONYMOUS block box out at `(x, y)` across `content_w`, and answers
/// the height it took.
///
/// It is the block box CSS 2.1 §9.2.1.1 says encloses a run of line boxes when
/// an inline is split around a block-level child. Its content box IS the box —
/// no margin, no border, no padding — so `(x, y, content_w)` go straight down.
///
/// `parent_style` is the style of the box that CONTAINS this one, used only when the
/// tree cannot answer. It is the same value in every reachable case today: the
/// parent of an anonymous box is the box of the very container it inherits from.
/// Keeping the parameter rather than an `unwrap_or_default()` is the difference
/// between falling back to the right style and falling back to a 16px black
/// default — which would change the font of the text inside and answer plausibly
/// while being wrong.
#[allow(clippy::too_many_arguments)]
fn layout_anonymous_box(
    dom: &Dom,
    tree: &BoxTree,
    anonymous: BoxId,
    x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_style: &ComputedStyle,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    debug_assert!(
        matches!(tree.kind(anonymous), BoxKind::Anonymous { .. }),
        "{anonymous:?} nao e anonima: este caminho existe para a caixa SEM no, e uma \
         caixa de elemento tem de ir por `layout_block`, que lhe resolve a caixa"
    );
    // The style is asked of the TREE and of the document through it, never
    // captured: `style_source` answers the element this box inherits from, and
    // `style` asks `computed_style_idx` fresh. A copy taken when the tree was
    // built would be one frame behind for the whole of an animation — the tree is
    // memoised without `anim_epoch` on purpose (see `BoxKind`).
    let da_arvore = tree.style(dom, anonymous);
    let css: &ComputedStyle = da_arvore.as_deref().unwrap_or(parent_style);
    // `style_source` and not a node of its own. It is the CONTAINER, and it is
    // the right answer for the four questions `layout_children_vertical` still
    // asks of a node: the DOM child list the window of `sequencia` is cut from,
    // the owner of the inline flow (`dono`), and the two sibling questions
    // (`in_inline_context`, `whitespace_is_inline_separator`) which look a child
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
        tree.style_source(anonymous),
        anonymous,
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
