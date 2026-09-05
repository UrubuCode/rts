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
//! - **contentor flex** (row/column, `wrap` incluído): a posição de um item de
//!   tamanho ZERO alinhado por `justify-content` (eixo principal) e
//!   `align-self`/`align-items` (eixo cruzado) — `grid` cai no caminho de
//!   bloco por agora (corte dito: sem fixture a pedi-lo).
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
        return posicao_estatica_flex(css, &parent_css, content);
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

/// Caso do contentor flex (Flexbox §4.1): a posição de um item de tamanho
/// ZERO alinhado por `justify-content`/`align-self` — física, já resolvida
/// contra `row-reverse`/`column-reverse` pelo mesmo mapa que `coluna.rs` usa
/// para o eixo real.
fn posicao_estatica_flex(css: &ComputedStyle, parent_css: &ComputedStyle, content: Rect) -> (f32, f32) {
    let fd = parent_css
        .flex_direction
        .unwrap_or(crate::style::FlexDirection::Row);
    let reverse = matches!(
        fd,
        crate::style::FlexDirection::RowReverse | crate::style::FlexDirection::ColumnReverse
    );
    let justify = super::coluna::fisico_para_eixo(
        parent_css
            .justify
            .unwrap_or(crate::style::JustifyContent::FlexStart),
        reverse,
    );
    let align = css
        .align_self
        .unwrap_or(parent_css.align_items.unwrap_or(crate::style::AlignItems::Stretch));
    let main = |start: f32, size: f32| match justify {
        crate::style::JustifyContent::FlexEnd => start + size,
        crate::style::JustifyContent::Center => start + size / 2.0,
        _ => start,
    };
    let cross = |start: f32, size: f32| match align {
        crate::style::AlignItems::FlexEnd | crate::style::AlignItems::LastBaseline => start + size,
        crate::style::AlignItems::Center => start + size / 2.0,
        _ => start,
    };
    if fd.is_column() {
        (cross(content.x, content.w), main(content.y, content.h))
    } else {
        (main(content.x, content.w), cross(content.y, content.h))
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
        let (x, y) = posicao_estatica_flex(&css, &parent_css, content);
        assert_eq!(x, 10.0 + 100.0);
        assert_eq!(y, 20.0 + 100.0);
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
        let (x, y) = posicao_estatica_flex(&css, &parent_css, content);
        assert_eq!(x, 100.0, "align-items:center no eixo cruzado (horizontal)");
        assert_eq!(y, 100.0, "justify-content:flex-end no eixo principal (vertical)");
    }
}
