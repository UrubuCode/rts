//! SONDA TEMPORÁRIA — quantos ELEMENTOS acabam com valor efetivo, por propriedade.
//!
//! A auditoria ordenou os candidatos por DECLARAÇÕES numa folha. Essa régua já
//! enganou uma vez de forma medida: 2 378 dos 2 411 insets relativos estavam no
//! `tw2.css`, em classes utilitárias que nenhum elemento casa. Uma declaração é
//! uma intenção; um elemento com valor efetivo é um pixel a pintar.
//!
//! Varre elementos E pseudo-elementos: a primeira versão desta sonda via só
//! elementos e teria respondido sobre metade da pergunta.

use crate::style::props::ComputedStyle;

/// `(nome, tem valor?)` — uma linha por campo que a auditoria apontou.
fn campos(c: &ComputedStyle) -> Vec<(&'static str, bool)> {
    vec![
        ("grid-column-start", c.grid_column_start.is_some()),
        ("grid-column-end", c.grid_column_end.is_some()),
        ("grid-row-start", c.grid_row_start.is_some()),
        ("grid-row-end", c.grid_row_end.is_some()),
        ("grid-auto-flow", c.grid_auto_flow.is_some()),
        ("grid-auto-columns", c.grid_auto_columns.is_some()),
        ("align-content", c.align_content.is_some()),
        ("justify-self", c.justify_self.is_some()),
        ("transform-origin", c.transform_origin.is_some()),
        ("object-fit", c.object_fit.is_some()),
        ("object-position", c.object_position.is_some()),
        ("background-image url()", c.bg_image.as_deref().is_some_and(|u| u.contains("url("))),
        ("background-position", c.bg_position.is_some()),
        ("background-size", c.bg_size.is_some()),
        ("background-repeat", c.bg_repeat.is_some()),
        ("background-clip", c.background_clip.is_some()),
        ("mask-image", c.mask_image.is_some()),
        ("mix-blend-mode", c.mix_blend_mode.is_some()),
        ("text-shadow", c.text_shadow.is_some()),
        ("text-decoration-color", c.text_decoration_color.is_some()),
        ("word-spacing", c.word_spacing.is_some_and(|v| v != 0.0)),
        // NAO-DEFAULT: `ter valor` nao e `mudar alguma coisa`. `direction:ltr` e
        // `tab-size:8` sao os iniciais — contar essas como candidatas inflaciona
        // a coluna com elementos onde honrar a propriedade nao move um pixel.
        ("direction != ltr", matches!(c.direction, Some(crate::style::Direction::Rtl))),
        ("tab-size != 8", c.tab_size.is_some_and(|v| v != 8.0)),
        ("caption-side != top", matches!(c.caption_side, Some(crate::style::vocab::CaptionSide::Bottom))),
        ("scrollbar-width != auto", matches!(c.scrollbar_width, Some(crate::style::vocab::ScrollbarWidth::Thin | crate::style::vocab::ScrollbarWidth::None))),
        ("pointer-events != auto", matches!(c.pointer_events, Some(crate::style::vocab::PointerEvents::None))),
        ("tab-size", c.tab_size.is_some()),
        ("direction", c.direction.is_some()),
        ("cursor", c.cursor.is_some()),
        ("pointer-events", c.pointer_events.is_some()),
        ("zoom", c.zoom.is_some()),
        ("line-clamp", c.line_clamp.is_some()),
        ("scrollbar-width", c.scrollbar_width.is_some()),
        ("hyphens", c.hyphens.is_some()),
        ("text-wrap", c.text_wrap.is_some()),
        ("font-stretch", c.font_stretch.is_some()),
        ("caption-side", c.caption_side.is_some()),
        ("caret-color", c.caret_color.is_some()),
    ]
}

#[test]
#[ignore = "sonda: corre com --ignored; precisa das paginas reais na raiz"]
fn elementos_com_valor_efetivo() {
    let raiz = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let paginas = [
        "pagina.html",
        "google.html",
        "wa-app.html",
        "paridade/wiki-rust-en.html",
        "paridade/hn.html",
        "paridade/python-docs.html",
        "paridade/wiki-fisica.html",
        "paridade/bootstrap-cover.html",
        "examples/bootstrap-5.3.8-examples/cover/index.html",
        "examples/dashboard.html",
        "examples/claude-site/index.html",
        "examples/claude-ai-site.html",
        "examples/urubu.html",
        "site/demo.html",
    ];
    let mut total: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut por_pagina: Vec<(String, usize)> = Vec::new();

    for pg in paginas {
        let Ok(mut html) = std::fs::read_to_string(format!("{raiz}/{pg}")) else {
            println!("SONDA (sem ficheiro) {pg}");
            continue;
        };
        // A FOLHA ao lado. Sem isto mede-se a página com 5% do CSS dela: o
        // `pagina.html` traz 13 835 bytes de `<style>` e o `pagina.css` tem
        // 257 592. É a armadilha que o `docs/ui/css-support.md` já documenta
        // ("a combinada JÁ embute a folha") e eu caí nela na primeira passagem.
        let css_path = format!("{raiz}/{}", pg.trim_end_matches(".html").to_string() + ".css");
        let mut bytes_css = 0usize;
        if let Ok(css) = std::fs::read_to_string(&css_path) {
            bytes_css = css.len();
            html = format!("<style>{css}</style>{html}");
        }
        let dom = crate::dom::parse_html_to_dom(&html);
        let ids = dom.query_all("*");
        let mut n_nos = ids.len();
        for id in &ids {
            if let Some(css) = dom.computed_style(*id) {
                for (nome, tem) in campos(&css) {
                    if tem {
                        *total.entry(nome).or_default() += 1;
                    }
                }
            }
            if let Some(idx) = dom.resolve(*id) {
                for pe in [
                    crate::style::PseudoElement::Before,
                    crate::style::PseudoElement::After,
                ] {
                    if let Some(b) = dom.pseudo_box(idx, pe) {
                        n_nos += 1;
                        for (nome, tem) in campos(&b.css) {
                            if tem {
                                *total.entry(nome).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
        por_pagina.push((format!("{pg}  (+{bytes_css}B css)"), n_nos));
    }

    println!("SONDA paginas:");
    let mut soma = 0;
    for (p, n) in &por_pagina {
        println!("SONDA   {p:34} {n:6} nos (com pseudos)");
        soma += n;
    }
    println!("SONDA TOTAL DE NOS: {soma}");
    println!("SONDA --- elementos com valor efetivo, somando as {} paginas:", por_pagina.len());
    let mut linhas: Vec<_> = total.into_iter().collect();
    linhas.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (nome, n) in &linhas {
        println!("SONDA   {nome:24} {n:6}");
    }
    println!("SONDA --- ZERO em todas as paginas:");
    for (nome, _) in campos(&ComputedStyle::default()) {
        if !linhas.iter().any(|(x, _)| *x == nome) {
            println!("SONDA   {nome}");
        }
    }
}
