//! `overflow`, fundo, bordas por lado, `outline` e a cauda de texto e listas
//!
//! Os braços vieram do `match` de `fmt.rs` VERBATIM: o `impl` próprio
//! mantém o `self` a ser `self` e a indentação a mesma, que é o que
//! torna a extração comparável linha a linha com o original.

use super::*;

impl ComputedStyle {
    pub(in crate::style::fmt) fn get_property_caixa_fluxo(&self, n: &str) -> Option<String> {
        Some(match n {
            "overflow" => match (self.overflow_x, self.overflow_y) {
                (None, None) => String::new(),
                _ => {
                    let (x, y) = self.overflow_pair();
                    if x == y {
                        x.to_string()
                    } else {
                        format!("{x} {y}")
                    }
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
            //
            // As três seguem `crate::style::decoracao::fmt_bg_*`: com UMA
            // camada de `background-image` (ou nenhuma), o resultado é
            // exatamente o de antes desta linha — o `unico` que a função
            // recebe. Com MAIS de uma, o Blink reporta uma entrada por camada
            // de IMAGEM, mesmo para a longhand que ninguém declarou (cai no
            // inicial repetido).
            "background-repeat" => crate::style::decoracao::fmt_bg_repeat(
                self.bg_image_layers.as_deref(),
                self.bg_repeat_layers.as_deref(),
                self.bg_repeat.map(|r| r.css().to_string()).unwrap_or_default(),
            ),
            "background-position" => crate::style::decoracao::fmt_bg_position(
                self.bg_image_layers.as_deref(),
                self.bg_position_layers.as_deref(),
                self.bg_position
                    .map(|p| format!("{} {}", fmt_dim(p.x), fmt_dim(p.y)))
                    .unwrap_or_default(),
            ),
            "background-size" => crate::style::decoracao::fmt_bg_size(
                self.bg_image_layers.as_deref(),
                self.bg_size_layers.as_deref(),
                match self.bg_size {
                    None => String::new(),
                    Some(crate::style::BgSize::Auto) => "auto".into(),
                    Some(crate::style::BgSize::Cover) => "cover".into(),
                    Some(crate::style::BgSize::Contain) => "contain".into(),
                    Some(crate::style::BgSize::Len(w, h)) => {
                        format!("{} {}", fmt_dim(w), fmt_dim(h))
                    }
                },
            ),
            // ── Bordas por lado: reportam o EFETIVO (com o fallback da uniforme),
            // que é o que o browser reporta — `border: 1px solid red` faz o
            // `border-top-color` responder `rgb(255, 0, 0)`, não vazio.
            "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width" => fmt_px(side_of(self, &n).width),
            "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style" => format!("{:?}", side_of(self, &n).style).to_ascii_lowercase(),
            "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color" => fmt_color(side_of(self, &n).color),
            "outline-width" => self.outline_width.map(fmt_px).unwrap_or_default(),
            "outline-style" => self
                .outline_style
                .map(|s| format!("{s:?}").to_ascii_lowercase())
                .unwrap_or_default(),
            "outline-color" => self.outline_color.map(fmt_color).unwrap_or_default(),
            "outline-offset" => self.outline_offset.map(fmt_px).unwrap_or_default(),
            // ── Texto / listas / fluxo ────────────────────────────────────────
            "vertical-align" => self
                .vertical_align
                .map(|v| v.css().to_string())
                .unwrap_or_default(),
            "clear" => self.clear.map(|c| c.css().to_string()).unwrap_or_default(),
            "word-break" => self
                .word_break
                .map(|w| w.css().to_string())
                .unwrap_or_default(),
            "overflow-wrap" | "word-wrap" => self
                .overflow_wrap
                .map(|w| w.css().to_string())
                .unwrap_or_default(),
            "direction" => self
                .direction
                .map(|d| d.css().to_string())
                .unwrap_or_default(),
            "writing-mode" => self
                .writing_mode
                .map(|w| w.css().to_string())
                .unwrap_or_default(),
            "text-indent" => self.text_indent.map(fmt_dim).unwrap_or_default(),
            "list-style-type" => self
                .list_style_type
                .map(|t| t.css().to_string())
                .unwrap_or_default(),
            // `url(...)` sempre com aspas duplas no computed — ver `fmt_url`
            // para porquê e para os outros dois sítios que a mesma regra
            // atinge (`background-image`, `cursor`, ambos em `fmt/mod.rs`).
            "list-style-image" => self
                .list_style_image
                .as_deref()
                .map(fmt_url)
                .unwrap_or_default(),
            "list-style-position" => self
                .list_style_position
                .map(|p| p.css().to_string())
                .unwrap_or_default(),
            // ── Tabela ────────────────────────────────────────────────────────
            "border-collapse" => self
                .border_collapse
                .map(|c| c.css().to_string())
                .unwrap_or_default(),
            // O Chrome responde os DOIS eixos sempre (`2px 2px`), mesmo quando a
            // folha declarou um só — é o valor computado, não o declarado.
            "border-spacing" => self
                .border_spacing
                .map(|s| format!("{} {}", fmt_dim(s.h), fmt_dim(s.v)))
                .unwrap_or_default(),
            "table-layout" => self
                .table_layout
                .map(|t| t.css().to_string())
                .unwrap_or_default(),
            // `cursor: url(...), pointer` — só a PRIMEIRA forma funcional (se
            // houver) leva aspas; o resto do shorthand (a keyword de
            // fallback) fica como está. `fmt_url` devolve `raw` inalterado
            // quando não reconhece `url(...)`, por isso aplicá-la ao valor
            // inteiro só muda algo quando o valor É (ou começa por) uma url.
            "cursor" => self
                .cursor
                .as_deref()
                .map(|v| match v.split_once(',') {
                    Some((url, resto)) if url.trim_start().starts_with("url(") => {
                        format!("{}, {}", fmt_url(url.trim()), resto.trim())
                    }
                    _ => fmt_url(v),
                })
                .unwrap_or_default(),
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
                    fmt_flex_wrap(w.unwrap_or(crate::style::FlexWrap::NoWrap))
                ),
            },
            // O 2º lote responde do seu próprio módulo — ver `style::vocab`.
            _ => return None,
        })
    }
}
