//! Lote T — as unidades que dependem da MÉTRICA da fonte, contra o Blink
//! (`tests/css/claude-font-unidades-ch-ex.html`, Edge 152, 2026-09-04):
//! `10ch` = 87,97px, `10ex` = 78,44px, `line-height: normal` = 19px na
//! monoespaçada a 16px, e uma família inexistente cai na seguinte da lista.
//! O medidor aqui é o `ApproxMeasurer` (constantes calibradas em
//! `style::text_metrics`), por isso a tolerância é a do corpus: 1px.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  div { background: #eee; margin-bottom: 5px; }
  #dezch { width: 10ch; height: 10px; }
  #dezex { width: 10ex; height: 10px; }
  #normal { line-height: normal; }
  #mono, #fallback { display: inline-block; white-space: nowrap; }
  #fallback { font-family: FamiliaQueNaoExiste, monospace; }
</style>
<div id="dezch"></div>
<div id="dezex"></div>
<div id="normal">uma linha com line-height normal</div>
<div id="mono">abcdefghij</div><br>
<div id="fallback">abcdefghij</div>"#;

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

#[test]
fn ch_e_ex_medem_pela_fonte_e_a_familia_inexistente_cai_na_seguinte() {
    let (dom, list) = geometria(HTML, 1280.0);
    let dezch = rect(&dom, &list, "#dezch", 0);
    let dezex = rect(&dom, &list, "#dezex", 0);
    let normal = rect(&dom, &list, "#normal", 0);
    let mono = rect(&dom, &list, "#mono", 0);
    let fallback = rect(&dom, &list, "#fallback", 0);
    assert!(perto(dezch.w, 87.97), "10ch: Blink 87.97, obtido {}", dezch.w);
    assert!(perto(dezex.w, 78.44), "10ex: Blink 78.44, obtido {}", dezex.w);
    assert!(perto(normal.h, 19.0), "line-height normal: Blink 19, obtido {}", normal.h);
    assert!(perto(mono.w, 87.97), "10 chars mono: Blink 87.97, obtido {}", mono.w);
    assert_eq!(fallback.w, mono.w, "a família inexistente cai na monospace da lista");
    let css = dom.computed_style(dom.query("#fallback").expect("#fallback")).expect("css");
    assert_eq!(css.computed_value("font-family", None), "FamiliaQueNaoExiste, monospace");
}
