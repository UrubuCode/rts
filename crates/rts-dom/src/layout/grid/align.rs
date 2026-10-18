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
    col_gutter_gap: f32,
    row_gutter_gap: f32,
) -> (f32, f32) {
    let tw = col_sub
        .map(|cs| cs.content_span())
        .unwrap_or_else(|| {
            col_sizes.iter().sum::<f32>() + super::collapse::gutters(ncols, col_collapsed, col_gutter_gap)
        })
        .max(0.0);

    let th = row_sub
        .map(|rs| rs.content_span())
        .unwrap_or_else(|| {
            row_sizes.iter().sum::<f32>() + super::collapse::gutters(nrows, row_collapsed, row_gutter_gap)
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
            } else if child_wm == crate::style::WritingMode::VerticalRl || child_wm == crate::style::WritingMode::SidewaysRl {
                cell_x + cell_w - iw
            } else {
                cell_x
            }
        } else if is_rtl {
            cell_x + cell_w - iw
        } else {
            cell_x
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
    } else if !super::distribute::has_pct(item_css.height)
        && let Some(tb) = row_baselines
            .get(if align == crate::style::AlignItems::Baseline { cell.r0 } else { cell.r1.saturating_sub(1) })
            .and_then(|t| t.as_ref())
    {
        let first = align == crate::style::AlignItems::Baseline;
        let font = crate::layout::font_px(item_css, crate::layout::DEFAULT_FONT_SIZE);
        let rc = crate::layout::ResolveCtx {
            parent_content_w: cell_w,
            node_font_size: font,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let ascent = if is_item_ortho {
            let mb = item_css.margin.bottom.resolve(&rc).unwrap_or(0.0);
            (ih - mb).max(0.0)
        } else {
            super::baseline::measure_item_ascent(dom, child, cell.box_id, cell_w, ih, first, ctx)
        };
        let is_opposite = !is_item_ortho && child_wm != parent_wm && !child_wm.is_horizontal() && !parent_wm.is_horizontal();
        if is_opposite && tb.has_end {
            tb.align_offset_end(ascent, cell_h)
                .map_or_else(|| cell_y + cell_align_offset(align, cell_h, ih), |o| cell_y + o)
        } else {
            tb.align_offset(align, ascent, cell_h)
                .map_or_else(|| cell_y + cell_align_offset(align, cell_h, ih), |o| cell_y + o)
        }
    } else {
        cell_y + cell_align_offset(align, cell_h, ih)
    };
    (x, y)
}

pub(super) struct ComputedStyleOverrideScope<'a> {
    dom: &'a crate::Dom,
    node: crate::layout::NodeIdx,
    prev: Option<std::rc::Rc<crate::style::ComputedStyle>>,
}

impl<'a> ComputedStyleOverrideScope<'a> {
    pub fn enter(
        dom: &'a crate::Dom,
        node: crate::layout::NodeIdx,
        style: Option<crate::style::ComputedStyle>,
    ) -> Option<Self> {
        let style = style?;
        let prev = dom.override_computed_style(node, style);
        Some(Self { dom, node, prev })
    }
}

impl Drop for ComputedStyleOverrideScope<'_> {
    fn drop(&mut self) {
        self.dom.restore_computed_style(self.node, self.prev.take());
    }
}

pub(in crate::layout) fn item_content_metrics(
    css: &crate::style::ComputedStyle,
    font_size: f32,
    resolve: &crate::layout::ResolveCtx,
    is_ortho_frame: bool,
) -> (f32, f32, f32) {
    use crate::style::WritingMode;
    let frame_wm = crate::layout::block::rotated::rotated_frame_mode();
    let child_wm = css.writing_mode.unwrap_or_default();
    let wm = if is_ortho_frame { child_wm } else { frame_wm.unwrap_or(child_wm) };
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let (b_start, b_end) = if frame_wm.is_some() && !is_ortho_frame {
        (bt, bb)
    } else {
        match wm {
            WritingMode::HorizontalTb => (bt, bb),
            WritingMode::VerticalLr | WritingMode::SidewaysLr => (bl, br),
            WritingMode::VerticalRl | WritingMode::SidewaysRl => (br, bl),
        }
    };
    let (m_start, m_end) = if frame_wm.is_some() && !is_ortho_frame {
        (css.margin.top.resolve(resolve).unwrap_or(0.0), css.margin.bottom.resolve(resolve).unwrap_or(0.0))
    } else {
        match wm {
            WritingMode::HorizontalTb => (css.margin.top.resolve(resolve).unwrap_or(0.0), css.margin.bottom.resolve(resolve).unwrap_or(0.0)),
            WritingMode::VerticalLr | WritingMode::SidewaysLr => (css.margin.left.resolve(resolve).unwrap_or(0.0), css.margin.right.resolve(resolve).unwrap_or(0.0)),
            WritingMode::VerticalRl | WritingMode::SidewaysRl => (css.margin.right.resolve(resolve).unwrap_or(0.0), css.margin.left.resolve(resolve).unwrap_or(0.0)),
        }
    };
    let (p_start, p_end) = if frame_wm.is_some() && !is_ortho_frame {
        (css.padding.top.resolve(resolve).unwrap_or(0.0).max(0.0), css.padding.bottom.resolve(resolve).unwrap_or(0.0).max(0.0))
    } else {
        match wm {
            WritingMode::HorizontalTb => (css.padding.top.resolve(resolve).unwrap_or(0.0).max(0.0), css.padding.bottom.resolve(resolve).unwrap_or(0.0).max(0.0)),
            WritingMode::VerticalLr | WritingMode::SidewaysLr => (css.padding.left.resolve(resolve).unwrap_or(0.0).max(0.0), css.padding.right.resolve(resolve).unwrap_or(0.0).max(0.0)),
            WritingMode::VerticalRl | WritingMode::SidewaysRl => (css.padding.right.resolve(resolve).unwrap_or(0.0).max(0.0), css.padding.left.resolve(resolve).unwrap_or(0.0).max(0.0)),
        }
    };

    let (a, d) = match wm {
        WritingMode::HorizontalTb => (0.8 * font_size, 0.2 * font_size),
        WritingMode::VerticalLr | WritingMode::SidewaysLr => (0.2 * font_size, 0.8 * font_size),
        WritingMode::VerticalRl | WritingMode::SidewaysRl => (0.8 * font_size, 0.2 * font_size),
    };

    let b_first = m_start + b_start + p_start + a;
    let b_last = m_end + b_end + p_end + d;
    let outer_sz = m_start + b_start + p_start + (2.0 * font_size) + p_end + b_end + m_end;
    (b_first, b_last, outer_sz)
}

pub(in crate::layout) fn item_content_baseline_metrics(
    css: &crate::style::ComputedStyle,
    font_size: f32,
    resolve: &crate::layout::ResolveCtx,
    is_ortho_frame: bool,
) -> (f32, f32) {
    let (b_first, b_last, _) = item_content_metrics(css, font_size, resolve, is_ortho_frame);
    (b_first, b_last)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_content_baseline_override(
    _dom: &crate::Dom,
    child: crate::layout::NodeIdx,
    cell: &super::lines::GridCell,
    item_css: &crate::style::ComputedStyle,
    is_ortho: bool,
    cell_w: f32,
    cell_h: f32,
    nat_w: f32,
    nat_h: f32,
    row_baselines: &[Option<super::baseline::TrackBaseline>],
    col_baselines: &[Option<super::baseline::TrackBaseline>],
    ctx: &crate::layout::LayoutCtx,
) -> (Option<crate::style::ComputedStyle>, f32, f32) {
    use crate::style::{JustifyContent, Side, WritingMode};
    let Some(ac) = item_css.align_content else { return (None, 0.0, 0.0); };
    let is_fb = ac == JustifyContent::Baseline;
    let is_lb = ac == JustifyContent::LastBaseline;
    if !is_fb && !is_lb {
        return (None, 0.0, 0.0);
    }

    let tb = if is_ortho {
        col_baselines.get(if is_fb { cell.c0 } else { cell.c1.saturating_sub(1) }).and_then(|o| o.as_ref())
    } else {
        row_baselines.get(if is_fb { cell.r0 } else { cell.r1.saturating_sub(1) }).and_then(|o| o.as_ref())
    };
    let Some(tb) = tb else { return (None, 0.0, 0.0); };

    let child_wm = item_css.writing_mode.unwrap_or_default();
    let font = crate::layout::font_px(item_css, crate::layout::DEFAULT_FONT_SIZE);
    let rc = crate::layout::ResolveCtx {
        parent_content_w: cell_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };

    let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(child);
    let (ascent, descent, outer_sz) = item_content_metrics(item_css, font, &rc, is_ortho_frame);
    let frame_wm = crate::layout::block::rotated::rotated_frame_mode();
    let is_in_frame = frame_wm.is_some();
    let cell_sz = if tb.max_size > 0.0 {
        tb.max_size
    } else if is_ortho {
        cell_w
    } else {
        cell_h
    };
    let item_sz = if tb.max_size > 0.0 {
        outer_sz
    } else if is_ortho {
        nat_w
    } else {
        nat_h
    };

    let align = item_css.align_self.unwrap_or(crate::style::AlignItems::Normal);
    let is_stretch = matches!(align, crate::style::AlignItems::Stretch);

    let (delta_start, delta_end) = if is_fb {
        let d_start = (tb.max_ascent - ascent).max(0.0);
        let d_end = if is_stretch {
            (cell_sz - (item_sz + d_start)).max(0.0)
        } else {
            0.0
        };
        (d_start, d_end)
    } else {
        let d_end = (tb.last_max_descent - descent).max(0.0);
        let d_start = if is_stretch {
            (cell_sz - (item_sz + d_end)).max(0.0)
        } else {
            0.0
        };
        (d_start, d_end)
    };

    if delta_start == 0.0 && delta_end == 0.0 {
        return (None, 0.0, 0.0);
    }

    let mut new_css = item_css.clone();
    let add_side = |s: &Side, delta: f32| -> Side {
        let cur = s.resolve(&rc).unwrap_or(0.0);
        Side::px_len(cur + delta)
    };

    if is_ortho_frame {
        new_css.padding.top = add_side(&new_css.padding.top, delta_start);
        new_css.padding.bottom = add_side(&new_css.padding.bottom, delta_end);
        (Some(new_css), delta_start + delta_end, 0.0)
    } else if is_in_frame || child_wm.is_horizontal() {
        new_css.padding.top = add_side(&new_css.padding.top, delta_start);
        new_css.padding.bottom = add_side(&new_css.padding.bottom, delta_end);
        (Some(new_css), 0.0, delta_start + delta_end)
    } else if child_wm == WritingMode::VerticalLr || child_wm == WritingMode::SidewaysLr {
        new_css.padding.left = add_side(&new_css.padding.left, delta_start);
        new_css.padding.right = add_side(&new_css.padding.right, delta_end);
        (Some(new_css), delta_start + delta_end, 0.0)
    } else {
        new_css.padding.right = add_side(&new_css.padding.right, delta_start);
        new_css.padding.left = add_side(&new_css.padding.left, delta_end);
        (Some(new_css), delta_start + delta_end, 0.0)
    }
}

pub(in crate::layout) fn resolve_gap(
    d: Option<crate::style::Dimension>,
    axis_size: Option<f32>,
    resolve: &crate::layout::ResolveCtx,
) -> f32 {
    let Some(d) = d else { return 0.0; };
    match d {
        crate::style::Dimension::Px(px) => px.max(0.0),
        crate::style::Dimension::Percent(pct) => {
            axis_size.map_or(0.0, |sz| (sz * pct / 100.0).max(0.0))
        }
        crate::style::Dimension::Calc(c) => {
            let basis = axis_size.unwrap_or(0.0);
            let pct_val = basis * c.pct / 100.0;
            let v = c.px
                + pct_val
                + resolve.node_font_size * c.em
                + resolve.root_font_size * c.rem
                + resolve.viewport_w * c.vw / 100.0
                + resolve.viewport_h * c.vh / 100.0;
            v.max(0.0)
        }
        other => other.resolve(resolve).unwrap_or(0.0).max(0.0),
    }
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
