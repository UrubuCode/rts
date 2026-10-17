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
    pub end_max_ascent: f32,
    pub end_max_descent: f32,
    pub has_end: bool,
    pub max_size: f32,
}

impl TrackBaseline {
    pub fn is_empty(&self) -> bool {
        !self.has_first && !self.has_last && !self.has_end
    }

    pub fn required_size(&self) -> f32 {
        let mut s = 0.0f32;
        if self.has_first {
            let has_single = self.single_max_ascent > 0.0 || self.single_max_descent > 0.0;
            if has_single {
                s = s.max(self.max_ascent + self.single_max_descent);
            }
        }
        if self.has_last {
            let has_single = self.single_last_max_ascent > 0.0 || self.single_last_max_descent > 0.0;
            if has_single {
                s = s.max(self.single_last_max_ascent + self.last_max_descent);
            }
        }
        if self.has_end {
            s = s.max(self.end_max_ascent + self.end_max_descent);
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

    pub fn align_offset_end(&self, ascent: f32, cell_size: f32) -> Option<f32> {
        if self.has_end {
            Some((cell_size - self.end_max_descent - ascent).max(0.0))
        } else {
            None
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

pub(in crate::layout) fn measure_item_baseline(
    dom: &Dom,
    child: NodeIdx,
    box_id: BoxId,
    item_w: f32,
    item_h: f32,
    first: bool,
    ctx: &LayoutCtx,
) -> (f32, f32) {
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
        Some(DisplayKind::Flex | DisplayKind::InlineFlex | DisplayKind::FlexWrap | DisplayKind::InlineFlexWrap)
    ) && crate::layout::flex::baseline::has_element_items(dom, child)
    {
        let ascent = crate::layout::flex::baseline::container_ascent(dom, child, border_h, item_w, ctx);
        let a = mt + ascent.min(border_h);
        return (a, (item_h - a).max(0.0));
    }

    let frame = crate::layout::block::rotated::rotated_frame_mode()
        .filter(|_| !crate::layout::block::rotated::is_ortho_in_frame(child));
    if let Some(frame_wm) = frame {
        let (a, d) = match frame_wm {
            crate::style::WritingMode::VerticalLr | crate::style::WritingMode::SidewaysLr => (0.2 * font, 0.8 * font),
            _ => (0.8 * font, 0.2 * font),
        };
        let bt = css.border_widths.top.resolve(&rc).unwrap_or(0.0);
        let bb = css.border_widths.bottom.resolve(&rc).unwrap_or(0.0);
        let pt = css.padding.top.resolve(&rc).unwrap_or(0.0).max(0.0);
        let pb = css.padding.bottom.resolve(&rc).unwrap_or(0.0).max(0.0);
        let b = if first { mt + bt + pt + a } else { item_h - mb - bb - pb - d };
        return (b, (item_h - b).max(0.0));
    }

    let start = crate::layout::inline::line_baseline::mark();
    let ortho = crate::layout::block::rotated_ortho::mark();
    let mut scratch = crate::layout::DisplayList::for_dom(dom);
    crate::layout::layout_block(
        dom, child, box_id, 0.0, 0.0, item_w, None, Some(item_w), None,
        false, true, &crate::layout::BlockFormattingContext::new(), ctx, &mut scratch,
    );
    crate::layout::block::rotated_ortho::restore(ortho);
    let (first_b, _, last_b) =
        crate::layout::inline::line_baseline::collect_from_block(dom, child, start, 0.0, item_h);
    crate::layout::inline::line_baseline::discard(start);

    let inner = if first { first_b.or(last_b).unwrap_or(mt + border_h) } else { last_b.or(first_b).unwrap_or(mt + border_h) };
    (inner, (item_h - inner).max(0.0))
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
    measure_item_baseline(dom, child, box_id, item_w, item_h, first, ctx).0
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
            dom.computed_style_idx(c.child).is_some_and(|s| {
                matches!(s.align_self, Some(AlignItems::Baseline | AlignItems::LastBaseline))
                    || matches!(s.align_content, Some(crate::style::JustifyContent::Baseline | crate::style::JustifyContent::LastBaseline))
            })
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
        let child_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        let is_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();
        let align = css.align_self.unwrap_or(container_align);
        let has_content_baseline = matches!(css.align_content, Some(crate::style::JustifyContent::Baseline | crate::style::JustifyContent::LastBaseline));
        let is_self_baseline = matches!(align, AlignItems::Baseline | AlignItems::LastBaseline);
        if (!is_self_baseline && (!has_content_baseline || is_ortho)) || super::distribute::has_pct(css.height) {
            continue;
        }
        let first = if is_self_baseline {
            align == AlignItems::Baseline
        } else {
            matches!(css.align_content, Some(crate::style::JustifyContent::Baseline))
        };
        let r = if first { c.r0 } else { c.r1.saturating_sub(1) };
        if r >= nrows {
            continue;
        }
        let cw = super::grid::span_size(col_sizes, c.c0, c.c1, col_gap);
        let (_, h) = crate::layout::measure_block(dom, c.child, c.box_id, cw, None, None, None, true, ctx);
        let (ascent, descent) = if has_content_baseline && !is_self_baseline {
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let rc = ResolveCtx {
                parent_content_w: cw, node_font_size: font, root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w, viewport_h: ctx.viewport_h,
            };
            let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(c.child);
            super::align::item_content_baseline_metrics(&css, font, &rc, is_ortho_frame)
        } else if is_ortho {
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let rc = ResolveCtx {
                parent_content_w: cw, node_font_size: font, root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w, viewport_h: ctx.viewport_h,
            };
            let mb = css.margin.bottom.resolve(&rc).unwrap_or(0.0);
            ((h - mb).max(0.0), mb)
        } else {
            measure_item_baseline(dom, c.child, c.box_id, cw, h, first, ctx)
        };

        let is_opposite = !is_ortho && child_wm != parent_wm && !child_wm.is_horizontal() && !parent_wm.is_horizontal();
        let single = c.rows() == 1;
        let tb = out[r].get_or_insert_with(TrackBaseline::default);
        if is_opposite {
            tb.end_max_ascent = tb.end_max_ascent.max(ascent);
            tb.end_max_descent = tb.end_max_descent.max(descent);
            tb.has_end = true;
        } else if first {
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
        let Some(css) = dom.computed_style_idx(c.child) else { continue; };
        let Some(ac) = css.align_content else { continue; };
        let is_fb = ac == crate::style::JustifyContent::Baseline;
        let is_lb = ac == crate::style::JustifyContent::LastBaseline;
        if !is_fb && !is_lb { continue; }
        let c_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        if c_wm.is_horizontal() != parent_wm.is_horizontal() { continue; }
        let r = if is_fb { c.r0 } else { c.r1.saturating_sub(1) };
        if let Some(Some(tb)) = out.get_mut(r) {
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let rc = ResolveCtx { parent_content_w: 0.0, node_font_size: font, root_font_size: crate::style::root_font_size(), viewport_w: ctx.viewport_w, viewport_h: ctx.viewport_h };
            let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(c.child);
            let (ascent, descent, sz) = super::align::item_content_metrics(&css, font, &rc, is_ortho_frame);
            let delta = if is_fb { (tb.max_ascent - ascent).max(0.0) } else { (tb.last_max_descent - descent).max(0.0) };
            tb.max_size = tb.max_size.max(sz + delta);
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
            let (mut ascent, mut descent) = measure_item_baseline(dom, child, box_id, item_w, h, first, ctx);
            if item.start == 0 {
                ascent += smbp_y;
            }
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

pub(in crate::layout) fn compute_col_baselines(
    dom: &Dom,
    cells: &[GridCell],
    ncols: usize,
    resolve: &ResolveCtx,
) -> Vec<Option<TrackBaseline>> {
    let mut out: Vec<Option<TrackBaseline>> = vec![None; ncols];
    let is_in_frame = crate::layout::block::rotated::rotated_frame_mode().is_some();
    for c in cells {
        let Some(css) = dom.computed_style_idx(c.child) else { continue; };
        let Some(ac) = css.align_content else { continue; };
        let is_fb = ac == crate::style::JustifyContent::Baseline;
        let is_lb = ac == crate::style::JustifyContent::LastBaseline;
        if !is_fb && !is_lb { continue; }
        let c_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(c.child);
        let is_col_ortho = if is_in_frame { is_ortho_frame } else { !c_wm.is_horizontal() };
        if !is_col_ortho { continue; }
        let col = if is_fb { c.c0 } else { c.c1.saturating_sub(1) };
        if col >= ncols { continue; }
        let font = font_px(&css, DEFAULT_FONT_SIZE);
        let (ascent, descent, _) = super::align::item_content_metrics(&css, font, resolve, is_ortho_frame);
        let tb = out[col].get_or_insert_with(TrackBaseline::default);
        if is_fb {
            tb.max_ascent = tb.max_ascent.max(ascent);
            tb.max_descent = tb.max_descent.max(descent);
            tb.has_first = true;
        } else {
            tb.last_max_ascent = tb.last_max_ascent.max(ascent);
            tb.last_max_descent = tb.last_max_descent.max(descent);
            tb.has_last = true;
        }
    }
    for c in cells {
        let Some(css) = dom.computed_style_idx(c.child) else { continue; };
        let Some(ac) = css.align_content else { continue; };
        let is_fb = ac == crate::style::JustifyContent::Baseline;
        let is_lb = ac == crate::style::JustifyContent::LastBaseline;
        if !is_fb && !is_lb { continue; }
        let c_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(c.child);
        let is_col_ortho = if is_in_frame { is_ortho_frame } else { !c_wm.is_horizontal() };
        if !is_col_ortho { continue; }
        let col = if is_fb { c.c0 } else { c.c1.saturating_sub(1) };
        if let Some(Some(tb)) = out.get_mut(col) {
            let font = font_px(&css, DEFAULT_FONT_SIZE);
            let (ascent, descent, sz) = super::align::item_content_metrics(&css, font, resolve, is_ortho_frame);
            let delta = if is_fb { (tb.max_ascent - ascent).max(0.0) } else { (tb.last_max_descent - descent).max(0.0) };
            tb.max_size = tb.max_size.max(sz + delta);
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
                content_row_h[r] = content_row_h[r].max(tb.required_size()).max(tb.max_size);
            }
        }
    }
}

pub(crate) use super::container_baseline::container_ascent;


