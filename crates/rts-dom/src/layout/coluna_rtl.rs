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
//! espelho só se aplicava quando o `writing-mode` computado era HORIZONTAL
//! — na altura, o motor não fazia layout de `writing-mode` vertical nenhum
//! (tratava tudo como horizontal), e espelhar o `direction:rtl` num
//! contentor que já não era disposto corretamente só divergia mais.
//!
//! **Lote `flex-writing-mode`**: agora que `writing-mode` troca de verdade
//! qual eixo físico é o principal (`eixos_flex.rs`), o eixo X CONTINUA a ser
//! o que este ficheiro espelha — só que quando `writing-mode` é vertical, X
//! deixou de ser o eixo inline (que `direction` decide) e passou a ser o
//! eixo de BLOCO, cujo sentido `direction` nunca toca: é `vertical-rl`/
//! `sideways-rl` (RTL) contra `vertical-lr`/`sideways-lr` (LTR) que decidem,
//! não o `direction` do contentor. `eixos_flex::eixo_x_invertido` é a
//! resposta única — o mesmo cálculo, direction OU writing-mode, consoante o
//! caso, que este ficheiro delegava a duas condições soltas antes.

/// Espelha uma posição X calculada em LTR para o lado físico certo quando o
/// eixo X corre invertido — `direction:rtl` num `writing-mode` horizontal,
/// ou `vertical-rl`/`sideways-rl` (que invertem X sozinhos, sem `direction`)
/// — reflectindo a caixa `[x, x+w]` dentro do content-box
/// `[content_x, content_x+content_w]`.
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
    let wm = writing_mode.unwrap_or_default();
    let dir = direction.unwrap_or_default();
    if super::eixos_flex::eixo_x_invertido(wm, dir) {
        content_x + (content_x + content_w) - (x + w)
    } else {
        x
    }
}
