//! Colocação por linha (`grid-column/row-start/end`, span, negativos) e
//! auto-flow (`row`/`column`, tracks implícitas) — o lote R
//! (`crates/rts-dom/PLAN.md` §5.R). Os valores esperados são DERIVADOS DA
//! SPEC (CSS Grid 1 §8) e não ainda medidos no Chrome; quando o orquestrador
//! correr `claude-grid-linhas.html`/`claude-grid-implicito.html` no Blink,
//! troca-se aqui pelos números medidos se divergirem.

use super::*;

#[test]
fn grid_column_start_end_numerico_poe_o_item_na_celula_certa() {
    // 4 colunas de 100px; item com grid-column: 3/5 (linhas 1-based) ocupa as
    // colunas de índice 2 e 3 (0-based) — x = 200, largura = 200 (span 2).
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:repeat(4,100px);width:400px'>\
         <div id=a style='grid-column:3/5;grid-row:1'>A</div></div>",
    );
    let ctx = LayoutCtx {
        viewport_w: 1000.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query("#a").unwrap()).unwrap();
    let a = list.geometry().rects[&idx];
    assert!((a.x - 200.0).abs() < 1.0, "x da coluna 3: {}", a.x);
    assert!((a.w - 200.0).abs() < 1.0, "span 2 colunas: {}", a.w);
}

#[test]
fn grid_column_negativo_conta_do_fim_do_eixo_explicito() {
    // 4 colunas: `-1` é a última linha (a 5ª), `-3` é a 3ª. `grid-column:-3/-1`
    // ocupa as colunas 2 e 3 (0-based) — as duas últimas.
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:repeat(4,100px);width:400px'>\
         <div id=a style='grid-column:-3/-1;grid-row:1'>A</div></div>",
    );
    let ctx = LayoutCtx {
        viewport_w: 1000.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query("#a").unwrap()).unwrap();
    let a = list.geometry().rects[&idx];
    assert!((a.x - 200.0).abs() < 1.0, "x das 2 últimas colunas: {}", a.x);
    assert!((a.w - 200.0).abs() < 1.0, "largura das 2 últimas: {}", a.w);
}

#[test]
fn grid_span_sem_ancora_de_fim_conta_a_partir_do_start() {
    // `grid-column: 2 / span 2` — começa na coluna 1 (0-based), span 2.
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:repeat(4,100px);width:400px'>\
         <div id=a style='grid-column:2 / span 2;grid-row:1'>A</div></div>",
    );
    let ctx = LayoutCtx {
        viewport_w: 1000.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query("#a").unwrap()).unwrap();
    let a = list.geometry().rects[&idx];
    assert!((a.x - 100.0).abs() < 1.0, "x da coluna 2: {}", a.x);
    assert!((a.w - 200.0).abs() < 1.0, "span 2: {}", a.w);
}

#[test]
fn grid_auto_flow_row_cria_linhas_implicitas_com_grid_auto_rows() {
    // 2 colunas explícitas, 5 itens: 3 linhas, a 3ª implícita a 50px
    // (`grid-auto-rows`), sem `grid-template-rows` nenhum.
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:100px 100px;grid-auto-rows:50px'>\
         <div id=a>1</div><div id=b>2</div><div id=c>3</div><div id=d>4</div><div id=e>5</div>\
         </div>",
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
    let e = rect("#e");
    // item 5 é o único da 3ª linha (row-major: 1,2 / 3,4 / 5) — y = 2 linhas
    // anteriores. As duas primeiras linhas têm altura = conteúdo (texto de 1
    // char no ApproxMeasurer); a 3ª usa grid-auto-rows:50px.
    assert!(e.x < 1.0, "5º item começa a coluna 0: {}", e.x);
    assert!(e.y > 0.0, "5º item está numa linha depois da 1ª: {}", e.y);
}

#[test]
fn grid_auto_flow_column_cria_colunas_implicitas() {
    // 1 coluna explícita, sem grid-template-rows (linha única para o eixo
    // fixo do flow column), `grid-auto-flow:column` + `grid-auto-columns:80px`:
    // 4 itens ficam em 4 colunas distintas, cada uma a 80px a partir da 2ª
    // (a 1ª usa a coluna explícita de 100px).
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:100px;\
         grid-auto-flow:column;grid-auto-columns:80px'>\
         <div id=a>1</div><div id=b>2</div><div id=c>3</div><div id=d>4</div>\
         </div>",
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
    let (a, b, c, d) = (rect("#a"), rect("#b"), rect("#c"), rect("#d"));
    assert!((a.x - 0.0).abs() < 1.0, "1ª coluna explícita: {}", a.x);
    assert!((b.x - 100.0).abs() < 1.0, "2ª coluna implícita: {}", b.x);
    assert!((c.x - 180.0).abs() < 1.0, "3ª coluna implícita: {}", c.x);
    assert!((d.x - 260.0).abs() < 1.0, "4ª coluna implícita: {}", d.x);
    // todos na mesma linha (y igual): o flow column não cria linhas extra
    // quando não há grid-template-rows.
    assert!((a.y - b.y).abs() < 1.0 && (b.y - c.y).abs() < 1.0);
}
