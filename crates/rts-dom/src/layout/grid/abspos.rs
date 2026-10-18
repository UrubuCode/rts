//! Static position anchors and grid-area containing blocks for out-of-flow items inside a grid container.

use super::lines::{
    effective_lines, find_named_line_backward, find_named_line_forward, Edge,
};
use crate::boxes::BoxId;
use crate::dom::Dom;
use crate::layout::grid::subgrid::SubgriddedAxis;
use crate::layout::positioned::positioned::is_out_of_flow;
use crate::layout::ResolveCtx;
use crate::paint::list::DisplayList;
use crate::paint::Rect;
use crate::style::grid_lines::GridLine;
use crate::style::{ComputedStyle, GridAreas};

#[derive(Clone, Copy, Debug)]
pub(in crate::layout) struct GridCbRel {
    pub rel_x: f32,
    pub rel_y: f32,
    pub w: f32,
    pub h: f32,
    pub col_auto: bool,
    pub row_auto: bool,
}

thread_local! {
    static GRID_CB_RECTS: std::cell::RefCell<crate::fasthash::FastMap<BoxId, GridCbRel>> =
        std::cell::RefCell::new(crate::fasthash::FastMap::default());
}

pub(in crate::layout) fn get_grid_cb(box_id: BoxId, parent_cb: Rect) -> Option<Rect> {
    GRID_CB_RECTS.with(|cell| {
        cell.borrow().get(&box_id).map(|rel| {
            let x = if rel.col_auto { parent_cb.x } else { parent_cb.x + rel.rel_x };
            let w = if rel.col_auto { parent_cb.w } else { rel.w };
            let y = if rel.row_auto { parent_cb.y } else { parent_cb.y + rel.rel_y };
            let h = if rel.row_auto { parent_cb.h } else { rel.h };
            Rect::new(x, y, w, h)
        })
    })
}

pub(in crate::layout) fn clear_grid_cb() {
    GRID_CB_RECTS.with(|cell| cell.borrow_mut().clear());
}

fn col_line_x(
    line: i32,
    ncols: usize,
    col_x: &[f32],
    col_sizes: &[f32],
    col_sub: Option<&SubgriddedAxis>,
    content_x: f32,
    _content_w: f32,
    is_rtl: bool,
) -> f32 {
    if ncols == 0 {
        return content_x;
    }
    let idx = (line - 1).clamp(0, ncols as i32) as usize;
    if let Some(cs) = col_sub {
        content_x + cs.line_pos.get(idx).copied().unwrap_or(0.0)
    } else if is_rtl {
        if idx == 0 {
            col_x.first().copied().unwrap_or(content_x) + col_sizes.first().copied().unwrap_or(0.0)
        } else {
            col_x.get(idx - 1).copied().unwrap_or(content_x)
        }
    } else if let Some(&x) = col_x.get(idx) {
        x
    } else {
        col_x.last().copied().unwrap_or(content_x) + col_sizes.last().copied().unwrap_or(0.0)
    }
}

fn row_line_y(
    line: i32,
    nrows: usize,
    row_y: &[f32],
    row_sizes: &[f32],
    row_sub: Option<&SubgriddedAxis>,
    content_y: f32,
) -> f32 {
    if nrows == 0 {
        return content_y;
    }
    let idx = (line - 1).clamp(0, nrows as i32) as usize;
    if let Some(rs) = row_sub {
        content_y + rs.line_pos.get(idx).copied().unwrap_or(0.0)
    } else if let Some(&y) = row_y.get(idx) {
        y
    } else {
        row_y.last().copied().unwrap_or(content_y) + row_sizes.last().copied().unwrap_or(0.0)
    }
}

fn collect_oof_descendants(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    current_box: BoxId,
    is_direct: bool,
    out: &mut Vec<(BoxId, crate::NodeIdx, bool)>,
) {
    for &child_box in tree.children_without_generated(current_box) {
        if let Some(child_node) = tree.node_of(child_box) {
            if is_out_of_flow(dom, child_node) {
                out.push((child_box, child_node, is_direct));
            } else {
                let css = dom.computed_style_idx(child_node);
                let establishes_cb = css.as_ref().map_or(false, |c| {
                    c.position.map_or(false, |p| p != crate::style::Position::Static)
                        || c.transform.is_some()
                });
                if !establishes_cb {
                    collect_oof_descendants(dom, tree, child_box, false, out);
                }
            }
        } else {
            collect_oof_descendants(dom, tree, child_box, is_direct, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn record_out_of_flow(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    container: BoxId,
    css: &ComputedStyle,
    resolve: &ResolveCtx,
    content_x: f32,
    content_y: f32,
    total_w: f32,
    total_h: f32,
    container_content_h: Option<f32>,
    is_rtl: bool,
    col_sub: Option<&SubgriddedAxis>,
    row_sub: Option<&SubgriddedAxis>,
    col_x: &[f32],
    row_y: &[f32],
    col_sizes: &[f32],
    row_sizes: &[f32],
    col_lines: &[crate::style::LineNames],
    row_lines: &[crate::style::LineNames],
    areas: Option<&GridAreas>,
    ncols: usize,
    nrows: usize,
    col_shift: i32,
    row_shift: i32,
    explicit_cols: usize,
    explicit_rows: usize,
    _list: &mut DisplayList,
) {
    let eff_col_lines = effective_lines(col_lines, explicit_cols, areas, true, col_sub.is_some());
    let eff_row_lines = effective_lines(row_lines, explicit_rows, areas, false, row_sub.is_some());
    let auto_line = GridLine::Auto;

    let padding = |lado: crate::style::Side| match lado {
        crate::style::Side::Len(d) => d.resolve(resolve).unwrap_or(0.0),
        _ => 0.0,
    };
    let pt = padding(css.padding.top);
    let pr = padding(css.padding.right);
    let pb = padding(css.padding.bottom);
    let pl = padding(css.padding.left);

    let container_w = if resolve.parent_content_w > 0.0 {
        resolve.parent_content_w
    } else {
        total_w
    };
    let container_h = container_content_h
        .or_else(|| crate::layout::positioned::positioned::resolve_height(css.height, None, resolve))
        .unwrap_or(total_h);
    let pad_left = content_x - pl;
    let pad_right = content_x + container_w + pr;
    let pad_top = content_y - pt;
    let pad_bottom = content_y + container_h + pb;
    let establishes_cb = css.position.map_or(false, |p| p != crate::style::Position::Static)
        || css.transform.is_some();

    let mut oof_boxes = Vec::new();
    collect_oof_descendants(dom, tree, container, true, &mut oof_boxes);

fn resolve_abspos_edge(
    l: &GridLine,
    end_edge: bool,
    explicit: usize,
    lines: &[crate::style::LineNames],
) -> Edge {
    match l {
        GridLine::Auto => Edge::Auto,
        GridLine::Line(n) => {
            if *n > 0 {
                Edge::Abs(*n)
            } else if *n < 0 {
                let total_lines = explicit as i32 + 1;
                Edge::Abs(total_lines + *n + 1)
            } else {
                Edge::Auto
            }
        }
        GridLine::Span(n) => Edge::Span(*n),
        GridLine::Named(id, nth) => {
            let n = nth.unwrap_or(1);
            if let Some(line) = super::lines::named_line(id, n, end_edge, lines) {
                Edge::Abs(line)
            } else {
                Edge::Auto
            }
        }
        GridLine::SpanNamed(id, n) => Edge::SpanNamed(id.clone(), *n),
    }
}

    for (child_box, child_node, _) in oof_boxes {
        let child_css = dom.computed_style_idx(child_node).unwrap_or_default();
        let cs = child_css.grid_column_start.as_ref().unwrap_or(&auto_line);
        let ce = child_css.grid_column_end.as_ref().unwrap_or(&auto_line);
        let rs = child_css.grid_row_start.as_ref().unwrap_or(&auto_line);
        let re = child_css.grid_row_end.as_ref().unwrap_or(&auto_line);

        if establishes_cb {
            let col_s_line = match resolve_abspos_edge(cs, false, explicit_cols, &eff_col_lines) {
                Edge::Abs(n) => Some(n + col_shift),
                Edge::Span(n) => {
                    if let Edge::Abs(b) = resolve_abspos_edge(ce, true, explicit_cols, &eff_col_lines) {
                        Some((b + col_shift) - n as i32)
                    } else {
                        None
                    }
                }
                Edge::SpanNamed(id, n) => {
                    if let Edge::Abs(b) = resolve_abspos_edge(ce, true, explicit_cols, &eff_col_lines) {
                        find_named_line_backward(&id, b, n, &eff_col_lines).map(|l| l + col_shift)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let col_e_line = match resolve_abspos_edge(ce, true, explicit_cols, &eff_col_lines) {
                Edge::Abs(n) => Some(n + col_shift),
                Edge::Span(n) => {
                    if let Edge::Abs(a) = resolve_abspos_edge(cs, false, explicit_cols, &eff_col_lines) {
                        Some((a + col_shift) + n as i32)
                    } else {
                        None
                    }
                }
                Edge::SpanNamed(id, n) => {
                    if let Edge::Abs(a) = resolve_abspos_edge(cs, false, explicit_cols, &eff_col_lines) {
                        find_named_line_forward(&id, a, n, &eff_col_lines).map(|l| l + col_shift)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let (col_s_line, col_e_line) = match (col_s_line, col_e_line) {
                (Some(s), Some(e)) if s > e => (Some(e), Some(s)),
                (Some(s), Some(e)) if s == e => (Some(s), Some(s + 1)),
                other => other,
            };
            let edge_start_x = match col_s_line {
                Some(l) => col_line_x(l, ncols, col_x, col_sizes, col_sub, content_x, container_w, is_rtl),
                None => if is_rtl { pad_right } else { pad_left },
            };
            let edge_end_x = match col_e_line {
                Some(l) => col_line_x(l, ncols, col_x, col_sizes, col_sub, content_x, container_w, is_rtl),
                None => if is_rtl { pad_left } else { pad_right },
            };
            let cb_x = edge_start_x.min(edge_end_x);
            let cb_w = (edge_start_x.max(edge_end_x) - cb_x).max(0.0);

            let row_s_line = match resolve_abspos_edge(rs, false, explicit_rows, &eff_row_lines) {
                Edge::Abs(n) => Some(n + row_shift),
                Edge::Span(n) => {
                    if let Edge::Abs(b) = resolve_abspos_edge(re, true, explicit_rows, &eff_row_lines) {
                        Some((b + row_shift) - n as i32)
                    } else {
                        None
                    }
                }
                Edge::SpanNamed(id, n) => {
                    if let Edge::Abs(b) = resolve_abspos_edge(re, true, explicit_rows, &eff_row_lines) {
                        find_named_line_backward(&id, b, n, &eff_row_lines).map(|l| l + row_shift)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let row_e_line = match resolve_abspos_edge(re, true, explicit_rows, &eff_row_lines) {
                Edge::Abs(n) => Some(n + row_shift),
                Edge::Span(n) => {
                    if let Edge::Abs(a) = resolve_abspos_edge(rs, false, explicit_rows, &eff_row_lines) {
                        Some((a + row_shift) + n as i32)
                    } else {
                        None
                    }
                }
                Edge::SpanNamed(id, n) => {
                    if let Edge::Abs(a) = resolve_abspos_edge(rs, false, explicit_rows, &eff_row_lines) {
                        find_named_line_forward(&id, a, n, &eff_row_lines).map(|l| l + row_shift)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let (row_s_line, row_e_line) = match (row_s_line, row_e_line) {
                (Some(s), Some(e)) if s > e => (Some(e), Some(s)),
                (Some(s), Some(e)) if s == e => (Some(s), Some(s + 1)),
                other => other,
            };
            let edge_start_y = match row_s_line {
                Some(l) => row_line_y(l, nrows, row_y, row_sizes, row_sub, content_y),
                None => pad_top,
            };
            let edge_end_y = match row_e_line {
                Some(l) => row_line_y(l, nrows, row_y, row_sizes, row_sub, content_y),
                None => pad_bottom,
            };
            let cb_y = edge_start_y.min(edge_end_y);
            let cb_h = (edge_end_y - edge_start_y).abs();
            let mut rel_x = cb_x - pad_left;
            let mut rel_y = cb_y - pad_top;
            let mut w = cb_w;
            let mut h = cb_h;
            let has_col_lines = col_s_line.is_some() || col_e_line.is_some();
            let has_row_lines = row_s_line.is_some() || row_e_line.is_some();
            let mut col_auto = !has_col_lines;
            let mut row_auto = !has_row_lines;
            if let Some(wm) = crate::layout::rotated_frame_mode() {
                match wm {
                    crate::style::WritingMode::VerticalLr => {
                        rel_x = cb_y - pad_top;
                        rel_y = cb_x - pad_left;
                        w = cb_h;
                        h = cb_w;
                        col_auto = !has_row_lines;
                        row_auto = !has_col_lines;
                    }
                    crate::style::WritingMode::VerticalRl => {
                        let total_pad_h = pad_bottom - pad_top;
                        rel_x = total_pad_h - (cb_y - pad_top + cb_h);
                        rel_y = cb_x - pad_left;
                        w = cb_h;
                        h = cb_w;
                        col_auto = !has_row_lines;
                        row_auto = !has_col_lines;
                    }
                    crate::style::WritingMode::SidewaysLr => {
                        let total_pad_w = pad_right - pad_left;
                        rel_x = cb_y - pad_top;
                        rel_y = total_pad_w - (cb_x - pad_left + cb_w);
                        w = cb_h;
                        h = cb_w;
                        col_auto = !has_row_lines;
                        row_auto = !has_col_lines;
                    }
                    _ => {}
                }
            }
            if has_col_lines || has_row_lines {
                GRID_CB_RECTS.with(|cell| {
                    cell.borrow_mut().insert(
                        child_box,
                        GridCbRel { rel_x, rel_y, w, h, col_auto, row_auto },
                    );
                });
            }
        }
    }
}
