//! The used border-width for a form control's own scalar frame reservation
//! (`layout/input.rs::medida_do_input`), kept in its own module so that file
//! stays under the workspace's 500-line ceiling (`CLAUDE.md`) — it was 503
//! with this logic inline.

use crate::style::values::BorderStyle;

/// `border-style: none` (or `hidden`) makes the USED border-width 0
/// regardless of any declared `border-width` (CSS2.1 §border-width) —
/// `paint`'s own `style::borders::resolved_sides` already honours this per
/// side; `medida_do_input`'s scalar reservation did not, so a `<textarea>`
/// styled with `border: none` (an explicit author override, as WPT's
/// `css-text/white-space/textarea-pre-wrap-*` fixtures all declare) still
/// reserved the UA sheet's 2px per side (`input:not(...), textarea {
/// border-width: 2px; }` in `style/ua.css`) in the box the flow allots.
///
/// `estilo: None` (the property was never declared by anyone) answers
/// visible — the UA sheet always sets `border-style: inset` (parsed as
/// `Solid`) for a real text `<input>`/`<textarea>`, so a `None` here only
/// happens for controls this frame is never used on (`sem_moldura`'s
/// `borda_ua` is already 0 for those), and defaulting to invisible would
/// wrongly zero the frame the day that stops being true.
pub(in crate::layout) fn largura_borda_usada(
    declarada: Option<f32>,
    padrao_ua: f32,
    estilo: Option<BorderStyle>,
) -> f32 {
    let visivel = estilo.map(BorderStyle::is_visible).unwrap_or(true);
    if visivel {
        declarada.unwrap_or(padrao_ua).max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estilo_none_zera_a_largura_declarada_e_o_padrao() {
        assert_eq!(largura_borda_usada(Some(5.0), 2.0, Some(BorderStyle::None)), 0.0);
        assert_eq!(largura_borda_usada(None, 2.0, Some(BorderStyle::None)), 0.0);
    }

    #[test]
    fn estilo_hidden_tambem_zera() {
        assert_eq!(largura_borda_usada(Some(5.0), 2.0, Some(BorderStyle::Hidden)), 0.0);
    }

    #[test]
    fn estilo_visivel_usa_a_declarada_ou_o_padrao() {
        assert_eq!(largura_borda_usada(Some(5.0), 2.0, Some(BorderStyle::Solid)), 5.0);
        assert_eq!(largura_borda_usada(None, 2.0, Some(BorderStyle::Solid)), 2.0);
    }

    #[test]
    fn estilo_ausente_e_visivel_por_omissao() {
        assert_eq!(largura_borda_usada(None, 2.0, None), 2.0);
    }
}
