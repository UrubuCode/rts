//! The record one item of a WRAPPED column carries between the passes of
//! `column_wrap.rs`: its main size (grows and shrinks per column), its
//! natural cross size, its flex factors and its `auto` main-axis margins.
//!
//! Apart from `column_wrap.rs` because it answers "what is measured about an
//! item before the columns exist", where that file answers "how the columns
//! are formed and placed". Rejected: putting it beside `FlexItem` in
//! `row.rs` or `ColItem` in `column.rs` — the three records answer the same
//! question for three directions, and this lot moves rather than unifies.

use super::*;

/// Um item pré-medido: `main` é a altura outer (cresce/encolhe por coluna,
/// mesmo papel de `ColItem::h` em `column.rs`); `cross` é a largura NATURAL
/// (shrink-to-fit — para um item com `width` explícito é essa largura, sem
/// depender de quantas colunas existem: por isso pode ser medida ANTES de se
/// saber a largura da coluna).
pub(super) struct Item {
    pub(super) node: NodeIdx,
    pub(super) box_id: crate::boxes::BoxId,
    pub(super) main: f32,
    pub(super) cross: f32,
    pub(super) is_text: bool,
    pub(super) grow: f32,
    pub(super) shrink: f32,
    pub(super) min_main: f32,
    pub(super) align_self: Option<crate::style::AlignItems>,
    /// só estica no eixo cruzado quando a LARGURA não é explícita — o mesmo
    /// "`can_stretch`" de `FlexItem` em `row.rs`, só que no eixo trocado.
    pub(super) can_stretch: bool,
    pub(super) order: i32,
    /// margens `auto` no eixo PRINCIPAL (vertical) — mesma leitura de
    /// `ColItem::mt_auto`/`mb_auto` em `column.rs`, que faltava aqui (corte
    /// dito no cabeçalho do módulo até este lote): uma margem `auto` vence o
    /// `justify-content` da COLUNA que a contém (spec §8.1) — sem isto, um
    /// item de `margin-bottom:auto` numa coluna de `flex-wrap` recebia o
    /// mesmo offset `space-around`/`space-between` das colunas SEM margem
    /// `auto`, em vez de ficar encostado ao início com o livre absorvido no
    /// fim (`flexbox-column-row-gap-001`, WPT).
    pub(super) mt_auto: bool,
    pub(super) mb_auto: bool,
}
