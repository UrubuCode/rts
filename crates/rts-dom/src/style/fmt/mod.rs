//! Serialização de valores COMPUTADOS no formato que o browser reporta
//! (`getComputedStyle(el).prop`): cor → `rgb(r, g, b)` / `rgba(...)`, comprimento
//! → `Npx`, enums → keyword. Validado contra o Chrome real (ver `fmt_color`).

use super::fmt_values::{
    display_css, fmt_align, fmt_color, fmt_dim, fmt_flex_wrap, fmt_justify, fmt_px, fmt_tracks,
    fmt_url, overflow_css, side_css, side_css_resolved, side_of,
};
use super::props::ComputedStyle;
use super::values::{Dimension, LineHeight, TextAlign, TextTransform, WhiteSpace};

mod tempo;
mod flex_grid;
mod caixa_fluxo;

impl ComputedStyle {
    /// O font-size deste elemento em px, ou o default de 16px quando a cascade
    /// não o fixou. Serve os valores computados que se resolvem CONTRA a fonte
    /// (hoje o `line-height` multiplicador). Só a forma absoluta conta: a
    /// cascade resolve `em`/`%`/`rem` para `Px` cedo, e uma forma relativa que
    /// chegue aqui não tem contra o que resolver.
    /// Os dois eixos de `overflow` como o computed os reporta, com a regra que
    /// só o computed tem: **um eixo `visible` ao lado de um eixo que não é
    /// `visible` computa para `auto`**. É da spec (`overflow` §3) e não é
    /// cosmética — `overflow-x: hidden` sozinho torna o eixo Y rolável, que é
    /// exatamente o que uma faixa horizontal recortada precisa. Medido no
    /// corpus: `overflow-x: hidden` responde `hidden auto` no Chrome, e nós
    /// respondíamos `hidden visible`.
    fn overflow_pair(&self) -> (&'static str, &'static str) {
        use crate::scrollbar::Overflow::Visible;
        let x = self.overflow_x.unwrap_or(Visible);
        let y = self.overflow_y.unwrap_or(Visible);
        let (x, y) = match (x == Visible, y == Visible) {
            (true, false) => (crate::scrollbar::Overflow::Auto, y),
            (false, true) => (x, crate::scrollbar::Overflow::Auto),
            _ => (x, y),
        };
        (overflow_css(x), overflow_css(y))
    }

    fn font_size_px(&self) -> f32 {
        match self.font_size {
            Some(Dimension::Px(v)) => v,
            _ => 16.0,
        }
    }

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
            // `background-image: <img1>, <img2>` — DUAS OU MAIS camadas (ver
            // `style::decoracao`). Precede os dois braços abaixo (gradiente
            // único / url única) porque os dois só sabem uma camada; uma
            // declaração de camada ÚNICA continua a cair neles, inalterada.
            "background-image"
                if self
                    .bg_image_layers
                    .as_deref()
                    .is_some_and(|s| super::lengths::split_top(s, ',').len() > 1) =>
            {
                crate::style::decoracao::fmt_bg_image_layers(
                    self.bg_image_layers.as_deref().unwrap(),
                )
            }
            // `background`/`background-image` com gradiente reportam o gradiente; senão
            // a cor sólida. (Este braço precede o de cor sólida abaixo p/ vencer.)
            "background" | "background-image" if self.gradient.is_some() => {
                let g = self.gradient.unwrap();
                format!(
                    "linear-gradient({}deg, {}, {})",
                    g.angle_deg,
                    fmt_color(g.c0),
                    fmt_color(g.c1)
                )
            }
            // `background-image: url(...)` sem gradiente — o valor cru
            // (`bg_image`) com o mesmo `url("…")` que `list-style-image` e
            // `cursor` agora levam. Antes desta linha `background-image`
            // sem gradiente não tinha NENHUM braço nesta função e caía na
            // cadeia `_ =>` no fim, que também não a respondia — o
            // `getComputedStyle` devolvia `""` para um `background-image`
            // declarado, e não só sem aspas.
            "background-image" => self.bg_image.as_deref().map(fmt_url).unwrap_or_default(),
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
                // O browser reporta o line-height RESOLVIDO em px — `line-height:
                // 2` num elemento de 16px responde `32px`, e o mesmo `2` herdado
                // por um filho de 32px responde `64px`. O font-size do nó está
                // aqui (a cascade resolve-o para Px cedo), portanto a nota antiga
                // "sem o font-size do nó, reportamos o cru" já não valia: era ela
                // que fazia o computed responder `2`.
                Some(LineHeight::Px(p)) => fmt_px(p),
                Some(LineHeight::Mult(m)) => fmt_px(m * self.font_size_px()),
                // O browser reporta `normal` tal e qual (é o único valor de
                // line-height que o computed NÃO resolve para px).
                Some(LineHeight::Normal) => "normal".into(),
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
            // `_resolved`: margin/padding em `em`/`rem` chegam em px, como o
            // Chrome — ver `fmt_values::side_css_resolved`.
            "margin-top" => side_css_resolved(self, self.margin.top),
            "margin-right" => side_css_resolved(self, self.margin.right),
            "margin-bottom" => side_css_resolved(self, self.margin.bottom),
            "margin-left" => side_css_resolved(self, self.margin.left),
            "border-width" => self.border_width.map(fmt_px).unwrap_or_default(),
            "border-color" => self.border_color.map(fmt_color).unwrap_or_default(),
            "border-style" => self
                .border_style
                .map(|s| format!("{s:?}").to_ascii_lowercase())
                .unwrap_or_default(),
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
            // O `flow-root` computa como `block` na CAIXA mas o browser
            // responde `flow-root` — a palavra é o valor computado, não um
            // atalho para `block`. Sem esta linha, o campo existia e o
            // `getComputedStyle` continuava a mentir.
            "display" if self.flow_root == Some(true) => "flow-root".into(),
            "display" => self
                .display
                .map(|d| display_css(d).to_string())
                .unwrap_or_default(),
            "box-sizing" => match self.border_box {
                Some(true) => "border-box".into(),
                Some(false) => "content-box".into(),
                None => String::new(),
            },
            "justify-content" => self.justify.map(fmt_justify).unwrap_or_default(),
            "align-items" => self.align_items.map(fmt_align).unwrap_or_default(),
            "align-self" => self.align_self.map(fmt_align).unwrap_or_default(),
            "opacity" => self.opacity.map(|v| format!("{v}")).unwrap_or_default(),
            "aspect-ratio" => self
                .aspect_ratio
                .map(|r| format!("{r}"))
                .unwrap_or_default(),
            "z-index" => self.z_index.map(|z| format!("{z}")).unwrap_or_default(),
            _ => self.get_property_tempo(n.as_str())
                .or_else(|| self.get_property_flex_grid(n.as_str()))
                .or_else(|| self.get_property_caixa_fluxo(n.as_str()))
                .or_else(|| super::vocab::get_property(self, n.as_str()))
                .or_else(|| super::radius::get_property(self, n.as_str()))
                .or_else(|| super::grid_lines::get_property(self, n.as_str()))
                .or_else(|| super::painting::get_property(self, n.as_str()))
                .unwrap_or_default(),
        }
    }
}

/// Milissegundos → o formato em que o browser responde um tempo de CSS: segundos
/// com sufixo `s` e sem zeros à direita (`300` → `"0.3s"`). `format!("{}")` num
/// `f32` já corta os zeros, por isso não há tabela de casas decimais aqui.
fn fmt_seconds(ms: f32) -> String {
    format!("{}s", ms / 1000.0)
}

/// [`Easing`] → o texto da propriedade. As curvas nomeadas voltam pelo nome; a
/// `cubic-bezier`/`steps` voltam pela forma funcional, que é o que o browser faz
/// (ele NÃO reduz `cubic-bezier(.25,.1,.25,1)` a `ease`, e imitar essa redução
/// seria inventar uma equivalência que a spec não promete).
fn fmt_easing(e: crate::anim::Easing) -> String {
    use crate::anim::Easing;
    match e {
        Easing::Linear => "linear".into(),
        Easing::Ease => "ease".into(),
        Easing::EaseIn => "ease-in".into(),
        Easing::EaseOut => "ease-out".into(),
        Easing::EaseInOut => "ease-in-out".into(),
        Easing::CubicBezier(a, b, c, d) => format!("cubic-bezier({a}, {b}, {c}, {d})"),
        Easing::Steps(n) => format!("steps({n})"),
    }
}
