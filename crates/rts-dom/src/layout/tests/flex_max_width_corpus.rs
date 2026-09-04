//! `tests/css/claude-flex-item-max-width.html` contra o Blink (Edge 152,
//! 2026-09-04): o item flex respeita `max-width`/`min-width` no eixo
//! principal e as margens `auto` absorvem o espaço livre. É o
//! `.cover-container.w-100.mx-auto{max-width:42em}` do Bootstrap cover, que
//! saía com 1280px em vez de 672 centrados (36/57 elementos da página a 1px).

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; height: 40px; background: #eee; margin-bottom: 5px; }
  #teto { width: 100%; max-width: 42em; margin: 0 auto; background: #fc0; }
  #piso { min-width: 200px; background: #0c0; }
  #cresce { flex-grow: 1; max-width: 300px; background: #c0f; }
  #direita { width: 100px; margin-left: auto; background: #0cf; }
</style>
<div class="f"><div id="teto">x</div></div>
<div class="f"><div id="piso">y</div><div id="cresce">z</div><div id="direita">w</div></div>"#;

#[test]
fn item_flex_respeita_max_min_width_e_margens_auto() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#teto"), (304.0, 0.0, 672.0, 40.0), "width 100% capado a 42em e centrado por margin auto");
    assert_eq!(r("#piso"), (0.0, 45.0, 200.0, 40.0), "min-width acima do conteúdo");
    assert_eq!(r("#cresce"), (200.0, 45.0, 300.0, 40.0), "flex-grow capado pelo max-width");
    assert_eq!(r("#direita"), (1180.0, 45.0, 100.0, 40.0), "margin-left auto empurra para a direita");
}
