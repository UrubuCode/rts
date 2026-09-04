//! `tests/css/claude-flex-margens-auto-transversal.html` contra o Blink (Edge
//! 152, 2026-09-04): `margin: auto` centra o item no eixo transversal (r1 em
//! 75,35), `margin-top: auto` empurra-o para o fundo mesmo com `align-items:
//! flex-start` (r2 em y=180), `margin-bottom: auto` deixa-o no topo (r3 em
//! 150,110) e, em coluna, `margin: auto` centra na horizontal (c1 em 75,255 —
//! isso é o layout de bloco do item, não `flex_margens_auto`; fica fixado
//! para que ninguém o centre uma segunda vez).

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; width: 200px; height: 100px; background: #eee; margin-bottom: 10px; align-items: flex-start; }
  .f > div { width: 50px; height: 30px; background: #c00; }
  #r1 { margin: auto; }
  #r2 { margin-top: auto; }
  #r3 { margin-bottom: auto; margin-left: auto; background: #0a0; }
  #col { display: flex; flex-direction: column; width: 200px; height: 100px; background: #eee; }
  #c1 { width: 50px; height: 30px; background: #00c; margin: auto; }
</style>
<div class="f" id="fa"><div id="r1"></div></div>
<div class="f" id="fb"><div id="r2"></div><div id="r3"></div></div>
<div id="col"><div id="c1"></div></div>"#;

#[test]
fn margens_auto_absorvem_o_espaco_transversal_e_vencem_o_align() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#r1"), (75.0, 35.0, 50.0, 30.0), "margin: auto centra nos dois eixos");
    assert_eq!(r("#r2"), (0.0, 180.0, 50.0, 30.0), "margin-top: auto = fundo, apesar do flex-start");
    assert_eq!(r("#r3"), (150.0, 110.0, 50.0, 30.0), "margin-bottom: auto = topo; margin-left: auto = direita");
    assert_eq!(r("#c1"), (75.0, 255.0, 50.0, 30.0), "em coluna centra na horizontal");
}
