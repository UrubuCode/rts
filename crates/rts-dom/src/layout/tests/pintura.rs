//! Pintura e tipografia: cor, itálico, sublinhado do UA, `line-height`,
//! `text-transform`, visibilidade, opacidade, `mask-image` e substituídos.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;

    #[test]
    fn em_produz_texto_italico() {
        // A UA-stylesheet regista `<em>` como FLAG_ITALIC; sem regra de autor
        // nenhuma, é ela a única fonte da resposta.
        let itens = textos_italicos("<p><em>enfase</em></p>");
        assert!(
            itens.iter().any(|(t, i)| t.contains("enfase") && *i),
            "o <em> devia sair itálico: {itens:?}"
        );
    }

    #[test]
    fn texto_sem_font_style_nao_e_italico() {
        let itens = textos_italicos("<p>direito</p>");
        assert!(
            itens.iter().all(|(_, i)| !*i),
            "sem font-style nada é itálico: {itens:?}"
        );
    }

    #[test]
    fn font_style_normal_no_filho_desliga_o_italico() {
        // Regra de AUTOR vence a UA — e vence no sentido de DESLIGAR, que é o
        // caso que um `unwrap_or(ua)` mal escrito perderia.
        let itens = textos_italicos("<p><em style='font-style:normal'>direito</em></p>");
        assert!(
            itens.iter().any(|(t, i)| t.contains("direito") && !*i),
            "font-style:normal no <em> devia desligar: {itens:?}"
        );
    }

    #[test]
    fn italico_do_pai_e_herdado_pelo_filho() {
        let itens = textos_italicos("<p style='font-style:italic'><span>herdado</span></p>");
        assert!(
            itens.iter().any(|(t, i)| t.contains("herdado") && *i),
            "o span devia herdar o itálico do <p>: {itens:?}"
        );
    }

    #[test]
    fn italico_e_negrito_sao_eixos_independentes() {
        // `font-weight:bold` + `<em>` é bold-italic: colapsar os dois bits num
        // só perderia esta combinação, que é o que a família "bold-italic" do
        // egui existe para pintar.
        //
        // O peso vem de uma regra de AUTOR e não de um `<b>` de propósito: o
        // negrito da UA-stylesheet NÃO é lido por ninguém (o mesmo defeito que
        // este lote corrige para o itálico, ainda por corrigir para o peso), e
        // um `<b>` aqui faria o teste falhar por uma razão que não é a que ele
        // pergunta.
        crate::block::install_ua_defaults();
        let list = layout("<p><em style='font-weight:bold'>ambos</em></p>", 800.0);
        let achou = list.materialized().iter().any(|it| {
            matches!(it, DisplayItem::Text { text, bold, italic, .. }
                if text.contains("ambos") && *bold && *italic)
        });
        assert!(achou, "<em><b> devia sair negrito E itálico");
    }

    /// Um `checkbox`/`radio` é um quadradinho de 13x13, não um campo de texto de
    /// 190x26 — e a medida que o fluxo RESERVA na linha é a mesma que a emissão
    /// pinta, porque agora as duas perguntam à mesma função.
    #[test]
    fn checkbox_e_um_quadrado_e_nao_um_campo_de_texto() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let caixa = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
            *list
                .geometry()
                .rects
                .get(&idx)
                .expect("o input devia ter caixa")
        };
        // Tolerância e não igualdade: 13 é o tamanho intrínseco, mas chega aqui
        // por uma cadeia de contas em f32 e sai 13.000001 conforme o que a
        // precede na linha. O que o teste afirma é o QUADRADO de 13, não a
        // reprodutibilidade bit a bit de uma soma de floats.
        let quadrado = |r: Rect, quem: &str| {
            assert!(
                (r.w - 13.0).abs() < 0.01 && (r.h - 13.0).abs() < 0.01,
                "{quem} devia ser um quadrado de 13: {r:?}"
            );
        };
        quadrado(
            caixa("<div>a <input id='x' type='checkbox'> b</div>"),
            "o checkbox",
        );
        quadrado(
            caixa("<div>a <input id='x' type='radio'> b</div>"),
            "o radio",
        );
        // o campo de texto continua a ser um campo de texto.
        let t = caixa("<div>a <input id='x' type='text'> b</div>");
        assert!(
            t.w > 100.0,
            "campo de texto mantém a largura de campo: {t:?}"
        );
    }

    /// `height: %` num `<input>` mede-se contra a ALTURA do containing block, não
    /// contra a largura. A Wikipédia usa o "checkbox hack" — oito
    /// `<input type=checkbox>` com `height:100%` — e cada um vinha com a largura
    /// da viewport de altura: o pior rácio de erro da página inteira.
    #[test]
    fn altura_percentual_de_input_mede_se_no_eixo_vertical() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let dom = parse_html_to_dom(
            "<div style='height:400px'><input id='x' type='checkbox' style='height:100%'></div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("sem caixa");
        assert_eq!(r.h, 400.0, "100% da ALTURA do pai, não da largura: {r:?}");
    }

    #[test]
    fn svg_reserva_a_caixa() {
        // um `<svg>` reserva a caixa (width/height do atributo, ou razão do
        // viewBox) mesmo sem desenhar o vetor — o logo/ícones ocupam o espaço.
        let dom = parse_html_to_dom(
            "<div><svg id=logo width=272 height=92 viewBox='0 0 272 92'></svg></div>\
             <svg id=ico width=24 height=24></svg>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let r = |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let logo = r("#logo");
        assert!(
            (logo.w - 272.0).abs() < 1.0 && (logo.h - 92.0).abs() < 1.0,
            "logo: {}x{}",
            logo.w,
            logo.h
        );
        let ico = r("#ico");
        assert!(
            (ico.w - 24.0).abs() < 1.0 && (ico.h - 24.0).abs() < 1.0,
            "ico: {}x{}",
            ico.w,
            ico.h
        );
    }

    #[test]
    fn link_ua_azul_sublinhado_por_run() {
        // `<a>` sem CSS de autor: cor azul default + underline (deco=1) SÓ no seu
        // texto — o texto adjacente do parágrafo fica preto e sem decoração.
        crate::block::define(
            "p",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let dom = parse_html_to_dom("<p>antes <a href=x>link</a> depois</p>");
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let segs: Vec<(String, u32, u8)> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text {
                    text,
                    color,
                    decoration,
                    ..
                } => Some((text.to_string(), *color, *decoration)),
                _ => None,
            })
            .collect();
        let link = segs
            .iter()
            .find(|(t, ..)| t.contains("link"))
            .expect("run do link");
        assert_eq!(link.1, 0x0000EEFF, "link azul");
        assert_eq!(link.2, 1, "link sublinhado");
        // o texto ao redor (preto, sem deco) é um segmento SEPARADO.
        assert!(
            segs.iter()
                .any(|(t, c, d)| t.contains("antes") && *c == 0x000000FF && *d == 0)
        );
    }

    #[test]
    fn line_height_e_text_transform() {
        // line-height do CSS respeitado + text-transform aplicado (#1749). Usa <div>
        // (sem margin default da UA, ao contrário de <p>) p/ isolar o line-height.
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let dom = parse_html_to_dom(
            "<style>div{line-height:3;text-transform:uppercase}</style><div>oi</div><div>tchau</div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(String, f32)> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { text, y, .. } => Some((text.to_string(), *y)),
                _ => None,
            })
            .collect();
        // uppercase aplicado.
        assert!(texts.iter().any(|(t, _)| t == "OI"));
        assert!(texts.iter().any(|(t, _)| t == "TCHAU"));
        // line-height:3 = 3×16 = 48px entre as linhas (div sem margin).
        let y_oi = texts.iter().find(|(t, _)| t == "OI").unwrap().1;
        let y_tchau = texts.iter().find(|(t, _)| t == "TCHAU").unwrap().1;
        assert!(
            (y_tchau - y_oi - 48.0).abs() < 5.0,
            "line-height: {y_oi} → {y_tchau}"
        );
    }

    #[test]
    fn bounding_rect_dos_cards() {
        // getBoundingClientRect: o border-box de cada nó-bloco. Os 3 cards (flex,
        // 32% border-box) têm os MESMOS rects que o dump mostra (x=20/322/624, w=302).
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
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma x=0.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card' id='a'>1</div><div class='card' id='b'>2</div><div class='card' id='c'>3</div>\
             </row>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        // resolve os NodeIdx dos 3 cards e mede cada um.
        let a = dom.resolve(dom.query("#a").unwrap()).unwrap();
        let b = dom.resolve(dom.query("#b").unwrap()).unwrap();
        let c = dom.resolve(dom.query("#c").unwrap()).unwrap();
        let ra = bounding_rect(&dom, a, &ctx).expect("card a tem rect");
        let rb = bounding_rect(&dom, b, &ctx).expect("card b tem rect");
        let rc = bounding_rect(&dom, c, &ctx).expect("card c tem rect");
        // border-box = 32% de 1000 = 320 cada; lado a lado.
        assert!((ra.w - 320.0).abs() < 1.0, "largura ~320: {ra:?}");
        assert!((rb.w - 320.0).abs() < 1.0);
        assert!((rc.w - 320.0).abs() < 1.0);
        assert_eq!(ra.x, 0.0); // (sem padding no body de teste, x começa em 0)
        assert!(
            rb.x > ra.x && rc.x > rb.x,
            "X crescente: {ra:?} {rb:?} {rc:?}"
        );
        assert_eq!(ra.y, rb.y); // mesma linha (flex)
    }

    #[test]
    fn bounding_rect_none_para_texto() {
        // texto/inline não tem rect próprio (a API só dá rect de elemento-bloco).
        let dom = parse_html_to_dom("<p>oi</p>");
        crate::block::define(
            "p",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let p = dom.resolve(dom.query("p").unwrap()).unwrap();
        // o <p> (bloco) TEM rect.
        assert!(bounding_rect(&dom, p, &ctx).is_some());
        // o nó de texto filho NÃO tem (não é bloco).
        let txt = dom.node(p).children[0];
        assert!(bounding_rect(&dom, txt, &ctx).is_none());
    }
    /// `visibility:hidden` esconde SEM tirar do fluxo — é o que o distingue de
    /// `display:none`, e a distinção decide layouts reais: o MediaWiki esconde
    /// os menus que abrem ao clique assim, e sem ela o menu aparecia por cima
    /// do artigo.
    #[test]
    fn visibility_hidden_ocupa_espaco_e_nao_pinta() {
        let dom = parse_html_to_dom(
            "<style>.oculto{visibility:hidden}</style>             <div class='oculto' style='height:50px;background:#ff0000'>invisível</div>             <div style='height:20px'>depois</div>",
        );
        let m = ApproxMeasurer;
        let ctx = LayoutCtx {
            viewport_w: 400.0,
            viewport_h: 300.0,
            measurer: &m,
        };
        let lista = layout_document(&dom, &ctx);
        let itens = lista.materialized();
        // o segundo bloco começa DEPOIS dos 50px do primeiro: o espaço ficou.
        let segundo = dom.query_all("div")[1];
        let y = lista
            .rect_of(dom.resolve(segundo).unwrap())
            .map(|r| r.y)
            .unwrap_or(0.0);
        assert!(
            y >= 50.0,
            "o elemento oculto tem de ocupar o espaço dele (y={y})"
        );
        // e nada do que ele pinta tem alpha.
        for item in &itens {
            match item {
                DisplayItem::SolidRect { color, .. } if (color >> 8) == 0xFF0000 => {
                    assert_eq!(
                        color & 0xFF,
                        0,
                        "o fundo de um elemento oculto não é pintado"
                    );
                }
                DisplayItem::Text { text, color, .. } if text.contains("invisível") => {
                    assert_eq!(
                        color & 0xFF,
                        0,
                        "o texto de um elemento oculto não é pintado"
                    );
                }
                _ => {}
            }
        }
    }

    /// Um `<input>` com `opacity: 0` não pinta fundo nem borda.
    ///
    /// Vale uma página inteira: a Wikipédia usa o "checkbox hack" — um
    /// `<input type=checkbox>` invisível, dimensionado à altura do documento,
    /// para abrir menus sem JavaScript. Com o fundo branco da UA pintado opaco
    /// por cima de tudo, o que se via era uma janela EM BRANCO, com o layout e a
    /// lista de pintura inteiramente corretos.
    #[test]
    fn input_com_opacidade_zero_nao_pinta_fundo_nem_borda() {
        let dom =
            parse_html_to_dom("<input id='oculto' style='opacity:0;width:200px;height:100px'>");
        let m = ApproxMeasurer;
        let ctx = LayoutCtx {
            viewport_w: 400.0,
            viewport_h: 300.0,
            measurer: &m,
        };
        let lista = layout_document(&dom, &ctx);
        for item in lista.materialized() {
            match item {
                DisplayItem::SolidRect { color, .. } | DisplayItem::Border { color, .. } => {
                    assert_eq!(
                        color & 0xFF,
                        0,
                        "um input invisível não pinta (cor #{color:08X})"
                    );
                }
                _ => {}
            }
        }
    }

    /// Um elemento com `background-color` E `mask-image` não emite fundo.
    ///
    /// É o ícone monocromático do MediaWiki (`.cdx-button__icon`: cor de fundo
    /// mais uma máscara que lhe dá a forma). Sem carregar a máscara, pintar o
    /// fundo dá o retângulo inteiro — os blocos cinzentos que apareciam no lugar
    /// do ☰ e da lupa na Wikipédia. O `-webkit-mask-image` conta igual: a folha
    /// real declara os dois lado a lado.
    #[test]
    fn elemento_com_mask_image_nao_pinta_fundo() {
        for prop in ["mask-image", "-webkit-mask-image"] {
            let html = format!(
                "<div style='background-color:#404244;{prop}:url(icone.svg);width:20px;height:20px'></div>"
            );
            let dom = parse_html_to_dom(&html);
            let m = ApproxMeasurer;
            let ctx = LayoutCtx {
                viewport_w: 400.0,
                viewport_h: 300.0,
                measurer: &m,
            };
            let lista = layout_document(&dom, &ctx);
            for item in lista.materialized() {
                if let DisplayItem::SolidRect { color, .. } = item {
                    assert_ne!(
                        color, 0x404244FF,
                        "com `{prop}` o fundo não é pintado — sem a máscara seria um bloco"
                    );
                }
            }
        }
    }

    /// O mesmo fundo, SEM máscara declarada, continua a pintar — a supressão é
    /// da máscara, não uma exceção nova para a cor.
    #[test]
    fn elemento_sem_mask_image_pinta_o_fundo() {
        let dom = parse_html_to_dom(
            "<div style='background-color:#404244;width:20px;height:20px'></div>",
        );
        let m = ApproxMeasurer;
        let ctx = LayoutCtx {
            viewport_w: 400.0,
            viewport_h: 300.0,
            measurer: &m,
        };
        let lista = layout_document(&dom, &ctx);
        let pintou = lista
            .materialized()
            .iter()
            .any(|i| matches!(i, DisplayItem::SolidRect { color, .. } if *color == 0x404244FF));
        assert!(pintou, "sem máscara, o fundo declarado é pintado");
    }
