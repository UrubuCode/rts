//! CSS Grid Level 1 §11.5: Resolve Intrinsic Track Sizes (Spanning Item Distribution).
//! Shared track distribution algorithm across both column and row axes.

use super::*;
use crate::layout::ResolveCtx;
use crate::style::{Dimension, GridTrack, TrackBound};

pub(in crate::layout) struct SpanningItem {
    pub start: usize,
    pub end: usize,
    pub min_size: f32,
    pub min_content: f32,
    pub max_content: f32,
}

pub(in crate::layout) fn distribute_spanning_tracks(
    tracks: &[GridTrack],
    items: &[SpanningItem],
    ntracks: usize,
    gap: f32,
    container_size: Option<f32>,
    resolve: &ResolveCtx,
    had_single: &[bool],
    max_track: &mut [f32],
    min_track: &mut [f32],
) {
    let dim_val = |d: &Dimension| -> f32 {
        match d {
            Dimension::Percent(pct) => container_size.map_or(0.0, |c| c * pct / 100.0),
            other => other.resolve(resolve).unwrap_or(0.0),
        }
        .max(0.0)
    };

    // Step 1: Base sizes (min-content contributions)
    for item in items {
        let end = item.end.min(ntracks);
        let gutters = (end.saturating_sub(item.start + 1)) as f32 * gap;
        let mut current_base = gutters;
        let mut target_min_tracks = Vec::new();
        let mut has_min_content_track = false;
        for i in item.start..end {
            match tracks.get(i) {
                Some(GridTrack::Fixed(d)) => current_base += dim_val(d),
                Some(GridTrack::Bounded { min, .. }) => current_base += dim_val(min),
                Some(GridTrack::Auto) => {
                    let cur = if had_single.get(i).copied().unwrap_or(false) {
                        max_track[i].max(min_track[i])
                    } else {
                        min_track[i]
                    };
                    current_base += cur;
                    target_min_tracks.push(i);
                }
                Some(GridTrack::Intrinsic { min, .. }) => match min {
                    TrackBound::Fixed(d) => current_base += dim_val(d),
                    TrackBound::MinContent => {
                        current_base += min_track[i];
                        target_min_tracks.push(i);
                        has_min_content_track = true;
                    }
                    TrackBound::Auto => {
                        let cur = if had_single.get(i).copied().unwrap_or(false) {
                            max_track[i].max(min_track[i])
                        } else {
                            min_track[i]
                        };
                        current_base += cur;
                        target_min_tracks.push(i);
                    }
                    TrackBound::MaxContent => {
                        current_base += max_track[i].max(min_track[i]);
                        target_min_tracks.push(i);
                    }
                    TrackBound::FitContent(d) => current_base += dim_val(d),
                },
                _ => { current_base += min_track[i]; }
            }
        }
        let req_base = if has_min_content_track { item.min_content } else { item.min_size };
        let empty_min: Vec<usize> = target_min_tracks
            .iter()
            .copied()
            .filter(|&i| !had_single.get(i).copied().unwrap_or(false))
            .collect();
        let targets = if !empty_min.is_empty() { empty_min } else { target_min_tracks };
        if req_base > current_base && !targets.is_empty() {
            let extra = (req_base - current_base) / targets.len() as f32;
            for &i in &targets {
                min_track[i] += extra;
                max_track[i] = max_track[i].max(min_track[i]);
            }
        }
    }

    // Phase 2: Growth limits
    for item in items {
        let end = item.end.min(ntracks);
        let gutters = (end.saturating_sub(item.start + 1)) as f32 * gap;

        // Step 2: Growth limits for tracks with max function min-content
        let mut current_min_growth = gutters;
        let mut min_growth_tracks = Vec::new();
        for i in item.start..end {
            match tracks.get(i) {
                Some(GridTrack::Fixed(d)) => current_min_growth += dim_val(d),
                Some(GridTrack::Bounded { min, max }) => {
                    let mx = dim_val(max);
                    current_min_growth += if mx > 0.0 { mx } else { dim_val(min) };
                }
                Some(GridTrack::Auto) => {
                    current_min_growth += max_track[i].max(min_track[i]);
                    min_growth_tracks.push(i);
                }
                Some(GridTrack::Intrinsic { max, .. }) => match max {
                    TrackBound::Fixed(d) => current_min_growth += dim_val(d),
                    _ => {
                        let cur = if matches!(max, TrackBound::MinContent) {
                            min_track[i]
                        } else {
                            max_track[i].max(min_track[i])
                        };
                        current_min_growth += cur;
                        min_growth_tracks.push(i);
                    }
                },
                _ => { current_min_growth += max_track[i]; }
            }
        }
        if item.min_content > current_min_growth && !min_growth_tracks.is_empty() {
            let extra = (item.min_content - current_min_growth) / min_growth_tracks.len() as f32;
            for &i in &min_growth_tracks {
                if matches!(tracks.get(i), Some(GridTrack::Intrinsic { max: TrackBound::MinContent, .. })) {
                    min_track[i] += extra;
                    max_track[i] = max_track[i].max(min_track[i]);
                } else {
                    max_track[i] += extra;
                }
            }
        }

        // Step 3: Growth limits for tracks with max function max-content / auto / fit-content
        let has_explicit_max_content = (item.start..end).any(|i| {
            matches!(tracks.get(i), Some(GridTrack::Intrinsic { max: TrackBound::MaxContent, .. }))
        });

        let mut current_max_growth = gutters;
        let mut max_growth_tracks = Vec::new();
        for i in item.start..end {
            match tracks.get(i) {
                Some(GridTrack::Fixed(d)) => current_max_growth += dim_val(d),
                Some(GridTrack::Bounded { min, max }) => {
                    let mx = dim_val(max);
                    current_max_growth += if mx > 0.0 { mx } else { dim_val(min) };
                }
                Some(GridTrack::Auto) => {
                    current_max_growth += max_track[i].max(min_track[i]);
                    if !has_explicit_max_content {
                        max_growth_tracks.push(i);
                    }
                }
                Some(GridTrack::Intrinsic { max, .. }) => match max {
                    TrackBound::Fixed(d) => current_max_growth += dim_val(d),
                    TrackBound::MinContent => current_max_growth += min_track[i],
                    TrackBound::MaxContent => {
                        current_max_growth += max_track[i].max(min_track[i]);
                        max_growth_tracks.push(i);
                    }
                    TrackBound::Auto => {
                        current_max_growth += max_track[i].max(min_track[i]);
                        if !has_explicit_max_content {
                            max_growth_tracks.push(i);
                        }
                    }
                    TrackBound::FitContent(d) => {
                        let cur = max_track[i].max(min_track[i]);
                        let cap = dim_val(d).max(min_track[i]);
                        current_max_growth += cur.min(cap);
                        if !has_explicit_max_content {
                            max_growth_tracks.push(i);
                        }
                    }
                },
                _ => { current_max_growth += max_track[i]; }
            }
        }
        if item.max_content > current_max_growth && !max_growth_tracks.is_empty() {
            let mut remaining = item.max_content - current_max_growth;
            let mut active = max_growth_tracks.clone();
            let empty: Vec<usize> = active
                .iter()
                .copied()
                .filter(|&i| !had_single.get(i).copied().unwrap_or(false))
                .collect();
            if !empty.is_empty() {
                active = empty;
            }
            while !active.is_empty() && remaining > 1e-4 {
                let quota = remaining / active.len() as f32;
                let mut next_active = Vec::new();
                for &i in &active {
                    let cap = match tracks.get(i) {
                        Some(GridTrack::Intrinsic { max: TrackBound::FitContent(d), .. }) => {
                            dim_val(d).max(min_track[i])
                        }
                        _ => f32::INFINITY,
                    };
                    let cur = max_track[i].max(min_track[i]);
                    let can_grow = (cap - cur).max(0.0);
                    let grow = can_grow.min(quota);
                    max_track[i] = cur + grow;
                    remaining -= grow;
                    if can_grow > quota + 1e-4 {
                        next_active.push(i);
                    }
                }
                if next_active.len() == active.len() || remaining <= 1e-4 {
                    if remaining > 1e-4 {
                        let unbounded: Vec<usize> = active
                            .into_iter()
                            .filter(|&i| {
                                !matches!(
                                    tracks.get(i),
                                    Some(GridTrack::Intrinsic { max: TrackBound::FitContent(_), .. })
                                )
                            })
                            .collect();
                        if !unbounded.is_empty() {
                            let each = remaining / unbounded.len() as f32;
                            for &i in &unbounded {
                                max_track[i] += each;
                            }
                        }
                    }
                    break;
                }
                active = next_active;
            }
        }
    }
}

pub(in crate::layout) fn reduce_tracks_overflow(
    sizes: &mut [f32],
    min_sizes: &[f32],
    candidate_tracks: &[usize],
    overflow: f32,
) {
    if candidate_tracks.is_empty() || overflow <= 1e-4 {
        return;
    }
    let (single_tracks, other_tracks): (Vec<usize>, Vec<usize>) = candidate_tracks
        .iter()
        .copied()
        .partition(|&i| min_sizes.get(i).copied().unwrap_or(0.0) > 0.0);

    let single_reducible: f32 = single_tracks
        .iter()
        .map(|&i| (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0))
        .sum();

    if single_reducible >= overflow {
        let factor = overflow / single_reducible;
        for &i in &single_tracks {
            let can = (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0);
            sizes[i] -= can * factor;
        }
    } else if single_reducible > 0.0 {
        for &i in &single_tracks {
            let can = (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0);
            sizes[i] -= can;
        }
        let rem = overflow - single_reducible;
        let other_reducible: f32 = other_tracks
            .iter()
            .map(|&i| (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0))
            .sum();
        if other_reducible > 0.0 {
            let factor = (rem / other_reducible).min(1.0);
            for &i in &other_tracks {
                let can = (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0);
                sizes[i] -= can * factor;
            }
        }
    } else {
        let total_reducible: f32 = other_tracks
            .iter()
            .map(|&i| (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0))
            .sum();
        if total_reducible > 0.0 {
            let factor = (overflow / total_reducible).min(1.0);
            for &i in &other_tracks {
                let can = (sizes[i] - min_sizes.get(i).copied().unwrap_or(0.0)).max(0.0);
                sizes[i] -= can * factor;
            }
        }
    }
}

pub(in crate::layout) fn expand_bounded_tracks(
    sizes: &mut [f32],
    ceilings: &[f32],
    bounded_indices: &[usize],
    free_space: f32,
) -> f32 {
    let mut remaining = free_space;
    if bounded_indices.is_empty() || remaining <= 0.0 {
        return remaining;
    }
    let mut active: Vec<usize> = (0..bounded_indices.len())
        .filter(|&idx| sizes[bounded_indices[idx]] < ceilings[idx])
        .collect();
    while !active.is_empty() && remaining > 1e-4 {
        let quota = remaining / active.len() as f32;
        let mut next_active = Vec::new();
        for &idx in &active {
            let i = bounded_indices[idx];
            let ceiling = ceilings[idx];
            let grow = (ceiling - sizes[i]).min(quota);
            sizes[i] += grow;
            remaining -= grow;
            if sizes[i] < ceiling - 1e-4 {
                next_active.push(idx);
            }
        }
        if next_active.len() == active.len() {
            break;
        }
        active = next_active;
    }
    remaining
}
