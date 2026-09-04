//! Grid: dimensionamento de tracks, áreas nomeadas, colocação automática e
//! alinhamento na célula.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;

    #[test]
    fn grid_fr_track_sizing() {
        // grid-template-columns: 200px 1fr 2fr num container 620 → 200/140/280.
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma posições absolutas.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style='display:grid;grid-template-columns:200px 1fr 2fr;width:620px'>\
             <div id=a>A</div><div id=b>B</div><div id=c>C</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            list.geometry().rects[&idx]
        };
        let a = rect("#a");
        let b = rect("#b");
        let c = rect("#c");
        assert!((a.w - 200.0).abs() < 1.0, "col px: {}", a.w);
        assert!((b.w - 140.0).abs() < 1.0, "col 1fr: {}", b.w); // (620-200)/3
        assert!((c.w - 280.0).abs() < 1.0, "col 2fr: {}", c.w); // 2×140
        assert!(
            (b.x - 200.0).abs() < 1.0 && (c.x - 340.0).abs() < 1.0,
            "posições"
        );
    }

    #[test]
    fn area_nomeada_poe_sidebar_e_conteudo_lado_a_lado() {
        // Sem áreas nomeadas os dois filhos caem na colocação automática de um grid
        // de 1 coluna e EMPILHAM — que é o que punha o artigo da Wikipédia fora da
        // viewport. Com a matriz, o `lado` fica na coluna 0 e o `conteudo` na 1, na
        // MESMA linha.
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma a.x=0.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style=\"display:grid;width:600px;grid-template-columns:200px 1fr;\
             grid-template-areas:'lado conteudo'\">\
             <div id=b style='grid-area:conteudo'>conteudo</div>\
             <div id=a style='grid-area:lado'>lado</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rect =
            |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let a = rect("#a");
        let b = rect("#b");
        assert!(
            (a.y - b.y).abs() < 1.0,
            "mesma linha: a.y={} b.y={}",
            a.y,
            b.y
        );
        assert!(
            b.x > a.x,
            "conteudo à direita do lado: a.x={} b.x={}",
            a.x,
            b.x
        );
        // a ordem do DOM tem o conteúdo PRIMEIRO — a matriz é que manda, não ela.
        assert!(
            (a.x - 0.0).abs() < 1.0 && (a.w - 200.0).abs() < 1.0,
            "lado: x={} w={}",
            a.x,
            a.w
        );
        assert!(
            (b.x - 200.0).abs() < 1.0 && (b.w - 400.0).abs() < 1.0,
            "conteudo: x={} w={}",
            b.x,
            b.w
        );
    }

    #[test]
    fn area_que_atravessa_colunas_cobre_o_gap() {
        // 'topo topo' / 'lado conteudo': o topo ocupa as DUAS colunas, e o span
        // inclui o gap do meio (senão o cabeçalho ficaria 24px mais estreito que a
        // linha que ele encima).
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma c.x=224 absoluto.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style=\"display:grid;width:624px;column-gap:24px;\
             grid-template-columns:200px 400px;\
             grid-template-areas:'topo topo' 'lado conteudo'\">\
             <div id=t style='grid-area:topo'>t</div>\
             <div id=l style='grid-area:lado'>l</div>\
             <div id=c style='grid-area:conteudo'>c</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rect =
            |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let t = rect("#t");
        let l = rect("#l");
        let c = rect("#c");
        assert!(
            (t.w - 624.0).abs() < 1.0,
            "topo cobre as 2 colunas + gap: {}",
            t.w
        );
        assert!(l.y > t.y, "lado abaixo do topo: t.y={} l.y={}", t.y, l.y);
        assert!((l.y - c.y).abs() < 1.0, "lado e conteudo na mesma linha");
        assert!(
            (c.x - 224.0).abs() < 1.0,
            "conteudo após 200px + 24 de gap: {}",
            c.x
        );
    }

    #[test]
    fn filho_sem_grid_area_continua_na_colocacao_automatica() {
        // Um item nomeado NÃO desliga o auto-placement dos outros: o sem nome cai na
        // primeira célula livre, que é a linha implícita abaixo da área ocupada.
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma s.x=0.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style=\"display:grid;width:400px;grid-template-columns:1fr 1fr;\
             grid-template-areas:'x x'\">\
             <div id=n style='grid-area:x'>n</div><div id=s>s</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rect =
            |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let n = rect("#n");
        let s = rect("#s");
        assert!(
            (n.w - 400.0).abs() < 1.0,
            "nomeado ocupa as 2 colunas: {}",
            n.w
        );
        assert!(
            s.y > n.y,
            "sem nome vai para a linha seguinte: n.y={} s.y={}",
            n.y,
            s.y
        );
        assert!((s.x - 0.0).abs() < 1.0, "e para a 1ª coluna livre: {}", s.x);
    }

    #[test]
    fn grid_align_items_center_centraliza_na_celula() {
        // single-column grid de altura fixa + align-items:center → o item de
        // altura menor centraliza verticalmente na track (o logo do google).
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma y=74 absoluto.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style='display:grid;align-items:center;height:240px'>\
             <div id=logo style='height:92px'>x</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#logo").unwrap()).unwrap();
        let r = list.geometry().rects[&idx];
        assert!(
            (r.y - 74.0).abs() < 2.0,
            "y centralizado: {} (esperado 74=(240-92)/2)",
            r.y
        );
        assert!((r.h - 92.0).abs() < 2.0, "altura preservada: {}", r.h);
    }
