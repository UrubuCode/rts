//! Expansion of `repeat(auto-fill|auto-fit, ...)` track patterns (CSS Grid 1 §7.2.3.2/3).

use crate::style::{Dimension, GridTrack, LineNames, TrackBound, TrackList};

pub(crate) fn expand_auto_repeats(
    track_list: &TrackList,
    container: f32,
    gap: f32,
    min_size: Option<f32>,
) -> (Vec<GridTrack>, Vec<bool>, Vec<LineNames>) {
    use GridTrack as T;
    let track_base = |t: &T| match t {
        T::Fixed(Dimension::Px(p)) | T::Bounded { min: Dimension::Px(p), .. } => *p,
        T::Fixed(Dimension::Percent(pct)) | T::Bounded { min: Dimension::Percent(pct), .. } if container > 0.0 => pct * container / 100.0,
        T::Flex { min: TrackBound::Fixed(Dimension::Px(p)), .. } => *p,
        T::Flex { min: TrackBound::Fixed(Dimension::Percent(pct)), .. } if container > 0.0 => pct * container / 100.0,
        _ => 0.0,
    };
    let (other_base, other_count) = track_list
        .entries
        .iter()
        .filter(|(_, t)| !matches!(t, T::AutoRepeat { .. }))
        .fold((0.0f32, 0usize), |(b, c), (_, t)| (b + track_base(t), c + 1));

    let mut out_tracks = Vec::with_capacity(track_list.entries.len());
    let mut collapsible = Vec::with_capacity(track_list.entries.len());
    let mut out_lines = Vec::with_capacity(track_list.entries.len() + 1);

    let mut current_line: LineNames = Vec::new();

    for (entry_lines, track) in &track_list.entries {
        current_line.extend(entry_lines.iter().cloned());
        match track {
            T::AutoRepeat {
                tracks: pattern,
                fit,
                count_unit,
            } => {
                if pattern.entries.is_empty() {
                    continue;
                }
                let m = pattern.entries.len();
                let total_size = |n: usize| -> f32 {
                    let total_tracks = other_count + n * m;
                    let total_gaps = total_tracks.saturating_sub(1) as f32 * gap;
                    other_base + (n as f32) * count_unit + total_gaps
                };
                let n = if *count_unit <= 0.0 || m == 0 {
                    1
                } else if container > 0.0 {
                    let mut best = 1;
                    while total_size(best + 1) <= container {
                        best += 1;
                    }
                    best
                } else if let Some(ms) = min_size.filter(|&s| s > 0.0) {
                    let mut best = 1;
                    while total_size(best) < ms {
                        best += 1;
                    }
                    best
                } else {
                    1
                };
                for _ in 0..n {
                    for (p_lines, p_track) in &pattern.entries {
                        current_line.extend(p_lines.iter().cloned());
                        out_lines.push(std::mem::take(&mut current_line));
                        out_tracks.push(p_track.clone());
                        collapsible.push(*fit);
                    }
                    current_line.extend(pattern.trailing.iter().cloned());
                }
            }
            other => {
                out_lines.push(std::mem::take(&mut current_line));
                out_tracks.push(other.clone());
                collapsible.push(false);
            }
        }
    }
    current_line.extend(track_list.trailing.iter().cloned());
    out_lines.push(current_line);

    (out_tracks, collapsible, out_lines)
}
