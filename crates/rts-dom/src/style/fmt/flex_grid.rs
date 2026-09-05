//! Espaçamento de texto, transformação, flex e grid
//!
//! Os braços vieram do `match` de `fmt.rs` VERBATIM: o `impl` próprio
//! mantém o `self` a ser `self` e a indentação a mesma, que é o que
//! torna a extração comparável linha a linha com o original.

use super::*;

impl ComputedStyle {
    pub(in crate::style::fmt) fn get_property_flex_grid(&self, n: &str) -> Option<String> {
        Some(match n {
            "letter-spacing" => self
                .letter_spacing
                .map(|v| {
                    if v == 0.0 {
                        "normal".to_string()
                    } else {
                        format!("{v}px")
                    }
                })
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
            // Como o browser: `getComputedStyle().transform` devolve a MATRIZ
            // resolvida (`matrix(a, b, c, d, e, f)`), não a lista de funções
            // declaradas. Sem o box-size aqui (é uma pergunta de estilo, não
            // de layout), a fração `%` do translate resolve a 0 — o mesmo
            // corte que `transform_origin` já tinha antes deste lote.
            "transform" => self
                .transform
                .filter(|t| !t.is_identity())
                .map(|t| {
                    let m = t.ops.resolve(0.0, 0.0);
                    format!("matrix({}, {}, {}, {}, {}, {})", m.a, m.b, m.c, m.d, m.e, m.f)
                })
                .unwrap_or_else(|| "none".to_string()),
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
                Some(crate::style::values::Visibility::Collapse) => "collapse".into(),
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
            _ => return None,
        })
    }
}
