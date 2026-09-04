//! O valor que `bloco.rs` precisa ANTES de `content_y`, para o colapso de
//! margem PAI→PRIMEIRO-FILHO (CSS 2.1 §8.3.1).
//!
//! `escaped_child_margins` (`bloco.rs`) já calcula este número — mas só
//! DEPOIS de layoutar os filhos, porque também precisa do lado de BAIXO
//! (que depende de `bottom_auto_height`, só conhecido depois da altura
//! explícita ser resolvida). O `content_y` que os filhos recebem PARA SEREM
//! layoutados precisa só do lado de CIMA, e precisa dele MAIS CEDO — por
//! isso esta metade vive à parte, calculada uma segunda vez com os mesmos
//! critérios (sem borda/padding no topo, sem BFC), e não dentro de
//! `escaped_child_margins`.
//!
//! Alternativa rejeitada: mover TODO o corpo de `escaped_child_margins` para
//! antes do dispatch dos filhos, resolvendo `bottom_auto_height` mais cedo
//! também. Funcionaria, mas reordenar uma função de 830 linhas para mover dez
//! é o tipo de "movimento" que `bloco.rs` já recusa no seu próprio cabeçalho
//! — dividir por dentro deixa de ser mecânico. Uma função pequena, à parte,
//! que repete só a pergunta do lado de cima, custa dez linhas e zero risco
//! sobre o resto da função.

use super::*;
use super::bloco::{edge_margin_from_children, establishes_block_formatting_context};

/// A margem-top do primeiro filho de bloco de `id`, SE ela escapa através de
/// `id` (sem borda/padding no topo, sem contexto de formatação de bloco
/// próprio) — senão `0.0`. É o mesmo valor que `content_y` precisa somar (via
/// `collapse_margin`) para não contar a margem do filho a dobro.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn escapada_no_topo(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    content_w: f32,
    font_size: f32,
    pad_top: f32,
    border_top: f32,
    ctx: &LayoutCtx,
) -> f32 {
    if pad_top != 0.0 || border_top != 0.0 || establishes_block_formatting_context(dom, id, css) {
        return 0.0;
    }
    edge_margin_from_children(dom, id, content_w, font_size, ctx, false).unwrap_or(0.0)
}
