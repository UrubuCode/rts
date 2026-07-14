//! Serialização de valores COMPUTADOS no formato que o browser reporta
//! (`getComputedStyle(el).prop`): cor → `rgb(r, g, b)` / `rgba(...)`, comprimento
//! → `Npx`, enums → keyword. Validado contra o Chrome real (ver `fmt_color`).

use super::props::ComputedStyle;
use super::values::{
    AlignItems, Dimension, DisplayKind, JustifyContent, LineHeight, Rgba, TextAlign,
    TextTransform, WhiteSpace,
};

impl ComputedStyle {
    /// Valor COMPUTADO de uma propriedade CSS por NOME, serializado no formato que o
    /// browser reporta (`getComputedStyle(el).prop`): cor → `rgb(r, g, b)` /
    /// `rgba(r, g, b, a)`; comprimento → `Npx`; enums → o keyword. `""` se a
    /// propriedade não está definida. PARSEAR/SERIALIZAR nome CSS é permitido (vive
    /// aqui, não na engine — invariante 4: a engine nunca casa nome; isto é o DOM).
    ///
    /// ⚠️ CORTES vs getComputedStyle real (o modelo de estilo é mais grosso):
    /// - `font-weight` só `400`/`700` (o modelo é `bold: bool` — `500`/`900`/`lighter`
    ///   colapsam); `font-style` só `normal`/`italic` (`oblique`→`italic`).
    /// - comprimentos relativos (`%`/`em`/`rem`/`vw`/`vh`) NÃO são resolvidos para px
    ///   no computed (o browser resolve `width:60%`→`768px`); aqui sai o valor cru.
    /// - `background` é tratado como só-cor (sem image/position/repeat — o shorthand
    ///   completo não é modelado).
    pub fn get_property(&self, name: &str) -> String {
        let n = name.trim().to_ascii_lowercase();
        match n.as_str() {
            "color" => self.color.map(fmt_color).unwrap_or_default(),
            // `background`/`background-image` com gradiente reportam o gradiente; senão
            // a cor sólida. (Este braço precede o de cor sólida abaixo p/ vencer.)
            "background" | "background-image" if self.gradient.is_some() => {
                let g = self.gradient.unwrap();
                format!("linear-gradient({}deg, {}, {})", g.angle_deg, fmt_color(g.c0), fmt_color(g.c1))
            }
            "background-color" | "background" => self.bg.map(fmt_color).unwrap_or_default(),
            "font-size" => self.font_size.map(fmt_dim).unwrap_or_default(),
            "font-weight" => match self.bold {
                Some(true) => "700".into(),
                Some(false) => "400".into(),
                None => String::new(),
            },
            "font-style" => match self.italic {
                Some(true) => "italic".into(),
                Some(false) => "normal".into(),
                None => String::new(),
            },
            "text-align" => match self.text_align {
                Some(TextAlign::Left) => "left".into(),
                Some(TextAlign::Right) => "right".into(),
                Some(TextAlign::Center) => "center".into(),
                Some(TextAlign::Justify) => "justify".into(),
                None => String::new(),
            },
            "line-height" => match self.line_height {
                // o browser reporta line-height computado em px (resolve o multiplicador
                // contra o font-size); aqui sem o font-size do nó, reportamos o cru.
                Some(LineHeight::Px(p)) => fmt_px(p),
                Some(LineHeight::Mult(m)) => format!("{m}"),
                None => String::new(),
            },
            "white-space" => match self.white_space {
                Some(WhiteSpace::Normal) => "normal".into(),
                Some(WhiteSpace::Nowrap) => "nowrap".into(),
                Some(WhiteSpace::Pre) => "pre".into(),
                Some(WhiteSpace::PreWrap) => "pre-wrap".into(),
                Some(WhiteSpace::PreLine) => "pre-line".into(),
                None => String::new(),
            },
            "text-transform" => match self.text_transform {
                Some(TextTransform::None) => "none".into(),
                Some(TextTransform::Uppercase) => "uppercase".into(),
                Some(TextTransform::Lowercase) => "lowercase".into(),
                Some(TextTransform::Capitalize) => "capitalize".into(),
                None => String::new(),
            },
            "font-family" => self.font_family.clone().unwrap_or_default(),
            "padding-top" => side_css(self.padding.top),
            "padding-right" => side_css(self.padding.right),
            "padding-bottom" => side_css(self.padding.bottom),
            "padding-left" => side_css(self.padding.left),
            "margin-top" => side_css(self.margin.top),
            "margin-right" => side_css(self.margin.right),
            "margin-bottom" => side_css(self.margin.bottom),
            "margin-left" => side_css(self.margin.left),
            "border-width" => self.border_width.map(fmt_px).unwrap_or_default(),
            "border-color" => self.border_color.map(fmt_color).unwrap_or_default(),
            "border-style" => self.border_style.map(|s| format!("{s:?}").to_ascii_lowercase()).unwrap_or_default(),
            "border-radius" => self.corner_radius.map(fmt_px).unwrap_or_default(),
            "width" => self.width.map(fmt_dim).unwrap_or_default(),
            "height" => self.height.map(fmt_dim).unwrap_or_default(),
            "min-width" => self.min_width.map(fmt_dim).unwrap_or_default(),
            "max-width" => self.max_width.map(fmt_dim).unwrap_or_default(),
            "min-height" => self.min_height.map(fmt_dim).unwrap_or_default(),
            "max-height" => self.max_height.map(fmt_dim).unwrap_or_default(),
            "float" => self
                .float_side
                .map(|f| format!("{f:?}").to_ascii_lowercase())
                .unwrap_or_default(),
            "position" => self
                .position
                .map(|p| format!("{p:?}").to_ascii_lowercase())
                .unwrap_or_default(),
            "top" => self.inset_top.map(fmt_dim).unwrap_or_default(),
            "right" => self.inset_right.map(fmt_dim).unwrap_or_default(),
            "bottom" => self.inset_bottom.map(fmt_dim).unwrap_or_default(),
            "left" => self.inset_left.map(fmt_dim).unwrap_or_default(),
            "display" => self.display.map(fmt_display).unwrap_or_default(),
            "box-sizing" => match self.border_box {
                Some(true) => "border-box".into(),
                Some(false) => "content-box".into(),
                None => String::new(),
            },
            "justify-content" => self.justify.map(fmt_justify).unwrap_or_default(),
            "align-items" => self.align_items.map(fmt_align).unwrap_or_default(),
            "align-self" => self.align_self.map(fmt_align).unwrap_or_default(),
            "opacity" => self.opacity.map(|v| format!("{v}")).unwrap_or_default(),
            "aspect-ratio" => self.aspect_ratio.map(|r| format!("{r}")).unwrap_or_default(),
            "z-index" => self.z_index.map(|z| format!("{z}")).unwrap_or_default(),
            "transition" | "transition-duration" => self
                .transition
                .map(|t| format!("all {}s {}s", t.duration_ms / 1000.0, t.delay_ms / 1000.0))
                .unwrap_or_default(),
            "letter-spacing" => self
                .letter_spacing
                .map(|v| if v == 0.0 { "normal".to_string() } else { format!("{v}px") })
                .unwrap_or_default(),
            "text-decoration" | "text-decoration-line" => self
                .text_decoration
                .map(|d| {
                    match d {
                        crate::style::values::TextDecoration::None => "none",
                        crate::style::values::TextDecoration::Underline => "underline",
                        crate::style::values::TextDecoration::LineThrough => "line-through",
                        crate::style::values::TextDecoration::Overline => "overline",
                    }
                    .to_string()
                })
                .unwrap_or_default(),
            "box-shadow" => self
                .box_shadow
                .map(|s| format!("{}px {}px {}px {}px", s.dx, s.dy, s.blur, s.spread))
                .unwrap_or_default(),
            "transform" => self
                .transform
                .map(|t| {
                    format!(
                        "translate({}px + {}%, {}px + {}%) scale({}, {}) rotate({}deg)",
                        t.tx, t.tx_pct * 100.0, t.ty, t.ty_pct * 100.0, t.sx, t.sy, t.rot_deg
                    )
                })
                .unwrap_or_default(),
            "flex-grow" => self.flex_grow.map(|v| format!("{v}")).unwrap_or_default(),
            "flex-shrink" => self.flex_shrink.map(|v| format!("{v}")).unwrap_or_default(),
            "flex-basis" => self.flex_basis.map(fmt_dim).unwrap_or_default(),
            "order" => self.order.map(|v| format!("{v}")).unwrap_or_default(),
            "gap" | "column-gap" => self.gap.map(fmt_dim).unwrap_or_default(),
            "row-gap" => self.row_gap.map(fmt_dim).unwrap_or_default(),
            _ => String::new(),
        }
    }
}

/// Um lado de margin/padding → string CSS: comprimento cru (`fmt_dim` — px sai
/// `Npx` como antes; relativo sai `Nrem` etc, corte documentado), `auto`, ou `""`.
fn side_css(s: crate::style::Side) -> String {
    match s {
        crate::style::Side::Len(d) => fmt_dim(d),
        crate::style::Side::Auto => "auto".into(),
        crate::style::Side::Unset => String::new(),
    }
}

/// Serializa uma cor `0xRRGGBBAA` no formato do browser: `rgb(r, g, b)` se opaco
/// (alpha 255), senão `rgba(r, g, b, a)` com alpha 0-1 (até 2 casas, sem zeros à
/// direita). É o que o `getComputedStyle().color` reporta.
pub(crate) fn fmt_color(c: Rgba) -> String {
    let r = (c >> 24) & 0xFF;
    let g = (c >> 16) & 0xFF;
    let b = (c >> 8) & 0xFF;
    let a = c & 0xFF;
    if a == 0xFF {
        format!("rgb({r}, {g}, {b})")
    } else {
        // alpha 0-1 = a/255, arredondado a 2 casas — é o que o Chrome real reporta
        // (VALIDADO no browser: #0000ff80 → "rgba(0, 0, 255, 0.5)", não 0.501961; a
        // verificação adversarial sugeriu precisão cheia mas a medição desempatou).
        let af = (a as f32 / 255.0 * 100.0).round() / 100.0;
        let mut s = format!("{af}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        format!("rgba({r}, {g}, {b}, {s})")
    }
}

/// Comprimento em pontos → `Npx` (sem casas se inteiro: `14px`, não `14.0px`).
pub(crate) fn fmt_px(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}px", v as i64)
    } else {
        format!("{v}px")
    }
}

/// Uma `Dimension` computada → string CSS (px/%/auto…).
pub(crate) fn fmt_dim(d: Dimension) -> String {
    match d {
        Dimension::Px(v) => fmt_px(v),
        Dimension::Percent(p) => format!("{p}%"),
        Dimension::Em(v) => format!("{v}em"),
        Dimension::Rem(v) => format!("{v}rem"),
        Dimension::Vw(v) => format!("{v}vw"),
        Dimension::Vh(v) => format!("{v}vh"),
        Dimension::Auto => "auto".into(),
        // calc: reconstrói a forma canônica com os termos não-zero.
        Dimension::Calc(c) => {
            let mut parts: Vec<String> = Vec::new();
            for (v, u) in [
                (c.px, "px"), (c.pct, "%"), (c.em, "em"),
                (c.rem, "rem"), (c.vw, "vw"), (c.vh, "vh"),
            ] {
                if v != 0.0 {
                    parts.push(format!("{v}{u}"));
                }
            }
            if parts.is_empty() {
                "0px".into()
            } else {
                format!("calc({})", parts.join(" + "))
            }
        }
    }
}

fn fmt_justify(j: JustifyContent) -> String {
    match j {
        JustifyContent::FlexStart => "flex-start",
        JustifyContent::FlexEnd => "flex-end",
        JustifyContent::Center => "center",
        JustifyContent::SpaceBetween => "space-between",
        JustifyContent::SpaceAround => "space-around",
        JustifyContent::SpaceEvenly => "space-evenly",
    }
    .into()
}

fn fmt_align(a: AlignItems) -> String {
    match a {
        AlignItems::Stretch => "stretch",
        AlignItems::FlexStart => "flex-start",
        AlignItems::FlexEnd => "flex-end",
        AlignItems::Center => "center",
    }
    .into()
}

/// `DisplayKind` → keyword CSS VÁLIDO para `getComputedStyle('display')`. NB:
/// `FlexWrap` é só um estado interno (flex + flex-wrap) — para a propriedade
/// `display` o keyword é `flex` (flex-wrap é uma propriedade separada). Não usar
/// `{:?}` (geraria `flexwrap`, inválido).
fn fmt_display(d: DisplayKind) -> String {
    match d {
        DisplayKind::Block => "block",
        DisplayKind::Flex | DisplayKind::FlexWrap => "flex",
        DisplayKind::Inline => "inline",
        DisplayKind::Grid => "grid",
        DisplayKind::None => "none",
    }
    .into()
}
