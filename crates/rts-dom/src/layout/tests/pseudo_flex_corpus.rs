//! `tests/css/claude-pseudo-item-flex.html` contra o Blink (Edge 152,
//! 2026-09-04): `::before`/`::after` com `content` de um flex são itens —
//! `#f` (before 20 + span 40 + caret de 12 de bordas) mede 72 e o span
//! começa a x=20; `#g` (span 40 + after "ab") mede 40 + a largura de "ab".

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .abs { position: absolute; left: 0; display: flex; align-items: center; background: #eee; }
  #f { top: 0; }
  #f::before { content: ""; display: block; width: 20px; height: 10px; background: #0c0; }
  #f::after { content: ""; display: inline-block; width: 0; height: 0; border-top: 6px solid #000; border-right: 6px solid transparent; border-bottom: 0; border-left: 6px solid transparent; }
  .abs span { display: block; width: 40px; height: 30px; background: #fc0; }
  #g { top: 40px; }
  #g::after { content: "ab"; background: #c0f; }
</style>
<div class="abs" id="f"><span id="fs">x</span></div>
<div class="abs" id="g"><span id="gs">y</span></div>"#;

#[test]
fn pseudo_elementos_de_um_flex_sao_itens() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#f"), (0.0, 0.0, 72.0, 30.0), "before 20 + span 40 + caret 12");
    assert_eq!(r("#fs"), (20.0, 0.0, 40.0, 30.0), "o span vem depois do ::before");
    let g = r("#g");
    assert!((g.2 - 57.59).abs() <= 1.0, "span 40 + \"ab\": {}", g.2);
    assert_eq!(r("#gs"), (0.0, 40.0, 40.0, 30.0));
}
