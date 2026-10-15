//! Subgrid sizing contributions (CSS Grid Level 2 §9)
//!
//! Subgrid items contribute intrinsic sizes to the parent grid's tracks.

use super::subgrid::{is_subgrid_item, subgrid_mbp};
use crate::Dom;
use crate::boxes::{BoxId, BoxTree};
use crate::dom::NodeIdx;
use crate::layout::grid::lines::{GridCell, collect_items, place_grid_items};
use crate::layout::measure_block;
use crate::layout::{LayoutCtx, ResolveCtx, font_px};
use crate::style::{DisplayKind, LineNames, WritingMode};
use crate::table;

pub(super) struct ContribItem {
    pub(super) child: Option<(NodeIdx, BoxId)>,
    pub(super) start: usize,
    pub(super) span: usize,
    pub(super) extra_size: f32,
    pub(super) extra_min: f32,
    pub(super) extra_mbp_x: f32,
    pub(super) avail_h: Option<f32>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_nested_items(
    dom: &Dom,
    tree: &BoxTree,
    node: NodeIdx,
    box_id: BoxId,
    span_cols: usize,
    span_rows: usize,
    col_lines: &[LineNames],
    row_lines: &[LineNames],
    target_axis_is_col: bool,
    parent_wm: WritingMode,
    base_offset: usize,
    parent_gap: f32,
    font_size: f32,
    top_rtl: bool,
    inherited_rows: Option<&[f32]>,
    ctx: &LayoutCtx,
    out: &mut Vec<ContribItem>,
) {
    let Some(css) = dom.computed_style_idx(node) else {
        return;
    };
    let resolve = ResolveCtx {
        parent_content_w: 0.0,
        node_font_size: font_px(&css, font_size),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let (smbp_x, embp_x, smbp_y, embp_y) = subgrid_mbp(&css, &resolve);
    let child_wm = css.writing_mode.unwrap_or_default();
    let is_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();
    let p_css = dom
        .node(node)
        .parent
        .and_then(|p| dom.computed_style_idx(p));
    let p_rtl = p_css
        .map(|c| c.direction == Some(crate::style::Direction::Rtl))
        .unwrap_or(false);
    let c_rtl = css.direction == Some(crate::style::Direction::Rtl);
    let rev = !is_ortho && (top_rtl != c_rtl);
    let (smbp, embp) = if target_axis_is_col == child_wm.is_horizontal() {
        if top_rtl {
            (embp_x, smbp_x)
        } else {
            (smbp_x, embp_x)
        }
    } else {
        (smbp_y, embp_y)
    };
    let resolved_standalone = if target_axis_is_col {
        super::aspect::resolve_standalone_row_tracks(
            &css,
            inherited_rows.and_then(|r| r.first().copied()),
            &resolve,
        )
    } else {
        None
    };
    let current_row_sizes = resolved_standalone.as_deref().or(inherited_rows);
    let sub_gap = if is_ortho {
        if target_axis_is_col {
            css.row_gap.or(css.gap)
        } else {
            css.gap
        }
    } else {
        if target_axis_is_col {
            css.gap
        } else {
            css.row_gap.or(css.gap)
        }
    }
    .and_then(|d| d.resolve(&resolve))
    .unwrap_or(parent_gap)
    .max(0.0);
    let half = (sub_gap - parent_gap) / 2.0;
    let target_span = if target_axis_is_col {
        span_cols
    } else {
        span_rows
    };

    let items = collect_items(dom, tree, box_id);
    let start_out_idx = out.len();
    if items.is_empty() {
        let (s, e) = if target_span <= 1 {
            (smbp + embp, 0.0)
        } else {
            (smbp, embp)
        };
        out.push(ContribItem {
            child: None,
            start: base_offset,
            span: 1,
            extra_size: s,
            extra_min: s,
            extra_mbp_x: 0.0,
            avail_h: None,
        });
        if target_span > 1 {
            out.push(ContribItem {
                child: None,
                start: base_offset + target_span - 1,
                span: 1,
                extra_size: e,
                extra_min: e,
                extra_mbp_x: 0.0,
                avail_h: None,
            });
        }
        return;
    }

    let flow = css
        .grid_auto_flow
        .unwrap_or(crate::style::grid_lines::GridAutoFlow {
            column: false,
            dense: false,
        });
    let is_col_sub = matches!(
        css.grid_template_columns.as_deref(),
        Some(crate::style::GridTemplate::Subgrid(_))
    );
    let is_row_sub = matches!(
        css.grid_template_rows.as_deref(),
        Some(crate::style::GridTemplate::Subgrid(_))
    );
    let (cells, _) = place_grid_items(
        dom,
        &items,
        css.grid_template_areas.as_deref(),
        col_lines,
        row_lines,
        span_cols,
        span_rows,
        flow,
        is_col_sub,
        is_row_sub,
    );
    for cell in cells {
        let (raw_start, track_span) = if target_axis_is_col {
            (cell.c0, cell.c1.saturating_sub(cell.c0).max(1))
        } else {
            (cell.r0, cell.r1.saturating_sub(cell.r0).max(1))
        };
        let track_start = if target_axis_is_col && rev {
            target_span.saturating_sub(raw_start + track_span)
        } else {
            raw_start
        };
        let cell_css = dom.computed_style_idx(cell.child).unwrap_or_default();
        let is_grid = cell_css
            .effective_display()
            .is_some_and(DisplayKind::is_grid_container);
        let cell_wm = cell_css.writing_mode.unwrap_or_default();
        let is_ortho_cell = cell_wm.is_horizontal() != parent_wm.is_horizontal();
        let (sub_col, sub_row) = is_subgrid_item(&cell_css, parent_wm);
        let nested_subgrid = is_grid && if target_axis_is_col { sub_col } else { sub_row };
        if nested_subgrid {
            let next_target_is_col = if is_ortho_cell {
                !target_axis_is_col
            } else {
                target_axis_is_col
            };
            let (next_cols, next_rows) = if next_target_is_col {
                let n_rows = if sub_row {
                    cell.r1.saturating_sub(cell.r0).max(1)
                } else {
                    match cell_css.grid_template_rows.as_deref() {
                        Some(crate::style::GridTemplate::Tracks(t)) => t.len().max(1),
                        _ => 1,
                    }
                };
                (track_span, n_rows)
            } else {
                let n_cols = if sub_col {
                    cell.c1.saturating_sub(cell.c0).max(1)
                } else {
                    match cell_css.grid_template_columns.as_deref() {
                        Some(crate::style::GridTemplate::Tracks(t)) => t.len().max(1),
                        _ => cell_css.grid_columns.unwrap_or(1).max(1) as usize,
                    }
                };
                (n_cols, track_span)
            };
            let dummy_cols = vec![Vec::new(); next_cols + 1];
            let dummy_rows = vec![Vec::new(); next_rows + 1];
            let start_nested = out.len();
            let nested_rows = if sub_row {
                current_row_sizes.map(|r| &r[cell.r0.min(r.len())..cell.r1.min(r.len())])
            } else {
                current_row_sizes.and_then(|r| r.get(cell.r0).map(std::slice::from_ref))
            };
            collect_nested_items(
                dom,
                tree,
                cell.child,
                cell.box_id,
                next_cols,
                next_rows,
                &dummy_cols,
                &dummy_rows,
                next_target_is_col,
                cell_wm,
                base_offset + track_start,
                sub_gap,
                font_size,
                top_rtl,
                nested_rows,
                ctx,
                out,
            );
            let (c_smbp_x, c_embp_x, _, _) = subgrid_mbp(&cell_css, &resolve);
            for it in &mut out[start_nested..] {
                it.extra_mbp_x += c_smbp_x + c_embp_x;
            }
        } else {
            let item_h = if target_axis_is_col {
                current_row_sizes.and_then(|r| r.get(cell.r0).copied())
            } else {
                None
            };
            out.push(ContribItem {
                child: Some((cell.child, cell.box_id)),
                start: base_offset + track_start,
                span: track_span,
                extra_size: 0.0,
                extra_min: 0.0,
                extra_mbp_x: 0.0,
                avail_h: item_h,
            });
        }
    }

    let max_occupied = out[start_out_idx..]
        .iter()
        .map(|it| it.start.saturating_sub(base_offset) + it.span)
        .max()
        .unwrap_or(0);
    let min_occupied = out[start_out_idx..]
        .iter()
        .map(|it| it.start.saturating_sub(base_offset))
        .min()
        .unwrap_or(0);
    let orig_len = out.len();
    if max_occupied > 0 && max_occupied < target_span && embp > 0.0 {
        out.push(ContribItem {
            child: None,
            start: base_offset + target_span - 1,
            span: 1,
            extra_size: embp,
            extra_min: embp,
            extra_mbp_x: 0.0,
            avail_h: None,
        });
    }
    if min_occupied > 0 && smbp > 0.0 {
        out.push(ContribItem {
            child: None,
            start: base_offset,
            span: 1,
            extra_size: smbp,
            extra_min: smbp,
            extra_mbp_x: 0.0,
            avail_h: None,
        });
    }
    for item in &mut out[start_out_idx..orig_len] {
        let rel_start = item.start.saturating_sub(base_offset);
        let rel_end = rel_start + item.span;
        let adj = (if rel_start == 0 { smbp } else { half })
            + (if rel_end >= target_span { embp } else { half });
        item.extra_size += adj;
        item.extra_min += adj;
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn collect_column_contributions(
    dom: &Dom,
    tree: &BoxTree,
    cells: &[GridCell],
    ncols: usize,
    needs_min: bool,
    font_size: f32,
    col_gap: f32,
    parent_wm: WritingMode,
    ctx: &LayoutCtx,
    parent_col_lines: &[LineNames],
    parent_areas: Option<&crate::style::GridAreas>,
    col_tracks: &[crate::style::GridTrack],
    resolve: &ResolveCtx,
    wmax: &mut [f32],
    wmin: &mut [f32],
) {
    for c in cells {
        let Some(css) = dom.computed_style_idx(c.child) else {
            continue;
        };
        if !css
            .effective_display()
            .is_some_and(DisplayKind::is_grid_container)
        {
            continue;
        }
        let (is_col_sub, is_row_sub) = is_subgrid_item(&css, parent_wm);
        if !is_col_sub {
            continue;
        }

        let child_wm = css.writing_mode.unwrap_or_default();
        let is_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();
        let sub_span = c.c1.saturating_sub(c.c0).max(1);
        let sub_span_rows = c.r1.saturating_sub(c.r0).max(1);
        let eff_rows = if is_row_sub {
            sub_span_rows
        } else {
            match css.grid_template_rows.as_deref() {
                Some(crate::style::GridTemplate::Tracks(t)) => t.len().max(1),
                _ => 1,
            }
        };

        let p_end = c.c1.min(parent_col_lines.len().saturating_sub(1));
        let p_slice = if c.c0 <= p_end && c.c0 < parent_col_lines.len() {
            &parent_col_lines[c.c0..=p_end]
        } else {
            &[]
        };
        let p_css = dom
            .node(c.child)
            .parent
            .and_then(|p| dom.computed_style_idx(p));
        let p_rtl = p_css
            .map(|c| c.direction == Some(crate::style::Direction::Rtl))
            .unwrap_or(false);
        let c_rtl = css.direction == Some(crate::style::Direction::Rtl);
        let rev = !is_ortho && (p_rtl != c_rtl);
        let mut sub_col_lines = match css.grid_template_columns.as_deref() {
            Some(crate::style::GridTemplate::Subgrid(l)) => {
                super::subgrid::expand_subgrid_line_names(l, sub_span, p_slice, rev)
            }
            _ => vec![Vec::new(); sub_span + 1],
        };
        super::subgrid::clamp_area_lines(
            parent_areas,
            c.c0,
            c.c1,
            sub_span,
            true,
            &mut sub_col_lines,
            rev,
        );
        let sub_row_lines = vec![Vec::new(); eff_rows + 1];

        let mut items = Vec::new();
        let (span_cols, span_rows) = if is_ortho {
            (eff_rows, sub_span)
        } else {
            (sub_span, eff_rows)
        };
        let (col_l, row_l) = if is_ortho {
            (&sub_row_lines, &sub_col_lines)
        } else {
            (&sub_col_lines, &sub_row_lines)
        };
        collect_nested_items(
            dom, tree, c.child, c.box_id, span_cols, span_rows, col_l, row_l, !is_ortho, child_wm,
            0, col_gap, font_size, p_rtl, None, ctx, &mut items,
        );

        let mut multi_span = Vec::new();
        for item in items {
            let span = item.span.max(1);
            let (w, min_w) = if let Some((child, box_id)) = item.child {
                let (bw, _) =
                    measure_block(dom, child, box_id, 0.0, item.avail_h, None, None, true, ctx);
                let cell_css = dom.computed_style_idx(child).unwrap_or_default();
                let is_ortho = cell_css.writing_mode.unwrap_or_default().is_horizontal()
                    != parent_wm.is_horizontal();
                let bmin = if is_ortho && item.avail_h.is_some() {
                    bw
                } else if needs_min {
                    table::min_content_with_floor(dom, child, font_size, ctx)
                } else {
                    0.0
                };
                (bw, bmin)
            } else {
                (0.0, 0.0)
            };
            if span == 1 {
                let p_col = c.c0 + item.start;
                if p_col < ncols {
                    wmax[p_col] = wmax[p_col].max(w + item.extra_size);
                    wmin[p_col] = wmin[p_col].max(min_w + item.extra_min);
                }
            } else {
                multi_span.push((item, w, min_w));
            }
        }
        multi_span.sort_by_key(|(it, _, _)| it.span);
        for (item, w, min_w) in multi_span {
            let p_start = c.c0 + item.start;
            let p_end = (p_start + item.span).min(ncols);
            if p_start >= p_end {
                continue;
            }
            let total_gaps = (p_end.saturating_sub(p_start + 1)) as f32 * col_gap;
            let mut current_span_w = total_gaps;
            let mut unfixed = Vec::new();
            for col in p_start..p_end {
                match col_tracks.get(col) {
                    Some(crate::style::GridTrack::Fixed(d)) => {
                        current_span_w += d.resolve(resolve).unwrap_or(0.0);
                    }
                    _ => {
                        current_span_w += wmax[col];
                        unfixed.push(col);
                    }
                }
            }
            let target_w = w + item.extra_size;
            if target_w > current_span_w {
                let targets = if !unfixed.is_empty() {
                    unfixed
                } else {
                    (p_start..p_end).collect()
                };
                let extra = (target_w - current_span_w) / targets.len() as f32;
                for col in targets {
                    wmax[col] += extra;
                }
            }
            if needs_min {
                let current_min: f32 =
                    (p_start..p_end).map(|col| wmin[col]).sum::<f32>() + total_gaps;
                let target_min = min_w + item.extra_min;
                if target_min > current_min {
                    let extra = (target_min - current_min) / (p_end - p_start) as f32;
                    for col in p_start..p_end {
                        wmin[col] += extra;
                    }
                }
            }
        }
    }
}

pub(in crate::layout) use super::subgrid_row_contrib::collect_row_contributions;
