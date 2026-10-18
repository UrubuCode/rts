//! Explicit track initialization for grid containers (CSS Grid 1 §7.3).

use super::subgrid::SubgriddedAxis;
use super::subgrid_names::subgrid_span;
use crate::layout::ResolveCtx;
use crate::style::{ComputedStyle, DisplayKind, GridAreas, GridTemplate, GridTrack, LineNames, TrackList};

pub(in crate::layout) fn init_col_tracks(
    col_sub: Option<&SubgriddedAxis>,
    css: &ComputedStyle,
    areas: Option<&GridAreas>,
    content_w: f32,
    col_gap: f32,
    container_content_h: Option<f32>,
    resolve: &ResolveCtx,
    is_p_grid: bool,
    is_ortho: bool,
) -> (Vec<GridTrack>, Vec<bool>, Vec<LineNames>) {
    if let Some(cs) = col_sub {
        (
            vec![GridTrack::Auto; cs.track_sizes.len()],
            vec![false; cs.track_sizes.len()],
            cs.line_names.clone(),
        )
    } else {
        let has_def = css.width.is_some()
            && !matches!(
                css.width,
                Some(crate::style::Dimension::MinContent) | Some(crate::style::Dimension::MaxContent)
            );
        let is_inl = matches!(css.effective_display(), Some(DisplayKind::InlineGrid));
        let min_s = super::aspect::container_min_size_for_auto_repeat(css, true, container_content_h, resolve);
        let c_w = if (has_def || (!is_inl && content_w > 0.0)) && min_s.map_or(true, |ms| content_w >= ms) {
            content_w
        } else {
            0.0
        };
        let is_sub_disabled = css.disables_subgrid == Some(true)
            || css.position.map_or(false, |p| p.out_of_flow());
        let (mut tracks, mut collapsible, mut line_names) = match css.grid_template_columns.as_deref() {
            Some(GridTemplate::Subgrid(_)) if is_p_grid && !is_sub_disabled => {
                let s = if is_ortho {
                    subgrid_span(css.grid_row_start.as_ref(), css.grid_row_end.as_ref())
                } else {
                    subgrid_span(css.grid_column_start.as_ref(), css.grid_column_end.as_ref())
                };
                (vec![GridTrack::Auto; s], vec![false; s], vec![Vec::new(); s + 1])
            }
            Some(GridTemplate::Tracks(t)) => super::tracks::expand_auto_repeats(t, c_w, col_gap, min_s),
            _ => match css.grid_columns.map(|c| c.max(1) as usize) {
                Some(cols) => {
                    let tl = TrackList::from_tracks(vec![GridTrack::Fr(1.0); cols]);
                    super::tracks::expand_auto_repeats(&tl, c_w, col_gap, min_s)
                }
                None => (Vec::new(), Vec::new(), Vec::new()),
            },
        };
        if let Some(a) = areas {
            let start_len = tracks.len();
            while tracks.len() < a.cols {
                let idx = tracks.len().saturating_sub(start_len);
                let tr = css
                    .grid_auto_columns
                    .as_ref()
                    .map_or(GridTrack::Auto, |t| t.track_at(idx).clone());
                tracks.push(tr);
                collapsible.push(false);
            }
            while line_names.len() <= tracks.len() {
                line_names.push(Vec::new());
            }
        }
        (tracks, collapsible, line_names)
    }
}

pub(in crate::layout) fn init_row_tracks(
    row_sub: Option<&SubgriddedAxis>,
    css: &ComputedStyle,
    areas: Option<&GridAreas>,
    container_content_h: Option<f32>,
    row_gap: f32,
    resolve: &ResolveCtx,
    is_p_grid: bool,
    is_ortho: bool,
) -> (Vec<GridTrack>, Vec<bool>, Vec<LineNames>) {
    if let Some(rs) = row_sub {
        (
            vec![GridTrack::Auto; rs.track_sizes.len()],
            vec![false; rs.track_sizes.len()],
            rs.line_names.clone(),
        )
    } else {
        let min_s = super::aspect::container_min_size_for_auto_repeat(css, false, container_content_h, resolve);
        let is_sub_disabled = css.disables_subgrid == Some(true)
            || css.position.map_or(false, |p| p.out_of_flow());
        let (mut tracks, mut collapsible, mut line_names) = match css.grid_template_rows.as_deref() {
            Some(GridTemplate::Subgrid(_)) if is_p_grid && !is_sub_disabled => {
                let s = if is_ortho {
                    subgrid_span(css.grid_column_start.as_ref(), css.grid_column_end.as_ref())
                } else {
                    subgrid_span(css.grid_row_start.as_ref(), css.grid_row_end.as_ref())
                };
                (vec![GridTrack::Auto; s], vec![false; s], vec![Vec::new(); s + 1])
            }
            Some(GridTemplate::Tracks(t)) => {
                super::tracks::expand_auto_repeats(t, container_content_h.unwrap_or(0.0), row_gap, min_s)
            }
            _ => (Vec::new(), Vec::new(), Vec::new()),
        };
        if let Some(a) = areas {
            let start_len = tracks.len();
            while tracks.len() < a.rows {
                let idx = tracks.len().saturating_sub(start_len);
                let tr = css
                    .grid_auto_rows
                    .as_ref()
                    .map_or(GridTrack::Auto, |t| t.track_at(idx).clone());
                tracks.push(tr);
                collapsible.push(false);
            }
            while line_names.len() <= tracks.len() {
                line_names.push(Vec::new());
            }
        }
        (tracks, collapsible, line_names)
    }
}
