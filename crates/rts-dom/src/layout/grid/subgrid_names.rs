//! Name list expansion and area line clamping for subgrids.

use crate::style::SubgridLine;
use crate::style::LineNames;

pub(in crate::layout) fn expand_subgrid_line_names(
    subgrid_lines: &[SubgridLine],
    track_count: usize,
    parent_lines: &[LineNames],
    reverse_parent: bool,
) -> Vec<LineNames> {
    let num_lines = track_count + 1;
    let mut out: Vec<LineNames> = vec![Vec::new(); num_lines];

    for (i, p_names) in parent_lines.iter().enumerate() {
        if i < num_lines {
            let out_idx = if reverse_parent { num_lines - 1 - i } else { i };
            out[out_idx].extend(p_names.iter().cloned());
        }
    }

    if subgrid_lines.is_empty() {
        return out;
    }

    let mut prefix: Vec<LineNames> = Vec::new();
    let mut auto_fill: Option<&Vec<LineNames>> = None;
    let mut suffix: Vec<LineNames> = Vec::new();

    for sl in subgrid_lines {
        match sl {
            SubgridLine::Line(names) => {
                if auto_fill.is_none() {
                    prefix.push(names.clone());
                } else {
                    suffix.push(names.clone());
                }
            }
            SubgridLine::AutoFill(pat) => {
                if auto_fill.is_none() {
                    auto_fill = Some(pat);
                }
            }
        }
    }

    let mut expanded: Vec<LineNames> = Vec::with_capacity(num_lines);
    expanded.extend(prefix);

    if let Some(pat) = auto_fill {
        let remaining_lines = num_lines.saturating_sub(expanded.len() + suffix.len());
        if !pat.is_empty() && remaining_lines > 0 {
            let reps = remaining_lines / pat.len();
            for _ in 0..reps {
                for p in pat {
                    expanded.push(p.clone());
                }
            }
        }
    }
    expanded.extend(suffix);

    for (i, names) in expanded.into_iter().enumerate() {
        if i < num_lines {
            let target_idx = i;
            for name in names {
                if !out[target_idx].contains(&name) {
                    out[target_idx].push(name);
                }
            }
        }
    }

    out
}

pub(in crate::layout) fn clamp_area_lines(
    areas: Option<&crate::style::GridAreas>,
    start_track: usize,
    end_track: usize,
    num_tracks: usize,
    is_col: bool,
    lines: &mut [LineNames],
    reverse_parent: bool,
) {
    let Some(areas) = areas else { return };
    for (name, area) in areas.iter() {
        let (a0, a1) = if is_col {
            (area.c0, area.c1)
        } else {
            (area.r0, area.r1)
        };
        if a0 < end_track && a1 > start_track {
            let mut s_idx = if a0 <= start_track {
                0
            } else {
                a0 - start_track
            };
            let mut e_idx = if a1 >= end_track {
                num_tracks
            } else {
                a1 - start_track
            };
            if reverse_parent {
                let orig_s = s_idx;
                s_idx = num_tracks.saturating_sub(e_idx);
                e_idx = num_tracks.saturating_sub(orig_s);
            }
            let s_name = format!("{}-start", name);
            let e_name = format!("{}-end", name);
            if s_idx < lines.len() && !lines[s_idx].contains(&s_name) {
                lines[s_idx].push(s_name);
            }
            if e_idx < lines.len() && !lines[e_idx].contains(&e_name) {
                lines[e_idx].push(e_name);
            }
        }
    }
}

pub(in crate::layout) fn subgrid_span(
    s0: Option<&crate::style::grid_lines::GridLine>,
    s1: Option<&crate::style::grid_lines::GridLine>,
) -> usize {
    use crate::style::grid_lines::GridLine::Span;
    match (s0, s1) {
        (Some(Span(s)), _) | (_, Some(Span(s))) => (*s as usize).max(1),
        _ => 1,
    }
}

pub(in crate::layout) fn is_parent_grid(dom: &crate::Dom, id: crate::dom::NodeIdx) -> bool {
    let Some(parent) = dom.node(id).parent else {
        return false;
    };
    let Some(p_css) = dom.computed_style_idx(parent) else {
        return false;
    };
    matches!(
        p_css.effective_display(),
        Some(crate::style::DisplayKind::Grid) | Some(crate::style::DisplayKind::InlineGrid)
    )
}
