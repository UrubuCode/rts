use super::*;
use crate::layout::grid::lines::GridCell;
use crate::layout::measure::aspect_ratio;
use crate::layout::{font_px, ResolveCtx};
use crate::style::{ComputedStyle, GridTrack, TrackBound};

pub(in crate::layout) struct ItemSizes {
    pub min_size: f32,
    pub min_content: f32,
    pub max_content: f32,
}

pub(in crate::layout) fn grid_item_min_content_width(
    dom: &Dom,
    child: NodeIdx,
    box_id: Option<crate::boxes::BoxId>,
    area_h: Option<f32>,
    grid_css: Option<&ComputedStyle>,
    font_size: f32,
    resolve: &ResolveCtx,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(child) else { return 0.0 };
    if let (Some(g_css), Some(bid)) = (grid_css, box_id) {
        if aspect::is_item_orthogonal(dom, child, g_css) {
            return crate::layout::measure_block(dom, child, bid, f32::INFINITY, area_h, None, None, true, ctx).0;
        }
    }
    let [bt, br, bb, bl] = crate::style::borders::used_widths(&css);
    let frame_h = css.padding.resolve_h(resolve) + bl + br + css.margin.resolve_h(resolve);
    let frame_v = css.padding.resolve_v(resolve) + bt + bb;
    let f = font_px(&css, font_size);
    let min_floor = crate::layout::measure::intrinsic_min_max::resolve(css.min_width, dom, child, f, ctx, resolve)
        .map(|w| w + frame_h)
        .unwrap_or(0.0);
    use crate::scrollbar::Overflow::{Clip, Visible};
    let axis_visible = |o: Option<crate::scrollbar::Overflow>| matches!(o.unwrap_or(Visible), Visible | Clip);
    if !axis_visible(css.overflow_x) || !axis_visible(css.overflow_y) {
        return min_floor.max(frame_h);
    }
    if let Some(w) = css.width.and_then(|d| crate::style::dimensao_absoluta(d, resolve)) {
        return (w + frame_h).max(min_floor);
    }
    let is_rep = matches!(&dom.node(child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
    let has_ratio = is_rep || aspect_ratio::declared_ratio(&css).is_some();
    if has_ratio {
        if let Some(h) = css.height.and_then(|d| resolve_height(Some(d), area_h, resolve)).or(area_h) {
            let content_h = (h - frame_v).max(0.0);
            if let Some((w, _)) = crate::inline_box::replaced_inline_size(
                dom, child, &css, ctx.viewport_w, Some(content_h), (None, Some(content_h)), ctx,
            ) {
                return (w + css.margin.resolve_h(resolve)).max(min_floor);
            }
            if let Some(w) = aspect_ratio::width_for_height(&css, content_h, frame_h, frame_v) {
                return (w + frame_h).max(min_floor);
            }
        }
        return min_floor.max(frame_h);
    }
    let mut content = crate::table::min_content(dom, child, f, ctx);
    if css.min_width.is_none() {
        if let Some(mx) = crate::layout::measure::intrinsic_min_max::resolve(css.max_width, dom, child, f, ctx, resolve) {
            content = content.min(mx + frame_h);
        }
    }
    content.max(min_floor)
}

pub(in crate::layout) fn grid_item_min_size_width(
    dom: &Dom,
    child: NodeIdx,
    box_id: Option<crate::boxes::BoxId>,
    area_h: Option<f32>,
    grid_css: Option<&ComputedStyle>,
    font_size: f32,
    resolve: &ResolveCtx,
    ctx: &LayoutCtx,
) -> f32 {
    let Some(css) = dom.computed_style_idx(child) else { return 0.0 };
    let [_, br, _, bl] = crate::style::borders::used_widths(&css);
    let frame_h = css.padding.resolve_h(resolve) + bl + br + css.margin.resolve_h(resolve);
    let f = font_px(&css, font_size);
    if let Some(w) = crate::layout::measure::intrinsic_min_max::resolve(css.min_width, dom, child, f, ctx, resolve) {
        w + frame_h
    } else {
        grid_item_min_content_width(dom, child, box_id, area_h, grid_css, font_size, resolve, ctx)
    }
}

pub(in crate::layout) fn distribute_spanning_columns<F, S>(
    col_tracks: &[GridTrack],
    cells: &[GridCell],
    ncols: usize,
    col_gap: f32,
    resolve: &ResolveCtx,
    is_sub: S,
    mut get_sizes: F,
    wmax: &mut [f32],
    wmin: &mut [f32],
) where
    F: FnMut(&GridCell) -> ItemSizes,
    S: Fn(&GridCell) -> bool,
{
    let mut spanning: Vec<&GridCell> = cells
        .iter()
        .filter(|c| c.c1 > c.c0 && c.c0 < ncols && !is_sub(c) && (c.c1 - c.c0) > 1)
        .collect();
    spanning.sort_by_key(|c| c.c1 - c.c0);

    for c in spanning {
        let end = c.c1.min(ncols);
        let gutters = (end.saturating_sub(c.c0 + 1)) as f32 * col_gap;
        let item = get_sizes(c);

        // Step 1: Base sizes (min track sizing functions)
        let mut current_base = gutters;
        let mut target_min_cols = Vec::new();
        let mut has_min_content_track = false;
        for col in c.c0..end {
            match col_tracks.get(col) {
                Some(GridTrack::Fixed(d)) => current_base += d.resolve(resolve).unwrap_or(0.0),
                Some(GridTrack::Bounded { min, .. }) => current_base += min.resolve(resolve).unwrap_or(0.0),
                Some(GridTrack::Auto) => {
                    current_base += wmin[col];
                    target_min_cols.push(col);
                }
                Some(GridTrack::Intrinsic { min, .. }) => {
                    match min {
                        TrackBound::Fixed(d) => current_base += d.resolve(resolve).unwrap_or(0.0),
                        TrackBound::MinContent => {
                            current_base += wmin[col];
                            target_min_cols.push(col);
                            has_min_content_track = true;
                        }
                        TrackBound::Auto | TrackBound::MaxContent => {
                            current_base += wmin[col];
                            target_min_cols.push(col);
                        }
                        TrackBound::FitContent(d) => current_base += d.resolve(resolve).unwrap_or(0.0),
                    }
                }
                _ => { current_base += wmin[col]; }
            }
        }
        let req_base = if has_min_content_track { item.min_content } else { item.min_size };
        if req_base > current_base && !target_min_cols.is_empty() {
            let extra = (req_base - current_base) / target_min_cols.len() as f32;
            for &col in &target_min_cols {
                wmin[col] += extra;
                wmax[col] = wmax[col].max(wmin[col]);
            }
        }

        // Step 2: Growth limits for tracks with max function min-content
        let mut current_min_growth = gutters;
        let mut min_growth_cols = Vec::new();
        for col in c.c0..end {
            match col_tracks.get(col) {
                Some(GridTrack::Fixed(d)) => current_min_growth += d.resolve(resolve).unwrap_or(0.0),
                Some(GridTrack::Bounded { min, max }) => {
                    current_min_growth += max.resolve(resolve).or_else(|| min.resolve(resolve)).unwrap_or(0.0);
                }
                Some(GridTrack::Auto) => {
                    current_min_growth += wmax[col].max(wmin[col]);
                    min_growth_cols.push(col);
                }
                Some(GridTrack::Intrinsic { max, .. }) => {
                    match max {
                        TrackBound::Fixed(d) => current_min_growth += d.resolve(resolve).unwrap_or(0.0),
                        _ => {
                            let cur = if matches!(max, TrackBound::MinContent) {
                                wmin[col]
                            } else {
                                wmax[col].max(wmin[col])
                            };
                            current_min_growth += cur;
                            min_growth_cols.push(col);
                        }
                    }
                }
                _ => { current_min_growth += wmax[col]; }
            }
        }
        if item.min_content > current_min_growth && !min_growth_cols.is_empty() {
            let extra = (item.min_content - current_min_growth) / min_growth_cols.len() as f32;
            for &col in &min_growth_cols {
                if matches!(col_tracks.get(col), Some(GridTrack::Intrinsic { max: TrackBound::MinContent, .. })) {
                    wmin[col] += extra;
                    wmax[col] = wmax[col].max(wmin[col]);
                } else {
                    wmax[col] += extra;
                }
            }
        }

        // Step 3: Growth limits for tracks with max function max-content / auto / fit-content
        let mut current_max_growth = gutters;
        let mut max_growth_cols = Vec::new();
        for col in c.c0..end {
            match col_tracks.get(col) {
                Some(GridTrack::Fixed(d)) => current_max_growth += d.resolve(resolve).unwrap_or(0.0),
                Some(GridTrack::Bounded { min, max }) => {
                    current_max_growth += max.resolve(resolve).or_else(|| min.resolve(resolve)).unwrap_or(0.0);
                }
                Some(GridTrack::Auto) => {
                    current_max_growth += wmax[col].max(wmin[col]);
                    max_growth_cols.push(col);
                }
                Some(GridTrack::Intrinsic { max, .. }) => {
                    match max {
                        TrackBound::Fixed(d) => current_max_growth += d.resolve(resolve).unwrap_or(0.0),
                        TrackBound::MinContent => {
                            current_max_growth += wmin[col];
                        }
                        TrackBound::MaxContent | TrackBound::Auto | TrackBound::FitContent(_) => {
                            current_max_growth += wmax[col].max(wmin[col]);
                            max_growth_cols.push(col);
                        }
                    }
                }
                _ => { current_max_growth += wmax[col]; }
            }
        }
        if item.max_content > current_max_growth && !max_growth_cols.is_empty() {
            let extra = (item.max_content - current_max_growth) / max_growth_cols.len() as f32;
            for col in max_growth_cols {
                wmax[col] = (wmax[col].max(wmin[col])) + extra;
            }
        }
    }
}

pub(in crate::layout) fn distribute_spanning_slice<F, S>(
    col_tracks: &[GridTrack],
    cells: &[GridCell],
    ncols: usize,
    col_gap: f32,
    resolve: &ResolveCtx,
    is_sub: S,
    mut get_width: F,
    slice: &mut [f32],
) where
    F: FnMut(&GridCell) -> f32,
    S: Fn(&GridCell) -> bool,
{
    for c in cells {
        if c.c1 <= c.c0 || c.c0 >= ncols || is_sub(c) {
            continue;
        }
        let span = c.c1 - c.c0;
        if span > 1 {
            let end = c.c1.min(ncols);
            let mut current_span_w = (end.saturating_sub(c.c0 + 1)) as f32 * col_gap;
            let mut content_cols = Vec::new();
            for col in c.c0..end {
                match col_tracks.get(col) {
                    Some(GridTrack::Fixed(d)) => current_span_w += d.resolve(resolve).unwrap_or(0.0),
                    _ => { current_span_w += slice[col]; content_cols.push(col); }
                }
            }
            let item_w = get_width(c);
            if item_w > current_span_w {
                let targets = if !content_cols.is_empty() { content_cols } else { (c.c0..end).collect() };
                let extra = (item_w - current_span_w) / targets.len() as f32;
                for col in targets { slice[col] += extra; }
            }
        }
    }
}
