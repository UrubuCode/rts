//! Subgrid (CSS Grid Level 2 §9) — tracks, lines, and gap borrowed from parent grid.

pub(in crate::layout) use super::subgrid_contrib::{collect_column_contributions, collect_row_contributions};
use crate::Dom;
use crate::boxes::BoxId;
use crate::dom::NodeIdx;
use crate::layout::ResolveCtx;
use crate::layout::grid::lines::GridCell;
use crate::style::{ComputedStyle, DisplayKind, GridTemplate, LineNames};

#[derive(Clone, Debug)]
pub(in crate::layout) struct SubgriddedAxis {
    pub track_sizes: Vec<f32>,
    pub track_starts: Vec<f32>,
    pub track_ends: Vec<f32>,
    pub line_pos: Vec<f32>,
    pub line_names: Vec<LineNames>,
    pub gap: f32,
    pub start_mbp: f32,
    pub end_mbp: f32,
    pub reversed: bool,
}

impl SubgriddedAxis {
    pub(in crate::layout) fn cell_pos_and_span(&self, c0: usize, c1: usize) -> (f32, f32) {
        if self.track_starts.is_empty() || self.track_ends.is_empty() {
            return (0.0, 0.0);
        }
        let i0 = c0.min(self.track_starts.len().saturating_sub(1));
        let i1 = c1.min(self.track_ends.len()).max(i0 + 1);
        let (left, right) = if self.reversed {
            (self.track_starts[i1 - 1], self.track_ends[i0])
        } else {
            (self.track_starts[i0], self.track_ends[i1 - 1])
        };
        (left, (right - left).max(0.0))
    }

    pub(in crate::layout) fn cell_span(&self, c0: usize, c1: usize) -> f32 {
        self.cell_pos_and_span(c0, c1).1
    }

    pub(in crate::layout) fn content_span(&self) -> f32 {
        if self.reversed {
            self.line_pos.first().copied().unwrap_or(0.0)
        } else {
            self.line_pos.last().copied().unwrap_or(0.0)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::layout) struct SubgridContext {
    pub cols: Option<SubgriddedAxis>,
    pub rows: Option<SubgriddedAxis>,
}

thread_local! {
    static ACTIVE_SUBGRIDS: std::cell::RefCell<crate::fasthash::FastMap<BoxId, SubgridContext>> =
        std::cell::RefCell::new(crate::fasthash::FastMap::default());
}

pub(in crate::layout) fn register_subgrid(box_id: BoxId, ctx: SubgridContext) {
    ACTIVE_SUBGRIDS.with(|m| {
        m.borrow_mut().insert(box_id, ctx);
    });
}

pub(in crate::layout) fn take_subgrid(box_id: BoxId) -> Option<SubgridContext> {
    ACTIVE_SUBGRIDS.with(|m| m.borrow_mut().remove(&box_id))
}

pub(in crate::layout) fn subgrid_mbp(
    css: &ComputedStyle,
    resolve: &ResolveCtx,
) -> (f32, f32, f32, f32) {
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let ml = css.margin.left.resolve(resolve).unwrap_or(0.0);
    let mr = css.margin.right.resolve(resolve).unwrap_or(0.0);
    let mt = css.margin.top.resolve(resolve).unwrap_or(0.0);
    let mb = css.margin.bottom.resolve(resolve).unwrap_or(0.0);
    let pl = css.padding.left.resolve(resolve).unwrap_or(0.0).max(0.0);
    let pr = css.padding.right.resolve(resolve).unwrap_or(0.0).max(0.0);
    let pt = css.padding.top.resolve(resolve).unwrap_or(0.0).max(0.0);
    let pb = css.padding.bottom.resolve(resolve).unwrap_or(0.0).max(0.0);

    (ml + bl + pl, mr + br + pr, mt + bt + pt, mb + bb + pb)
}

pub(in crate::layout) use super::subgrid_names::{clamp_area_lines, expand_subgrid_line_names, is_parent_grid, subgrid_span};

fn build_axis(
    subgrid_template: Option<&GridTemplate>,
    subgrid_gap_prop: Option<crate::style::Dimension>,
    p_tracks: &[f32],
    p_range: (usize, usize),
    p_gap: f32,
    p_lines: &[LineNames],
    p_areas: Option<&crate::style::GridAreas>,
    p_is_col: bool,
    smbp: f32,
    embp: f32,
    resolve: &ResolveCtx,
    reverse_parent: bool,
    is_rtl: bool,
) -> Option<SubgriddedAxis> {
    let subgrid_lines = match subgrid_template {
        Some(GridTemplate::Subgrid(l)) => l.as_slice(),
        _ => return None,
    };
    if p_tracks.is_empty() {
        return None;
    }
    let s0 = p_range.0.min(p_tracks.len());
    let s1 = p_range.1.min(p_tracks.len()).max(s0 + 1);
    let n = s1.saturating_sub(s0);
    if n == 0 {
        return None;
    }
    let p_slice = &p_tracks[s0..s1];
    let gap = subgrid_gap_prop
        .and_then(|d| match d {
            crate::style::Dimension::Percent(p) => {
                let span_size = p_slice.iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * p_gap;
                let content_span = (span_size - smbp - embp).max(0.0);
                Some(content_span * p / 100.0)
            }
            other => other.resolve(resolve),
        })
        .unwrap_or(p_gap)
        .max(0.0);
    let half = (gap - p_gap) / 2.0;

    let parent_rtl = reverse_parent ^ is_rtl;
    let phys_slice: Vec<f32> = if parent_rtl {
        p_slice.iter().rev().copied().collect()
    } else {
        p_slice.to_vec()
    };
    let mut phys_starts = Vec::with_capacity(n);
    let mut phys_ends = Vec::with_capacity(n);
    let mut cur = 0.0f32;
    for &pt in &phys_slice {
        cur += pt;
        phys_ends.push(cur);
        cur += p_gap;
    }
    let mut cur_s = 0.0f32;
    for &pt in &phys_slice {
        phys_starts.push(cur_s);
        cur_s += pt + p_gap;
    }

    let mut track_starts = Vec::with_capacity(n);
    let mut track_ends = Vec::with_capacity(n);
    let mut track_sizes = Vec::with_capacity(n);

    for k in 0..n {
        let p_idx = if is_rtl { n - 1 - k } else { k };
        let s = if p_idx == 0 {
            0.0
        } else {
            phys_starts[p_idx] - smbp + half
        };
        let e = if p_idx == n - 1 {
            (phys_ends[p_idx] - smbp - embp).max(s)
        } else {
            (phys_ends[p_idx] - smbp - half).max(s)
        };
        track_starts.push(s);
        track_ends.push(e);
        track_sizes.push((e - s).max(0.0));
    }

    let span_size: f32 = phys_slice.iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * p_gap;
    let content_span = (span_size - smbp - embp).max(0.0);
    let mut line_pos = Vec::with_capacity(n + 1);
    if is_rtl {
        line_pos.push(content_span);
        let mut cur_from_end = 0.0f32;
        for k in 1..n {
            cur_from_end += phys_slice[n - k] + p_gap;
            line_pos.push(span_size - cur_from_end - smbp);
        }
        line_pos.push(0.0);
    } else {
        line_pos.push(0.0);
        let mut cur = 0.0f32;
        for k in 0..n.saturating_sub(1) {
            cur += phys_slice[k];
            line_pos.push(cur - smbp);
            cur += p_gap;
        }
        line_pos.push(content_span);
    }

    let p_end = s1.min(p_lines.len().saturating_sub(1));
    let parent_slice = if s0 <= p_end && s0 < p_lines.len() {
        &p_lines[s0..=p_end]
    } else {
        &[]
    };
    let mut line_names = expand_subgrid_line_names(subgrid_lines, n, parent_slice, reverse_parent);
    clamp_area_lines(
        p_areas,
        s0,
        s1,
        n,
        p_is_col,
        &mut line_names,
        reverse_parent,
    );
    Some(SubgriddedAxis {
        track_sizes,
        track_starts,
        track_ends,
        line_pos,
        line_names,
        gap,
        start_mbp: smbp,
        end_mbp: embp,
        reversed: is_rtl,
    })
}

pub(in crate::layout) fn check_and_build(
    dom: &Dom,
    child: NodeIdx,
    cell: &GridCell,
    col_sizes: &[f32],
    row_sizes: &[f32],
    col_gap: f32,
    row_gap: f32,
    col_lines: &[LineNames],
    row_lines: &[LineNames],
    areas: Option<&crate::style::GridAreas>,
    resolve: &ResolveCtx,
) -> Option<SubgridContext> {
    let child_css = dom.computed_style_idx(child)?;
    if !child_css
        .effective_display()
        .is_some_and(DisplayKind::is_grid_container)
    {
        return None;
    }
    if child_css.disables_subgrid == Some(true) {
        return None;
    }
    let is_col_subgrid = matches!(
        child_css.grid_template_columns.as_deref(),
        Some(GridTemplate::Subgrid(_))
    );
    let is_row_subgrid = matches!(
        child_css.grid_template_rows.as_deref(),
        Some(GridTemplate::Subgrid(_))
    );
    if !is_col_subgrid && !is_row_subgrid {
        return None;
    }

    let parent_node = dom.node(child).parent;
    let parent_css = parent_node.and_then(|p| dom.computed_style_idx(p));
    let parent_wm = parent_css
        .as_ref()
        .and_then(|c| c.writing_mode)
        .unwrap_or_default();
    let parent_rtl = parent_css
        .as_ref()
        .map(|c| c.direction == Some(crate::style::Direction::Rtl))
        .unwrap_or(false);
    let child_wm = child_css.writing_mode.unwrap_or_default();
    let child_rtl = child_css.direction == Some(crate::style::Direction::Rtl);
    let is_orthogonal = child_wm.is_horizontal() != parent_wm.is_horizontal();
    let col_reverse = !is_orthogonal && (parent_rtl != child_rtl);
    let row_reverse = is_orthogonal && (parent_rtl != child_rtl);

    let (smbp_x, embp_x, smbp_y, embp_y) = subgrid_mbp(&child_css, resolve);

    let cols = if !is_orthogonal {
        build_axis(
            child_css.grid_template_columns.as_deref(),
            child_css.gap,
            col_sizes,
            (cell.c0, cell.c1),
            col_gap,
            col_lines,
            areas,
            true,
            smbp_x,
            embp_x,
            resolve,
            col_reverse,
            child_rtl,
        )
    } else {
        build_axis(
            child_css.grid_template_columns.as_deref(),
            child_css.gap,
            row_sizes,
            (cell.r0, cell.r1),
            row_gap,
            row_lines,
            areas,
            false,
            smbp_y,
            embp_y,
            resolve,
            col_reverse,
            child_rtl,
        )
    };

    let rows = if !is_orthogonal {
        build_axis(
            child_css.grid_template_rows.as_deref(),
            child_css.row_gap,
            row_sizes,
            (cell.r0, cell.r1),
            row_gap,
            row_lines,
            areas,
            false,
            smbp_y,
            embp_y,
            resolve,
            row_reverse,
            false,
        )
    } else {
        build_axis(
            child_css.grid_template_rows.as_deref(),
            child_css.row_gap,
            col_sizes,
            (cell.c0, cell.c1),
            col_gap,
            col_lines,
            areas,
            true,
            smbp_x,
            embp_x,
            resolve,
            row_reverse,
            false,
        )
    };

    Some(SubgridContext { cols, rows })
}

pub(in crate::layout) fn is_subgrid_item(
    css: &ComputedStyle,
    parent_wm: crate::style::WritingMode,
) -> (bool, bool) {
    if css.disables_subgrid == Some(true) {
        return (false, false);
    }
    let child_wm = css.writing_mode.unwrap_or_default();
    let is_orthogonal = child_wm.is_horizontal() != parent_wm.is_horizontal();
    let col = matches!(
        css.grid_template_columns.as_deref(),
        Some(GridTemplate::Subgrid(_))
    );
    let row = matches!(
        css.grid_template_rows.as_deref(),
        Some(GridTemplate::Subgrid(_))
    );
    if is_orthogonal {
        (row, col)
    } else {
        (col, row)
    }
}

pub(in crate::layout) fn init_col_tracks(
    col_sub: Option<&SubgriddedAxis>,
    css: &ComputedStyle,
    areas: Option<&crate::style::GridAreas>,
    content_w: f32,
    col_gap: f32,
    container_content_h: Option<f32>,
    resolve: &ResolveCtx,
    is_p_grid: bool,
) -> (Vec<crate::style::GridTrack>, Vec<bool>, Vec<LineNames>) {
    if let Some(cs) = col_sub {
        (vec![crate::style::GridTrack::Auto; cs.track_sizes.len()], vec![false; cs.track_sizes.len()], cs.line_names.clone())
    } else {
        let has_def = css.width.is_some() && !matches!(css.width, Some(crate::style::Dimension::MinContent) | Some(crate::style::Dimension::MaxContent));
        let is_inl = matches!(css.effective_display(), Some(crate::style::DisplayKind::InlineGrid));
        let min_s = super::aspect::container_min_size_for_auto_repeat(css, true, container_content_h, resolve);
        let c_w = if (has_def || (!is_inl && content_w > 0.0)) && min_s.map_or(true, |ms| content_w >= ms) { content_w } else { 0.0 };
        match css.grid_template_columns.as_deref() {
            Some(GridTemplate::Subgrid(_)) if is_p_grid => {
                let s = subgrid_span(css.grid_column_start.as_ref(), css.grid_column_end.as_ref());
                (vec![crate::style::GridTrack::Auto; s], vec![false; s], vec![Vec::new(); s + 1])
            }
            Some(GridTemplate::Tracks(t)) => super::tracks::expand_auto_repeats(t, c_w, col_gap, min_s),
            _ => match areas.map(|a| a.cols).or_else(|| css.grid_columns.map(|c| c.max(1) as usize)) {
                Some(cols) => {
                    let tl = crate::style::TrackList::from_tracks(vec![crate::style::GridTrack::Fr(1.0); cols]);
                    super::tracks::expand_auto_repeats(&tl, c_w, col_gap, min_s)
                }
                None => (Vec::new(), Vec::new(), Vec::new()),
            },
        }
    }
}

pub(in crate::layout) fn init_row_tracks(
    row_sub: Option<&SubgriddedAxis>,
    css: &ComputedStyle,
    container_content_h: Option<f32>,
    row_gap: f32,
    resolve: &ResolveCtx,
    is_p_grid: bool,
) -> (Vec<crate::style::GridTrack>, Vec<bool>, Vec<LineNames>) {
    if let Some(rs) = row_sub {
        (vec![crate::style::GridTrack::Auto; rs.track_sizes.len()], vec![false; rs.track_sizes.len()], rs.line_names.clone())
    } else {
        let min_s = super::aspect::container_min_size_for_auto_repeat(css, false, container_content_h, resolve);
        match css.grid_template_rows.as_deref() {
            Some(GridTemplate::Subgrid(_)) if is_p_grid => {
                let s = subgrid_span(css.grid_row_start.as_ref(), css.grid_row_end.as_ref());
                (vec![crate::style::GridTrack::Auto; s], vec![false; s], vec![Vec::new(); s + 1])
            }
            Some(GridTemplate::Tracks(t)) => super::tracks::expand_auto_repeats(t, container_content_h.unwrap_or(0.0), row_gap, min_s),
            _ => (Vec::new(), Vec::new(), Vec::new()),
        }
    }
}

pub(in crate::layout) fn register_if_subgrid(
    dom: &Dom,
    child: NodeIdx,
    cell: &GridCell,
    col_sizes: &[f32],
    row_sizes: &[f32],
    col_gap: f32,
    row_gap: f32,
    col_lines: &[LineNames],
    row_lines: &[LineNames],
    areas: Option<&crate::style::GridAreas>,
    resolve: &ResolveCtx,
    is_col_sub: bool,
    is_row_sub: bool,
) {
    let eff_cols =
        super::lines::effective_lines(col_lines, col_sizes.len(), areas, true, is_col_sub);
    let eff_rows =
        super::lines::effective_lines(row_lines, row_sizes.len(), areas, false, is_row_sub);
    if let Some(sctx) = check_and_build(
        dom, child, cell, col_sizes, row_sizes, col_gap, row_gap, &eff_cols, &eff_rows, areas,
        resolve,
    ) {
        register_subgrid(cell.box_id, sctx);
    }
}

pub(in crate::layout) fn cleanup_subgrids(cells: &[GridCell]) {
    for c in cells {
        take_subgrid(c.box_id);
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn register_col_subgrids_for_measure(
    dom: &Dom,
    cells: &[GridCell],
    col_sizes: &[f32],
    col_gap: f32,
    row_gap: f32,
    col_lines: &[LineNames],
    areas: Option<&crate::style::GridAreas>,
    resolve: &ResolveCtx,
    parent_wm: crate::style::WritingMode,
    is_col_sub: bool,
) {
    for cell in cells {
        let child_css = dom.computed_style_idx(cell.child).unwrap_or_default();
        let (is_c, _) = is_subgrid_item(&child_css, parent_wm);
        if is_c {
            register_if_subgrid(
                dom,
                cell.child,
                cell,
                col_sizes,
                &[],
                col_gap,
                row_gap,
                col_lines,
                &[],
                areas,
                resolve,
                is_col_sub,
                false,
            );
        }
    }
}
