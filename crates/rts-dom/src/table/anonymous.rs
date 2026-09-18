//! The ANONYMOUS table (CSS 2.1 §17.2.1, rule 3): the table box the box tree
//! wraps around table parts whose parent is not a table
//! (`boxes/build/tabela_anonima.rs`).
//!
//! It has no node, so it never reaches `layout_block`, which is where a table
//! ELEMENT gets its width. An anonymous box has no declarations of its own
//! (§17.2.1: its non-inherited properties are initial), so there is no `width`,
//! margin, border, padding or background to resolve — what is left is the
//! table's own rule for an auto width: shrink-to-fit, the max-content of its
//! columns capped at the available width and floored at their min-content.
//! That is all this module adds on top of the grid a real table already runs.

use super::*;
use crate::boxes::{BoxId, BoxTree};

/// The min-content and max-content widths of an anonymous table: its columns
/// plus the `border-spacing` gaps. What a parent that shrinks to fit (a float,
/// an inline-block) must see when it measures its content — the SUM of the
/// columns, where measuring the cells as stacked blocks answered the widest.
pub(crate) fn anonymous_table_widths(
    dom: &Dom,
    tree: &BoxTree,
    caixa: BoxId,
    font: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let g = collect(dom, tree, caixa);
    if g.cols == 0 {
        return (0.0, 0.0);
    }
    let css = tree.style(dom, caixa).unwrap_or_default();
    let ts = TableStyle::of(dom, None, &css, font, ctx);
    let cols = medir_colunas(dom, tree, &g, font, ctx, ts.spacing_h);
    let vaos = (g.cols + 1) as f32 * ts.spacing_h;
    let min = cols.iter().map(|c| c.min).sum::<f32>() + vaos;
    let max = cols.iter().map(|c| c.max).sum::<f32>() + vaos;
    (min, max.max(min))
}

/// Lays an anonymous table out at `(x, y)` in `avail_w`, and answers its
/// `(width, height)`. The width is shrink-to-fit; the box's rect is recorded
/// by `BoxId`, since it has no node for the DOM to ask about.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_anonymous_table(
    dom: &Dom,
    tree: &BoxTree,
    caixa: BoxId,
    x: f32,
    y: f32,
    avail_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let (min, max) = anonymous_table_widths(dom, tree, caixa, font_size, ctx);
    let w = max.min(avail_w).max(min);
    let css = tree.style(dom, caixa).unwrap_or_default();
    let ts = TableStyle::of(dom, None, &css, font_size, ctx);
    let g = collect(dom, tree, caixa);
    let h = lay_out_grid(dom, tree, &g, &ts, x, y, w, font_size, ctx, list);
    crate::layout::record_box_rect(list, caixa, Rect::new(x, y, w, h));
    (w, h)
}
