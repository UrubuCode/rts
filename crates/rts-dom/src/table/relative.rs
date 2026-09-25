//! `position: relative` on table-internal boxes — row and row-group.
//!
//! CSS Positioned Layout 3 §3.1: `position: relative` applies to
//! `table-row-group`, `table-header-group`, `table-footer-group`,
//! `table-row`, `table-cell` and `table-caption` (never to `table-column` or
//! `table-column-group`). `<td>` and `<caption>` already get this for free —
//! both go through the ordinary `layout_block` (`lay_out_grid` calls it for
//! every cell and every `<caption>`/stray block), and `layout_block` already
//! calls `aplica_offset_relativo` on its own box once it has positioned it.
//!
//! A `<tr>`/`<tbody>`/`<thead>`/`<tfoot>` never goes through `layout_block` —
//! `lay_out_grid` measures its rect directly (from the rows/columns it
//! already laid out) and paints it with `pinta_caixa`, so nothing ever asked
//! whether its `position` was `Relative`. This module is that missing ask. It
//! reuses `layout/relativo.rs::aplica_offset_relativo` rather than writing a
//! second offset routine — CLAUDE.md's "one source, generated views" applies
//! to a piece of LOGIC computing `(dx, dy)` just as much as to a data table,
//! and two routines answering the same question is exactly the drift that
//! rule exists to forbid.
//!
//! Percentage insets resolve against the TABLE's own content box
//! (`avail_w`/`avail_h`, passed in by the caller as the table's `content_w`
//! and its total content height) rather than the row's own box: a table row
//! has no containing block of its own the ordinary way a block does — its
//! "size" is a side effect of the grid, not something resolved before its
//! children are measured — so the nearest ancestor with a settled size is the
//! table.

use crate::boxes::BoxId;
use crate::layout::{DisplayList, LayoutCtx, aplica_offset_relativo};
use crate::style::ComputedStyle;
use crate::{Dom, NodeIdx};

/// Applies `position: relative` to a row or row-group whose content (its own
/// background/border plus every cell inside it) was emitted into
/// `list.pieces` starting at `desde`. A no-op when the node has no computed
/// style, or when `css.position` isn't `Relative`, or the resolved offset is
/// `(0, 0)` — `aplica_offset_relativo` itself already skips the walk in that
/// last case.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_table_part_relative_offset(
    dom: &Dom,
    node: NodeIdx,
    caixa: BoxId,
    avail_w: f32,
    avail_h: f32,
    font_size: f32,
    desde: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let Some(css) = dom.computed_style_idx(node) else {
        return;
    };
    aplica_offset(&css, caixa, avail_w, avail_h, font_size, desde, ctx, list);
}

fn aplica_offset(
    css: &ComputedStyle,
    caixa: BoxId,
    avail_w: f32,
    avail_h: f32,
    font_size: f32,
    desde: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    aplica_offset_relativo(caixa, css, avail_w, Some(avail_h), font_size, desde, ctx, list);
}
