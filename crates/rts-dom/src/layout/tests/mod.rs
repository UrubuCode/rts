//! Os testes do layout, movidos de `layout.rs` sem alteração de conteúdo.
//!
//! Aqui ficam só os HELPERS partilhados; cada área tem o seu submódulo. O
//! `const FRASE` vive aqui porque dois submódulos o usam — no original estava
//! ao nível do `mod tests`, e é o item que o chunker por `    }` não via.

mod bloco;
mod cache;
mod cache_flex;
mod colapso;
mod dimensoes;
mod flex;
mod flex_avancado;
mod flex_coluna_shrink_corpus;
mod flex_max_width_corpus;
mod flex_item_floats_corpus;
mod intrinseco_whitespace_corpus;
mod svg_atributo_corpus;
mod borda_por_lado_corpus;
mod ib_nowrap_corpus;
mod justify_fisico_corpus;
mod clearfix_corpus;
mod pseudo_flex_corpus;
mod borda_em_corpus;
mod float_corpus;
mod grid;
mod grid_colocacao;
mod grid_corpus;
mod grid_intrinseco_corpus;
mod hifen_corpus;
mod fontes_corpus;
mod inline;
mod inline_corpus;
mod inline_fragmentos_corpus;
mod pintura;
mod pintura_transform_clip;
mod pintura_juncao;
mod replaced_fundo;
mod replaced_pixels;
mod posicionado;
mod position_corpus;
mod texto_lote_s;
mod transform_corpus;
    use super::*;
    use crate::dom::parse_html_to_dom;

    /// Tolerância da comparação reuso × cálculo.
    ///
    /// Reusar um fragmento numa posição nova é somar um deslocamento às
    /// coordenadas, e somar não dá bit a bit o mesmo que calcular a posição do
    /// zero: `67.4` calculado vira `67.399994` deslocado. É a mesma aritmética
    /// que qualquer motor de layout com reuso tem, a diferença é da ordem de
    /// 1e-5 pontos — invisível em tela e irrelevante para hit-test. O que o
    /// teste NÃO tolera é diferença de conteúdo, de contagem ou de ordem.
    const TOL: f32 = 0.01;

    fn rects_equivalentes(a: &Rect, b: &Rect) -> bool {
        (a.x - b.x).abs() < TOL
            && (a.y - b.y).abs() < TOL
            && (a.w - b.w).abs() < TOL
            && (a.h - b.h).abs() < TOL
    }

    /// Os quatro cantos iguais a menos da tolerância — a mesma pergunta que se
    /// fazia a um raio só, feita quatro vezes em vez de uma.
    fn cantos_equivalentes(a: &Corners, b: &Corners) -> bool {
        (a.tl - b.tl).abs() < TOL
            && (a.tr - b.tr).abs() < TOL
            && (a.br - b.br).abs() < TOL
            && (a.bl - b.bl).abs() < TOL
    }

    /// Dois itens de pintura iguais a menos da tolerância acima. Texto, cor e
    /// tipo têm de bater EXATAMENTE: só a geometria admite o erro do
    /// deslocamento.
    fn itens_equivalentes(a: &DisplayItem, b: &DisplayItem) -> bool {
        use DisplayItem as D;
        match (a, b) {
            (
                D::SolidRect {
                    rect: ra,
                    color: ca,
                    radius: da,
                },
                D::SolidRect {
                    rect: rb,
                    color: cb,
                    radius: db,
                },
            ) => rects_equivalentes(ra, rb) && ca == cb && cantos_equivalentes(da, db),
            (
                D::Border {
                    rect: ra,
                    width: wa,
                    color: ca,
                    radius: da,
                },
                D::Border {
                    rect: rb,
                    width: wb,
                    color: cb,
                    radius: db,
                },
            ) => {
                rects_equivalentes(ra, rb)
                    && (wa - wb).abs() < TOL
                    && ca == cb
                    && (da - db).abs() < TOL
            }
            (
                D::Text {
                    x: xa,
                    y: ya,
                    text: ta,
                    color: ca,
                    size: sa,
                    mono: ma,
                    bold: ba,
                    italic: ia,
                    letter_spacing: la,
                    decoration: dea,
                },
                D::Text {
                    x: xb,
                    y: yb,
                    text: tb,
                    color: cb,
                    size: sb,
                    mono: mb,
                    bold: bb,
                    italic: ib,
                    letter_spacing: lb,
                    decoration: deb,
                },
            ) => {
                (xa - xb).abs() < TOL
                    && (ya - yb).abs() < TOL
                    && ta == tb
                    && ca == cb
                    && (sa - sb).abs() < TOL
                    && ma == mb
                    && ba == bb
                    && ia == ib
                    && (la - lb).abs() < TOL
                    && dea == deb
            }
            (D::EndClip { .. }, D::EndClip { .. }) => true,
            // As demais variantes não aparecem neste corpus; comparar por
            // igualdade estrita aqui é o certo — se um dia aparecerem com
            // deslocamento, o teste falha e o braço é escrito.
            _ => a == b,
        }
    }

    /// Registra `<div>` como bloco vertical (os testes precisam que a tag tenha
    /// layout de bloco para entrar no caminho `layout_block` dos filhos).
    fn def_div() {
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
    }

    /// Os itens de texto de um layout, como `(texto, itálico)`. Serve às provas
    /// do `font-style` — o que se pergunta é quais palavras saem inclinadas.
    fn textos_italicos(html: &str) -> Vec<(String, bool)> {
        crate::block::install_ua_defaults();
        let list = layout(html, 800.0);
        list.materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { text, italic, .. } => Some((text.to_string(), *italic)),
                _ => None,
            })
            .collect()
    }

    /// Layout determinístico com medidor aproximado e viewport fixo.
    ///
    /// Prefixa `body { margin: 0 }`: a folha de UA (lote I, `style/ua.css`)
    /// dá ao `body` os 8px que o Chrome também dá, e este corpus de testes
    /// mede coordenadas de conteúdo a partir de `(0,0)` — como o corpus real
    /// de `tests/css/` faz, que declara o mesmo reset em toda fixture que não
    /// é SOBRE a margem do body. Sem isto, cada rect vinha deslocado 8px.
    fn layout(html: &str, vw: f32) -> DisplayList {
        def_div();
        let dom = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
        let ctx = LayoutCtx {
            viewport_w: vw,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        layout_document(&dom, &ctx)
    }

    /// Primeiro `SolidRect` da lista (o fundo da 1ª caixa) — atalho de assert.
    ///
    /// PLANA (`materialized`), como o `all_rects`: desde que a saída passou a ser
    /// uma árvore de fragmentos, o fundo de um filho de bloco vive no fragmento
    /// dele e não no buffer próprio da lista. Ler `list.items` direto respondia
    /// "não há SolidRect nenhum" numa página que pinta — o erro não estava no
    /// motor, estava na navegação.
    fn first_rect(list: &DisplayList) -> Rect {
        list.materialized()
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("esperava ao menos um SolidRect")
    }

    /// Todos os itens de TEXTO, na ordem de pintura: `(texto, x, y, cor)`.
    /// Mesma razão do `first_rect` para não ler `list.items`.
    fn all_texts(list: &DisplayList) -> Vec<(String, f32, f32, u32)> {
        list.materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text {
                    text, x, y, color, ..
                } => Some((text.to_string(), *x, *y, *color)),
                _ => None,
            })
            .collect()
    }

    /// Helper: layout de um HTML num row flex e os rects (x ordenado) dos N cards.
    fn flex_card_rects(style: &str, n_cards: usize, vw: f32) -> Vec<Rect> {
        crate::block::define(
            "row",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        // flex-shrink:0 nos cards: estes testes validam JUSTIFY (inclusive em
        // overflow real, como o Chrome foi medido); sem o 0, o shrink default=1
        // (agora implementado!) encolheria os itens e não haveria overflow.
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; estes testes
        // medem `x` a partir do viewport.
        let mut html = format!(
            "<style>body{{margin:0}}row{{display:flex;{style}}} .c{{width:100px;flex-shrink:0;background:#111}}</style><row>"
        );
        for i in 0..n_cards {
            html.push_str(&format!("<div class='c' id='c{i}'>x</div>"));
        }
        html.push_str("</row>");
        let dom = parse_html_to_dom(&html);
        let ctx = LayoutCtx {
            viewport_w: vw,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let mut rects: Vec<Rect> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        rects.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        rects
    }

    /// Coleta todos os SolidRect da lista, em ordem (container primeiro, filhos
    /// depois — o fundo do container é inserido ATRÁS dos filhos).
    fn all_rects(list: &DisplayList) -> Vec<Rect> {
        // PLANA: os itens de uma subárvore reusada não estão no buffer próprio,
        // e um teste que lesse só ele veria a página pela metade.
        list.materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }

    /// Uma frase comprida, para forçar várias linhas com o `ApproxMeasurer`
    /// (0,5 × font-size por carácter): 16px × 0,5 = 8pt por carácter.
    const FRASE: &str = "alfa beta gama delta epsilon zeta eta teta iota kapa lambda mi ni xi omicron pi ro sigma tau upsilon fi qui psi omega";
