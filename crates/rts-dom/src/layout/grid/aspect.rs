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

use super::lines::{collect_items, place_grid_items};
use super::*;
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
        let track = explicit_rows
            .get(r)
            .or_else(|| auto_row.map(|t| t.track_at(r.saturating_sub(explicit_rows.len()))))?;
        match track {
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
    let resolve = ResolveCtx {
        parent_content_w: cb_w,
        node_font_size: font_px(&css, parent_font),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    if let Some((w, _)) = crate::inline_box::replaced_inline_size(
        dom,
        child,
        &css,
        ctx.viewport_w,
        area_h,
        (None, None),
        ctx,
    ) {
        return Some(w + css.margin.resolve_h(&resolve));
    }
    aspect_ratio::declared_ratio(&css)?;
    if !matches!(css.width, None | Some(Dimension::Auto)) {
        return None;
    }
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
    let mnw = css
        .min_width
        .and_then(|d| d.resolve(&resolve))
        .map(|v| content(v, frame_h));
    let mxw = css
        .max_width
        .and_then(|d| d.resolve(&resolve))
        .map(|v| content(v, frame_h));
    let w = crate::style::clamp_size(w, mnw, mxw);
    Some(w + frame_h + css.margin.resolve_h(&resolve))
}

pub(in crate::layout) fn grid_item_min_content_width(
    dom: &Dom,
    child: NodeIdx,
    area_h: Option<f32>,
    font_size: f32,
    resolve: &ResolveCtx,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(child) else { return 0.0 };
    let [bt, br, bb, bl] = crate::style::borders::used_widths(&css);
    let frame_h = css.padding.resolve_h(resolve) + bl + br + css.margin.resolve_h(resolve);
    let frame_v = css.padding.resolve_v(resolve) + bt + bb;
    if let Some(min_w) = css.min_width.and_then(|d| d.resolve(resolve)) {
        return min_w + frame_h;
    }
    use crate::scrollbar::Overflow::{Clip, Visible};
    let axis_visible = |o: Option<crate::scrollbar::Overflow>| matches!(o.unwrap_or(Visible), Visible | Clip);
    if !axis_visible(css.overflow_x) || !axis_visible(css.overflow_y) {
        return frame_h;
    }
    if let Some(w) = css.width.and_then(|d| d.resolve(resolve)) {
        return w + frame_h;
    }
    let is_rep = matches!(&dom.node(child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
    let has_ratio = is_rep || aspect_ratio::declared_ratio(&css).is_some();
    if has_ratio {
        if let Some(h) = css.height.and_then(|d| resolve_height(Some(d), area_h, resolve)).or(area_h) {
            let content_h = (h - frame_v).max(0.0);
            if let Some((w, _)) = crate::inline_box::replaced_inline_size(
                dom, child, &css, ctx.viewport_w, Some(content_h), (None, Some(content_h)), ctx,
            ) {
                return w + css.margin.resolve_h(resolve);
            }
            if let Some(w) = aspect_ratio::width_for_height(&css, content_h, frame_h, frame_v) {
                return w + frame_h;
            }
        }
        return frame_h;
    }
    crate::table::min_content(dom, child, font_size, ctx)
}

pub(in crate::layout) fn grid_item_min_content_height(
    dom: &Dom,
    child: NodeIdx,
    cell_w: f32,
    _font_size: f32,
    resolve: &ResolveCtx,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(child) else { return 0.0 };
    let [bt, br, bb, bl] = crate::style::borders::used_widths(&css);
    let frame_v = css.padding.resolve_v(resolve) + bt + bb + css.margin.resolve_v(resolve);
    let frame_h = css.padding.resolve_h(resolve) + bl + br;
    if let Some(min_h) = css.min_height.and_then(|d| d.resolve(resolve)) {
        return min_h + frame_v;
    }
    use crate::scrollbar::Overflow::{Clip, Visible};
    let axis_visible = |o: Option<crate::scrollbar::Overflow>| matches!(o.unwrap_or(Visible), Visible | Clip);
    if !axis_visible(css.overflow_x) || !axis_visible(css.overflow_y) {
        return frame_v;
    }
    if let Some(h) = css.height.and_then(|d| d.resolve(resolve)) {
        return h + frame_v;
    }
    let is_rep = matches!(&dom.node(child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
    let has_ratio = is_rep || aspect_ratio::declared_ratio(&css).is_some();
    if has_ratio {
        if let Some(w) = css.width.and_then(|d| d.resolve(resolve)).or_else(|| (cell_w > 0.0).then_some(cell_w)) {
            let content_w = (w - frame_h).max(0.0);
            if let Some((_, h)) = crate::inline_box::replaced_inline_size(
                dom, child, &css, content_w, None, (Some(content_w), None), ctx,
            ) {
                return h + css.margin.resolve_v(resolve);
            }
            if let Some(h) = aspect_ratio::height_for_width(&css, content_w, frame_h, frame_v) {
                return h + frame_v;
            }
        }
        return frame_v;
    }
    frame_v
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
    if !css
        .effective_display()
        .is_some_and(crate::style::DisplayKind::is_grid_container)
    {
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
    let (ncols, col_lines) = match css.grid_template_columns.as_deref() {
        Some(crate::style::GridTemplate::Tracks(t)) => {
            let (tr, _, cl) = super::tracks::expand_auto_repeats(t, 0.0, col_gap, None);
            (tr.len(), cl)
        }
        _ => {
            let n = match &areas {
                Some(a) => a.cols,
                None => css.grid_columns.unwrap_or(1).max(1) as usize,
            };
            (n, vec![Vec::new(); n + 1])
        }
    };
    let ncols = ncols.max(1);
    let (rows, row_lines) = match css.grid_template_rows.as_deref() {
        Some(crate::style::GridTemplate::Tracks(t)) => {
            let (tr, _, rl) = super::tracks::expand_auto_repeats(t, own_h.unwrap_or(0.0), row_gap, None);
            (tr, rl)
        }
        _ => (Vec::new(), Vec::new()),
    };
    let flow = css
        .grid_auto_flow
        .unwrap_or(crate::style::grid_lines::GridAutoFlow {
            column: false,
            dense: false,
        });
    let (cells, ncols) = place_grid_items(
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

pub(in crate::layout) fn resolve_standalone_row_tracks(
    css: &ComputedStyle,
    container_h: Option<f32>,
    resolve: &ResolveCtx,
) -> Option<Vec<f32>> {
    let t = match css.grid_template_rows.as_deref() {
        Some(crate::style::GridTemplate::Tracks(t)) => t,
        _ => return None,
    };
    let gap = css
        .gap
        .or(css.row_gap)
        .and_then(|d| d.resolve(resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let (expanded, _, _) = super::tracks::expand_auto_repeats(t, 0.0, gap, None);
    let h = css
        .height
        .and_then(|d| resolve_height(Some(d), container_h, resolve))
        .or(container_h);
    let mut fixed_sum = 0.0f32;
    let mut total_fr = 0.0f32;
    for track in &expanded {
        match track {
            crate::style::GridTrack::Fixed(d) => {
                fixed_sum += resolve_height(Some(*d), h, resolve).unwrap_or(0.0);
            }
            crate::style::GridTrack::Fr(fr) => {
                total_fr += fr;
            }
            _ => {}
        }
    }
    let total_gaps = expanded.len().saturating_sub(1) as f32 * gap;
    let flex_share = if total_fr > 0.0 {
        if let Some(tot) = h {
            (tot - fixed_sum - total_gaps).max(0.0) / total_fr
        } else {
            0.0
        }
    } else {
        0.0
    };
    let sizes = expanded
        .into_iter()
        .map(|track| match track {
            crate::style::GridTrack::Fixed(d) => resolve_height(Some(d), h, resolve).unwrap_or(0.0),
            crate::style::GridTrack::Fr(fr) => fr * flex_share,
            _ => 0.0,
        })
        .collect();
    Some(sizes)
}

pub(in crate::layout) fn find_containing_block_height(dom: &Dom, id: NodeIdx) -> Option<f32> {
    let mut curr = dom.node(id).parent;
    while let Some(pid) = curr {
        if let Some(pcss) = dom.computed_style_idx(pid) {
            if let Some(crate::style::Dimension::Px(px)) = pcss.height {
                if px > 0.0 {
                    return Some(px);
                }
            }
        }
        curr = dom.node(pid).parent;
    }
    None
}

pub(in crate::layout) fn container_min_size_for_auto_repeat(
    css: &ComputedStyle,
    is_col: bool,
    container_h: Option<f32>,
    resolve: &ResolveCtx,
) -> Option<f32> {
    if is_col {
        if let Some(mw) = css.min_width.and_then(|d| d.resolve(resolve)) {
            if mw > 0.0 {
                return Some(mw);
            }
        }
        if let Some(r) = aspect_ratio::declared_ratio(css) {
            if let Some(mnh) = resolve_height(css.min_height, container_h, resolve) {
                let border_box = css.border_box.unwrap_or(false);
                let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
                let fh = css.padding.resolve_h(resolve) + bl + br;
                let fv = css.padding.resolve_v(resolve) + bt + bb;
                let h = if border_box { (mnh - fv).max(0.0) } else { mnh };
                let w = h * r;
                let outer_w = if border_box { w + fh } else { w };
                return Some(outer_w);
            }
        }
    } else {
        if let Some(mh) = resolve_height(css.min_height, container_h, resolve) {
            if mh > 0.0 {
                return Some(mh);
            }
        }
        if let Some(r) = aspect_ratio::declared_ratio(css) {
            if let Some(mnw) = css.min_width.and_then(|d| d.resolve(resolve)) {
                let border_box = css.border_box.unwrap_or(false);
                let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
                let fh = css.padding.resolve_h(resolve) + bl + br;
                let fv = css.padding.resolve_v(resolve) + bt + bb;
                let w = if border_box { (mnw - fh).max(0.0) } else { mnw };
                let h = w / r;
                let outer_h = if border_box { h + fv } else { h };
                return Some(outer_h);
            }
        }
    }
    None
}

pub(in crate::layout) fn empty_grid_dimensions(
    col_tracks: &[crate::style::GridTrack],
    explicit_rows: &[crate::style::GridTrack],
    col_gap: f32,
    row_gap: f32,
    content_w: f32,
    container_h: Option<f32>,
    css: &ComputedStyle,
    resolve: &ResolveCtx,
) -> (f32, f32) {
    let w = if col_tracks.is_empty() {
        content_w
    } else {
        col_tracks
            .iter()
            .map(|t| match t {
                crate::style::GridTrack::Fixed(d) => d.resolve(resolve).unwrap_or(0.0),
                _ => 0.0,
            })
            .sum::<f32>()
            + col_tracks.len().saturating_sub(1) as f32 * col_gap
    };
    let has_definite_w = css.width.is_some()
        && !matches!(
            css.width,
            Some(crate::style::Dimension::MinContent) | Some(crate::style::Dimension::MaxContent)
        );
    let final_w = if has_definite_w { content_w } else { w };
    let h: f32 = explicit_rows
        .iter()
        .map(|t| match t {
            crate::style::GridTrack::Fixed(d) => {
                resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0)
            }
            _ => 0.0,
        })
        .sum();
    let gaps = explicit_rows.len().saturating_sub(1) as f32 * row_gap;
    let final_h = if explicit_rows.is_empty() {
        aspect_ratio::height_for_width(css, final_w, 0.0, 0.0).unwrap_or(0.0)
    } else {
        h + gaps
    };
    (final_w, final_h)
}
