//! Engine de estilo CSS NATIVO (puro RTS) — EGUI-FREE.
//!
//! Tipos PRÓPRIOS, nunca tipos do egui (`Color32`/`FontId`/`Vec2`): a cor é um
//! `u32` RGBA (`0xRRGGBBAA`), o tamanho um `f32`. Isso é deliberado e é uma
//! condição de aceite do roadmap (F0(d)): se este módulo dependesse do egui, a
//! separação "o motor de estilo é independente do backend de render" viraria
//! mentira (cai o argumento anti-`rts-html`). A conversão para os tipos do egui
//! acontece NO RENDER (`frame/render.rs`), não aqui.
//!
//! Duas fontes de estilo, ambas produzindo o mesmo `ComputedStyle`:
//! - `parse_inline`: parse do atributo `style="..."` (CSS string). Cobertura P0:
//!   `color`, `font-size` (px), `font-weight`, `font-style`.
//! - `apply_slot`: aplicação de um SLOT NUMÉRICO OPACO (invariante 4 — o Rust
//!   nunca casa nome CSS; o TS mapeia nome→índice). Base do `defineStyle` (F1).

/// Cor RGBA empacotada `0xRRGGBBAA` num `u32`. Tipo próprio (não `Color32`).
pub type Rgba = u32;

/// Propriedades de estilo COMPUTADAS, com tipos próprios (egui-free). Cada campo
/// é `Option` = "não especificado" → o render mantém o valor herdado/default.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct ComputedStyle {
    /// Cor do texto, `0xRRGGBBAA`.
    pub color: Option<Rgba>,
    /// Cor de fundo, `0xRRGGBBAA`.
    pub bg: Option<Rgba>,
    /// Tamanho da fonte em pontos (> 0).
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    // ── Box model (F2) — pontos (f32). `None` = não especificado. ───────────────
    /// Espaço INTERNO entre a borda e o conteúdo (todos os lados).
    pub padding: Option<f32>,
    /// Espaço EXTERNO ao redor da caixa (todos os lados).
    pub margin: Option<f32>,
    /// Espessura da borda em pontos (0 = sem borda).
    pub border_width: Option<f32>,
    /// Cor da borda, `0xRRGGBBAA`.
    pub border_color: Option<Rgba>,
    /// Raio dos cantos em pontos.
    pub corner_radius: Option<f32>,
}

impl ComputedStyle {
    /// `true` se algum atributo de CAIXA está setado (bg/padding/margin/border/
    /// raio) — gatilho para o render envolver o bloco num `egui::Frame`. Sem
    /// nenhum, o render desenha direto (sem o overhead do Frame).
    pub fn has_box(&self) -> bool {
        self.bg.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.border_width.is_some()
            || self.corner_radius.is_some()
    }

    /// Sobrepõe as propriedades `Some` de `other` sobre `self` (precedência CSS:
    /// `other` vence onde está setado; `None` mantém `self`). Usado para o
    /// `style=""` inline cair sobre o estilo-de-tag.
    pub fn merge_over(&mut self, other: &ComputedStyle) {
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.bg.is_some() {
            self.bg = other.bg;
        }
        if other.font_size.is_some() {
            self.font_size = other.font_size;
        }
        if other.bold.is_some() {
            self.bold = other.bold;
        }
        if other.italic.is_some() {
            self.italic = other.italic;
        }
        if other.padding.is_some() {
            self.padding = other.padding;
        }
        if other.margin.is_some() {
            self.margin = other.margin;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.corner_radius.is_some() {
            self.corner_radius = other.corner_radius;
        }
    }

    /// Lê o valor de um SLOT opaco como `i64`, ou `-1` se não-setado. Cores/dims
    /// retornam o `u32`/pontos diretamente. É como o LAYOUT (em TS) lê o estilo
    /// computado de um nó via a ABI `rts:dom` (`nodeStyleSlot`).
    pub fn slot_value(&self, slot: i64) -> i64 {
        let dim = |o: Option<f32>| o.map(|v| v as i64).unwrap_or(-1);
        match slot {
            SLOT_COLOR => self.color.map(|c| c as i64).unwrap_or(-1),
            SLOT_BG => self.bg.map(|c| c as i64).unwrap_or(-1),
            SLOT_FONT_SIZE => dim(self.font_size),
            SLOT_PADDING => dim(self.padding),
            SLOT_MARGIN => dim(self.margin),
            SLOT_BORDER_WIDTH => dim(self.border_width),
            SLOT_BORDER_COLOR => self.border_color.map(|c| c as i64).unwrap_or(-1),
            SLOT_CORNER_RADIUS => dim(self.corner_radius),
            _ => -1,
        }
    }
}

// ── Slots numéricos opacos (invariante 4) ──────────────────────────────────────
// O Rust NUNCA casa string CSS (`"background-color"`); o TS mapeia nome→índice e
// chama `defineStyle(tag, slot, val)`. Adicionar `box-shadow` = registrar um slot
// no TS, sem tocar aqui. Estes códigos são o contrato com a camada TS.
pub const SLOT_COLOR: i64 = 0;
pub const SLOT_BG: i64 = 1;
pub const SLOT_FONT_SIZE: i64 = 2;
// Box model (F2):
pub const SLOT_PADDING: i64 = 3;
pub const SLOT_MARGIN: i64 = 4;
pub const SLOT_BORDER_WIDTH: i64 = 5;
pub const SLOT_BORDER_COLOR: i64 = 6;
pub const SLOT_CORNER_RADIUS: i64 = 7;

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Mapa `tag → ComputedStyle`, povoado pelo TS via `defineStyle(tag, slot, val)`.
    /// É o estilo POR-TAG (uma UA-stylesheet de estilo, paralela ao `block::BLOCKS`
    /// de layout). O render consulta `lookup_style(tag)` e aplica antes do
    /// `style=""` inline do nó. Vazio até o TS registrar.
    static STYLES: RefCell<HashMap<String, ComputedStyle>> = RefCell::new(HashMap::new());
}

/// Registra/atualiza UM slot de estilo de uma TAG (primitivo `defineStyle`).
/// ACUMULA: chamar com slots diferentes na mesma tag mantém os anteriores
/// (`defineStyle("h1",0,cor)` + `defineStyle("h1",2,tam)` → cor E tamanho). O
/// `(slot, val)` é opaco (invariante 4); o Rust nunca vê o nome CSS.
pub fn define_style(tag: &str, slot: i64, val: i64) {
    STYLES.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.entry(tag.to_ascii_lowercase()).or_default();
        entry.apply_slot(slot, val);
    });
}

/// Consulta o `ComputedStyle` registrado de uma TAG. `None` ⇒ sem estilo de tag.
pub fn lookup_style(tag: &str) -> Option<ComputedStyle> {
    STYLES.with(|m| m.borrow().get(tag).copied())
}

impl ComputedStyle {
    /// Aplica um par `(slot, val)` OPACO (invariante 4). O `val` é interpretado
    /// conforme o slot: cor/bg = `u32` RGBA; font_size = pontos (o `i64` vira
    /// `f32`). Slot desconhecido é ignorado (robustez; o TS pode registrar slots
    /// futuros antes deste Rust conhecê-los). É a base do `defineStyle`/`setStyle`.
    pub fn apply_slot(&mut self, slot: i64, val: i64) {
        // Dimensões (padding/margin/border/raio) em pontos: `i64` → `f32`, clamp em
        // ≥ 0 (negativo não faz sentido numa caixa; ignora).
        let dim = |v: i64| -> Option<f32> {
            let f = v as f32;
            if f >= 0.0 { Some(f) } else { None }
        };
        match slot {
            SLOT_COLOR => self.color = Some(val as u32),
            SLOT_BG => self.bg = Some(val as u32),
            SLOT_FONT_SIZE => {
                let f = val as f32;
                if f > 0.0 {
                    self.font_size = Some(f);
                }
            }
            SLOT_PADDING => self.padding = dim(val),
            SLOT_MARGIN => self.margin = dim(val),
            SLOT_BORDER_WIDTH => self.border_width = dim(val),
            SLOT_BORDER_COLOR => self.border_color = Some(val as u32),
            SLOT_CORNER_RADIUS => self.corner_radius = dim(val),
            _ => {} // slot desconhecido: ignora (o TS mapeia o vocabulário CSS).
        }
    }
}

/// Parseia um `style="prop: valor; ..."` para um `ComputedStyle`. Ignora
/// propriedades/valores desconhecidos sem panicar (robustez de parser real).
pub fn parse_inline(style: &str) -> ComputedStyle {
    let mut css = ComputedStyle::default();
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let (prop, val) = match (it.next(), it.next()) {
            (Some(p), Some(v)) => (p.trim().to_ascii_lowercase(), v.trim()),
            _ => continue,
        };
        match prop.as_str() {
            "color" => css.color = parse_color(val),
            "background-color" | "background" => css.bg = parse_color(val),
            "font-size" => css.font_size = parse_px(val),
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

/// Parseia uma cor CSS para `u32` RGBA (`0xRRGGBBAA`): `#rgb`, `#rrggbb`,
/// `rgb(r,g,b)` ou um nome básico. Alpha implícito = `0xFF` (opaco).
pub fn parse_color(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let mut p = inner.split(',').map(|x| x.trim().parse::<u8>().ok());
        if let (Some(Some(r)), Some(Some(g)), Some(Some(b))) = (p.next(), p.next(), p.next()) {
            return Some(rgba(r, g, b));
        }
        return None;
    }
    named_color(v)
}

/// Compõe `0xRRGGBBAA` opaco a partir de componentes.
fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    match hex.len() {
        // #rgb → expande cada nibble (f → ff).
        3 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let r = ((n >> 8) & 0xF) as u8;
            let g = ((n >> 4) & 0xF) as u8;
            let b = (n & 0xF) as u8;
            Some(rgba(r * 17, g * 17, b * 17))
        }
        6 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            Some(rgba(
                ((n >> 16) & 0xFF) as u8,
                ((n >> 8) & 0xFF) as u8,
                (n & 0xFF) as u8,
            ))
        }
        _ => None,
    }
}

fn named_color(v: &str) -> Option<Rgba> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => rgba(0, 0, 0),
        "white" => rgba(255, 255, 255),
        "red" => rgba(255, 0, 0),
        "green" => rgba(0, 255, 0),
        "blue" => rgba(0, 0, 255),
        "yellow" => rgba(255, 255, 0),
        "gray" | "grey" => rgba(128, 128, 128),
        "lightgray" | "lightgrey" => rgba(211, 211, 211),
        "darkgray" | "darkgrey" => rgba(64, 64, 64),
        "orange" => rgba(255, 165, 0),
        "purple" => rgba(128, 0, 128),
        "cyan" => rgba(0, 255, 255),
        "magenta" => rgba(255, 0, 255),
        "transparent" => 0x0000_0000,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typography() {
        let c = parse_inline("color:#ff0000; font-size:18px; font-weight:bold; font-style:italic");
        assert_eq!(c.color, Some(0xFF0000FF));
        assert_eq!(c.font_size, Some(18.0));
        assert_eq!(c.bold, Some(true));
        assert_eq!(c.italic, Some(true));
    }

    #[test]
    fn color_forms() {
        assert_eq!(parse_color("#f00"), Some(0xFF0000FF));
        assert_eq!(parse_color("#00ff00"), Some(0x00FF00FF));
        assert_eq!(parse_color("rgb(10, 20, 30)"), Some(0x0A141EFF));
        assert_eq!(parse_color("blue"), Some(0x0000FFFF));
        assert_eq!(parse_color("nope"), None);
    }

    #[test]
    fn background_color() {
        let c = parse_inline("background-color: #112233");
        assert_eq!(c.bg, Some(0x112233FF));
        assert_eq!(c.color, None);
    }

    #[test]
    fn ignores_unknown() {
        let c = parse_inline("font-size:bogus; unknown:1; font-weight:300");
        assert_eq!(c.font_size, None);
        assert_eq!(c.bold, Some(false));
    }

    #[test]
    fn apply_slot_opaco() {
        // SLOT opaco (invariante 4): nenhum nome CSS, só índice + valor.
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_COLOR, 0x0088FFFF);
        s.apply_slot(SLOT_FONT_SIZE, 28);
        s.apply_slot(SLOT_BG, 0x111111FF);
        assert_eq!(s.color, Some(0x0088FFFF));
        assert_eq!(s.font_size, Some(28.0));
        assert_eq!(s.bg, Some(0x111111FF));
    }

    #[test]
    fn apply_slot_desconhecido_e_invalido_ignora() {
        let mut s = ComputedStyle::default();
        s.apply_slot(999, 123); // slot inexistente
        s.apply_slot(SLOT_FONT_SIZE, 0); // tamanho 0 inválido
        s.apply_slot(SLOT_FONT_SIZE, -5); // negativo inválido
        assert_eq!(s, ComputedStyle::default());
    }

    #[test]
    fn egui_free_garantia() {
        // Documenta a invariante F0(d): este módulo não nomeia tipos do egui.
        // A cor é u32; o teste compila SÓ se ComputedStyle for egui-free.
        let s = ComputedStyle { color: Some(0xAABBCCFF), ..Default::default() };
        let _raw: Option<u32> = s.color; // se fosse Color32, isto não compilaria.
    }

    #[test]
    fn box_model_slots() {
        // F2: slots de caixa (padding/margin/border/raio) via apply_slot opaco.
        let mut s = ComputedStyle::default();
        assert!(!s.has_box()); // vazio: sem caixa.
        s.apply_slot(SLOT_PADDING, 8);
        s.apply_slot(SLOT_MARGIN, 4);
        s.apply_slot(SLOT_BORDER_WIDTH, 2);
        s.apply_slot(SLOT_BORDER_COLOR, 0xFF0000FF);
        s.apply_slot(SLOT_CORNER_RADIUS, 6);
        s.apply_slot(SLOT_BG, 0x222222FF);
        assert_eq!(s.padding, Some(8.0));
        assert_eq!(s.margin, Some(4.0));
        assert_eq!(s.border_width, Some(2.0));
        assert_eq!(s.border_color, Some(0xFF0000FF));
        assert_eq!(s.corner_radius, Some(6.0));
        assert_eq!(s.bg, Some(0x222222FF));
        assert!(s.has_box());
    }

    #[test]
    fn box_slots_negativos_ignorados() {
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_PADDING, -3); // negativo não faz sentido numa caixa
        s.apply_slot(SLOT_CORNER_RADIUS, -1);
        assert_eq!(s.padding, None);
        assert_eq!(s.corner_radius, None);
    }

    #[test]
    fn has_box_so_com_texto_e_false() {
        // só cor/tamanho de texto NÃO conta como caixa (não vira egui::Frame).
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_COLOR, 0xFFFFFFFF);
        s.apply_slot(SLOT_FONT_SIZE, 18);
        assert!(!s.has_box());
    }

    #[test]
    fn define_style_acumula_por_tag() {
        // F1: defineStyle por slot OPACO acumula na mesma tag (cor + tamanho).
        // (thread_local — usa uma tag única pra não colidir com outros testes.)
        define_style("h1_acum", SLOT_COLOR, 0x0088FFFF);
        define_style("h1_acum", SLOT_FONT_SIZE, 28);
        let s = lookup_style("h1_acum").expect("tag registrada");
        assert_eq!(s.color, Some(0x0088FFFF));
        assert_eq!(s.font_size, Some(28.0));
        // tag não registrada → None.
        assert_eq!(lookup_style("tag_inexistente_xyz"), None);
    }
}
