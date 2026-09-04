//! `tests/css/claude-borda-por-lado-intrinseca.html` contra o Blink (Edge
//! 152, 2026-09-04): a borda POR LADO entra na largura intrínseca e na base
//! de um item flex — o caret `::after` do Bootstrap (só bordas, conteúdo
//! vazio) mede 12 e não 0.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .abs { position: absolute; left: 0; }
  #ib { top: 0; display: inline-block; }
  #caret { display: inline-block; width: 0; height: 0; border-top: 6px solid #000; border-right: 6px solid transparent; border-bottom: 0; border-left: 6px solid transparent; }
  #texto { display: inline-block; border-left: 3px solid #c00; border-right: 9px solid #00c; }
  #f { top: 40px; display: flex; }
  #item { width: 20px; height: 20px; padding: 0 5px; border-left: 4px solid #0c0; border-right: 8px solid #c0f; }
</style>
<div class="abs" id="ib"><span id="caret"></span><span id="texto">ab</span></div>
<div class="abs" id="f"><div id="item"></div></div>"#;

#[test]
fn a_borda_por_lado_entra_na_largura_intrinseca_e_na_base_flex() {
    let (dom, list) = geometria(HTML, 1280.0);
    let w = |s: &str| rect(&dom, &list, s, 0).w;
    assert_eq!(w("#caret"), 12.0, "só bordas: 6 + 6");
    assert!((w("#texto") - 29.59).abs() <= 1.0, "\"ab\" + 3 + 9: {}", w("#texto"));
    assert_eq!(w("#item"), 42.0, "item flex: 20 + 10 de padding + 12 de borda");
    assert!((w("#ib") - 41.59).abs() <= 1.0, "o inline-block que os contém: {}", w("#ib"));
}
