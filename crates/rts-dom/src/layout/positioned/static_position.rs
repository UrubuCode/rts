//! A STATIC POSITION de um `position:absolute`/`fixed` **sem nenhum inset**
//! num eixo (`top`/`left`/`right`/`bottom` todos ausentes nesse eixo, CSS 2.1
//! §10.3.7/§10.6.4): a posição que a caixa teria SE estivesse em fluxo normal.
//!
//! Hoje `posicionado.rs::layout_out_of_flow` cai na origem do CONTAINING BLOCK
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
//!   `align-self`/`align-items` (cross axis). `grid` still follows the block
//!   path because there is no fixture requiring it yet.
//!
//! Cortes documentados: a margem PRÓPRIA de `id` não entra na conta (todos os
//! casos medidos usam margem 0, o default); `space-between`/`space-around`/
//! `space-evenly` degradam para `flex-start` (um único item hipotético não tem
//! contra quem se distribuir).

use super::*;

/// A posição estática de `id`, em coordenadas ABSOLUTAS de página — a mesma
/// origem de `flow_rects`. `layout_out_of_flow` só lê o eixo que precisar (o
/// outro já veio de um `top`/`left`/`right`/`bottom` declarado).
pub(in crate::layout) fn posicao_estatica(
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
    // it would have been, which no sibling's rectangle can say (`ancora_estatica.rs`).
    if let Some(ancora) = flow_rects.get(&id) {
        return (ancora.x, ancora.y);
    }
    let Some(parent) = dom.node(id).parent else {
        return (0.0, 0.0);
    };
    let parent_css = dom.computed_style_idx(parent).unwrap_or_default();
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
        return posicao_estatica_flex(css, &parent_css, content, outer_w, outer_h, containing_block);
    }
    posicao_estatica_bloco(dom, id, parent, content, flow_rects)
}

/// Caso do contentor de bloco normal: o próximo irmão em fluxo já está onde
/// `id` estaria. Sem um seguinte, o fim do anterior; sem nenhum, o topo do
/// content — o `x` é sempre o do content (block-level começa à esquerda).
fn posicao_estatica_bloco(
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
    let rect_em_fluxo = |&s: &NodeIdx| {
        (!super::positioned::e_display_none(dom, s) && !super::positioned::is_out_of_flow(dom, s))
            .then(|| flow_rects.get(&s).copied())
            .flatten()
    };
    let y = siblings[i + 1..]
        .iter()
        .find_map(rect_em_fluxo)
        .map(|r| r.y)
        .or_else(|| {
            siblings[..i]
                .iter()
                .rev()
                .find_map(rect_em_fluxo)
                .map(|r| r.y + r.h)
        })
        .unwrap_or(content.y);
    (content.x, y)
}

/// Flex-container case (Flexbox §4.1): position the measured box using
/// `justify-content`/`align-self`. The physical axis has already been resolved
/// for `row-reverse`/`column-reverse` by the same mapping used by `coluna.rs`.
fn posicao_estatica_flex(
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
    let justify = crate::layout::flex::axes::fisico_para_eixo(
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

// Testes de comportamento (Dom real, via `layout()`) ficam em
// `layout/tests/posicao_estatica_corpus.rs` — os dois casos de bloco normal
// (com/sem irmão em fluxo) precisam de uma árvore real para exercitar a
// procura de irmãos. O que segue é só a matemática PURA de
// `posicao_estatica_flex`, que não precisa de `Dom` nenhum.
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
        let (x, y) = posicao_estatica_flex(&css, &parent_css, content, 40.0, 20.0, content);
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
        let (x, y) = posicao_estatica_flex(&css, &parent_css, content, 40.0, 20.0, content);
        assert_eq!(x, 80.0, "align-items:center on the horizontal cross axis");
        assert_eq!(y, 80.0, "justify-content:flex-end on the vertical main axis");
    }

    #[test]
    fn safe_end_uses_end_when_the_item_fits() {
        let mut css = ComputedStyle::default();
        css.align_self = Some(crate::style::AlignItems::SafeEnd);
        let parent_css = ComputedStyle::default();
        let content = Rect::new(0.0, 0.0, 100.0, 100.0);
        let (x, y) = posicao_estatica_flex(&css, &parent_css, content, 20.0, 30.0, content);
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
        let (x, _y) = posicao_estatica_flex(&css, &parent_css, content, 100.0, 20.0, containing_block);
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
        let (x, _y) = posicao_estatica_flex(&css, &parent_css, content, 40.0, 20.0, containing_block);
        assert_eq!(x, 5.0, "0 + (50-40)/2, fits inside the CB");
    }

    #[test]
    fn one_abspos_item_uses_distribution_fallbacks() {
        let css = ComputedStyle::default();
        let content = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_css = ComputedStyle::default();
        parent_css.justify = Some(crate::style::JustifyContent::SpaceAround);
        assert_eq!(posicao_estatica_flex(&css, &parent_css, content, 20.0, 20.0, content).0, 40.0);
        parent_css.justify = Some(crate::style::JustifyContent::SpaceBetween);
        assert_eq!(posicao_estatica_flex(&css, &parent_css, content, 20.0, 20.0, content).0, 0.0);
    }
}
