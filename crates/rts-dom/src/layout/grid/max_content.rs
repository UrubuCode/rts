//! Max-content width of a grid container (CSS Grid 1 §11 & CSS Sizing 3 §5).
//!
//! The max-content contribution of a grid container to its parent formatting context
//! is the sum of the max-content sizes of its column tracks, plus the gutters between them.

use super::lines::{collect_items, place_grid_items};
use crate::Dom;
use crate::boxes::{BoxId, BoxTree};
use crate::dom::NodeIdx;
use crate::layout::{LayoutCtx, ResolveCtx, font_px, intrinsic_outer_width};
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
    let (col_tracks, col_lines) = match css.grid_template_columns.as_deref() {
        Some(GridTemplate::Tracks(t)) => {
            let (tr, _, cl) = super::tracks::expand_auto_repeats(t, 0.0, col_gap);
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

    let (rows, row_lines) = match css.grid_template_rows.as_deref() {
        Some(GridTemplate::Tracks(t)) => {
            let (tr, _, rl) = super::tracks::expand_auto_repeats(t, 0.0, row_gap);
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
    let (cells, placed_cols) = place_grid_items(
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
    let ncols = ncols.max(placed_cols);

    let mut wmax = vec![0.0f32; ncols];
    let mut wmin = vec![0.0f32; ncols];
    for c in &cells {
        if c.c1 - c.c0 != 1 || c.c0 >= ncols {
            continue;
        }
        let area_h = super::aspect::definite_area_height(
            &rows,
            css.grid_auto_rows.as_ref(),
            (c.r0, c.r1),
            row_gap,
            None,
            &resolve,
        );
        let ratio_w = super::aspect::ratio_outer_width(dom, c.child, area_h, 0.0, font, ctx);
        let max = ratio_w.unwrap_or_else(|| intrinsic_outer_width(dom, c.child, font, ctx));
        let min = ratio_w
            .unwrap_or_else(|| crate::table::min_content_with_floor(dom, c.child, font, ctx));
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
        super::tracks::distribute_spanning_columns(
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
            |c| crate::table::min_content_with_floor(dom, c.child, font, ctx),
            &mut wmin,
        );
    } else {
        super::tracks::distribute_spanning_columns(
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
            |c| intrinsic_outer_width(dom, c.child, font, ctx),
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
