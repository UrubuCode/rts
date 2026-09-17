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
    let Some(parent) = dom.node(id).parent else {
        return (0.0, 0.0);
    };
    let parent_css = dom.computed_style_idx(parent).unwrap_or_default();
    let parent_box = flow_rects
        .get(&parent)
        .copied()
        .unwrap_or_else(|| Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h));
    let content = super::caixa_contentora::content_box(parent_box, &parent_css, ctx);
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
        (!super::posicionado::e_display_none(dom, s) && !super::posicionado::is_out_of_flow(dom, s))
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
    let justify = super::eixos_flex::fisico_para_eixo(
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
        crate::style::JustifyContent::Center => start + (size - item) / 2.0,
        _ => start,
    };
    let cross = |start: f32, size: f32, item: f32, cb_start: f32, cb_size: f32| match align {
        crate::style::AlignItems::FlexEnd | crate::style::AlignItems::LastBaseline => start + size - item,
        crate::style::AlignItems::Center => start + (size - item) / 2.0,
        crate::style::AlignItems::SafeCenter if item > cb_size => cb_start,
        crate::style::AlignItems::SafeCenter => start + (size - item) / 2.0,
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
}
