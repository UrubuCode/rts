//! Colour, opacity, italic and text-decoration as a painted item carries them.
//!
//! Moved from `layout/pintura.rs` on 2026-09-25 (PQ-A1); nothing in it changed.

use crate::style::ComputedStyle;

/// Código de decoração de texto p/ o `DisplayItem::Text` a partir do estilo:
/// 0=nenhuma, 1=underline, 2=line-through, 3=overline.
pub(crate) fn decoration_code(css: &ComputedStyle) -> u8 {
    match css.text_decoration {
        Some(crate::style::values::TextDecoration::Underline) => 1,
        Some(crate::style::values::TextDecoration::LineThrough) => 2,
        Some(crate::style::values::TextDecoration::Overline) => 3,
        _ => 0,
    }
}

/// Multiplica o ALPHA de uma cor `0xRRGGBBAA` por `opacity` ∈ [0,1] (o RGB fica
/// A cor com que um elemento pinta, dado o seu `visibility`.
///
/// `visibility:hidden` não salta o layout — o elemento ocupa o espaço na mesma —,
/// só não é pintado. Zerar o alpha é como isso se exprime numa display list que
/// não tem grupos de compositing, e a propriedade ser HERDADA faz o resto: os
/// descendentes chegam ao seu próprio layout já com ela posta.
/// `font-style: italic` resolvido para uma tag: o CSS computado vence, e a
/// UA-stylesheet responde quando ninguém declarou; `herdado` é o último recurso.
///
/// A consulta à UA passa por [`crate::block::lookup_inline`] — a TABELA que
/// regista `<i>` e `<em>` como `FLAG_ITALIC` — e não por um `match` sobre o
/// nome da tag. A alternativa rejeitada era exatamente esse `match`: o motor de
/// layout não nomeia tags HTML, a UA-stylesheet é que as nomeia, e é ela quem
/// muda quando o default de uma tag muda.
///
/// Sem este ramo, um `<em>` sem regra de autor não fica itálico nenhum — o mapa
/// da UA não tinha, até aqui, UM ÚNICO leitor em todo o motor.
pub(crate) fn italico(css: Option<&crate::style::ComputedStyle>, tag: Option<&str>, herdado: bool) -> bool {
    if let Some(v) = css.and_then(|c| c.italic) {
        return v;
    }
    let ua = tag.is_some_and(|t| crate::block::lookup_inline(t) & crate::block::FLAG_ITALIC != 0);
    ua || herdado
}

pub(crate) fn cor_visivel(css: &crate::style::ComputedStyle, cor: u32) -> u32 {
    if css.visibility.is_some_and(|v| v.suppresses_paint()) {
        cor & 0xFFFF_FF00
    } else {
        cor
    }
}

/// intacto; só o canal alpha escala). `opacity >= 1` devolve a cor inalterada.
pub(crate) fn apply_opacity(color: u32, opacity: f32) -> u32 {
    if opacity >= 1.0 {
        return color;
    }
    let op = opacity.clamp(0.0, 1.0);
    let a = (color & 0xFF) as f32;
    let new_a = (a * op).round().clamp(0.0, 255.0) as u32;
    (color & 0xFFFF_FF00) | new_a
}

