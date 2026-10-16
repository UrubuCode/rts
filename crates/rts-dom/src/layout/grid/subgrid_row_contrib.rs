//! Subgrid row sizing contributions (CSS Grid Level 2 §9).

use super::subgrid::subgrid_mbp;
use super::subgrid_contrib::collect_nested_items;
use crate::Dom;
use crate::layout::grid::lines::GridCell;
use crate::layout::measure_block;
use crate::layout::{font_px, LayoutCtx, ResolveCtx};
use crate::style::{DisplayKind, LineNames, WritingMode};

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn collect_row_contributions(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    cells: &[GridCell],
    col_sizes: &[f32],
    col_gap: f32,
    nrows: usize,
    font_size: f32,
    row_gap: f32,
    parent_wm: WritingMode,
    ctx: &LayoutCtx,
    parent_row_lines: &[LineNames],
    parent_areas: Option<&crate::style::GridAreas>,
    explicit_rows: &[crate::style::GridTrack],
    auto_row: Option<&crate::style::GridTrack>,
    parent_resolve: &ResolveCtx,
    container_content_h: Option<f32>,
    content_row_h: &mut [f32],
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
        let (is_sub_col, is_row_sub) = super::subgrid::is_subgrid_node(dom, c.child, parent_wm);
        if !is_row_sub {
            continue;
        }

        let resolve = ResolveCtx {
            parent_content_w: 0.0,
            node_font_size: font_px(&css, font_size),
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let (smbp_x, embp_x, _, _) = subgrid_mbp(&css, &resolve);
        let child_wm = dom.base_style_idx(c.child).and_then(|c| c.writing_mode).unwrap_or_default();
        let is_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();
        let sub_inner_w =
            (super::grid::span_size(col_sizes, c.c0, c.c1, col_gap) - smbp_x - embp_x).max(0.0);
        let sub_span_rows = c.r1.saturating_sub(c.r0).max(1);
        let sub_span_cols = c.c1.saturating_sub(c.c0).max(1);
        let eff_cols = if is_sub_col {
            sub_span_cols
        } else {
            match css.grid_template_columns.as_deref() {
                Some(crate::style::GridTemplate::Tracks(t)) => t.len().max(1),
                _ => css.grid_columns.unwrap_or(1).max(1) as usize,
            }
        };

        let p_end = c.r1.min(parent_row_lines.len().saturating_sub(1));
        let p_slice = if c.r0 <= p_end && c.r0 < parent_row_lines.len() {
            &parent_row_lines[c.r0..=p_end]
        } else {
            &[]
        };
        let p_css = dom
            .node(c.child)
            .parent
            .and_then(|p| dom.computed_style_idx(p));
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
        let mut sub_row_lines = match sub_template {
            Some(crate::style::GridTemplate::Subgrid(l)) => {
                super::subgrid::expand_subgrid_line_names(l, sub_span_rows, p_slice, rev)
            }
            _ => vec![Vec::new(); sub_span_rows + 1],
        };
        super::subgrid::clamp_area_lines(
            parent_areas,
            c.r0,
            c.r1,
            sub_span_rows,
            false,
            &mut sub_row_lines,
            rev,
        );
        let sub_col_lines = vec![Vec::new(); eff_cols + 1];

        let mut items = Vec::new();
        let (span_cols, span_rows, target_is_col) = if is_ortho {
            (sub_span_rows, eff_cols, true)
        } else {
            (eff_cols, sub_span_rows, false)
        };
        let (col_l, row_l) = if is_ortho {
            (&sub_row_lines, &sub_col_lines)
        } else {
            (&sub_col_lines, &sub_row_lines)
        };
        collect_nested_items(
            dom,
            tree,
            c.child,
            c.box_id,
            span_cols,
            span_rows,
            col_l,
            row_l,
            target_is_col,
            parent_wm,
            parent_row_dir,
            0,
            row_gap,
            font_size,
            None,
            ctx,
            &mut items,
        );

        let mut multi_span = Vec::new();
        for item in items {
            let span = item.span.max(1);
            let h = if let Some((child, box_id)) = item.child {
                let base_w = if is_sub_col {
                    (super::grid::span_size(col_sizes, c.c0, c.c1, col_gap) - smbp_x - embp_x)
                        .max(0.0)
                } else {
                    sub_inner_w / eff_cols as f32
                };
                let item_w = (base_w - item.extra_mbp_x).max(0.0);
                let (_, h) = measure_block(
                    dom, child, box_id, item_w, None, None, None, true, ctx,
                );
                h
            } else {
                0.0
            };
            if span == 1 {
                let p_row = c.r0 + item.start;
                if p_row < nrows {
                    content_row_h[p_row] = content_row_h[p_row].max(h + item.extra_size);
                }
            } else {
                multi_span.push((item, h));
            }
        }
        multi_span.sort_by_key(|(it, _)| it.span);
        for (item, h) in multi_span {
            let p_start = c.r0 + item.start;
            let p_end = (p_start + item.span).min(nrows);
            if p_start >= p_end {
                continue;
            }
            let total_gaps = (p_end.saturating_sub(p_start + 1)) as f32 * row_gap;
            let mut current_span_h = total_gaps;
            let mut unfixed = Vec::new();
            for r in p_start..p_end {
                let track = explicit_rows.get(r).or(auto_row);
                match track {
                    Some(crate::style::GridTrack::Fixed(d)) => {
                        current_span_h += crate::layout::positioned::positioned::resolve_height(
                            Some(*d),
                            container_content_h,
                            parent_resolve,
                        )
                        .unwrap_or(0.0);
                    }
                    _ => {
                        current_span_h += content_row_h[r];
                        unfixed.push(r);
                    }
                }
            }
            let target_h = h + item.extra_size;
            if target_h > current_span_h {
                let targets = if !unfixed.is_empty() {
                    unfixed
                } else {
                    (p_start..p_end).collect()
                };
                let extra = (target_h - current_span_h) / targets.len() as f32;
                for r in targets {
                    content_row_h[r] += extra;
                }
            }
        }
    }
}
