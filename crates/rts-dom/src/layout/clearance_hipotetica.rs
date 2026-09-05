//! A hipotética de CSS 2.1 §9.5.2 — a posição que um bloco com `clear`
//! teria SEM a clearance — para decidir SE ela é mesmo precisa.
//!
//! `vertical.rs` já sabia comparar essa hipotética com o fundo do float; o
//! que faltava é que ela inclui a margem que atravessa para o PRIMEIRO FILHO
//! (§8.3.1), não só a margem própria do bloco (`m`, que é 0 sempre que quem
//! declara `clear` não tem margem própria). Comparar o fundo do float com uma
//! hipotética que só conhece `m` acionava clearance para um bloco que já
//! tinha passado o float pela margem do filho — e a corrigia SOMANDO o fundo
//! à margem do filho mais abaixo, em vez de reconhecer que a margem sozinha já
//! bastava (`margin-collapse-clear-015`, WPT: 28% de pixels antes deste
//! módulo).

use super::*;
use crate::layout::bloco::{collapse_margin, escaped_margins_for_box};
use crate::layout::vertical::{junta_ao_strut, strut_colapsado, Strut};

/// A aresta de topo final de um bloco com `clear`: `fundo` só substitui a
/// hipotética quando esta — JÁ com a margem do primeiro filho incluída — não
/// alcança o float. Do contrário devolve `None` e quem chama mantém o que já
/// tinha calculado com `m` sozinho (o cancelamento de `escaped_top_pre` em
/// `bloco.rs` produz o valor certo por conta própria).
pub(in crate::layout) fn aresta_com_clearance(
    dom: &Dom,
    child: NodeIdx,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
    borda: f32,
    strut: Strut,
    m: f32,
    fundo: f32,
) -> Option<f32> {
    let escapada = escaped_margins_for_box(dom, child, content_w, font_size, ctx).0;
    let m_hipotetico = collapse_margin(m, escapada);
    let hipotetica = borda + strut_colapsado(junta_ao_strut(strut, m_hipotetico));
    (fundo > hipotetica).then_some(fundo)
}
