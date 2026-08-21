//! O que o `getComputedStyle` responde para este lote
//!
//! Extraído de `vocab.rs` sem alterar uma linha.

use super::*;

/// O valor de uma propriedade deste lote tal como o elemento a DECLAROU. `None`
/// = o nome não é deste lote; `Some("")` = é, mas não foi declarada.
///
/// Vazio, e não o valor inicial, porque este é o mesmo caminho que serve
/// `el.style.x` — que responde `""` para o que não está no `style=""` daquele
/// elemento. Quem cai no inicial é `computed_value`, contra a tabela de
/// `style::initial`, que é onde os iniciais vivem TODOS. Uma primeira versão
/// respondia o inicial aqui e teria feito `el.style.objectFit` responder `fill`
/// em todo o elemento do documento — o mesmo erro que o cabeçalho de
/// `style::initial` documenta.
pub fn get_property(css: &ComputedStyle, name: &str) -> Option<String> {
    let s = match name {
        "text-overflow" => opt(css.text_overflow.map(|v| v.css())),
        "clip" => css.clip.map(|v| v.css()).unwrap_or_default(),
        "text-wrap" => opt(css.text_wrap.map(|v| v.css())),
        "object-fit" => opt(css.object_fit.map(|v| v.css())),
        "unicode-bidi" => opt(css.unicode_bidi.map(|v| v.css())),
        "hyphens" => opt(css.hyphens.map(|v| v.css())),
        "scrollbar-width" => opt(css.scrollbar_width.map(|v| v.css())),
        "caption-side" => opt(css.caption_side.map(|v| v.css())),
        "pointer-events" => opt(css.pointer_events.map(|v| v.css())),
        "transform-origin" => css
            .transform_origin
            .map(|p| {
                format!(
                    "{} {}",
                    super::fmt_values::fmt_dim(p.x),
                    super::fmt_values::fmt_dim(p.y)
                )
            })
            .unwrap_or_default(),
        "text-decoration-color" => css
            .text_decoration_color
            .map(super::fmt_values::fmt_color)
            .unwrap_or_default(),
        // O computado de `font-stretch` é a PERCENTAGEM, mesmo quando o autor
        // escreveu o keyword — é o que o Chrome responde.
        "font-stretch" => css
            .font_stretch
            .map(|v| format!("{v}%"))
            .unwrap_or_default(),
        "zoom" => css.zoom.map(|v| format!("{v}")).unwrap_or_default(),
        "word-spacing" => css
            .word_spacing
            .map(|v| format!("{v}px"))
            .unwrap_or_default(),
        "-webkit-line-clamp" | "line-clamp" => {
            css.line_clamp.map(|n| n.to_string()).unwrap_or_default()
        }
        "column-width" => css
            .column_width
            .map(super::fmt_values::fmt_dim)
            .unwrap_or_default(),
        "object-position" => css
            .object_position
            .map(|p| {
                format!(
                    "{} {}",
                    super::fmt_values::fmt_dim(p.x),
                    super::fmt_values::fmt_dim(p.y)
                )
            })
            .unwrap_or_default(),
        _ => return None,
    };
    Some(s)
}

/// Um keyword declarado → a sua string; não declarado → `""`.
fn opt(v: Option<&'static str>) -> String {
    v.unwrap_or_default().to_string()
}
