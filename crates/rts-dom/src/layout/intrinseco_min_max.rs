//! `min-width`/`max-width` podem valer `min-content`/`max-content` (CSS
//! Sizing 3 §intrinsic-sizes) em vez de um comprimento — uma palavra-chave
//! que só a ÁRVORE resolve, nunca uma fórmula. `Dimension::resolve` não a
//! conhece (devolve `None`), e antes deste módulo só um sítio a entendia:
//! `flex_limites::limites_do_item`, e só para `MinContent`, e só na largura
//! do PRÓPRIO item flex. Um clamp de bloco comum (`bloco.rs`) descartava a
//! palavra por inteiro — `min-width: max-content` numa `<div>` qualquer
//! dentro de um item flex (não o item em si) não alargava nada
//! (`flex-item-content-is-min-width-max-content`, WPT).
//!
//! Módulo novo e não uma função a mais em `bloco.rs` (1266 linhas, teto de
//! 500, não cresce — PLAN.md §1) nem em `flex_limites.rs`: o primeiro ganha
//! só a CHAMADA, o segundo troca o seu próprio `match` inline por esta.

use super::*;

/// Resolve um `min-width`/`max-width` (já como `Option<Dimension>`) para um
/// comprimento: `min-content` vira o min-content REAL de `id`
/// (`crate::table::min_content`, o mesmo que o piso automático do
/// encolhimento já usa); `max-content` vira a largura intrínseca do
/// conteúdo (`content_natural_width`, `medida.rs`); qualquer outro valor
/// (comprimento, `%`, ausente) cai no `Dimension::resolve` de sempre.
pub(in crate::layout) fn resolve(
    d: Option<crate::style::Dimension>,
    dom: &Dom,
    id: NodeIdx,
    font_size: f32,
    ctx: &LayoutCtx,
    rc: &ResolveCtx,
) -> Option<f32> {
    match d {
        Some(crate::style::Dimension::MinContent) => {
            Some(crate::table::min_content(dom, id, font_size, ctx))
        }
        Some(crate::style::Dimension::MaxContent) => {
            Some(content_natural_width(dom, id, font_size, ctx))
        }
        other => other.and_then(|d| d.resolve(rc)),
    }
}
