//! `tests/css/claude-justify-left-right.html` contra o Blink (Edge 152,
//! 2026-09-04): `justify-content: left` encosta à esquerda mesmo em
//! `row-reverse` (3, 2, 1 a partir de x=0) e `right` à direita em `row`.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; width: 400px; height: 30px; background: #eee; margin-bottom: 10px; }
  .f span { width: 60px; height: 30px; background: #fc0; }
  #esq { flex-direction: row-reverse; justify-content: left; }
  #dir { justify-content: right; }
</style>
<div class="f" id="esq"><span id="l1">1</span><span>2</span><span id="l3">3</span></div>
<div class="f" id="dir"><span id="r1">1</span><span>2</span><span id="r3">3</span></div>"#;

#[test]
fn justify_left_e_right_sao_fisicos() {
    let (dom, list) = geometria(HTML, 1280.0);
    let x = |s: &str| rect(&dom, &list, s, 0).x;
    assert_eq!((x("#l3"), x("#l1")), (0.0, 120.0), "left em row-reverse: 3 a esquerda, 1 a x=120");
    assert_eq!((x("#r1"), x("#r3")), (220.0, 340.0), "right em row: encostados a direita");
}
