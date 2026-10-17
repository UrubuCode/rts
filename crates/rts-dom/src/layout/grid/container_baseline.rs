//! External baseline synthesis for grid containers (CSS Grid Level 1 §11.1).

use crate::Dom;
use crate::layout::{font_px, LayoutCtx, NodeIdx, ResolveCtx, DEFAULT_FONT_SIZE};
use crate::style::DisplayKind;

/// The baseline of a grid container seen from outside (CSS Grid Level 1 §11.1).
/// Synthesized from the first grid item in the first row, or the container's
/// content bottom edge if there are no items or overflow is clipped.
pub(crate) fn container_ascent(
    dom: &Dom,
    id: NodeIdx,
    h: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(id) else {
        return h;
    };
    if crate::layout::inline::line_stack::clips_overflow(&css) {
        return h;
    }
    let Some(&box_id) = dom.box_tree().boxes_of(id).first() else {
        return h;
    };
    let in_flow: Vec<NodeIdx> = dom
        .node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| match &dom.node(c).kind {
            crate::dom::NodeKind::Element { tag } => !crate::layout::is_non_rendered_tag(tag),
            crate::dom::NodeKind::Text(t) => !t.trim().is_empty(),
            _ => false,
        })
        .filter(|&c| !crate::layout::is_out_of_flow(dom, c) && !crate::layout::is_display_none(dom, c))
        .collect();
    if in_flow.is_empty() {
        return h;
    }
    let mut ordered = in_flow;
    ordered.sort_by_key(|&c| {
        dom.computed_style_idx(c)
            .and_then(|st| st.order)
            .unwrap_or(0)
    });
    let first = ordered[0];
    let mut leaf = first;
    while dom.computed_style_idx(leaf).map_or(false, |c| {
        matches!(c.effective_display(), Some(DisplayKind::Grid | DisplayKind::InlineGrid))
    }) {
        let ch: Vec<NodeIdx> = dom
            .node(leaf)
            .children
            .iter()
            .copied()
            .filter(|&c| match &dom.node(c).kind {
                crate::dom::NodeKind::Element { tag } => !crate::layout::is_non_rendered_tag(tag),
                crate::dom::NodeKind::Text(t) => !t.trim().is_empty(),
                _ => false,
            })
            .filter(|&c| !crate::layout::is_out_of_flow(dom, c) && !crate::layout::is_display_none(dom, c))
            .collect();
        if let Some(&c) = ch.first() {
            leaf = c;
        } else {
            break;
        }
    }

    let start = crate::layout::inline::line_baseline::mark();
    let ortho = crate::layout::block::rotated_ortho::mark();
    let mut scratch = crate::layout::DisplayList::for_dom(dom);
    crate::layout::layout_block(
        dom,
        id,
        box_id,
        0.0,
        0.0,
        content_w,
        None,
        None,
        None,
        false,
        true,
        &crate::layout::BlockFormattingContext::new(),
        ctx,
        &mut scratch,
    );
    crate::layout::block::rotated_ortho::restore(ortho);
    let (first_b, _, last_b) =
        crate::layout::inline::line_baseline::collect_from_block(dom, leaf, start, 0.0, h);
    crate::layout::inline::line_baseline::discard(start);

    let leaf_rect = scratch.rect_of_node(leaf).or_else(|| scratch.rect_of_node(first));
    if let Some(r) = leaf_rect {
        if let Some(b) = first_b.or(last_b) {
            b
        } else {
            let leaf_css = dom.computed_style_idx(leaf).unwrap_or_default();
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let leaf_font = font_px(&leaf_css, font);
            let leaf_rc = ResolveCtx {
                parent_content_w: content_w,
                node_font_size: leaf_font,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let mb = leaf_css.margin.bottom.resolve(&leaf_rc).unwrap_or(0.0);
            r.y + r.h + mb
        }
    } else {
        h
    }
}
