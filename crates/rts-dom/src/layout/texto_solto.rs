//! Um nó de TEXTO que chega sozinho ao fluxo de blocos — o `abc` de
//! `<div style="display:table">abc</div>` (a célula anónima da grelha é o
//! próprio nó de texto, `table/grid.rs`), ou texto directo na raiz.
//!
//! `bloco.rs` pintava-o com a fonte por omissão (16px, altura de linha do
//! medidor) e ignorava o pai: numa página com `font: 16px/20px` a linha saía
//! com 18 e não 20, e a cor/negrito/família do pai perdiam-se. O texto solto
//! HERDA — é o pai que diz a fonte — e é isso que aqui se lê
//! (`claude-table-texto-solto-sem-celula`: a `display:table` com texto mede
//! os mesmos 20px que o bloco irmão).
//!
//! Vive à parte porque `bloco.rs` já passa o tecto das 500 linhas.

use super::*;

/// Pinta o texto em `(x, y)` com o estilo do pai e devolve `(largura, altura)`
/// da linha. Texto só de whitespace não cria linha nenhuma: é o espaço entre
/// dois `<tr>`, não conteúdo.
pub(super) fn layout_texto_solto(dom: &Dom, id: NodeIdx, t: &str, x: f32, y: f32, ctx: &LayoutCtx, list: &mut DisplayList) -> (f32, f32) {
    if t.trim().is_empty() {
        return (0.0, 0.0);
    }
    let pai = dom.node(id).parent.and_then(|p| dom.computed_style_idx(p)).unwrap_or_default();
    let size = crate::layout::font_px(&pai, DEFAULT_FONT_SIZE);
    let bold = pai.bold.unwrap_or(false);
    let mono = pai.font_family.as_deref().map(crate::style::is_mono_family).unwrap_or(false);
    let lh = crate::inline_box::altura_da_linha(&pai, size, ctx.measurer);
    let tw = ctx.measurer.text_width(t, size, bold, false, mono);
    list.items.push(DisplayItem::Text {
        x,
        y,
        text: t.into(),
        color: pai.color.unwrap_or(0x000000FF),
        size,
        mono,
        bold,
        italic: false,
        letter_spacing: pai.letter_spacing.unwrap_or(0.0),
        decoration: 0,
    });
    (tw, lh)
}
