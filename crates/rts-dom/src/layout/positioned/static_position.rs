//! A STATIC POSITION de um `position:absolute`/`fixed` **sem nenhum inset**
//! num eixo (`top`/`left`/`right`/`bottom` todos ausentes nesse eixo, CSS 2.1
//! §10.3.7/§10.6.4): a posição que a caixa teria SE estivesse em fluxo normal.
//!
//! Hoje `positioned.rs::layout_out_of_flow` cai na origem do CONTAINING BLOCK
//! (`cb.x`/`cb.y`) quando os dois insets de um eixo faltam — errado sempre que
//! o nó não é o primeiro filho do seu contentor de fluxo, e sempre dentro de um
//! flex (Flexbox §4.1: a posição estática aí é alinhada por
//! `justify-content`/`align-self`, como se fosse o único item, não a origem
//! do contentor).
//!
//! Este módulo cobre dois casos, despachados pelo `display` do PAI de `id`:
//! - **contentor de bloco normal**: a caixa cairia onde o PRÓXIMO irmão em
//!   fluxo caiu de verdade — ele já ocupa esse lugar, porque um fora-de-fluxo
//!   não reserva espaço nenhum, e o irmão já foi layoutado com o colapso de
//!   margens correcto. Sem um seguinte, usa o fim do ANTERIOR; sem nenhum dos
//!   dois, o topo do content.
//! - **flex container** (row/column, including `wrap`): positions the box at
//!   its MEASURED SIZE, aligned by `justify-content` (main axis) and
//!   `align-self`/`align-items` (cross axis).
//! - **grid container**: `static_position_grid`, as the sole item of an area
//!   that is the container's content box (css-align-3 §4.4 / Grid §9.2).
//!
//! Cortes documentados: a margem PRÓPRIA de `id` não entra na conta (todos os
//! casos medidos usam margem 0, o default); `space-between`/`space-around`/
//! `space-evenly` degradam para `flex-start` (um único item hipotético não tem
//! contra quem se distribuir).

use super::*;

/// A posição estática de `id`, em coordenadas ABSOLUTAS de página — a mesma
/// origem de `flow_rects`. `layout_out_of_flow` só lê o eixo que precisar (o
/// outro já veio de um `top`/`left`/`right`/`bottom` declarado).
pub(in crate::layout) fn static_position_of(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
    ctx: &LayoutCtx,
    outer_w: f32,
    outer_h: f32,
    containing_block: Rect,
) -> (f32, f32) {
    // The box appeared in the middle of a LINE: the inline flow recorded where
    // it would have been, which no sibling's rectangle can say (`static_anchor.rs`).
    let Some(parent) = dom.node(id).parent else {
        return flow_rects.get(&id).map_or((0.0, 0.0), |a| (a.x, a.y));
    };
    if let Some(place) = boxless_inline_place(dom, parent, flow_rects, ctx) {
        return place;
    }
    let parent_css = dom.computed_style_idx(parent).unwrap_or_default();
    let rtl = parent_css.direction == Some(crate::style::Direction::Rtl);
    if let Some(anchor) = flow_rects.get(&id) {
        // A zero-width anchor is a point on a placed line. A wider one is the
        // free BAND of the line the box would have opened in the block flow
        // (`static_anchor::in_block_flow`): the line's `text-align` places a
        // zero-width box in it, and under rtl the box's RIGHT edge goes there
        // (§10.3.7). Cut, stated: a band squeezed to zero by floats reads as a
        // point.
        if anchor.w > 0.0 {
            let end = anchor.x + anchor.w;
            let at = match parent_css.text_align {
                Some(crate::style::TextAlign::Right) => end,
                Some(crate::style::TextAlign::Center) => anchor.x + anchor.w / 2.0,
                Some(_) => anchor.x,
                // The initial value is `start`: the right edge of an rtl line.
                None if rtl => end,
                None => anchor.x,
            };
            return (if rtl { at - outer_w } else { at }, anchor.y);
        }
        return (anchor.x, anchor.y);
    }
    let parent_box = flow_rects
        .get(&parent)
        .copied()
        .unwrap_or_else(|| Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h));
    let content = super::containing_block::content_box(parent_box, &parent_css, ctx);
    if matches!(
        parent_css.effective_display(),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex
                | crate::style::DisplayKind::InlineFlexWrap
        )
    ) {
        return static_position_flex(css, &parent_css, content, outer_w, outer_h, containing_block);
    }
    // Cuts, stated: a vertical grid (the axes would have to be swapped) and an
    // item placed on explicit lines (its area, not the content box, is the
    // alignment rect — Grid §11) still follow the block rule below; each moved
    // a WPT test from pass to fail when routed here.
    let horizontal = matches!(parent_css.writing_mode, None | Some(crate::style::WritingMode::HorizontalTb));
    let auto_lines = [css.grid_row_start, css.grid_row_end, css.grid_column_start, css.grid_column_end]
        .iter()
        .all(|l| matches!(l, None | Some(crate::style::grid_lines::GridLine::Auto)));
    if horizontal && auto_lines && parent_css.effective_display().is_some_and(|d| d.is_grid_container()) {
        return static_position_grid(css, &parent_css, content, outer_w, outer_h);
    }
    let (x, y) = static_position_block(dom, id, parent, content, flow_rects);
    // A block-level box starts at the START edge of its flow: the right one
    // under rtl (§10.3.7). This used to come out right only by accident, when
    // `rtl::used_margin_left` shifted the absolute box as if it were in flow.
    (if rtl { content.x + content.w - outer_w } else { x }, y)
}

/// Where an inline that generated NO box would start, with the relative offsets
/// of it and of every inline around it applied; `None` when `inline` has a
/// rect or is not an inline.
///
/// An inline whose only content is out-of-flow boxes and collapsible space
/// makes no line (`boxes::build::run_without_line_content`): its out-of-flow
/// children become children of the block container and nothing records a
/// rect for the inline. Asking `flow_rects` for it then fell back to the
/// VIEWPORT, and a `position: fixed`/`absolute` box inside it was drawn at
/// (0, 0) (`position-relative-003`, `nested-inline-abspos-child`). In flow it
/// would sit where the empty line starts — the container's content edge, below
/// the in-flow siblings before it — which is what `static_position_block`
/// answers for the inline's outermost box-less ancestor. The offsets are
/// resolved against that container (CSS 2.1 §10.1): its content width, and
/// its height only when declared, as for the inlines of a real line.
pub(in crate::layout) fn boxless_inline_place(
    dom: &Dom,
    inline: NodeIdx,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
    ctx: &LayoutCtx,
) -> Option<(f32, f32)> {
    let boxless_inline = |n: NodeIdx| {
        !flow_rects.contains_key(&n) && matches!(&dom.node(n).kind, NodeKind::Element { .. }) && !is_block_level(dom, n) && !is_inline_block(dom, n)
    };
    if !boxless_inline(inline) {
        return None;
    }
    let mut outermost = inline;
    let mut container = dom.node(inline).parent?;
    while boxless_inline(container) {
        outermost = container;
        container = dom.node(container).parent?;
    }
    let container_box = *flow_rects.get(&container)?;
    let container_css = dom.computed_style_idx(container).unwrap_or_default();
    let content = super::containing_block::content_box(container_box, &container_css, ctx);
    let (x, y) = static_position_block(dom, outermost, container, content, flow_rects);
    let cb_h = crate::inline_box::replaced_clamp::definite_cb_height(&container_css, Some(content.h));
    let (dx, dy) = super::relative::inline_offset(dom, Some(inline), content.w, cb_h, ctx);
    Some((x + dx, y + dy))
}

/// Caso do contentor de bloco normal: o próximo irmão em fluxo já está onde
/// `id` estaria. Sem um seguinte, o fim do anterior; sem nenhum, o topo do
/// content — o `x` é sempre o do content (block-level começa à esquerda).
fn static_position_block(
    dom: &Dom,
    id: NodeIdx,
    parent: NodeIdx,
    content: Rect,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
) -> (f32, f32) {
    let siblings = &dom.node(parent).children;
    let Some(i) = siblings.iter().position(|&c| c == id) else {
        return (content.x, content.y);
    };
    // Um nó de TEXTO (o espaço em branco entre tags, o caso comum) não tem
    // entrada própria em `flow_rects` — nunca é "o próximo irmão em fluxo"
    // para este efeito, mas também não deve PARAR a procura: `find_map`
    // continua para o irmão seguinte, ao contrário de `find` (que já tinha
    // parado no nó de texto, sem geometria, e caía sempre no fallback).
    let in_flow_rect = |&s: &NodeIdx| {
        (!super::positioned::is_display_none(dom, s) && !super::positioned::is_out_of_flow(dom, s))
            .then(|| flow_rects.get(&s).copied())
            .flatten()
    };
    let y = siblings[i + 1..]
        .iter()
        .find_map(in_flow_rect)
        .map(|r| r.y)
        .or_else(|| {
            siblings[..i]
                .iter()
                .rev()
                .find_map(in_flow_rect)
                .map(|r| r.y + r.h)
        })
        .unwrap_or(content.y);
    (content.x, y)
}

/// Flex-container case (Flexbox §4.1): position the measured box using
/// `justify-content`/`align-self`. The physical axis has already been resolved
/// for `row-reverse`/`column-reverse` by the same mapping used by `column.rs`.
fn static_position_flex(
    css: &ComputedStyle,
    parent_css: &ComputedStyle,
    content: Rect,
    outer_w: f32,
    outer_h: f32,
    containing_block: Rect,
) -> (f32, f32) {
    let fd = parent_css
        .flex_direction
        .unwrap_or(crate::style::FlexDirection::Row);
    let reverse = matches!(
        fd,
        crate::style::FlexDirection::RowReverse | crate::style::FlexDirection::ColumnReverse
    );
    let justify = crate::layout::flex::axes::physical_to_axis(
        parent_css
            .justify
            .unwrap_or(crate::style::JustifyContent::FlexStart),
        reverse,
        parent_css.direction.unwrap_or_default(),
    );
    let align = css
        .align_self
        .unwrap_or(parent_css.align_items.unwrap_or(crate::style::AlignItems::Stretch));
    let main = |start: f32, size: f32, item: f32| match justify {
        crate::style::JustifyContent::FlexEnd => start + size - item,
        crate::style::JustifyContent::Center
        | crate::style::JustifyContent::SpaceAround
        | crate::style::JustifyContent::SpaceEvenly => start + (size - item) / 2.0,
        _ => start,
    };
    // `safe`: fall back to the CB start when the ALIGNED POSITION overflows
    // the true containing block — not when the item is merely bigger than
    // the CB. The two diverge whenever the CB (`cb_start`/`cb_size`, from
    // the nearest positioned ancestor, css-align-3 §4.4 + the WPT
    // `flex-abspos-align-self-safe-outer-cb-*` fixtures) is wider than the
    // flex container itself (`start`/`size`, this item's immediate flex
    // parent): an item that fits inside the outer CB can still be centred
    // to a position outside it, because centring is computed against the
    // SMALLER flex container. Measured: CB width 200 at x=0, flex container
    // width 50 at x=0, item width 100, `align-self: safe center` in a
    // column flex — naive centre gives `x = 0 + (50-100)/2 = -25`, which is
    // `< cb_start(0)`, so it falls back to `cb_start`, even though
    // `item(100) <= cb_size(200)` (the old, wrong test) said it fit.
    let safe_fallback = |pos: f32, item: f32, cb_start: f32, cb_size: f32| {
        if pos < cb_start || pos + item > cb_start + cb_size {
            cb_start
        } else {
            pos
        }
    };
    let cross = |start: f32, size: f32, item: f32, cb_start: f32, cb_size: f32| match align {
        crate::style::AlignItems::FlexEnd | crate::style::AlignItems::LastBaseline => start + size - item,
        crate::style::AlignItems::SafeEnd => safe_fallback(start + size - item, item, cb_start, cb_size),
        crate::style::AlignItems::Center => start + (size - item) / 2.0,
        crate::style::AlignItems::SafeCenter => safe_fallback(start + (size - item) / 2.0, item, cb_start, cb_size),
        _ => start,
    };
    if fd.is_column() {
        (cross(content.x, content.w, outer_w, containing_block.x, containing_block.w), main(content.y, content.h, outer_h))
    } else {
        (main(content.x, content.w, outer_w), cross(content.y, content.h, outer_h, containing_block.y, containing_block.h))
    }
}

/// Grid-container case: the box is aligned as the SOLE item of a grid area
/// whose edges are the container's CONTENT box — the rect
/// `grid-abspos-staticpos-*` measures (the `-large-border-padding` refs put
/// the centred box at the content box's centre, not the padding box's). The
/// item's `align-self`/`justify-self` win over the container's
/// `align-items`/`justify-items`; `normal` (absent) and `stretch` behave as
/// `start`, because the box's size is already resolved by the abspos rules.
/// Under `direction: rtl` the inline axis runs from the right edge.
/// Rejected: the in-flow grid's `cell_align_offset` — it maps an absent value
/// to `stretch` and knows no rtl.
fn static_position_grid(css: &ComputedStyle, parent_css: &ComputedStyle, content: Rect, outer_w: f32, outer_h: f32) -> (f32, f32) {
    use crate::layout::flex::offsets::align_offset;
    use crate::style::AlignItems as A;
    let start_if_stretch = |a: Option<A>| match a.unwrap_or(A::FlexStart) {
        A::Stretch => A::FlexStart,
        a => a,
    };
    let align = start_if_stretch(css.align_self.or(parent_css.align_items));
    let justify = start_if_stretch(css.justify_self.or(parent_css.grid_justify_items));
    let y = content.y + align_offset(align, content.h, outer_h);
    let dx = align_offset(justify, content.w, outer_w);
    let x = if parent_css.direction == Some(crate::style::Direction::Rtl) {
        content.x + content.w - outer_w - dx
    } else {
        content.x + dx
    };
    (x, y)
}

// Testes de comportamento (Dom real, via `layout()`) ficam em
// `layout/tests/posicao_estatica_corpus.rs` — os dois casos de bloco normal
// (com/sem irmão em fluxo) precisam de uma árvore real para exercitar a
// procura de irmãos. O que segue é só a matemática PURA de
// `static_position_flex`, que não precisa de `Dom` nenhum.
#[cfg(test)]
mod tests {
    use super::*;

    /// Num flex `row` (default), sem inset nenhum: `justify-content:center` +
    /// `align-items:flex-end` alinha o item hipotético (tamanho zero) ao
    /// meio do eixo principal e ao fim do eixo cruzado — Flexbox §4.1.
    #[test]
    fn contentor_flex_alinha_pelo_justify_e_align() {
        let css = ComputedStyle::default();
        let mut parent_css = ComputedStyle::default();
        parent_css.justify = Some(crate::style::JustifyContent::Center);
        parent_css.align_items = Some(crate::style::AlignItems::FlexEnd);
        let content = Rect::new(10.0, 20.0, 200.0, 100.0);
        let (x, y) = static_position_flex(&css, &parent_css, content, 40.0, 20.0, content);
        assert_eq!(x, 10.0 + 80.0);
        assert_eq!(y, 20.0 + 80.0);
    }

    /// `flex-direction:column`: o eixo principal vira vertical (`justify`
    /// passa a mexer em `y`) e o cruzado horizontal (`align-items` em `x`).
    #[test]
    fn contentor_flex_column_troca_os_eixos() {
        let css = ComputedStyle::default();
        let mut parent_css = ComputedStyle::default();
        parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
        parent_css.justify = Some(crate::style::JustifyContent::FlexEnd);
        parent_css.align_items = Some(crate::style::AlignItems::Center);
        let content = Rect::new(0.0, 0.0, 200.0, 100.0);
        let (x, y) = static_position_flex(&css, &parent_css, content, 40.0, 20.0, content);
        assert_eq!(x, 80.0, "align-items:center on the horizontal cross axis");
        assert_eq!(y, 80.0, "justify-content:flex-end on the vertical main axis");
    }

    #[test]
    fn safe_end_uses_end_when_the_item_fits() {
        let mut css = ComputedStyle::default();
        css.align_self = Some(crate::style::AlignItems::SafeEnd);
        let parent_css = ComputedStyle::default();
        let content = Rect::new(0.0, 0.0, 100.0, 100.0);
        let (x, y) = static_position_flex(&css, &parent_css, content, 20.0, 30.0, content);
        assert_eq!((x, y), (0.0, 70.0));
    }

    /// Regression: the `safe` fallback was decided by `item.size > cb.size`,
    /// not by whether the ALIGNED POSITION overflows the CB. CB 200 wide at
    /// x=0, flex container (the item's `content` param) 50 wide at x=0, item
    /// 100 wide, `align-self: safe center` in a column flex: naive centring
    /// gives `x = 0 + (50-100)/2 = -25`, which is outside the CB
    /// (`< cb_start(0)`) — but `item(100) <= cb_size(200)` used to read as
    /// "fits", so the old code never fell back. css-align-3 §4.4 + WPT
    /// `flex-abspos-align-self-safe-outer-cb-*.tentative.html`.
    #[test]
    fn safe_center_falls_back_when_centred_position_overflows_the_outer_cb() {
        let mut css = ComputedStyle::default();
        css.align_self = Some(crate::style::AlignItems::SafeCenter);
        let mut parent_css = ComputedStyle::default();
        parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
        let content = Rect::new(0.0, 0.0, 50.0, 50.0); // the flex container itself
        let containing_block = Rect::new(0.0, 0.0, 200.0, 50.0); // the real CB, wider
        let (x, _y) = static_position_flex(&css, &parent_css, content, 100.0, 20.0, containing_block);
        assert_eq!(x, 0.0, "falls back to the CB start, not a negative centred offset");
    }

    /// Guard: the SAME centring still applies when it fits inside the outer
    /// CB (fallback must not fire unconditionally).
    #[test]
    fn safe_center_centers_when_it_fits_the_outer_cb() {
        let mut css = ComputedStyle::default();
        css.align_self = Some(crate::style::AlignItems::SafeCenter);
        let mut parent_css = ComputedStyle::default();
        parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
        let content = Rect::new(0.0, 0.0, 50.0, 50.0);
        let containing_block = Rect::new(0.0, 0.0, 200.0, 50.0);
        let (x, _y) = static_position_flex(&css, &parent_css, content, 40.0, 20.0, containing_block);
        assert_eq!(x, 5.0, "0 + (50-40)/2, fits inside the CB");
    }

    #[test]
    fn one_abspos_item_uses_distribution_fallbacks() {
        let css = ComputedStyle::default();
        let content = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_css = ComputedStyle::default();
        parent_css.justify = Some(crate::style::JustifyContent::SpaceAround);
        assert_eq!(static_position_flex(&css, &parent_css, content, 20.0, 20.0, content).0, 40.0);
        parent_css.justify = Some(crate::style::JustifyContent::SpaceBetween);
        assert_eq!(static_position_flex(&css, &parent_css, content, 20.0, 20.0, content).0, 0.0);
    }
}
