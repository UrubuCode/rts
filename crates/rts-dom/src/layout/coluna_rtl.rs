//! `direction:rtl` no eixo CRUZADO de uma flex-column.
//!
//! Numa `flex-direction:column`, o eixo cruzado (largura, X) É o eixo INLINE
//! (CSS Flexbox §4.1 + Writing Modes) — e `direction` é exatamente o que
//! decide qual borda física é o INÍCIO desse eixo. `layout_children_column`
//! posicionava sempre a partir da borda ESQUERDA física, ignorando
//! `css.direction`: um item não esticado (largura declarada) em RTL saía
//! encostado à esquerda onde o Chrome encosta à direita.
//!
//! Extraído de `coluna.rs` (que já está perto do teto de 500 linhas) em vez
//! de crescer lá — mesma razão de `coluna_shrink.rs` ao lado. Achado em
//! `claude-flex-column-rtl-cross-start` (WPT `flexbox_rtl-direction`).

/// Espelha uma posição X calculada em LTR para o lado físico certo quando
/// `direction:rtl` — reflecte a caixa `[x, x+w]` dentro do content-box
/// `[content_x, content_x+content_w]`.
///
/// Um item que ocupa a largura TODA do content-box (`w == content_w`, o
/// stretch de verdade) fica no mesmo sítio nos dois sentidos — não há folga
/// para espelhar — o que é o caso comum e a razão de isto ser seguro chamar
/// sempre, mesmo quando `direction` não é `rtl` (early-return: devolve `x`).
pub(in crate::layout) fn cross_x(
    direction: Option<crate::style::Direction>,
    content_x: f32,
    content_w: f32,
    x: f32,
    w: f32,
) -> f32 {
    if matches!(direction, Some(crate::style::Direction::Rtl)) {
        content_x + (content_x + content_w) - (x + w)
    } else {
        x
    }
}
