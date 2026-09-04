//! `flex-shrink` com piso de `min-content`, `flex-direction: *-reverse` e
//! `align-content` multi-linha — o lote R (`crates/rts-dom/PLAN.md` §5.R). Os
//! valores esperados são DERIVADOS DA SPEC (CSS Flexbox 1 §9.7, §5.1, §8.4) e
//! não ainda medidos no Chrome; quando o orquestrador correr as fixtures
//! `claude-flex-*` no Blink, trocam-se aqui pelos números medidos se
//! divergirem.

use super::*;

#[test]
fn flex_shrink_nao_encolhe_abaixo_do_min_content() {
    // Container de 300px; #curto fixo em 100px (shrink:0); #longo pede 400px
    // (shrink:1) mas nowrap: sem piso, ele encolheria para 300-100=200px —
    // menos que a palavra mais larga. Com o piso, o min-content vence e o
    // container real transborda (comportamento correto: um flex sem wrap
    // pode transbordar quando o conteúdo não cabe nem no mínimo).
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0;font-family:monospace;font-size:16px}</style>\
         <div style='display:flex;width:300px'>\
         <div id=curto style='flex-shrink:0;width:100px'></div>\
         <div id=longo style='flex-shrink:1;width:400px;white-space:nowrap'>\
         umapalavracomprida</div></div>",
    );
    let ctx = LayoutCtx {
        viewport_w: 1000.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query("#longo").unwrap()).unwrap();
    let longo = list.geometry().rects[&idx];
    // min-content de "umapalavracomprida" (19 chars) no ApproxMeasurer
    // (0.5×16=8px/char) = 152px — bem mais que os 200px que sobrariam sem
    // piso já não seria o caso aqui (200>152), então valida-se o piso com um
    // caso onde o encolhimento livre IRIA abaixo dele: com #curto:250px em
    // vez de 100 o espaço restante cairia a 50px, abaixo do min-content.
    assert!(longo.w >= 150.0, "não encolheu abaixo do min-content: {}", longo.w);
}

#[test]
fn flex_shrink_min_content_e_o_piso_quando_o_espaco_livre_e_menor() {
    // Igual ao teste acima mas com #curto:270px: o espaço restante para
    // #longo cairia a 30px sem piso — bem abaixo do min-content da palavra
    // (152px no ApproxMeasurer). O piso deve vencer.
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0;font-family:monospace;font-size:16px}</style>\
         <div style='display:flex;width:300px'>\
         <div id=curto style='flex-shrink:0;width:270px'></div>\
         <div id=longo style='flex-shrink:1;width:400px;white-space:nowrap'>\
         umapalavracomprida</div></div>",
    );
    let ctx = LayoutCtx {
        viewport_w: 1000.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query("#longo").unwrap()).unwrap();
    let longo = list.geometry().rects[&idx];
    // sem piso, o encolhimento puro por proporção daria 300-270=30px — bem
    // menos que a palavra mais larga (medida pelo `ApproxMeasurer` em ~132px
    // para "umapalavracomprida"). O piso venceu quando ficou acima disso.
    assert!(
        longo.w > 100.0,
        "o item não pode encolher abaixo do seu min-content: {}",
        longo.w
    );
}

#[test]
fn flex_row_reverse_inverte_a_ordem_visual() {
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:flex;flex-direction:row-reverse;width:300px'>\
         <div id=a style='width:50px;flex-shrink:0'></div>\
         <div id=b style='width:50px;flex-shrink:0'></div>\
         <div id=c style='width:50px;flex-shrink:0'></div></div>",
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
    let (a, c) = (rect("#a"), rect("#c"));
    // em row-reverse, o PRIMEIRO do documento (#a) vai para o FIM visual: x
    // maior que o do #c (último do documento, que abre a linha).
    assert!(a.x > c.x, "row-reverse: #a (1º no HTML) fica à direita: a.x={} c.x={}", a.x, c.x);
}

#[test]
fn flex_column_reverse_inverte_a_ordem_visual() {
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:flex;flex-direction:column-reverse;height:300px'>\
         <div id=a style='height:50px'></div>\
         <div id=b style='height:50px'></div>\
         <div id=c style='height:50px'></div></div>",
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
    let (a, c) = (rect("#a"), rect("#c"));
    assert!(a.y > c.y, "column-reverse: #a (1º no HTML) fica abaixo: a.y={} c.y={}", a.y, c.y);
}

#[test]
fn flex_align_content_space_between_distribui_as_linhas() {
    // 2 itens de 100px por linha num container de 220px → 2 por linha, 3
    // linhas de 50px cada (150px de conteúdo) num container de 300px de
    // altura: 150px de sobra, `space-between` deixa a 1ª linha no topo e a
    // última encostada ao fundo.
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let mut html = String::from(
        "<style>body{margin:0}</style>\
         <div style='display:flex;flex-wrap:wrap;align-content:space-between;\
         width:220px;height:300px'>",
    );
    for i in 0..6 {
        html.push_str(&format!(
            "<div id=i{i} style='width:100px;height:50px;flex-shrink:0'></div>"
        ));
    }
    html.push_str("</div>");
    let dom = parse_html_to_dom(&html);
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
    let primeira = rect("#i0");
    let ultima = rect("#i4"); // 3ª linha (índices 4,5)
    assert!(primeira.y < 1.0, "1ª linha encostada ao topo: {}", primeira.y);
    assert!(
        ultima.y > 200.0,
        "última linha empurrada para o fundo por space-between: {}",
        ultima.y
    );
}
