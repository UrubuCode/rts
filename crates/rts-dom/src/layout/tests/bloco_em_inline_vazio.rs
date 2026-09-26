//! Um inline que se parte em volta de um bloco SEM conteúdo inline de nenhum
//! dos lados — `<span><span style="display:block"></span></span>`.
//!
//! A partição (CSS 2.1 §9.2.1.1) não materializa fragmentos vazios, por isso o
//! inline de fora acaba com ZERO caixas, e o bloco de dentro sobe a filho do
//! contentor. É o caso de `CSS2/normal-flow/height-inherit-001.xht`, que desde
//! a série das caixas (053ed4f67) entrava em pânico em vez de pintar.

use super::*;

/// O HTML exacto do reftest do WPT, sem o `<head>`. O Chrome pinta o `.inner`
/// por cima do `.container` inteiro: 100×100 no mesmo sítio — é por isso que o
/// quadrado sai verde e não vermelho. `height: inherit` herda os 100px do
/// `.outer` mesmo que num inline `height` não se aplique (CSS 2.1 §6.2.1: é o
/// valor computado que se herda).
#[test]
fn bloco_num_inline_sem_mais_nada_cobre_o_contentor_com_a_altura_herdada() {
    let dom = parse_html_to_dom(
        r#"<style>
    .container { height: 100px; width: 100px; background: red; }
    .outer { height: 100px; }
    .inner { display: block; height: inherit; background: green; }
  </style>
  <p>Test passes if there is a filled green square and <strong>no red</strong>.</p>
  <div class="container">
    <div><span class="outer"><span class="inner" id="inner"></span></span></div>
  </div>"#,
    );
    let ctx = LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let geo = list.geometry_now();
    let rect_de = |sel: &str| {
        let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
        *geo.rects.get(&idx).unwrap_or_else(|| panic!("{sel} sem rect"))
    };
    let contentor = rect_de(".container");
    let inner = rect_de("#inner");
    assert_eq!((inner.w, inner.h), (100.0, 100.0), "{inner:?}");
    assert_eq!((inner.x, inner.y), (contentor.x, contentor.y), "{inner:?} vs {contentor:?}");
}
