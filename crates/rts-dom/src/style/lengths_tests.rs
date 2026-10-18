#[cfg(test)]
mod palavras_chave_intrinsecas {
    use crate::style::{Dimension, parse::parse_inline};
    use crate::table::tests::{geometria, rect};

    /// `width: max-content` deixa de ser DESCARTADO no parse.
    ///
    /// Respondia `None` — indistinguível de "não declarado" — e o elemento
    /// tomava a largura do pai.
    #[test]
    fn max_content_e_parseado_e_nao_descartado() {
        assert_eq!(
            parse_inline("width:max-content").width,
            Some(Dimension::MaxContent)
        );
    }

    /// `min-content` é parseado e suportado; `fit-content` continua descartado.
    #[test]
    fn min_content_e_parseado_e_fit_content_e_parseado() {
        assert_eq!(
            parse_inline("width:min-content").width,
            Some(Dimension::MinContent)
        );
        assert_eq!(
            parse_inline("width:fit-content").width,
            Some(Dimension::FitContent)
        );
    }

    /// `fit-content(<length-percentage>)` — a forma FUNCIONAL — é parseada para
    /// `FitContentLength`, distinta da keyword `fit-content`, com o argumento
    /// (px ou %) preservado no `CalcLen`.
    #[test]
    fn fit_content_funcional_e_parseado_com_o_argumento() {
        match parse_inline("width:fit-content(100px)").width {
            Some(Dimension::FitContentLength(c)) => {
                assert_eq!(c.px, 100.0, "o px do argumento tem de viajar");
                assert_eq!(c.pct, 0.0);
            }
            outro => panic!("esperava FitContentLength, veio {outro:?}"),
        }
        match parse_inline("width:fit-content(40%)").width {
            Some(Dimension::FitContentLength(c)) => assert_eq!(c.pct, 40.0),
            outro => panic!("esperava FitContentLength(%), veio {outro:?}"),
        }
    }

    /// `fit-content(L)` = `max(min-content, min(L, max-content))`: com dois
    /// blocos de 60px (min-content 60, max-content 120), `fit-content(100px)`
    /// assenta nos 100 — nem encolhe ao min nem transborda ao max (WPT
    /// `fit-content-length-percentage-001`).
    #[test]
    fn fit_content_funcional_clampa_entre_min_e_max() {
        let fonte = "<div style='width:fit-content(100px);height:50px' id='alvo'>\
            <div style='display:inline-block;width:60px'></div>\
            <div style='display:inline-block;width:60px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "fit-content(100px) entre 60 e 120 tem de dar 100: {a:?}"
        );
    }

    /// Um bloco não-replaced com altura definida e `aspect-ratio` contribui
    /// `altura×razão` para o min-content do pai: `width:min-content` num pai de
    /// `<div height:100px; aspect-ratio:1/1>` dá 100, não 0 (WPT
    /// `aspect-ratio/intrinsic-size-001`).
    #[test]
    fn aspect_ratio_com_altura_contribui_para_o_min_content() {
        let fonte = "<div style='width:min-content;height:100px' id='alvo'>\
            <div style='height:100px;aspect-ratio:1/1'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "a largura transferida pela razão tem de ser o min-content: {a:?}"
        );
    }

    /// Um `max-width` DEFINIDO vence o mínimo TRANSFERIDO pela razão a partir de
    /// `min-height` (CSS Sizing 4 §5.1): `height:50; aspect-ratio:2/1;
    /// min-height:100; max-width:100` dá 100×100 — a altura usada é 100, a razão
    /// pede 200 de largura, e o max-width corta para 100 (WPT
    /// `block-aspect-ratio-045`). Antes o mínimo transferido (200) vencia.
    #[test]
    fn max_width_definido_vence_o_minimo_transferido_pela_razao() {
        let fonte = "<div style='background:green;height:50px;aspect-ratio:2/1;min-height:100px;max-width:100px' id='alvo'></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// Uma `width` DEFINIDA vence o mínimo TRANSFERIDO pela razão a partir de
    /// `min-height` (CSS Sizing 4 §5.1): `width:100; aspect-ratio:2/1;
    /// max-width:150; min-height:100` fica 100×100 — a altura usada é 100, a
    /// razão pede 200 de largura de volta, mas a `width:100` declarada vence
    /// (WPT `block-aspect-ratio-048`). Antes o transferido (150) ganhava.
    #[test]
    fn width_definida_vence_o_minimo_transferido_pela_razao() {
        let fonte = "<div style='background:green;width:100px;aspect-ratio:2/1;max-width:150px;min-height:100px' id='alvo'></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// Um `max-height` DEFINIDO transfere-se pela razão a um MÁXIMO de largura
    /// (CSS Sizing 4 §5.1): um bloco de largura auto com `aspect-ratio:1/1;
    /// max-height:100%` (=100px) NÃO estica ao contentor — a largura é capada em
    /// 100 (WPT `block-aspect-ratio-020`). Antes esticava a 1280.
    #[test]
    fn max_height_transfere_um_maximo_de_largura_pela_razao() {
        let fonte = "<div style='height:100px'><div style='aspect-ratio:1/1;max-height:100%;background:green' id='alvo'>\
            <div style='width:20px;height:10px;display:inline-block'></div>\
            <div style='width:20px;height:10px;display:inline-block'></div>\
            <div style='width:20px;height:10px;display:inline-block'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 1);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// O máximo transferido de `max-height` NÃO corta abaixo do mínimo de
    /// conteúdo (WPT `block-aspect-ratio-043`): `height:200; aspect-ratio:1/2;
    /// max-height:100` (máx transferido 50) com filho `width:100` fica 100 de
    /// largura, não 50 — o min-content vence o máximo transferido.
    #[test]
    fn maximo_transferido_nao_corta_o_minimo_de_conteudo() {
        let fonte = "<div style='background:green;height:200px;aspect-ratio:1/2;max-height:100px' id='alvo'>\
            <div style='width:100px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!((a.w - 100.0).abs() < 1.0, "043: {a:?}");
    }

    /// Uma `width` definida não é capada pelo máximo transferido de `max-height`
    /// (WPT `block-aspect-ratio-039`): `width:100; aspect-ratio:1/2;
    /// max-height:100` fica 100 de largura (o máx transferido 50 não vence).
    #[test]
    fn width_definida_nao_e_capada_pelo_maximo_transferido() {
        let fonte = "<div style='background:green;width:100px;aspect-ratio:1/2;max-height:100px' id='alvo'>\
            <div style='height:200px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!((a.w - 100.0).abs() < 1.0, "039: {a:?}");
    }

    /// Com `overflow:hidden` o piso de conteúdo NÃO se aplica (o mínimo
    /// automático é 0): `aspect-ratio:1/1; overflow:hidden; height:100` com um
    /// filho de 500px fica na razão (100), cortando o filho (WPT
    /// `block-aspect-ratio-018`). Guarda contra o piso do teste `-015`.
    #[test]
    fn overflow_hidden_anula_o_piso_de_conteudo_na_razao() {
        let fonte = "<div style='background:green;height:100px;aspect-ratio:1/1;overflow:hidden' id='alvo'>\
            <div style='width:100px;display:inline-block'></div>\
            <div style='width:500px;display:inline-block'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// O min-content é o piso (min-width:auto) da largura derivada da razão num
    /// bloco de largura auto (CSS Sizing 4 §5.1): `height:100; aspect-ratio:1/2`
    /// com filho `width:100` fica 100 de largura, não os 50 que a razão pede —
    /// o conteúdo não cabe em 50 (WPT `block-aspect-ratio-015`).
    #[test]
    fn min_content_e_piso_da_largura_derivada_da_razao() {
        let fonte = "<div style='background:green;height:100px;aspect-ratio:1/2' id='alvo'>\
            <div style='width:100px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// `aspect-ratio: auto <ratio>` (em qualquer ordem) parseia a razão e marca
    /// o flag `auto`; `auto` sozinho não declara razão.
    #[test]
    fn aspect_ratio_auto_e_parseado_com_o_flag() {
        let a = parse_inline("aspect-ratio:auto 1/1");
        assert_eq!(a.aspect_ratio, Some(1.0));
        assert_eq!(a.aspect_ratio_auto, Some(true));
        let b = parse_inline("aspect-ratio:2/1 auto");
        assert_eq!(b.aspect_ratio, Some(2.0));
        assert_eq!(b.aspect_ratio_auto, Some(true));
        let c = parse_inline("aspect-ratio:3/2");
        assert_eq!(c.aspect_ratio, Some(1.5));
        assert_eq!(c.aspect_ratio_auto, Some(false));
        // `auto` sozinho não declara razão nenhuma.
        assert_eq!(parse_inline("aspect-ratio:auto").aspect_ratio, None);
    }

    /// O espelho em `max-content`: `width:max-content` num pai de
    /// `<div height:100px; aspect-ratio:1/1>` também é 100 (WPT
    /// `aspect-ratio/intrinsic-size-002`).
    #[test]
    fn aspect_ratio_com_altura_contribui_para_o_max_content() {
        let fonte = "<div style='width:max-content;height:100px' id='alvo'>\
            <div style='height:100px;aspect-ratio:1/1'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "max-content tem de transferir a razão: {a:?}"
        );
    }

    /// A largura passa a ser a do CONTEÚDO, e transborda o pai em vez de ceder.
    #[test]
    fn max_content_mede_o_conteudo_e_transborda_o_pai() {
        let com = "<div style='width:60px'><div style='width:max-content'>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let sem = "<div style='width:60px'><div>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let (d1, l1) = geometria(com, 1280.0);
        let (d2, l2) = geometria(sem, 1280.0);
        let a = rect(&d1, &l1, "div", 1);
        let b = rect(&d2, &l2, "div", 1);
        assert!(a.w > 100.0, "max-content tem de medir o conteúdo: {a:?}");
        assert!((b.w - 60.0).abs() < 0.5, "sem ele, a largura do pai: {b:?}");
    }

    /// E o `max-width` continua a morder POR CIMA, como manda a spec.
    #[test]
    fn o_teto_de_max_width_continua_a_morder_sobre_o_max_content() {
        let (dom, list) = geometria(
            "<div style='width:60px'><div style='width:max-content;max-width:50px'>             <ul><li>Pagina principal muito comprida aqui</li></ul></div></div>",
            1280.0,
        );
        let d = rect(&dom, &list, "div", 1);
        assert!((d.w - 50.0).abs() < 0.5, "o teto tem de vencer: {d:?}");
    }

    #[test]
    fn test_writing_directions_002() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-002.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-002-ref.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        for (i, node) in d_t.query_all("x").iter().enumerate() {
            let r_t = l_t.rect_of(d_t.resolve(*node).unwrap()).unwrap();
            let ref_node = d_r.query_all("x")[i];
            let r_r = l_r.rect_of(d_r.resolve(ref_node).unwrap()).unwrap();
            assert!(
                (r_t.x - r_r.x).abs() < 0.5,
                "x mismatch for item {i}: {r_t:?} vs {r_r:?}"
            );
            assert!(
                (r_t.w - r_r.w).abs() < 0.5,
                "w mismatch for item {i}: {r_t:?} vs {r_r:?}"
            );
        }
    }

    #[test]
    fn test_csft() {
        let test_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\contribution-size-flex-tracks-001.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let g = d_t.resolve(d_t.query(".grid").unwrap()).unwrap();
        let s = d_t.resolve(d_t.query(".subgrid").unwrap()).unwrap();
        let it = d_t.resolve(d_t.query(".item").unwrap()).unwrap();
        assert_eq!(
            l_t.rect_of(g),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 100.0,
                h: 100.0
            })
        );
        assert_eq!(
            l_t.rect_of(s),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 100.0,
                h: 100.0
            })
        );
        assert_eq!(
            l_t.rect_of(it),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 10.0,
                h: 25.0
            })
        );
    }

    #[test]
    fn test_wd003() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-003.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-003-ref.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        let items_t = d_t.query_all(".grid-item");
        let items_r = d_r.query_all(".grid-item");
        assert_eq!(items_t.len(), items_r.len());
        for (i, (&it, &ir)) in items_t.iter().zip(&items_r).enumerate() {
            let rt = l_t.rect_of(d_t.resolve(it).unwrap()).unwrap();
            let rr = l_r.rect_of(d_r.resolve(ir).unwrap()).unwrap();
            assert!(
                (rt.x - rr.x).abs() < 0.5,
                "item {i} x mismatch: {rt:?} vs {rr:?}"
            );
            assert!(
                (rt.y - rr.y).abs() < 0.5,
                "item {i} y mismatch: {rt:?} vs {rr:?}"
            );
        }
    }

    #[test]
    fn test_ln11() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-011.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-011-ref.html",
        )
        .unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        let xt = d_t.query_all("x");
        let xr = d_r.query_all("x");
        for (i, (&nt, &nr)) in xt.iter().zip(&xr).enumerate() {
            let rt = l_t.rect_of(d_t.resolve(nt).unwrap());
            let rr = l_r.rect_of(d_r.resolve(nr).unwrap());
            assert_eq!(rt, rr, "x {i} mismatch");
        }
    }
}
