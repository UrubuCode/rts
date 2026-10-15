//! Max-content width of a grid container (CSS Grid 1 §11 & CSS Sizing 3 §5).
//!
//! The max-content contribution of a grid container to its parent formatting context
//! is the sum of the max-content sizes of its column tracks, plus the gutters between them.

use super::lines::{collect_items, place_grid_items};
use crate::Dom;
use crate::boxes::{BoxId, BoxTree};
use crate::dom::NodeIdx;
use crate::layout::{LayoutCtx, ResolveCtx, font_px};
use crate::style::{GridTemplate, GridTrack};

pub(crate) fn grid_min_content_width(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    box_id: Option<BoxId>,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    grid_content_width(dom, tree, id, box_id, font, true, ctx)
}

pub(crate) fn grid_max_content_width(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    box_id: Option<BoxId>,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    grid_content_width(dom, tree, id, box_id, font, false, ctx)
}

fn grid_content_width(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    box_id: Option<BoxId>,
    font: f32,
    is_min: bool,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(id) else {
        return 0.0;
    };
    let Some(concrete_box) = box_id.or_else(|| tree.boxes_of(id).first().copied()) else {
        return 0.0;
    };

    let resolve = ResolveCtx {
        parent_content_w: 0.0,
        node_font_size: font_px(&css, font),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let col_gap = css
        .gap
        .or(css.row_gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let row_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);

    let areas = css.grid_template_areas.clone();
    let container_h = css.height.and_then(|d| d.resolve(&resolve))
        .or_else(|| super::aspect::find_containing_block_height(dom, id))
        .unwrap_or(0.0);
    let is_p_grid = super::subgrid::is_parent_grid(dom, id);
    let min_col_s = super::aspect::container_min_size_for_auto_repeat(&css, true, Some(container_h), &resolve);
    let (col_tracks, col_lines) = match css.grid_template_columns.as_deref() {
        Some(GridTemplate::Subgrid(_)) if is_p_grid => {
            let s = super::subgrid::subgrid_span(css.grid_column_start.as_ref(), css.grid_column_end.as_ref());
            (vec![GridTrack::Auto; s], vec![Vec::new(); s + 1])
        }
        Some(GridTemplate::Tracks(t)) => {
            let (tr, _, cl) = super::tracks::expand_auto_repeats(t, 0.0, col_gap, min_col_s);
            (tr, cl)
        }
        _ => {
            let n = match &areas {
                Some(a) => a.cols,
                None => css.grid_columns.unwrap_or(1).max(1) as usize,
            };
            (vec![GridTrack::Auto; n], vec![Vec::new(); n + 1])
        }
    };
    let ncols = col_tracks.len().max(1);

    let min_row_s = super::aspect::container_min_size_for_auto_repeat(&css, false, Some(container_h), &resolve);
    let (rows, row_lines) = match css.grid_template_rows.as_deref() {
        Some(GridTemplate::Subgrid(_)) if is_p_grid => {
            let s = super::subgrid::subgrid_span(css.grid_row_start.as_ref(), css.grid_row_end.as_ref());
            (vec![GridTrack::Auto; s], vec![Vec::new(); s + 1])
        }
        Some(GridTemplate::Tracks(t)) => {
            let (tr, _, rl) = super::tracks::expand_auto_repeats(t, container_h, row_gap, min_row_s);
            (tr, rl)
        }
        _ => (Vec::new(), Vec::new()),
    };

    let items = collect_items(dom, tree, concrete_box);
    let flow = css
        .grid_auto_flow
        .unwrap_or(crate::style::grid_lines::GridAutoFlow {
            column: false,
            dense: false,
        });
    let placement = place_grid_items(
        dom,
        &items,
        areas.as_deref(),
        &col_lines,
        &row_lines,
        ncols,
        rows.len(),
        flow,
        false,
        false,
    );
    let (cells, placed_cols) = (placement.cells, placement.ncols);
    let explicit_cols_n = col_tracks.len();
    let mut col_tracks = col_tracks;
    let ncols = ncols.max(placed_cols);
    while col_tracks.len() < ncols {
        let idx = col_tracks.len().saturating_sub(explicit_cols_n);
        let tr = css.grid_auto_columns.as_ref()
            .map_or(crate::style::GridTrack::Auto, |t| t.track_at(idx).clone());
        col_tracks.push(tr);
    }

    let mut wmax = vec![0.0f32; ncols];
    let mut wmin = vec![0.0f32; ncols];
    let item_w = |c: &super::lines::GridCell| {
        let area_h = super::aspect::definite_area_height(
            &rows,
            css.grid_auto_rows.as_ref(),
            (c.r0, c.r1),
            row_gap,
            None,
            &resolve,
        );
        super::aspect::grid_item_outer_width(dom, c.child, c.box_id, area_h, 0.0, &css, font, ctx)
    };
    for c in &cells {
        if c.c1 - c.c0 != 1 || c.c0 >= ncols {
            continue;
        }
        let max = item_w(c);
        let min = if super::aspect::is_item_orthogonal(dom, c.child, &css) {
            max
        } else {
            let area_h = super::aspect::definite_area_height(
                &rows,
                css.grid_auto_rows.as_ref(),
                (c.r0, c.r1),
                row_gap,
                None,
                &resolve,
            );
            super::aspect::ratio_outer_width(dom, c.child, area_h, 0.0, font, ctx)
                .unwrap_or_else(|| crate::table::min_content_with_floor(dom, c.child, font, ctx))
        };
        wmax[c.c0] = wmax[c.c0].max(max);
        wmin[c.c0] = wmin[c.c0].max(min);
    }

    super::subgrid::collect_column_contributions(
        dom,
        tree,
        &cells,
        ncols,
        is_min,
        font,
        col_gap,
        css.writing_mode.unwrap_or_default(),
        ctx,
        &col_lines,
        areas.as_deref(),
        &col_tracks,
        &resolve,
        &mut wmax,
        &mut wmin,
    );

    if is_min {
        super::distribute::distribute_spanning_slice(
            &col_tracks,
            &cells,
            ncols,
            col_gap,
            &resolve,
            |c| {
                dom.computed_style_idx(c.child).is_some_and(|css| {
                    super::subgrid::is_subgrid_item(&css, css.writing_mode.unwrap_or_default()).0
                })
            },
            |c| {
                if super::aspect::is_item_orthogonal(dom, c.child, &css) {
                    item_w(c)
                } else {
                    crate::table::min_content_with_floor(dom, c.child, font, ctx)
                }
            },
            &mut wmin,
        );
    } else {
        super::distribute::distribute_spanning_slice(
            &col_tracks,
            &cells,
            ncols,
            col_gap,
            &resolve,
            |c| {
                dom.computed_style_idx(c.child).is_some_and(|css| {
                    super::subgrid::is_subgrid_item(&css, css.writing_mode.unwrap_or_default()).0
                })
            },
            |c| item_w(c),
            &mut wmax,
        );
    }

    let dim_val = |d: &crate::style::Dimension, fallback: f32| -> f32 {
        match d {
            crate::style::Dimension::Percent(_) => fallback,
            _ => d.resolve(&resolve).unwrap_or(fallback),
        }
    };
    let mut max_fr_share = 0.0f32;
    for (i, track) in col_tracks.iter().enumerate() {
        if let GridTrack::Fr(f) = track {
            if *f > 0.0 {
                let w = if is_min {
                    wmin.get(i).copied().unwrap_or(0.0)
                } else {
                    wmax.get(i).copied().unwrap_or(0.0)
                };
                max_fr_share = max_fr_share.max(w / f);
            }
        }
    }

    let mut total = 0.0f32;
    for (i, track) in col_tracks.iter().enumerate() {
        let wx = wmax.get(i).copied().unwrap_or(0.0);
        let wm = wmin.get(i).copied().unwrap_or(0.0);
        let size = if is_min {
            match track {
                GridTrack::Fixed(d) => dim_val(d, wm),
                GridTrack::Bounded { min, .. } => dim_val(min, wm),
                GridTrack::Intrinsic { min, .. } => match min {
                    crate::style::TrackBound::Fixed(d) => dim_val(d, wm),
                    crate::style::TrackBound::MaxContent => wx,
                    crate::style::TrackBound::FitContent(d) => dim_val(d, wm).min(wx),
                    _ => wm,
                },
                GridTrack::Fr(f) => max_fr_share * f,
                _ => wm,
            }
        } else {
            match track {
                GridTrack::Fixed(d) => dim_val(d, wx),
                GridTrack::Bounded { max, .. } => dim_val(max, wx),
                GridTrack::Intrinsic { max, .. } => match max {
                    crate::style::TrackBound::Fixed(d) => dim_val(d, wx),
                    crate::style::TrackBound::FitContent(d) => dim_val(d, wx).min(wx),
                    crate::style::TrackBound::MinContent => wm,
                    _ => wx,
                },
                GridTrack::Fr(f) => max_fr_share * f,
                _ => wx,
            }
        };
        total += size;
    }
    total += (col_tracks.len().saturating_sub(1)) as f32 * col_gap;

    let ratio_floor = super::aspect::intrinsic_floor(dom, tree, id, concrete_box, font, ctx);
    total.max(ratio_floor.unwrap_or(0.0))
}
