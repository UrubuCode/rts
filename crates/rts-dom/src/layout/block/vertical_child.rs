//! Child classification and block child placement for vertical flow layout.
//!
//! Split out from `vertical_flow.rs` to keep both files strictly under the 500-line ceiling.

use super::box_kind::font_px;
use super::margin_collapse::{Strut, collapsed_strut, collapses_through, join_strut};
use super::*;
use crate::boxes::BoxId;

pub(super) fn classify_child(
    dom: &Dom,
    child: NodeIdx,
    child_css: Option<&ComputedStyle>,
    child_fragment: Option<BoxId>,
) -> (bool, bool) {
    let (child_block, child_inline_block) = match &dom.node(child).kind {
        // A `<br>` is a line break whatever `display` says short of `none`
        // (Blink ignores it): `.c > * { display: inline-block }` made it a box.
        NodeKind::Element { tag } if tag == "br" => (false, false),
        NodeKind::Element { tag } => {
            let replaced = tag == "svg";
            let effective = child_css.as_ref().and_then(|c| c.effective_display());
            let explicit_block = effective.map(|d| !d.is_inline_level()).unwrap_or(false);
            let declared_inline = effective == Some(crate::style::DisplayKind::Inline);
            let block = if declared_inline {
                replaced
                    || child_css.as_deref().is_some_and(|c| {
                        !crate::layout::block::box_kind::ignores_inline_dimensions(c)
                            && crate::inline_box::cria_caixa_apesar_de_inline(c)
                            && !crate::inline_box::inline_por_fragmentos(c)
                    })
            } else {
                replaced
                    || effective.is_some()
                    || crate::block::lookup(tag).is_some()
                    || child_css
                        .as_deref()
                        .map(|c| {
                            (c.has_box() || c.height.is_some())
                                && !crate::inline_box::inline_por_fragmentos(c)
                        })
                        .unwrap_or(false)
            };
            let inline_block = if effective
                .is_some_and(|d| d != crate::style::DisplayKind::Inline && d.is_inline_level())
            {
                true
            } else if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                !explicit_block
            } else if crate::block::lookup(tag).is_some() || explicit_block {
                false
            } else if child_css
                .as_deref()
                .is_some_and(crate::layout::block::box_kind::ignores_inline_dimensions)
            {
                false
            } else {
                child_css
                    .as_deref()
                    .map(|c| {
                        (c.has_box() || c.height.is_some())
                            && !crate::inline_box::inline_por_fragmentos(c)
                    })
                    .unwrap_or(false)
            };
            (block, inline_block)
        }
        _ => (false, false),
    };
    match child_fragment {
        Some(_) => (false, false),
        None => (child_block, child_inline_block),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn layout_block_flow_child(
    dom: &Dom,
    child: NodeIdx,
    child_box: BoxId,
    child_css: Option<&ComputedStyle>,
    content_x: f32,
    content_w: f32,
    avail_h: Option<f32>,
    font_size: f32,
    clearance: Option<f32>,
    border: &mut f32,
    strut: &mut Strut,
    child_y: &mut f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let (m, m_bottom) = child_css
        .as_ref()
        .map(|c| {
            let r = ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_px(c, font_size),
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let mv = c.margin_v.unwrap_or(0.0);
            let mv_top = if c.margin.top == crate::style::Side::Unset {
                mv
            } else {
                0.0
            };
            let mv_bottom = if c.margin.bottom == crate::style::Side::Unset {
                mv
            } else {
                0.0
            };
            (
                c.margin.top.resolve(&r).unwrap_or(0.0) + mv_top,
                c.margin.bottom.resolve(&r).unwrap_or(0.0) + mv_bottom,
            )
        })
        .unwrap_or((0.0, 0.0));
    let with_top = join_strut(*strut, m);
    let mut edge = *border + collapsed_strut(with_top);
    if let Some(clear_to) = clearance {
        edge = edge.max(clear_to);
    }
    if let Some(clear_to) = child_css.as_deref().and_then(|cs| {
        super::bfc_avoids_float::push_down(
            dom, child, cs, edge, content_x, content_w, font_size, bfc, ctx,
        )
    }) {
        edge = edge.max(clear_to);
    }
    *child_y = edge - m;
    let ((_, h), _) = layout_block_reusing(
        dom,
        child,
        child_box,
        content_x,
        *child_y,
        content_w,
        avail_h,
        || (m, m_bottom),
        None,
        None,
        false,
        false,
        bfc,
        ctx,
        list,
    );
    let (_, escaped_bottom) =
        crate::layout::block::block::escaped_margins_for_box(dom, child, content_w, font_size, ctx);
    let effective_bottom = crate::layout::block::block::collapse_margin(m_bottom, escaped_bottom);
    if collapses_through(h, m, effective_bottom) {
        *strut = join_strut(with_top, effective_bottom);
    } else {
        *border = edge + (h - m - effective_bottom);
        *strut = join_strut((0.0, 0.0), effective_bottom);
    }
    *child_y = *border + collapsed_strut(*strut);
}
