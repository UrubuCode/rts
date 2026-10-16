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
