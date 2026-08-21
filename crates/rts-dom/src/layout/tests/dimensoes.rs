//! Dimensões: `min`/`max` width e height, percentagens, `calc`, unidades
//! relativas.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada. A indentação de 4 espaços é a do `mod tests` de origem e foi
//! MANTIDA: há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;


    #[test]
    fn min_max_width_clamp() {
        // VALIDADO no Chrome: used_width = clamp(min, width, max) (#1751).
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let cases = [
            ("width:500px;max-width:300px", 300.0), // max limita
            ("width:50px;min-width:200px", 200.0),  // min eleva
            ("width:1000px;max-width:400px;min-width:100px", 400.0), // clamp
            ("width:600px;max-width:50%", 400.0),   // % de 800
        ];
        for (style, expected) in cases {
            let dom = parse_html_to_dom(&format!("<div id=\"t\" style=\"{style}\">x</div>"));
            let t = dom.query("#t").unwrap();
            let ctx = LayoutCtx {
                viewport_w: 800.0,
                viewport_h: 600.0,
                measurer: &ApproxMeasurer,
            };
            let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
            assert!(
                (rect.w - expected).abs() < 1.0,
                "{style}: w={} esperado {expected}",
                rect.w
            );
        }
    }


    #[test]
    fn min_max_height_clamp() {
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        // height:500 max-height:200 → caixa de 200.
        let dom = parse_html_to_dom(
            "<div id=\"t\" style=\"height:500px;max-height:200px;width:100px\">x</div>",
        );
        let t = dom.query("#t").unwrap();
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
        assert!((rect.h - 200.0).abs() < 1.0, "max-height: h={}", rect.h);
        // min-height:300 num conteúdo pequeno → caixa de 300.
        let dom2 =
            parse_html_to_dom("<div id=\"t\" style=\"min-height:300px;width:100px\">x</div>");
        let t2 = dom2.query("#t").unwrap();
        let rect2 = bounding_rect(&dom2, dom2.resolve(t2).unwrap(), &ctx).unwrap();
        assert!(rect2.h >= 300.0, "min-height: h={}", rect2.h);
    }


    #[test]
    fn text_align_desloca_o_texto() {
        // text-align center/right desloca o texto pelo espaço livre (#1749).
        let dom = parse_html_to_dom(
            "<style>#c{text-align:center;width:400px}#r{text-align:right;width:400px}</style><div id=\"c\">x</div><div id=\"r\">y</div>",
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
                DisplayItem::Text { text, x, .. } => Some((text.to_string(), *x)),
                _ => None,
            })
            .collect();
        // "x" (1 char, ~8px = 16×0.5) centrado em 400 → x ≈ (400-8)/2 = 196.
        let cx = texts.iter().find(|(t, _)| t == "x").unwrap().1;
        assert!((cx - 196.0).abs() < 2.0, "center: {cx}");
        // "y" à direita → x ≈ 400-8 = 392.
        let rx = texts.iter().find(|(t, _)| t == "y").unwrap().1;
        assert!((rx - 392.0).abs() < 2.0, "right: {rx}");
    }


    #[test]
    fn calc_de_altura_resolve_contra_a_altura() {
        // `calc(100% - 560px)` num `height` resolve o `%` contra a ALTURA do
        // containing block (800), não a largura (1000): 800-560=240, não 440.
        let dom = parse_html_to_dom(
            "<div style='height:800px'>\
             <div id=c style='height:calc(100% - 560px);background:#eee'>x</div>\
             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let c = dom.query("#c").unwrap();
        let idx = dom.resolve(c).unwrap();
        assert!(
            (list.geometry().rects[&idx].h - 240.0).abs() < 2.0,
            "calc height: {} (esperado 240 = 800-560)",
            list.geometry().rects[&idx].h
        );
    }


    #[test]
    fn flex_grow_vertical_da_altura_a_filho_100pct() {
        // flex column 800px: navbar 60 + item flex-grow:1 (cresce p/ 740) e o
        // filho height:100% do item resolve contra os 740 (não a altura própria).
        let dom = parse_html_to_dom(
            "<div style='display:flex;flex-direction:column;height:800px'>\
             <div style='height:60px'>nav</div>\
             <div style='flex-grow:1'><div id=alvo style='height:100%;background:#00f'>x</div></div>\
             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let alvo = dom.query("#alvo").unwrap();
        let idx = dom.resolve(alvo).unwrap();
        let r = list.geometry().rects[&idx];
        assert!(
            (r.h - 740.0).abs() < 2.0,
            "altura do filho 100%: {} (esperado 740)",
            r.h
        );
        assert!((r.y - 60.0).abs() < 2.0, "y do filho: {}", r.y);
    }


    #[test]
    fn height_percent_resolve_contra_altura_do_pai() {
        // `height:%` resolve contra a ALTURA do containing block (antes resolvia
        // errado contra a LARGURA). Pai height:200 → filho 50% = 100.
        let list = layout(
            "<div style='height:200; background:#111'>\
               <div style='height:50%; background:#222'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].h, 200.0);
        assert_eq!(r[1].h, 100.0, "50% de 200: {r:?}");
        // pai SEM height (auto): height:% do filho vira auto (altura do conteúdo,
        // 1 linha ≈ 26) — fiel ao browser, não 50% da largura (que daria 300).
        let l2 = layout(
            "<div style='background:#111'><div style='height:50%; background:#222'>x</div></div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!(
            r2[1].h < 40.0,
            "height %% com pai auto = altura natural: {r2:?}"
        );
    }


    #[test]
    fn unidades_relativas_em_padding_e_margem_negativa() {
        // padding: 1rem = 16px (root 16, o default de browser) — o `p-3` do
        // Bootstrap; margem NEGATIVA puxa (os gutters `.row` usam margin -12px).
        let list = layout("<div style='background:#111; padding:1rem'>x</div>", 600.0);
        let tx = all_texts(&list)
            .first()
            .map(|(_, x, y, _)| (*x, *y))
            .unwrap();
        assert_eq!(tx, (16.0, 16.0), "texto após o padding de 1rem = 16px");
        // margem negativa: o segundo bloco com margin-top:-10 SOBE sobre o primeiro.
        let l2 = layout(
            "<div style='background:#111; height:30'>a</div>             <div style='background:#222; height:30; margin-top:-10px'>b</div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert_eq!(r2[1].y, 20.0, "30 - 10 (negativa nao clampa): {r2:?}");
    }


    #[test]
    fn max_width_em_bate_com_o_chrome() {
        // `max-width: 42em` com root 16 = 672px — o `.cover-container` do Bootstrap
        // cover, VALIDADO numero-a-numero no Chrome (viewport 1000: rect 164,0,672).
        let list = layout(
            "<div style='background:#111; max-width:42em; margin:0 auto; height:50'>x</div>",
            1000.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].w, 672.0, "42em x 16: {r:?}");
        assert_eq!(r[0].x, 164.0, "(1000-672)/2, centrado como no Chrome");
    }
