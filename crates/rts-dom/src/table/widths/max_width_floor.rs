//! `d` (`width`/`max-width` absoluto) em caixa OUTER — extraído de `mod.rs`
//! (teto de 500) para o `min_content` de lá poder clampar o PISO por
//! `max-width` como já clampa o max-content em `intrinsic_outer_width`
//! (`medida.rs`) — WPT `float-non-replaced-width-008..012`, invariante
//! min-content ⩽ max-content.

use crate::style::{ComputedStyle, ResolveCtx};

pub(in crate::table::widths) fn comprimento_outer(
    d: Option<crate::style::Dimension>,
    css: &ComputedStyle,
    resolve: &ResolveCtx,
    frame: f32,
) -> Option<f32> {
    crate::style::dimensao_absoluta(d?, resolve)
        .map(|w| w + if css.border_box.unwrap_or(false) { 0.0 } else { frame })
}
