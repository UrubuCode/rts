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
//!
//! RETRABALHO (lote `flex-justify-logico`, `overflow-top-left` do WPT): o
//! espelho só se aplica quando o `writing-mode` computado é HORIZONTAL
//! (`horizontal-tb`, o default) — um `writing-mode:vertical-rl` troca os
//! eixos de bloco/inline de verdade, e o motor não faz esse layout (trata
//! tudo como horizontal, corte de `WritingMode`); espelhar o `direction:rtl`
//! num contentor que já não é disposto corretamente só divergia mais da
//! referência (também tratada como bloco/horizontal pelo motor).

/// Espelha uma posição X calculada em LTR para o lado físico certo quando
/// `direction:rtl` E o `writing-mode` é horizontal — reflecte a caixa
/// `[x, x+w]` dentro do content-box `[content_x, content_x+content_w]`.
///
/// Um item que ocupa a largura TODA do content-box (`w == content_w`, o
/// stretch de verdade) fica no mesmo sítio nos dois sentidos — não há folga
/// para espelhar — o que é o caso comum e a razão de isto ser seguro chamar
/// sempre, mesmo quando `direction` não é `rtl` (early-return: devolve `x`).
/// `w` é a largura OUTER verdadeira do item (não grampeada ao `content_w`):
/// um item mais largo do que o contentor dá um espelho NEGATIVO, que é o
/// transbordo pela ESQUERDA que o RTL pede (`claude-rtl-filho-transborda`).
pub(in crate::layout) fn cross_x(
    direction: Option<crate::style::Direction>,
    writing_mode: Option<crate::style::WritingMode>,
    content_x: f32,
    content_w: f32,
    x: f32,
    w: f32,
) -> f32 {
    let rtl = matches!(direction, Some(crate::style::Direction::Rtl));
    let horizontal = writing_mode.unwrap_or_default().is_horizontal();
    if rtl && horizontal {
        content_x + (content_x + content_w) - (x + w)
    } else {
        x
    }
}
