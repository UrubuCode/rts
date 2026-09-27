//! PLACING a float: measure its box, find the first band where it fits from a
//! given top, lay it out and register it in the BFC.
//!
//! This lived whole in the float branch of `vertical_flow.rs`. It moved out because
//! it gained a SECOND caller asking the same question: the block stack, for a
//! float that is a direct child, and the inline flow (`in_line.rs`), for
//! a float that appears in the middle of a line — CSS 2.1 §9.5.1 puts it at the
//! top of the line it appears in. Two copies of the band search would be the
//! second truth this crate has already paid for elsewhere.

use super::*;
use crate::boxes::BoxId;

/// The outer box (margins included) of a float, before it has a place.
///
/// Measured apart because the inline flow needs the WIDTH to decide whether
/// the float fits in what is left of the current line, before knowing where to
/// put it.
pub(in crate::layout) fn measure_float(
    dom: &Dom,
    // The tree that ISSUED `box_id` — the current `DisplayList`'s. A `BoxId` is
    // only valid in the tree that built it (`box-tree.md` §10).
    tree: &crate::boxes::BoxTree,
    child: NodeIdx,
    box_id: BoxId,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    // `child_outer_width` does not clamp by `max-width`/`min-width` on purpose
    // (it is the flex base function, which the spec requires UNclamped) — a
    // float needs its own EFFECTIVE width to be placed, or the next sibling
    // started beyond where the real layout drew it (WPT
    // `flexbox-min-height-auto-002b`). The clamp is on the limit SHIFTED by the
    // margin: `max-width`/`min-width` are about the CONTENT, not the outer box
    // `child_outer_width` answers, and clamping the raw outer cut the MARGIN
    // too. The style comes from the TREE (invariant I6 of `box-tree.md`).
    let ccss = tree
        .style(dom, box_id)
        .or_else(|| dom.computed_style_idx(child))
        .unwrap_or_default();
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let margin_h = ccss.margin.resolve_h(&rc);
    let w = crate::style::clamp_size(
        child_outer_width(dom, child, content_w, font_size, ctx),
        ccss.min_width.and_then(|d| d.resolve(&rc)).map(|v| v + margin_h),
        ccss.max_width.and_then(|d| d.resolve(&rc)).map(|v| v + margin_h),
    );
    let h = child_outer_height(dom, child, box_id, content_w, avail_h, parent_css, font_size, ctx);
    (w, h)
}

/// Puts the `w × h` float in the first free band from `top_from`, lays it out
/// and registers the exclusion in the BFC. Answers the top it ended at.
///
/// Where it fits: try `top_from`; if the free band there is too narrow, move
/// down to the bottom of each float in the way, in the order they end. Two
/// floats on the same side that fit side by side stay side by side — the
/// Bootstrap brand+nav header, and what the first attempt already answers.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn place_float(
    dom: &Dom,
    child: NodeIdx,
    box_id: BoxId,
    side: crate::style::FloatSide,
    (w, h): (f32, f32),
    top_from: f32,
    content_x: f32,
    content_w: f32,
    avail_h: Option<f32>,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut top = top_from;
    let mut bottoms = bfc.bottoms();
    bottoms.sort_by(f32::total_cmp);
    let (mut bx, mut bw) = bfc.free_band(top, h, content_x, content_w);
    for f in bottoms {
        if bw >= w || f <= top {
            continue;
        }
        top = f;
        (bx, bw) = bfc.free_band(top, h, content_x, content_w);
    }
    let x = if side == crate::style::FloatSide::Left { bx } else { bx + bw - w };
    layout_block(
        dom,
        child,
        box_id,
        x,
        top,
        content_w,
        avail_h,
        None,
        None,
        false,
        true,
        // A float establishes its OWN BFC (CSS 2.1 §9.4.1) — `block.rs` makes
        // a new one inside for its content anyway; this value is never read.
        &BlockFormattingContext::new(),
        ctx,
        list,
    );
    // Registered in the responsible BFC — the SHARED reference, not a local
    // copy: that is what lets this float reach the SIBLINGS of the ancestor
    // that established the BFC, not only this container's (see `block/bfc.rs`
    // and `claude-float-clear.html`).
    bfc.push(Exclusion {
        top,
        bottom: top + h,
        side,
        edge: if side == crate::style::FloatSide::Left { x + w } else { x },
    });
    top
}
