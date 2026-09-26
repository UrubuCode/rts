//! `aspect-ratio` on a GRID item (CSS Sizing 4 §5, CSS Grid §6.6): a height
//! that is definite against the item's grid area transfers through the ratio
//! to the width — for the item's contribution to the column tracks, for its
//! final width in the area, and for the max-content width of the container
//! that holds it (`inline-grid`, a float, `width: max-content`).
//!
//! The transfer itself is `measure::aspect_ratio`, shared with `block.rs`;
//! this module only answers the grid's half of the question — WHEN the height
//! is definite (every row the item spans is a fixed track, or the tracks are
//! already sized) — so the three readers above ask one function and cannot
//! disagree about what the item is worth.

use super::*;
use super::lines::{collect_items, place_grid_items};
use crate::layout::measure::aspect_ratio;
use crate::style::{Dimension, GridTrack};

/// The height of the grid area over rows `r0..r1` when each of them is a
/// FIXED track, which is what makes a percentage height inside it definite
/// before track sizing runs (Grid §6.5 / §11.5). `None` as soon as one row is
/// intrinsic or flexible: its size depends on the items, this one included.
pub(in crate::layout) fn definite_area_height(
    explicit_rows: &[GridTrack],
    auto_row: Option<&GridTrack>,
    (r0, r1): (usize, usize),
    row_gap: f32,
    container_h: Option<f32>,
    resolve: &ResolveCtx,
) -> Option<f32> {
    let mut sum = 0.0;
    for r in r0..r1.max(r0 + 1) {
        match explicit_rows.get(r).or(auto_row)? {
            GridTrack::Fixed(d) => sum += resolve_height(Some(*d), container_h, resolve)?,
            _ => return None,
        }
    }
    Some(sum + (r1.saturating_sub(r0)).saturating_sub(1) as f32 * row_gap)
}

/// The OUTER width (margin box) of grid item `child` when it declares a ratio,
/// an `auto` width and a height definite against `area_h` — or `None`, and the
/// caller sizes the item as it always did. The ratio-determining height is
/// clamped by `min-`/`max-height` BEFORE the transfer and the width by
/// `min-`/`max-width` after it (Sizing 4 §5.1), in the box `box-sizing` names.
pub(in crate::layout) fn ratio_outer_width(
    dom: &Dom,
    child: NodeIdx,
    area_h: Option<f32>,
    cb_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let css = dom.computed_style_idx(child)?;
    aspect_ratio::declared_ratio(&css)?;
    if !matches!(css.width, None | Some(Dimension::Auto)) {
        return None;
    }
    let resolve = ResolveCtx {
        parent_content_w: cb_w,
        node_font_size: font_px(&css, parent_font),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let [bt, br, bb, bl] = crate::style::borders::used_widths(&css);
    let frame_h = css.padding.resolve_h(&resolve) + bl + br;
    let frame_v = css.padding.resolve_v(&resolve) + bt + bb;
    let border_box = css.border_box.unwrap_or(false);
    let content = |v: f32, frame: f32| if border_box { (v - frame).max(0.0) } else { v };
    let h = content(resolve_height(css.height, area_h, &resolve)?, frame_v);
    let mnh = resolve_height(css.min_height, area_h, &resolve).map(|v| content(v, frame_v));
    let mxh = resolve_height(css.max_height, area_h, &resolve).map(|v| content(v, frame_v));
    let h = crate::style::clamp_size(h, mnh, mxh);
    let w = aspect_ratio::width_for_height(&css, h, frame_h, frame_v)?;
    let mnw = css.min_width.and_then(|d| d.resolve(&resolve)).map(|v| content(v, frame_h));
    let mxw = css.max_width.and_then(|d| d.resolve(&resolve)).map(|v| content(v, frame_h));
    let w = crate::style::clamp_size(w, mnw, mxw);
    Some(w + frame_h + css.margin.resolve_h(&resolve))
}

/// A LOWER bound on the max-content width of grid container `id` from the
/// items whose ratio sizes them: per column, the widest such item, summed with
/// the gutters. `None` when the box is not a grid or no item declares a
/// ratio — the common case, answered before any placement is done.
///
/// A bound and not the answer because the intrinsic measure of a grid is still
/// the block rule (the widest item, `measure/tree.rs`); what this adds is the
/// one thing that rule cannot see — that an item with `height: 100%` and a
/// ratio is as wide as its ROW makes it, not as its (empty) content.
pub(in crate::layout) fn intrinsic_floor(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    font: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    // The same question `block.rs` asks before it hands a box to the grid.
    let css = dom.computed_style_idx(id)?;
    if !css.effective_display().is_some_and(crate::style::DisplayKind::is_grid_container) {
        return None;
    }
    let items = collect_items(dom, tree, box_id);
    let any_ratio = items.iter().any(|i| {
        dom.computed_style_idx(i.node)
            .is_some_and(|c| aspect_ratio::declared_ratio(&c).is_some())
    });
    if !any_ratio {
        return None;
    }
    // No containing block is known here: percentages of the gaps and of the
    // container's own height resolve to nothing, as elsewhere in max-content.
    let resolve = ResolveCtx {
        parent_content_w: 0.0,
        node_font_size: font_px(&css, font),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = |d: Option<Dimension>| d.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let col_gap = gap(css.gap.or(css.row_gap));
    let row_gap = gap(css.row_gap.or(css.gap));
    let own_h = resolve_height(css.height, None, &resolve);
    let areas = css.grid_template_areas.clone();
    let ncols = match &css.grid_template_columns {
        Some(t) => super::tracks::expand_auto_repeats((**t).clone(), 0.0, col_gap).0.len(),
        None => match &areas {
            Some(a) => a.cols,
            None => css.grid_columns.unwrap_or(1).max(1) as usize,
        },
    }
    .max(1);
    let rows = match &css.grid_template_rows {
        Some(t) => super::tracks::expand_auto_repeats((**t).clone(), own_h.unwrap_or(0.0), row_gap).0,
        None => Vec::new(),
    };
    let flow = css.grid_auto_flow.unwrap_or(crate::style::grid_lines::GridAutoFlow {
        column: false,
        dense: false,
    });
    let (cells, ncols) = place_grid_items(dom, &items, areas.as_deref(), ncols, rows.len(), flow);
    let ncols = ncols.max(1);
    let mut floors = vec![0.0f32; ncols];
    for c in cells.iter().filter(|c| c.c1 - c.c0 == 1 && c.c0 < ncols) {
        let area_h = definite_area_height(
            &rows,
            css.grid_auto_rows.as_ref(),
            (c.r0, c.r1),
            row_gap,
            own_h,
            &resolve,
        );
        if let Some(w) = ratio_outer_width(dom, c.child, area_h, 0.0, font, ctx) {
            floors[c.c0] = floors[c.c0].max(w);
        }
    }
    let widest: f32 = floors.iter().sum();
    (widest > 0.0).then(|| widest + (floors.len() - 1) as f32 * col_gap)
}
