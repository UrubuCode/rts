//! Baseline alignment for CSS Grid items and subgrids (CSS Grid Level 1 §10.5, Level 2 §11.6, CSS Box Alignment Level 3 §9).

use crate::Dom;
use crate::boxes::BoxId;
use crate::layout::grid::lines::GridCell;
use crate::layout::{font_px, LayoutCtx, NodeIdx, ResolveCtx, DEFAULT_FONT_SIZE};
use crate::style::{AlignItems, DisplayKind};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TrackBaseline {
    pub max_ascent: f32,
    pub max_descent: f32,
    pub single_max_ascent: f32,
    pub single_max_descent: f32,
    pub last_max_ascent: f32,
    pub last_max_descent: f32,
    pub single_last_max_ascent: f32,
    pub single_last_max_descent: f32,
    pub has_first: bool,
    pub has_last: bool,
}

impl TrackBaseline {
    pub fn is_empty(&self) -> bool {
        !self.has_first && !self.has_last
    }

    pub fn required_size(&self) -> f32 {
        let mut s = 0.0f32;
        if self.has_first {
            s = s.max(self.single_max_ascent + self.single_max_descent);
        }
        if self.has_last {
            s = s.max(self.single_last_max_ascent + self.single_last_max_descent);
        }
        s
    }

    pub fn align_offset(&self, align: AlignItems, ascent: f32, cell_size: f32) -> Option<f32> {
        match align {
            AlignItems::Baseline if self.has_first => {
                Some((self.max_ascent - ascent).max(0.0))
            }
            AlignItems::LastBaseline if self.has_last => {
                Some((cell_size - self.last_max_descent - ascent).max(0.0))
            }
            _ => None,
        }
    }
}

thread_local! {
    static ACTIVE_BASELINES: std::cell::RefCell<crate::fasthash::FastMap<BoxId, Vec<Option<TrackBaseline>>>> =
        std::cell::RefCell::new(crate::fasthash::FastMap::default());
}

pub(in crate::layout) fn register_subgrid_baselines(
    box_id: BoxId,
    baselines: &[Option<TrackBaseline>],
    smbp_y: f32,
    embp_y: f32,
) {
    let mut adjusted = baselines.to_vec();
    if let Some(Some(tb)) = adjusted.first_mut() {
        tb.max_ascent = (tb.max_ascent - smbp_y).max(0.0);
        tb.single_max_ascent = (tb.single_max_ascent - smbp_y).max(0.0);
    }
    if let Some(Some(tb)) = adjusted.last_mut() {
        tb.last_max_descent = (tb.last_max_descent - embp_y).max(0.0);
        tb.single_last_max_descent = (tb.single_last_max_descent - embp_y).max(0.0);
    }
    ACTIVE_BASELINES.with(|m| {
        m.borrow_mut().insert(box_id, adjusted);
    });
}

pub(in crate::layout) fn take_subgrid_baselines(box_id: BoxId) -> Option<Vec<Option<TrackBaseline>>> {
    ACTIVE_BASELINES.with(|m| m.borrow_mut().remove(&box_id))
}

pub(in crate::layout) fn cleanup_baselines(cells: &[GridCell]) {
    ACTIVE_BASELINES.with(|m| {
        let mut map = m.borrow_mut();
        for c in cells {
            map.remove(&c.box_id);
        }
    });
}

pub(in crate::layout) fn measure_item_ascent(
    dom: &Dom,
    child: NodeIdx,
    box_id: BoxId,
    item_w: f32,
    item_h: f32,
    first: bool,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(child).unwrap_or_default();
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let rc = ResolveCtx {
        parent_content_w: item_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let mt = css.margin.top.resolve(&rc).unwrap_or(0.0);
    let mb = css.margin.bottom.resolve(&rc).unwrap_or(0.0);
    let border_h = (item_h - mt - mb).max(0.0);

    if matches!(
        css.effective_display(),
        Some(
            DisplayKind::Flex
                | DisplayKind::InlineFlex
                | DisplayKind::FlexWrap
                | DisplayKind::InlineFlexWrap
        )
    ) && crate::layout::flex::baseline::has_element_items(dom, child)
    {
        let ascent = crate::layout::flex::baseline::container_ascent(dom, child, border_h, item_w, ctx);
        return mt + ascent.min(border_h);
    }

    let start = crate::layout::inline::line_baseline::mark();
    let ortho = crate::layout::block::rotated_ortho::mark();
    let mut scratch = crate::layout::DisplayList::for_dom(dom);
    crate::layout::layout_block(
        dom,
        child,
        box_id,
        0.0,
        0.0,
        item_w,
        None,
        Some(item_w),
        None,
        false,
        true,
        &crate::layout::BlockFormattingContext::new(),
        ctx,
        &mut scratch,
    );
    crate::layout::block::rotated_ortho::restore(ortho);
    let (first_b, _, last_b) =
        crate::layout::inline::line_baseline::collect_from_block(dom, child, start, 0.0, border_h);
    crate::layout::inline::line_baseline::discard(start);

    let inner = if first {
        first_b.or(last_b).unwrap_or(border_h)
    } else {
        last_b.or(first_b).unwrap_or(border_h)
    };
    mt + inner.min(border_h)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn compute_row_baselines(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    cells: &[GridCell],
    _container_box_id: BoxId,
    container_align: AlignItems,
    nrows: usize,
    col_sizes: &[f32],
    col_gap: f32,
    row_gap: f32,
    parent_wm: crate::style::WritingMode,
    ctx: &LayoutCtx,
) -> Vec<Option<TrackBaseline>> {
    let has_any_baseline = matches!(container_align, AlignItems::Baseline | AlignItems::LastBaseline)
        || cells.iter().any(|c| {
            dom.computed_style_idx(c.child)
                .and_then(|s| s.align_self)
                .is_some_and(|a| matches!(a, AlignItems::Baseline | AlignItems::LastBaseline))
        });

    if !has_any_baseline {
        return vec![None; nrows];
    }

    let mut out: Vec<Option<TrackBaseline>> = vec![None; nrows];

    for c in cells {
        let Some(css) = dom.computed_style_idx(c.child) else {
            continue;
        };
        let is_row_sub = super::subgrid::is_subgrid_node(dom, c.child, parent_wm).1;
        if is_row_sub {
            continue;
        }
        let align = css.align_self.unwrap_or(container_align);
        if !matches!(align, AlignItems::Baseline | AlignItems::LastBaseline) {
            continue;
        }
        let first = align == AlignItems::Baseline;
        let r = if first { c.r0 } else { c.r1.saturating_sub(1) };
        if r >= nrows {
            continue;
        }
        let cw = super::grid::span_size(col_sizes, c.c0, c.c1, col_gap);
        let (_, h) = crate::layout::measure_block(dom, c.child, c.box_id, cw, None, None, None, true, ctx);
        let ascent = measure_item_ascent(dom, c.child, c.box_id, cw, h, first, ctx);
        let descent = (h - ascent).max(0.0);

        let single = c.rows() == 1;
        let tb = out[r].get_or_insert_with(TrackBaseline::default);
        if first {
            tb.max_ascent = tb.max_ascent.max(ascent);
            tb.max_descent = tb.max_descent.max(descent);
            if single {
                tb.single_max_ascent = tb.single_max_ascent.max(ascent);
                tb.single_max_descent = tb.single_max_descent.max(descent);
            }
            tb.has_first = true;
        } else {
            tb.last_max_ascent = tb.last_max_ascent.max(ascent);
            tb.last_max_descent = tb.last_max_descent.max(descent);
            if single {
                tb.single_last_max_ascent = tb.single_last_max_ascent.max(ascent);
                tb.single_last_max_descent = tb.single_last_max_descent.max(descent);
            }
            tb.has_last = true;
        }
    }

    for c in cells {
        let is_row_sub = super::subgrid::is_subgrid_node(dom, c.child, parent_wm).1;
        if !is_row_sub {
            continue;
        }
        let Some(css) = dom.computed_style_idx(c.child) else {
            continue;
        };
        let resolve = ResolveCtx {
            parent_content_w: 0.0,
            node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let (smbp_x, embp_x, smbp_y, embp_y) = super::subgrid::subgrid_mbp(&css, &resolve);
        let sub_span_rows = c.r1.saturating_sub(c.r0).max(1);
        let sub_span_cols = c.c1.saturating_sub(c.c0).max(1);
        let sub_inner_w = (super::grid::span_size(col_sizes, c.c0, c.c1, col_gap) - smbp_x - embp_x).max(0.0);
        let child_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        let is_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();
        let p_css = dom.node(c.child).parent.and_then(|p| dom.computed_style_idx(p));
        let p_dir = p_css.and_then(|c| c.direction).unwrap_or_default();
        let c_dir = css.direction.unwrap_or_default();
        let parent_row_dir = super::subgrid::axis_direction(parent_wm, p_dir, false);
        let child_axis_dir = super::subgrid::axis_direction(child_wm, c_dir, is_ortho);
        let rev = parent_row_dir != child_axis_dir;
        let sub_template = if is_ortho {
            css.grid_template_columns.as_deref()
        } else {
            css.grid_template_rows.as_deref()
        };
        let sub_row_lines = match sub_template {
            Some(crate::style::GridTemplate::Subgrid(l)) => {
                super::subgrid::expand_subgrid_line_names(l, sub_span_rows, &[], rev)
            }
            _ => vec![Vec::new(); sub_span_rows + 1],
        };
        let sub_col_lines = vec![Vec::new(); sub_span_cols + 1];
        let (span_cols, span_rows, target_is_col) = if is_ortho {
            (sub_span_rows, sub_span_cols, true)
        } else {
            (sub_span_cols, sub_span_rows, false)
        };
        let (col_l, row_l) = if is_ortho {
            (&sub_row_lines, &sub_col_lines)
        } else {
            (&sub_col_lines, &sub_row_lines)
        };

        let mut items = Vec::new();
        super::subgrid_contrib::collect_nested_items(
            dom, tree, c.child, c.box_id, span_cols, span_rows, col_l, row_l,
            target_is_col, parent_wm, parent_row_dir, 0, row_gap, DEFAULT_FONT_SIZE, None, ctx, &mut items,
        );

        for item in items {
            if item.span != 1 {
                continue;
            }
            let p_row = c.r0 + item.start;
            if p_row >= nrows {
                continue;
            }
            let Some((child, box_id)) = item.child else {
                continue;
            };
            let sub_align = if is_ortho {
                css.grid_justify_items.unwrap_or(AlignItems::Normal)
            } else {
                css.align_items.unwrap_or(container_align)
            };
            let item_align = if is_ortho {
                dom.computed_style_idx(child)
                    .and_then(|s| s.justify_self)
                    .unwrap_or(sub_align)
            } else {
                dom.computed_style_idx(child)
                    .and_then(|s| s.align_self)
                    .unwrap_or(sub_align)
            };
            if !matches!(item_align, AlignItems::Baseline | AlignItems::LastBaseline) {
                continue;
            }
            let item_w = (sub_inner_w / sub_span_cols as f32 - item.extra_mbp_x).max(0.0);
            let (_, h) = crate::layout::measure_block(dom, child, box_id, item_w, None, None, None, true, ctx);
            let first = item_align == AlignItems::Baseline;
            let mut ascent = measure_item_ascent(dom, child, box_id, item_w, h, first, ctx);
            if item.start == 0 {
                ascent += smbp_y;
            }
            let mut descent = (h - ascent).max(0.0);
            if item.start + 1 >= sub_span_rows {
                descent += embp_y;
            }

            let tb = out[p_row].get_or_insert_with(TrackBaseline::default);
            if first {
                tb.max_ascent = tb.max_ascent.max(ascent);
                tb.max_descent = tb.max_descent.max(descent);
                tb.single_max_ascent = tb.single_max_ascent.max(ascent);
                tb.single_max_descent = tb.single_max_descent.max(descent);
                tb.has_first = true;
            } else {
                tb.last_max_ascent = tb.last_max_ascent.max(ascent);
                tb.last_max_descent = tb.last_max_descent.max(descent);
                tb.single_last_max_ascent = tb.single_last_max_ascent.max(ascent);
                tb.single_last_max_descent = tb.single_last_max_descent.max(descent);
                tb.has_last = true;
            }
        }
    }

    out
}

pub(in crate::layout) fn apply_baseline_sizes(
    content_row_h: &mut [f32],
    baselines: &[Option<TrackBaseline>],
) {
    for (r, tb) in baselines.iter().enumerate() {
        if let Some(tb) = tb {
            if r < content_row_h.len() {
                content_row_h[r] = content_row_h[r].max(tb.required_size());
            }
        }
    }
}

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
        crate::layout::inline::line_baseline::collect_from_block(dom, first, start, 0.0, h);
    crate::layout::inline::line_baseline::discard(start);

    let first_rect = scratch.rect_of_node(first);
    if let Some(r) = first_rect {
        if let Some(b) = first_b.or(last_b) {
            b
        } else {
            let first_css = dom.computed_style_idx(first).unwrap_or_default();
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let first_font = font_px(&first_css, font);
            let first_rc = ResolveCtx {
                parent_content_w: content_w,
                node_font_size: first_font,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let mb = first_css.margin.bottom.resolve(&first_rc).unwrap_or(0.0);
            r.y + r.h + mb
        }
    } else {
        h
    }
}

