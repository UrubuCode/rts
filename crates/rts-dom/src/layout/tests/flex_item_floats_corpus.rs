//! `tests/css/claude-flex-item-contem-floats.html` contra o Blink (Edge 152,
//! 2026-09-04): um item de flex é um contexto de formatação independente e
//! CONTÉM os seus floats (40px), enquanto o mesmo conteúdo num bloco normal
//! mede 0 e o `clear` seguinte desce até ao fundo dos floats (y=100).

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .coluna { display: flex; flex-direction: column; background: #eee; margin-bottom: 20px; }
  .esq { float: left; width: 100px; height: 40px; background: #0c0; }
  .dir { float: right; width: 80px; height: 30px; background: #c0f; }
  #depois { clear: both; }
</style>
<div class="coluna"><div id="item"><div class="esq" id="esq">a</div><div class="dir" id="dir">b</div></div></div>
<div id="bloco"><div class="esq">c</div><div class="dir">d</div></div>
<div id="depois">fim</div>"#;

#[test]
fn item_de_flex_contem_os_seus_floats_e_o_bloco_normal_nao() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#item"), (0.0, 0.0, 1280.0, 40.0), "o item flex contém os floats");
    assert_eq!(r("#dir"), (1200.0, 0.0, 80.0, 30.0));
    assert_eq!(r("#bloco"), (0.0, 60.0, 1280.0, 0.0), "um bloco normal não os contém");
    assert_eq!(r("#depois").1, 100.0, "o clear desce até ao fundo dos floats do bloco");
}
