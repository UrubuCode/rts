//! Box alignment helpers inside a grid cell.

/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
///
/// `Center`/`FlexEnd` (sem o prefixo `safe`) NÃO recortam em `.max(0.0)` — a
/// grelha é `unsafe` por omissão (css-align-3 §4.4: só as formas `safe *`
/// pedem o fallback), e um item maior do que a célula transborda dos DOIS
/// lados quando `justify-items`/`align-items` é `center` puro. O
/// `.max(0.0)` que aqui estava antes desta correcção tornava `center`/
/// `flex-end` sempre seguros, ao contrário do default da spec — `SafeCenter`/
/// `SafeEnd` (que ANTES caíam no `_ => 0.0`, i.e. start incondicional) são o
/// lugar certo para esse recorte.
pub(super) fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    let free = cell - item;
    match a {
        crate::style::AlignItems::Center => free / 2.0,
        crate::style::AlignItems::FlexEnd
        | crate::style::AlignItems::Right
        | crate::style::AlignItems::SelfEnd
        | crate::style::AlignItems::LastBaseline => free,
        crate::style::AlignItems::SafeCenter => (free / 2.0).max(0.0),
        crate::style::AlignItems::SafeEnd => free.max(0.0),
        _ => 0.0, // FlexStart / Left / Stretch / SelfStart
    }
}

/// Soma os tamanhos das trilhas `start..end` mais os gaps entre elas.
pub(in crate::layout) fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let end = end.max(start + 1).min(sizes.len());
    let start = start.min(sizes.len() - 1);
    let n = end.saturating_sub(start);
    sizes[start..end].iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * gap
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_cell_rect(
    cell: &super::lines::GridCell,
    _is_ortho: bool,
    is_rtl: bool,
    col_sub: Option<&super::subgrid::SubgriddedAxis>,
    row_sub: Option<&super::subgrid::SubgriddedAxis>,
    col_x: &[f32],
    row_y: &[f32],
    col_sizes: &[f32],
    row_sizes: &[f32],
    col_gap: f32,
    row_gap: f32,
    content_x: f32,
    content_y: f32,
    nrows: usize,
) -> (f32, f32, f32, f32) {
    let (cx, cw) = if let Some(cs) = col_sub {
        let (mut rel_x, w) = cs.cell_pos_and_span(cell.c0, cell.c1);
        if cs.reversed && crate::layout::rotated_frame_mode().is_some_and(|wm| wm == crate::style::WritingMode::SidewaysLr) {
            rel_x = (cs.content_span() - (rel_x + w)).max(0.0);
        }
        (content_x + rel_x, w)
    } else {
        let c0 = if is_rtl {
            cell.c1.saturating_sub(1).min(col_x.len().saturating_sub(1))
        } else {
            cell.c0.min(col_x.len().saturating_sub(1))
        };
        (col_x[c0], span_size(col_sizes, cell.c0, cell.c1, col_gap))
    };

    let (cy, ch) = if let Some(rs) = row_sub {
        let (mut rel_y, h) = rs.cell_pos_and_span(cell.r0, cell.r1.min(nrows));
        if rs.reversed && crate::layout::rotated_frame_mode().is_some_and(|wm| matches!(wm, crate::style::WritingMode::VerticalRl | crate::style::WritingMode::SidewaysRl)) {
            rel_y = (rs.content_span() - (rel_y + h)).max(0.0);
        }
        (content_y + rel_y, h)
    } else {
        let r0 = cell.r0.min(row_y.len().saturating_sub(1));
        (row_y[r0], span_size(row_sizes, cell.r0, cell.r1.min(nrows), row_gap))
    };

    (cx, cw, cy, ch)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn subgrid_total_dims(
    _is_ortho: bool,
    col_sub: Option<&super::subgrid::SubgriddedAxis>,
    row_sub: Option<&super::subgrid::SubgriddedAxis>,
    col_sizes: &[f32],
    row_sizes: &[f32],
    ncols: usize,
    nrows: usize,
    col_collapsed: &[bool],
    row_collapsed: &[bool],
    col_gap: f32,
    row_gap: f32,
) -> (f32, f32) {
    let tw = col_sub
        .map(|cs| cs.content_span())
        .unwrap_or_else(|| {
            col_sizes.iter().sum::<f32>() + super::collapse::gutters(ncols, col_collapsed, col_gap)
        })
        .max(0.0);

    let th = row_sub
        .map(|rs| rs.content_span())
        .unwrap_or_else(|| {
            row_sizes.iter().sum::<f32>() + super::collapse::gutters(nrows, row_collapsed, row_gap)
        })
        .max(0.0);

    (tw, th)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_item_offsets(
    cell: &super::lines::GridCell,
    child: crate::dom::NodeIdx,
    dom: &crate::Dom,
    item_css: &crate::style::ComputedStyle,
    is_rtl: bool,
    cell_x: f32,
    cell_w: f32,
    cell_y: f32,
    cell_h: f32,
    justify: crate::style::AlignItems,
    align: crate::style::AlignItems,
    iw: f32,
    ih: f32,
    row_baselines: &[Option<super::baseline::TrackBaseline>],
    col_baselines: &[Option<super::baseline::TrackBaseline>],
    ctx: &crate::layout::LayoutCtx,
) -> (f32, f32) {
    let parent_wm = dom
        .node(child)
        .parent
        .and_then(|p| dom.base_style_idx(p))
        .and_then(|c| c.writing_mode)
        .unwrap_or_default();
    let child_wm = dom.base_style_idx(child).and_then(|c| c.writing_mode).unwrap_or_default();
    let is_item_ortho = child_wm.is_horizontal() != parent_wm.is_horizontal();

    let (auto_l, auto_r) = (item_css.margin.left.is_auto(), item_css.margin.right.is_auto());
    let x = if auto_l || auto_r {
        let free = (cell_w - iw).max(0.0);
        if auto_l && auto_r { cell_x + free / 2.0 } else if auto_l { cell_x + free } else { cell_x }
    } else if matches!(justify, crate::style::AlignItems::Baseline | crate::style::AlignItems::LastBaseline) {
        if is_item_ortho {
            if let Some(tb) = col_baselines.get(cell.c0).and_then(|t| t.as_ref()) {
                let first = justify == crate::style::AlignItems::Baseline;
                let ascent = super::baseline::measure_item_ascent(dom, child, cell.box_id, cell_w, ih, first, ctx);
                tb.align_offset(justify, ascent, cell_w)
                    .map_or_else(|| cell_x + cell_align_offset(justify, cell_w, iw), |o| cell_x + o)
            } else {
                cell_x + cell_align_offset(justify, cell_w, iw)
            }
        } else {
            cell_x + cell_align_offset(justify, cell_w, iw)
        }
    } else {
        match justify {
            crate::style::AlignItems::Left => cell_x,
            crate::style::AlignItems::Right => cell_x + cell_w - iw,
            crate::style::AlignItems::SelfStart => if item_css.direction == Some(crate::style::Direction::Rtl) { cell_x + cell_w - iw } else { cell_x },
            crate::style::AlignItems::SelfEnd => if item_css.direction == Some(crate::style::Direction::Rtl) { cell_x } else { cell_x + cell_w - iw },
            _ if is_rtl => cell_x + cell_w - iw - cell_align_offset(justify, cell_w, iw),
            _ => cell_x + cell_align_offset(justify, cell_w, iw),
        }
    };
    let (auto_t, auto_b) = (item_css.margin.top.is_auto(), item_css.margin.bottom.is_auto());
    let y = if auto_t || auto_b {
        let free = (cell_h - ih).max(0.0);
        if auto_t && auto_b { cell_y + free / 2.0 } else if auto_t { cell_y + free } else { cell_y }
    } else if matches!(align, crate::style::AlignItems::Stretch) {
        cell_y
    } else if !is_item_ortho
        && let Some(tb) = row_baselines
            .get(if align == crate::style::AlignItems::Baseline { cell.r0 } else { cell.r1.saturating_sub(1) })
            .and_then(|t| t.as_ref())
    {
        let first = align == crate::style::AlignItems::Baseline;
        let ascent = super::baseline::measure_item_ascent(dom, child, cell.box_id, cell_w, ih, first, ctx);
        tb.align_offset(align, ascent, cell_h)
            .map_or_else(|| cell_y + cell_align_offset(align, cell_h, ih), |o| cell_y + o)
    } else {
        cell_y + cell_align_offset(align, cell_h, ih)
    };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::cell_align_offset;
    use crate::style::AlignItems as A;

    #[test]
    fn center_is_unsafe_by_default() {
        assert_eq!(cell_align_offset(A::Center, 100.0, 200.0), -50.0);
        assert_eq!(cell_align_offset(A::FlexEnd, 100.0, 200.0), -100.0);
    }

    #[test]
    fn safe_falls_back_to_start_when_the_item_overflows() {
        assert_eq!(cell_align_offset(A::SafeCenter, 100.0, 200.0), 0.0);
        assert_eq!(cell_align_offset(A::SafeEnd, 100.0, 200.0), 0.0);
    }

    #[test]
    fn safe_matches_unsafe_when_the_item_fits() {
        assert_eq!(cell_align_offset(A::Center, 100.0, 40.0), 30.0);
        assert_eq!(cell_align_offset(A::SafeCenter, 100.0, 40.0), 30.0);
        assert_eq!(cell_align_offset(A::FlexEnd, 100.0, 40.0), 60.0);
        assert_eq!(cell_align_offset(A::SafeEnd, 100.0, 40.0), 60.0);
    }
}
