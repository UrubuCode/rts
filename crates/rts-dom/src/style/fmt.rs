//! Serialização de valores COMPUTADOS no formato que o browser reporta
//! (`getComputedStyle(el).prop`): cor → `rgb(r, g, b)` / `rgba(...)`, comprimento
//! → `Npx`, enums → keyword. Validado contra o Chrome real (ver `fmt_color`).

use super::fmt_values::{
    display_css, fmt_align, fmt_color, fmt_dim, fmt_justify, fmt_px, fmt_tracks,
    overflow_css, side_css, side_of,
};
use super::props::ComputedStyle;
use super::values::{
    Dimension, LineHeight, TextAlign, TextTransform, WhiteSpace,
};

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
            "display" => self.display.map(|d| display_css(d).to_string()).unwrap_or_default(),
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
            "transition" => self
                .transition
                .map(|t| format!("all {}s {}s", t.duration_ms / 1000.0, t.delay_ms / 1000.0))
                .unwrap_or_default(),
            // As longhands respondem O SEU valor, não o shorthand inteiro:
            // `transition-duration` respondia `all 0.3s 0s`, que não é um valor
            // válido da propriedade que foi perguntada. Sem transição declarada o
            // browser devolve o INICIAL (`0s`), não vazio.
            "transition-duration" => {
                self.transition.map(|t| fmt_seconds(t.duration_ms)).unwrap_or_default()
            }
            "transition-delay" => {
                self.transition.map(|t| fmt_seconds(t.delay_ms)).unwrap_or_default()
            }
            "transition-timing-function" => {
                self.transition.map(|t| fmt_easing(t.easing)).unwrap_or_default()
            }
            // O modelo transiciona `all` e não guarda a lista declarada — ver
            // `style::timing`. Responder `all` é o que ele faz de facto.
            "transition-property" => {
                if self.transition.is_some() { "all".into() } else { String::new() }
            }
            // Vazio quando não há animação declarada: este caminho serve também
            // `el.style.x`, e o INICIAL vem de `style::initial` (ver o cabeçalho
            // daquele módulo — cair no inicial aqui estragava o `el.style`).
            "animation-name" => self
                .animation
                .as_ref()
                .map(|a| if a.name.is_empty() { "none".to_string() } else { a.name.clone() })
                .unwrap_or_default(),
            "animation-duration" => {
                self.animation.as_ref().map(|a| fmt_seconds(a.duration_ms)).unwrap_or_default()
            }
            "animation-delay" => {
                self.animation.as_ref().map(|a| fmt_seconds(a.delay_ms)).unwrap_or_default()
            }
            "animation-timing-function" => {
                self.animation.as_ref().map(|a| fmt_easing(a.easing)).unwrap_or_default()
            }
            "animation-iteration-count" => match self.animation.as_ref().map(|a| a.iterations) {
                Some(None) => "infinite".into(),
                Some(Some(n)) => format!("{n}"),
                None => String::new(),
            },
            "animation-direction" => match self.animation.as_ref().map(|a| a.direction) {
                None => String::new(),
                Some(crate::anim::AnimDirection::Reverse) => "reverse".into(),
                Some(crate::anim::AnimDirection::Alternate) => "alternate".into(),
                Some(crate::anim::AnimDirection::AlternateReverse) => "alternate-reverse".into(),
                Some(crate::anim::AnimDirection::Normal) => "normal".into(),
            },
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
            // `gap` (shorthand) imprime `<row> <column>`, e um valor só quando
            // os dois coincidem — era só o de coluna, o que perdia metade do
            // valor de `gap: 10px 20px`.
            "gap" => match (self.row_gap, self.gap) {
                (None, None) => String::new(),
                (r, c) => {
                    let (rs, cs) = (
                        r.map(fmt_dim).unwrap_or_else(|| "normal".into()),
                        c.map(fmt_dim).unwrap_or_else(|| "normal".into()),
                    );
                    if rs == cs { rs } else { format!("{rs} {cs}") }
                }
            },
            "column-gap" => self.gap.map(fmt_dim).unwrap_or_default(),
            "visibility" => match self.visibility {
                Some(crate::style::values::Visibility::Hidden) => "hidden".into(),
                Some(crate::style::values::Visibility::Visible) => "visible".into(),
                None => String::new(),
            },
            "flex-direction" => self
                .flex_direction
                .map(|d| match d {
                    crate::style::FlexDirection::Row => "row",
                    crate::style::FlexDirection::RowReverse => "row-reverse",
                    crate::style::FlexDirection::Column => "column",
                    crate::style::FlexDirection::ColumnReverse => "column-reverse",
                })
                .map(|s| s.to_string())
                .unwrap_or_default(),
            "flex-wrap" => match self.flex_wrap {
                Some(true) => "wrap".into(),
                Some(false) => "nowrap".into(),
                None => String::new(),
            },
            // As trilhas de grid: o browser reporta os tamanhos JÁ RESOLVIDOS em
            // px (`repeat(3, 1fr)` num container de 450px sai `150px 150px
            // 150px`). Aqui saem na forma DECLARADA, porque o computed não tem o
            // container à mão — a resolução é do layout. É um desvio conhecido
            // contra o Chrome, e fica escrito em vez de responder vazio.
            "grid-template-columns" => fmt_tracks(self.grid_template_columns.as_deref()),
            "grid-template-rows" => fmt_tracks(self.grid_template_rows.as_deref()),
            "grid-area" => self.grid_area.clone().unwrap_or_default(),
            // O browser reporta a matriz re-serializada linha a linha entre aspas.
            // Aqui ela é reportada a partir do RETÂNGULO de cada nome (a matriz crua
            // não é guardada), o que reconstrói o valor para as áreas retangulares —
            // que são as únicas legais na spec.
            "grid-template-areas" => match &self.grid_template_areas {
                None => String::new(),
                Some(a) => (0..a.rows)
                    .map(|r| {
                        let cells: Vec<String> = (0..a.cols)
                            .map(|c| a.name_at(r, c).unwrap_or(".").to_string())
                            .collect();
                        format!("\"{}\"", cells.join(" "))
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            },
            "row-gap" => self.row_gap.map(fmt_dim).unwrap_or_default(),
            // `overflow` (shorthand): um keyword quando os dois eixos coincidem,
            // dois — `hidden auto`, eixo X primeiro — quando não. Não existia
            // braço nenhum: a propriedade que a página mais declara dos três
            // respondia vazio enquanto `overflow-x` respondia certo.
            "overflow" => match (self.overflow_x, self.overflow_y) {
                (None, None) => String::new(),
                _ => {
                    let (x, y) = self.overflow_pair();
                    if x == y { x.to_string() } else { format!("{x} {y}") }
                }
            },
            "overflow-x" => match self.overflow_x {
                None if self.overflow_y.is_none() => String::new(),
                _ => self.overflow_pair().0.to_string(),
            },
            "overflow-y" => match self.overflow_y {
                None if self.overflow_x.is_none() => String::new(),
                _ => self.overflow_pair().1.to_string(),
            },
            // ── Fundo (as camadas do shorthand) ───────────────────────────────
            "background-repeat" => self.bg_repeat.map(|r| r.css().to_string()).unwrap_or_default(),
            "background-position" => self
                .bg_position
                .map(|p| format!("{} {}", fmt_dim(p.x), fmt_dim(p.y)))
                .unwrap_or_default(),
            "background-size" => match self.bg_size {
                None => String::new(),
                Some(crate::style::BgSize::Auto) => "auto".into(),
                Some(crate::style::BgSize::Cover) => "cover".into(),
                Some(crate::style::BgSize::Contain) => "contain".into(),
                Some(crate::style::BgSize::Len(w, h)) => {
                    format!("{} {}", fmt_dim(w), fmt_dim(h))
                }
            },
            // ── Bordas por lado: reportam o EFETIVO (com o fallback da uniforme),
            // que é o que o browser reporta — `border: 1px solid red` faz o
            // `border-top-color` responder `rgb(255, 0, 0)`, não vazio.
            "border-top-width" | "border-right-width" | "border-bottom-width"
            | "border-left-width" => fmt_px(side_of(self, &n).width),
            "border-top-style" | "border-right-style" | "border-bottom-style"
            | "border-left-style" => {
                format!("{:?}", side_of(self, &n).style).to_ascii_lowercase()
            }
            "border-top-color" | "border-right-color" | "border-bottom-color"
            | "border-left-color" => fmt_color(side_of(self, &n).color),
            "outline-width" => self.outline_width.map(fmt_px).unwrap_or_default(),
            "outline-style" => self
                .outline_style
                .map(|s| format!("{s:?}").to_ascii_lowercase())
                .unwrap_or_default(),
            "outline-color" => self.outline_color.map(fmt_color).unwrap_or_default(),
            "outline-offset" => self.outline_offset.map(fmt_px).unwrap_or_default(),
            // ── Texto / listas / fluxo ────────────────────────────────────────
            "vertical-align" => {
                self.vertical_align.map(|v| v.css().to_string()).unwrap_or_default()
            }
            "clear" => self.clear.map(|c| c.css().to_string()).unwrap_or_default(),
            "word-break" => self.word_break.map(|w| w.css().to_string()).unwrap_or_default(),
            "overflow-wrap" | "word-wrap" => {
                self.overflow_wrap.map(|w| w.css().to_string()).unwrap_or_default()
            }
            "direction" => self.direction.map(|d| d.css().to_string()).unwrap_or_default(),
            "text-indent" => self.text_indent.map(fmt_dim).unwrap_or_default(),
            "list-style-type" => {
                self.list_style_type.map(|t| t.css().to_string()).unwrap_or_default()
            }
            "list-style-image" => self.list_style_image.clone().unwrap_or_default(),
            "list-style-position" => {
                self.list_style_position.map(|p| p.css().to_string()).unwrap_or_default()
            }
            // ── Tabela ────────────────────────────────────────────────────────
            "border-collapse" => {
                self.border_collapse.map(|c| c.css().to_string()).unwrap_or_default()
            }
            // O Chrome responde os DOIS eixos sempre (`2px 2px`), mesmo quando a
            // folha declarou um só — é o valor computado, não o declarado.
            "border-spacing" => self
                .border_spacing
                .map(|s| format!("{} {}", fmt_dim(s.h), fmt_dim(s.v)))
                .unwrap_or_default(),
            "table-layout" => self.table_layout.map(|t| t.css().to_string()).unwrap_or_default(),
            "cursor" => self.cursor.clone().unwrap_or_default(),
            "flex-flow" => match (self.flex_direction, self.flex_wrap) {
                (None, None) => String::new(),
                (d, w) => format!(
                    "{} {}",
                    match d {
                        Some(x) => match x {
                            crate::style::FlexDirection::Row => "row",
                            crate::style::FlexDirection::RowReverse => "row-reverse",
                            crate::style::FlexDirection::Column => "column",
                            crate::style::FlexDirection::ColumnReverse => "column-reverse",
                        },
                        None => "row",
                    },
                    if w == Some(true) { "wrap" } else { "nowrap" }
                ),
            },
            // O 2º lote responde do seu próprio módulo — ver `style::vocab`.
            _ => super::vocab::get_property(self, n.as_str())
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
