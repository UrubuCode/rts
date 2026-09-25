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
//! Percentage insets resolve against the TABLE's own content box for the
//! WIDTH axis (`avail_w`, the table's `content_w` — a table always sizes its
//! own width before laying out any row, so it is always definite) but only
//! CONDITIONALLY for the height axis: CSS 2.1 §9.3.2 / CSS Positioned Layout 3
//! §3.1 resolve a percentage `top`/`bottom` against the containing block's
//! height only when that height is DEFINITE — the table's own explicitly
//! specified `height` (or its `aspect-ratio`/flex-forced equivalent), never
//! the auto result of stacking the rows. `avail_h` is therefore
//! `Option<f32>` here, the SAME shape `layout/posicionado.rs::resolve_height`
//! already uses for an ordinary block's `%` height against its parent
//! (`None` = parent height auto → the percentage computes to `auto`, i.e. no
//! offset): the caller in `table/mod.rs` passes exactly the
//! `explicit_content_h` it computes for the table's own children, and `None`
//! when the table's height is itself auto.

use crate::boxes::BoxId;
use crate::layout::{DisplayItem, DisplayList, LayoutCtx, Rect, aplica_offset_relativo};
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
    avail_h: Option<f32>,
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
    avail_h: Option<f32>,
    font_size: f32,
    desde: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    aplica_offset_relativo(caixa, css, avail_w, avail_h, font_size, desde, ctx, list);
}

/// Pinta fundo e borda de uma caixa que não passou pelo `layout_block` (linha
/// ou grupo de linhas), inserindo os itens em `at` para ficarem ATRÁS do que já
/// lá está. Sem isto, um `<tr>` com `background` não pintava nada: a linha
/// nunca é um bloco, e era o `layout_block` que fazia esta parte para todos os
/// outros. Vive aqui (em vez de `table/mod.rs`, que a chama) para manter esse
/// módulo dentro do teto de 500 linhas — não é uma questão de offset relativo,
/// mas é a caixa de uma linha/grupo, o mesmo par que este módulo já trata.
pub(super) fn pinta_caixa(
    dom: &Dom,
    id: NodeIdx,
    rect: Rect,
    at: usize,
    list: &mut DisplayList,
) {
    let Some(css) = dom.computed_style_idx(id) else {
        return;
    };
    if !css.has_box() {
        return;
    }
    let radius = css.corner_radius.unwrap_or(0.0);
    let mut em = Vec::new();
    if let Some(bg) = css.bg {
        em.push(DisplayItem::SolidRect {
            rect,
            color: bg,
            radius: crate::layout::Corners::from_style(&css, 0.0),
        });
    }
    em.extend(crate::layout::border_items(
        &css,
        rect,
        radius,
        1.0,
        // A borda de uma célula respeita o `filter` dela como a de qualquer
        // outra caixa; passar a identidade aqui faria a mesma folha pintar
        // diferente consoante o elemento fosse ou não uma célula de tabela.
        crate::painteffects::filtro(css.filter.as_deref().unwrap_or("")),
    ));
    for (i, item) in em.into_iter().enumerate() {
        list.pieces.insert(at + i, crate::layout::Piece::Item(item));
    }
}
