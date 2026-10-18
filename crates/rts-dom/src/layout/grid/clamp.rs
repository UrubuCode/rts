//! CSS Grid Level 1 §6.6: Automatic Minimum Size of Grid Items.
//!
//! "Specifically, if the grid item spans only grid tracks that each have a fixed
//! max track sizing function, its automatic minimum size in that dimension is
//! further clamped to less than or equal to the size necessary to fit its margin
//! box within the resulting grid area, calculated using the fixed max track
//! sizing function for each track."

use super::*;
use crate::style::{Dimension, GridTrack, ResolveCtx, TrackBound};

/// Returns the fixed maximum size of a track if its max track sizing function is fixed.
pub(crate) fn fixed_max_track_size(
    track: &GridTrack,
    container_size: Option<f32>,
    resolve: &ResolveCtx,
) -> Option<f32> {
    let dim_px = |d: &Dimension| -> Option<f32> {
        match d {
            Dimension::Percent(pct) => container_size.map(|c| (c * pct / 100.0).max(0.0)),
            other => other.resolve(resolve).map(|p| p.max(0.0)),
        }
    };
    match track {
        GridTrack::Fixed(d) => dim_px(d),
        GridTrack::Bounded { max, .. } => dim_px(max),
        GridTrack::Intrinsic {
            max: TrackBound::Fixed(d),
            ..
        } => dim_px(d),
        _ => None,
    }
}

/// Returns the clamped ceiling for the automatic minimum size of an item
/// spanning `start..end` if all spanned tracks have fixed max track sizing functions.
pub(crate) fn clamped_automatic_minimum_size(
    tracks: &[GridTrack],
    start: usize,
    end: usize,
    gap: f32,
    container_size: Option<f32>,
    resolve: &ResolveCtx,
) -> Option<f32> {
    if start >= end || end > tracks.len() {
        return None;
    }
    let mut total = 0.0f32;
    for (idx, t) in tracks[start..end].iter().enumerate() {
        if !matches!(t, GridTrack::Auto | GridTrack::Intrinsic { min: TrackBound::Auto, .. }) {
            return None;
        }
        let m = fixed_max_track_size(t, container_size, resolve)?;
        total += m;
        if idx > 0 {
            total += gap;
        }
    }
    Some(total.max(0.0))
}

/// Clamps the automatic minimum width of an item per CSS Grid §6.6.
pub(crate) fn clamp_item_min_w(
    mut min: f32,
    dom: &Dom,
    child: NodeIdx,
    tracks: &[GridTrack],
    c0: usize,
    c1: usize,
    gap: f32,
    content_w: Option<f32>,
    resolve: &ResolveCtx,
) -> f32 {
    let item_css = dom.computed_style_idx(child);
    let is_auto_min = item_css
        .as_ref()
        .map_or(true, |st| st.min_width.is_none() || matches!(st.min_width, Some(Dimension::Auto)));
    if is_auto_min {
        if let Some(ceiling) = clamped_automatic_minimum_size(tracks, c0, c1, gap, content_w, resolve) {
            let [_, br, _, bl] = item_css.as_deref().map(crate::style::borders::used_widths).unwrap_or([0.0; 4]);
            let frame_h = item_css.as_ref().map(|c| c.padding.resolve_h(resolve) + bl + br + c.margin.resolve_h(resolve)).unwrap_or(0.0);
            min = min.min(ceiling.max(frame_h));
        }
    }
    min
}

/// Clamps the automatic minimum height of an item per CSS Grid §6.6.
pub(crate) fn clamp_item_min_h(
    mut min: f32,
    dom: &Dom,
    child: NodeIdx,
    tracks: &[GridTrack],
    r0: usize,
    r1: usize,
    gap: f32,
    container_h: Option<f32>,
    resolve: &ResolveCtx,
) -> f32 {
    let item_css = dom.computed_style_idx(child);
    let is_auto_min = item_css
        .as_ref()
        .map_or(true, |st| st.min_height.is_none() || matches!(st.min_height, Some(Dimension::Auto)));
    if is_auto_min {
        if let Some(ceiling) = clamped_automatic_minimum_size(tracks, r0, r1, gap, container_h, resolve) {
            let [bt, _, bb, _] = item_css.as_deref().map(crate::style::borders::used_widths).unwrap_or([0.0; 4]);
            let frame_v = item_css.as_ref().map(|c| c.padding.resolve_v(resolve) + bt + bb + c.margin.resolve_v(resolve)).unwrap_or(0.0);
            min = min.min(ceiling.max(frame_v));
        }
    }
    min
}

/// Clamps the automatic minimum size of a grid item during item sizing.
pub(crate) fn clamp_auto_size(
    mut auto: f32,
    sz: Option<Dimension>,
    mx: Option<Dimension>,
    track_ceil: Option<f32>,
) -> f32 {
    let def_px = |d| match d { Some(Dimension::Px(p)) => Some(p), _ => None };
    if let Some(p) = def_px(sz) { auto = auto.min(p); }
    if let Some(m) = def_px(mx) { auto = auto.min(m); }
    if let Some(c) = track_ceil { auto = auto.min(c); }
    auto
}


