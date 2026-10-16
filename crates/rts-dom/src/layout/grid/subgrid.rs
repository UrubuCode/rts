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
    pub is_orthogonal: bool,
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(in crate::layout) enum PhysicalAxisDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl PhysicalAxisDirection {
    #[inline]
    pub fn is_reversed(self) -> bool {
        matches!(self, Self::RightToLeft | Self::BottomToTop)
    }
}

pub(in crate::layout) fn axis_direction(
    wm: crate::style::WritingMode,
    dir: crate::style::Direction,
    is_col: bool,
) -> PhysicalAxisDirection {
    use crate::style::{Direction::*, WritingMode::*};
    use PhysicalAxisDirection::*;
    if is_col {
        match (wm, dir) {
            (HorizontalTb, Ltr) => LeftToRight,
            (HorizontalTb, Rtl) => RightToLeft,
            (VerticalRl | VerticalLr | SidewaysRl, Ltr) => TopToBottom,
            (VerticalRl | VerticalLr | SidewaysRl, Rtl) => BottomToTop,
            (SidewaysLr, Ltr) => BottomToTop,
            (SidewaysLr, Rtl) => TopToBottom,
        }
    } else {
        match wm {
            HorizontalTb => TopToBottom,
            VerticalRl | SidewaysRl => RightToLeft,
            VerticalLr | SidewaysLr => LeftToRight,
        }
    }
}

fn is_frame_inverted(wm: crate::style::WritingMode, is_col: bool) -> bool {
    if is_col {
        wm == crate::style::WritingMode::SidewaysLr
    } else {
        matches!(
            wm,
            crate::style::WritingMode::VerticalRl | crate::style::WritingMode::SidewaysRl
        )
    }
}

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
    parent_axis_dir: PhysicalAxisDirection,
    subgrid_axis_dir: PhysicalAxisDirection,
    subgrid_wm: crate::style::WritingMode,
    subgrid_is_col: bool,
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

    let parent_reversed = parent_axis_dir.is_reversed();
    let subgrid_reversed = subgrid_axis_dir.is_reversed();
    let is_reversed = parent_axis_dir != subgrid_axis_dir;
    let local_reversed = if subgrid_wm.is_horizontal() {
        subgrid_reversed
    } else {
        is_reversed
    };

    let phys_slice: Vec<f32> = if parent_reversed {
        p_slice.iter().rev().copied().collect()
    } else {
        p_slice.to_vec()
    };

    let mut phys_starts = Vec::with_capacity(n);
    let mut phys_ends = Vec::with_capacity(n);
    let mut cur = 0.0f32;
    for &pt in &phys_slice {
        phys_starts.push(cur);
        cur += pt;
        phys_ends.push(cur);
        cur += p_gap;
    }

    let mut track_starts = Vec::with_capacity(n);
    let mut track_ends = Vec::with_capacity(n);
    let mut track_sizes = Vec::with_capacity(n);

    for k in 0..n {
        let p_idx = if local_reversed { n - 1 - k } else { k };
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
    if local_reversed {
        line_pos.push(content_span);
        let mut cur_from_end = 0.0f32;
        for k in 1..n {
            cur_from_end += phys_slice[n - k] + p_gap;
            line_pos.push(span_size - cur_from_end - smbp);
        }
        line_pos.push(0.0);
    } else {
        line_pos.push(0.0);
        for k in 0..n.saturating_sub(1) {
            line_pos.push(phys_ends[k] - smbp);
        }
        line_pos.push(content_span);
    }

    let p_end = s1.min(p_lines.len().saturating_sub(1));
    let parent_slice = if s0 <= p_end && s0 < p_lines.len() {
        &p_lines[s0..=p_end]
    } else {
        &[]
    };
    let mut line_names = expand_subgrid_line_names(subgrid_lines, n, parent_slice, is_reversed);
    clamp_area_lines(
        p_areas,
        s0,
        s1,
        n,
        p_is_col,
        &mut line_names,
        is_reversed,
    );
    Some(SubgriddedAxis {
        track_sizes,
        track_starts,
        track_ends,
        line_pos,
        line_names,
        gap,
        reversed: local_reversed,
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
    let parent_wm = crate::layout::block::rotated::rotated_frame_mode()
        .or_else(|| parent_css.as_ref().and_then(|c| c.writing_mode))
        .unwrap_or_default();
    let parent_dir = parent_css
        .as_ref()
        .and_then(|c| c.direction)
        .unwrap_or_default();
    let child_wm = dom.base_style_idx(child).and_then(|c| c.writing_mode).unwrap_or_default();
    let child_dir = child_css.direction.unwrap_or_default();
    let is_orthogonal = child_wm.is_horizontal() != parent_wm.is_horizontal();

    let (parent_col_dir, parent_row_dir) = (
        axis_direction(parent_wm, parent_dir, true),
        axis_direction(parent_wm, parent_dir, false),
    );
    let (subgrid_col_dir, subgrid_row_dir) = (
        axis_direction(child_wm, child_dir, true),
        axis_direction(child_wm, child_dir, false),
    );

    let (smbp_x, embp_x, smbp_y, embp_y) = subgrid_mbp(&child_css, resolve);
    let (smbp_c, embp_c, smbp_r, embp_r) = if child_wm.is_horizontal() {
        (smbp_x, embp_x, smbp_y, embp_y)
    } else {
        (smbp_y, embp_y, smbp_x, embp_x)
    };

    let (p_tracks_c, p_range_c, p_gap_c, p_lines_c, p_is_col_c, parent_dir_c) =
        if !is_orthogonal {
            (col_sizes, (cell.c0, cell.c1), col_gap, col_lines, true, parent_col_dir)
        } else {
            (row_sizes, (cell.r0, cell.r1), row_gap, row_lines, false, parent_row_dir)
        };
    let cols = build_axis(
        child_css.grid_template_columns.as_deref(),
        child_css.gap,
        p_tracks_c,
        p_range_c,
        p_gap_c,
        p_lines_c,
        areas,
        p_is_col_c,
        smbp_c,
        embp_c,
        resolve,
        parent_dir_c,
        subgrid_col_dir,
        child_wm,
        true,
    );

    let (p_tracks_r, p_range_r, p_gap_r, p_lines_r, p_is_col_r, parent_dir_r) =
        if !is_orthogonal {
            (row_sizes, (cell.r0, cell.r1), row_gap, row_lines, false, parent_row_dir)
        } else {
            (col_sizes, (cell.c0, cell.c1), col_gap, col_lines, true, parent_col_dir)
        };
    let rows = build_axis(
        child_css.grid_template_rows.as_deref(),
        child_css.row_gap,
        p_tracks_r,
        p_range_r,
        p_gap_r,
        p_lines_r,
        areas,
        p_is_col_r,
        smbp_r,
        embp_r,
        resolve,
        parent_dir_r,
        subgrid_row_dir,
        child_wm,
        false,
    );

    Some(SubgridContext {
        cols,
        rows,
        is_orthogonal,
    })
}


pub(in crate::layout) fn is_subgrid_node(
    dom: &Dom,
    child: NodeIdx,
    parent_wm: crate::style::WritingMode,
) -> (bool, bool) {
    let Some(css) = dom.computed_style_idx(child) else { return (false, false); };
    if css.disables_subgrid == Some(true) { return (false, false); }
    let child_wm = dom.base_style_idx(child).and_then(|c| c.writing_mode).unwrap_or_default();
    let is_orthogonal = child_wm.is_horizontal() != parent_wm.is_horizontal();
    let col = matches!(css.grid_template_columns.as_deref(), Some(GridTemplate::Subgrid(_)));
    let row = matches!(css.grid_template_rows.as_deref(), Some(GridTemplate::Subgrid(_)));
    if is_orthogonal { (row, col) } else { (col, row) }
}

pub(in crate::layout) fn is_subgrid_item(
    css: &ComputedStyle,
    parent_wm: crate::style::WritingMode,
) -> (bool, bool) {
    if css.disables_subgrid == Some(true) { return (false, false); }
    let child_wm = css.writing_mode.unwrap_or_default();
    let is_orthogonal = child_wm.is_horizontal() != parent_wm.is_horizontal();
    let col = matches!(css.grid_template_columns.as_deref(), Some(GridTemplate::Subgrid(_)));
    let row = matches!(css.grid_template_rows.as_deref(), Some(GridTemplate::Subgrid(_)));
    if is_orthogonal { (row, col) } else { (col, row) }
}

pub(in crate::layout) use super::init_tracks::{init_col_tracks, init_row_tracks};


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
