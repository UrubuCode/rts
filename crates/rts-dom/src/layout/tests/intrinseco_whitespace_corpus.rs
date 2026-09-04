//! `tests/css/claude-intrinseco-whitespace.html` contra o Blink (Edge 152,
//! 2026-09-04): a largura shrink-to-fit colapsa o whitespace do HTML — o
//! flex com dois filhos de 20px separados por indentação mede 40, o
//! inline-block com "  a   b  " mede "a b" (26,39 no Blink; aqui a mono
//! calibrada dá o mesmo), e só whitespace mede 0.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .abs { position: absolute; left: 0; }
  #flex { top: 0; display: flex; background: #fc0; }
  #flex span { display: block; width: 20px; height: 20px; background: #0c0; }
  #ib { top: 40px; display: inline-block; background: #0cf; }
  #vazio { top: 80px; display: flex; background: #c0f; height: 10px; }
</style>
  <div class="abs" id="flex">
        <span></span>
        <span></span>
      </div>
  <div class="abs" id="ib">  a   b  </div>
  <div class="abs" id="vazio">
  </div>"#;

#[test]
fn a_largura_intrinseca_colapsa_o_whitespace_do_html() {
    let (dom, list) = geometria(HTML, 1280.0);
    let w = |s: &str| rect(&dom, &list, s, 0).w;
    assert_eq!(w("#flex"), 40.0, "a indentação entre os filhos não é conteúdo");
    assert!((w("#ib") - 26.39).abs() <= 1.0, "\"  a   b  \" mede \"a b\": {}", w("#ib"));
    assert_eq!(w("#vazio"), 0.0, "só whitespace mede 0");
}
