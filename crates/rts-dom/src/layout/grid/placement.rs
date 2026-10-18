//! Placement of grid items into cells (CSS Grid Layout Module §8.5).

use std::collections::HashSet;
use crate::dom::Dom;
use crate::layout::grid::lines::{axis_placement, effective_lines, GridCell, GridItem};
use crate::style::grid_lines::{GridAutoFlow, GridLine};
use crate::style::{GridAreas, LineNames};

/// Marca `r0..r1 × c0..c1` como ocupado.
fn mark(taken: &mut HashSet<(usize, usize)>, r0: usize, c0: usize, r1: usize, c1: usize) {
    for r in r0..r1 {
        for c in c0..c1 {
            taken.insert((r, c));
        }
    }
}

fn is_free(taken: &HashSet<(usize, usize)>, r0: usize, c0: usize, r1: usize, c1: usize) -> bool {
    !(r0..r1).any(|r| (c0..c1).any(|c| taken.contains(&(r, c))))
}

fn find_free_row(
    taken: &HashSet<(usize, usize)>,
    c0: usize,
    c1: usize,
    span: usize,
    mut r: usize,
) -> usize {
    while !is_free(taken, r, c0, r + span, c1) {
        r += 1;
    }
    r
}

fn find_free_col(
    taken: &HashSet<(usize, usize)>,
    r0: usize,
    r1: usize,
    span: usize,
    mut c: usize,
) -> usize {
    while !is_free(taken, r0, c, r1, c + span) {
        c += 1;
    }
    c
}

fn find_free_row_major(
    taken: &HashSet<(usize, usize)>,
    ncols: usize,
    r_span: usize,
    c_span: usize,
    start_r: usize,
    start_c: usize,
) -> (usize, usize) {
    let mut r = start_r;
    let mut c = start_c;
    loop {
        if (c == 0 || c + c_span <= ncols.max(1)) && is_free(taken, r, c, r + r_span, c + c_span) {
            return (r, c);
        }
        c += 1;
        if c >= ncols.max(1) {
            c = 0;
            r += 1;
        }
    }
}

fn find_free_col_major(
    taken: &HashSet<(usize, usize)>,
    row_bound: usize,
    r_span: usize,
    c_span: usize,
    start_r: usize,
    start_c: usize,
) -> (usize, usize) {
    let mut r = start_r;
    let mut c = start_c;
    loop {
        if (r == 0 || r + r_span <= row_bound.max(1))
            && is_free(taken, r, c, r + r_span, c + c_span)
        {
            return (r, c);
        }
        r += 1;
        if r >= row_bound.max(1) {
            r = 0;
            c += 1;
        }
    }
}

fn line_name_exists(lines: &[LineNames], name: &str) -> bool {
    let suffixed_s = format!("{}-start", name);
    let suffixed_e = format!("{}-end", name);
    lines.iter().any(|ln| ln.iter().any(|l| l == name || l == &suffixed_s || l == &suffixed_e))
}

fn span_of(start: &GridLine, end: &GridLine, lines: &[LineNames]) -> usize {
    match (start, end) {
        (GridLine::Span(n), _) | (_, GridLine::Span(n)) => (*n as usize).max(1),
        (GridLine::SpanNamed(id, n), _) | (_, GridLine::SpanNamed(id, n)) => {
            if line_name_exists(lines, id) {
                (*n as usize).max(1)
            } else {
                1
            }
        }
        _ => 1,
    }
}

#[derive(Clone, Debug)]
pub(in crate::layout) struct PlacementResult {
    pub cells: Vec<GridCell>,
    pub ncols: usize,
    pub col_shift: i32,
    pub row_shift: i32,
}

struct ItemPlacementInfo {
    item: GridItem,
    colp: Option<(usize, usize)>,
    rowp: Option<(usize, usize)>,
    c_span: usize,
    r_span: usize,
}

/// Coloca os filhos e devolve `PlacementResult` (CSS Grid §8.5).
pub(in crate::layout) fn place_grid_items(
    dom: &Dom,
    children: &[GridItem],
    areas: Option<&GridAreas>,
    col_lines: &[LineNames],
    row_lines: &[LineNames],
    explicit_cols: usize,
    explicit_rows: usize,
    auto_flow: GridAutoFlow,
    is_col_sub: bool,
    is_row_sub: bool,
) -> PlacementResult {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut ncols = explicit_cols.max(1);

    let eff_col_lines = effective_lines(col_lines, explicit_cols, areas, true, is_col_sub);
    let eff_row_lines = effective_lines(row_lines, explicit_rows, areas, false, is_row_sub);

    let mut definite_both = Vec::new();
    let mut phase2 = Vec::new();
    let mut phase3 = Vec::new();
    let auto_line = GridLine::Auto;

    let mut min_col = 0i32;
    let mut min_row = 0i32;
    let mut raw_infos = Vec::with_capacity(children.len());

    for &child in children {
        let css = dom.computed_style_idx(child.node).unwrap_or_default();
        let cs = css.grid_column_start.as_ref().unwrap_or(&auto_line);
        let ce = css.grid_column_end.as_ref().unwrap_or(&auto_line);
        let rs = css.grid_row_start.as_ref().unwrap_or(&auto_line);
        let re = css.grid_row_end.as_ref().unwrap_or(&auto_line);

        let colp = axis_placement(cs, ce, explicit_cols, &eff_col_lines, is_col_sub);
        let rowp = axis_placement(rs, re, explicit_rows, &eff_row_lines, is_row_sub);
        if let Some((c0, _)) = colp {
            min_col = min_col.min(c0);
        }
        if let Some((r0, _)) = rowp {
            min_row = min_row.min(r0);
        }
        let c_span = colp
            .map(|(c0, c1)| (c1 - c0).max(1) as usize)
            .unwrap_or_else(|| span_of(cs, ce, &eff_col_lines));
        let r_span = rowp
            .map(|(r0, r1)| (r1 - r0).max(1) as usize)
            .unwrap_or_else(|| span_of(rs, re, &eff_row_lines));
        raw_infos.push((child, colp, rowp, c_span, r_span));
    }

    let col_shift = if min_col < 0 { -min_col } else { 0 };
    let row_shift = if min_row < 0 { -min_row } else { 0 };

    for (child, colp, rowp, c_span, r_span) in raw_infos {
        let colp = colp.map(|(c0, c1)| ((c0 + col_shift) as usize, (c1 + col_shift) as usize));
        let rowp = rowp.map(|(r0, r1)| ((r0 + row_shift) as usize, (r1 + row_shift) as usize));

        let info = ItemPlacementInfo {
            item: child,
            colp,
            rowp,
            c_span,
            r_span,
        };

        match (colp, rowp) {
            (Some(_), Some(_)) => definite_both.push(info),
            (None, Some(_)) if !auto_flow.column => phase2.push(info),
            (Some(_), None) if auto_flow.column => phase2.push(info),
            _ => phase3.push(info),
        }
    }

    for info in definite_both {
        let (c0, c1) = info.colp.unwrap();
        let (r0, r1) = info.rowp.unwrap();
        ncols = ncols.max(c1);
        mark(&mut taken, r0, c0, r1, c1);
        cells.push(GridCell {
            child: info.item.node,
            box_id: info.item.box_id,
            r0,
            c0,
            r1,
            c1,
        });
    }

    let mut cursor_r = 0;
    let mut cursor_c = 0;

    for info in phase2 {
        let (r0, c0, r1, c1) = if !auto_flow.column {
            let (r0, r1) = info.rowp.unwrap();
            let start_c = if auto_flow.dense {
                0
            } else if r0 != cursor_r {
                0
            } else {
                cursor_c
            };
            let c0 = find_free_col(&taken, r0, r1, info.c_span, start_c);
            ncols = ncols.max(c0 + info.c_span);
            if !auto_flow.dense {
                cursor_r = r0;
                cursor_c = c0 + info.c_span;
            }
            (r0, c0, r1, c0 + info.c_span)
        } else {
            let (c0, c1) = info.colp.unwrap();
            ncols = ncols.max(c1);
            let start_r = if auto_flow.dense {
                0
            } else if c0 != cursor_c {
                0
            } else {
                cursor_r
            };
            let r0 = find_free_row(&taken, c0, c1, info.r_span, start_r);
            if !auto_flow.dense {
                cursor_r = r0 + info.r_span;
                cursor_c = c0;
            }
            (r0, c0, r0 + info.r_span, c1)
        };
        mark(&mut taken, r0, c0, r1, c1);
        cells.push(GridCell {
            child: info.item.node,
            box_id: info.item.box_id,
            r0,
            c0,
            r1,
            c1,
        });
    }

    cursor_r = 0;
    cursor_c = 0;
    let row_bound = explicit_rows.max(1);

    for info in phase3 {
        let (r0, c0, r1, c1) = if !auto_flow.column {
            if let Some((c0, c1)) = info.colp {
                ncols = ncols.max(c1);
                let start_r = if auto_flow.dense {
                    0
                } else if c0 < cursor_c {
                    cursor_r + 1
                } else {
                    cursor_r
                };
                let r0 = find_free_row(&taken, c0, c1, info.r_span, start_r);
                if !auto_flow.dense {
                    cursor_r = r0;
                    cursor_c = c1;
                }
                (r0, c0, r0 + info.r_span, c1)
            } else {
                let (start_r, start_c) = if auto_flow.dense {
                    (0, 0)
                } else {
                    (cursor_r, cursor_c)
                };
                let (r, c) =
                    find_free_row_major(&taken, ncols, info.r_span, info.c_span, start_r, start_c);
                if !auto_flow.dense {
                    if c + info.c_span >= ncols {
                        cursor_r = r + 1;
                        cursor_c = 0;
                    } else {
                        cursor_r = r;
                        cursor_c = c + info.c_span;
                    }
                }
                (r, c, r + info.r_span, c + info.c_span)
            }
        } else {
            if let Some((r0, r1)) = info.rowp {
                let start_c = if auto_flow.dense {
                    0
                } else if r0 < cursor_r {
                    cursor_c + 1
                } else {
                    cursor_c
                };
                let c0 = find_free_col(&taken, r0, r1, info.c_span, start_c);
                if !auto_flow.dense {
                    if r1 >= row_bound {
                        cursor_c = c0 + 1;
                        cursor_r = 0;
                    } else {
                        cursor_c = c0;
                        cursor_r = r1;
                    }
                }
                (r0, c0, r1, c0 + info.c_span)
            } else {
                let (start_r, start_c) = if auto_flow.dense {
                    (0, 0)
                } else {
                    (cursor_r, cursor_c)
                };
                let (r, c) = find_free_col_major(
                    &taken,
                    row_bound,
                    info.r_span,
                    info.c_span,
                    start_r,
                    start_c,
                );
                if !auto_flow.dense {
                    if r + info.r_span >= row_bound {
                        cursor_c = c + 1;
                        cursor_r = 0;
                    } else {
                        cursor_c = c;
                        cursor_r = r + info.r_span;
                    }
                }
                (r, c, r + info.r_span, c + info.c_span)
            }
        };
        ncols = ncols.max(c1);
        mark(&mut taken, r0, c0, r1, c1);
        cells.push(GridCell {
            child: info.item.node,
            box_id: info.item.box_id,
            r0,
            c0,
            r1,
            c1,
        });
    }

    PlacementResult {
        cells,
        ncols,
        col_shift,
        row_shift,
    }
}
