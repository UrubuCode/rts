//! Engine de estilo CSS NATIVO (puro RTS) — FASE 0: parse do atributo inline
//! `style="..."` para propriedades de tipografia computadas. Nada de browser/
//! webview: é um motor próprio que lê CSS e produz valores que o `render_dom`
//! aplica direto nos widgets egui.
//!
//! Cobertura P0 (tipografia inline): `color`, `font-size` (px), `font-weight`
//! (bold/normal/numérico), `font-style` (italic). FASES seguintes (ver roadmap no
//! PR): `<style>` + seletores (tag/.class/#id) + cascata/herança; box model
//! (margin/padding/width/background); flexbox; grid; unidades %/em/vh.

use egui::Color32;

/// Propriedades de tipografia parseadas de um `style="..."`. Cada campo é
/// `Option` = "não especificado" → o `render_dom` mantém o valor herdado/default.
#[derive(Clone, Copy, Default)]
pub struct InlineCss {
    pub color: Option<Color32>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// Parseia uma lista de declarações `prop: valor; prop: valor`. Ignora
/// propriedades/valores desconhecidos (sem panicar) — robustez de parser real.
pub fn parse_inline(style: &str) -> InlineCss {
    let mut css = InlineCss::default();
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let (prop, val) = match (it.next(), it.next()) {
            (Some(p), Some(v)) => (p.trim().to_ascii_lowercase(), v.trim()),
            _ => continue,
        };
        match prop.as_str() {
            "color" => css.color = parse_color(val),
            "font-size" => css.size = parse_px(val),
            "font-weight" => css.bold = Some(is_bold(val)),
            "font-style" => {
                css.italic =
                    Some(val.eq_ignore_ascii_case("italic") || val.eq_ignore_ascii_case("oblique"))
            }
            _ => {}
        }
    }
    css
}

/// `font-size` em px (aceita "18px" ou "18"). Ignora unidades não-px por ora
/// (em/%/rem chegam na fase de unidades). Só valores > 0.
fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let num = v.strip_suffix("px").unwrap_or(v);
    num.trim().parse::<f32>().ok().filter(|n| *n > 0.0)
}

/// `font-weight`: `bold`/`bolder` ou peso numérico ≥ 600 → negrito.
fn is_bold(v: &str) -> bool {
    let v = v.trim();
    if v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") {
        return true;
    }
    v.parse::<u32>().map(|w| w >= 600).unwrap_or(false)
}

/// Parseia uma cor CSS: `#rgb`, `#rrggbb`, `rgb(r,g,b)` ou um nome básico.
pub fn parse_color(v: &str) -> Option<Color32> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let mut p = inner.split(',').map(|x| x.trim().parse::<u8>().ok());
        if let (Some(Some(r)), Some(Some(g)), Some(Some(b))) = (p.next(), p.next(), p.next()) {
            return Some(Color32::from_rgb(r, g, b));
        }
        return None;
    }
    named_color(v)
}

fn parse_hex(hex: &str) -> Option<Color32> {
    match hex.len() {
        // #rgb → expande cada nibble (f → ff).
        3 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let r = ((n >> 8) & 0xF) as u8;
            let g = ((n >> 4) & 0xF) as u8;
            let b = (n & 0xF) as u8;
            Some(Color32::from_rgb(r * 17, g * 17, b * 17))
        }
        6 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            Some(Color32::from_rgb(
                ((n >> 16) & 0xFF) as u8,
                ((n >> 8) & 0xFF) as u8,
                (n & 0xFF) as u8,
            ))
        }
        _ => None,
    }
}

fn named_color(v: &str) -> Option<Color32> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => Color32::BLACK,
        "white" => Color32::WHITE,
        "red" => Color32::RED,
        "green" => Color32::GREEN,
        "blue" => Color32::BLUE,
        "yellow" => Color32::YELLOW,
        "gray" | "grey" => Color32::GRAY,
        "lightgray" | "lightgrey" => Color32::LIGHT_GRAY,
        "darkgray" | "darkgrey" => Color32::DARK_GRAY,
        "orange" => Color32::from_rgb(255, 165, 0),
        "purple" => Color32::from_rgb(128, 0, 128),
        "cyan" => Color32::from_rgb(0, 255, 255),
        "magenta" => Color32::from_rgb(255, 0, 255),
        "transparent" => Color32::TRANSPARENT,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typography() {
        let c = parse_inline("color:#ff0000; font-size:18px; font-weight:bold; font-style:italic");
        assert_eq!(c.color, Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(c.size, Some(18.0));
        assert_eq!(c.bold, Some(true));
        assert_eq!(c.italic, Some(true));
    }

    #[test]
    fn color_forms() {
        assert_eq!(parse_color("#f00"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(parse_color("#00ff00"), Some(Color32::from_rgb(0, 255, 0)));
        assert_eq!(parse_color("rgb(10, 20, 30)"), Some(Color32::from_rgb(10, 20, 30)));
        assert_eq!(parse_color("blue"), Some(Color32::BLUE));
        assert_eq!(parse_color("nope"), None);
    }

    #[test]
    fn ignores_unknown() {
        let c = parse_inline("font-size:bogus; unknown:1; font-weight:300");
        assert_eq!(c.size, None);
        assert_eq!(c.bold, Some(false));
    }
}
