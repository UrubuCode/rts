//! `tests/css/claude-clear-em-pseudo.html` contra o Blink (Edge 152,
//! 2026-09-04): o clearfix `::after{display:block;clear:both}` faz o
//! contentor conter os floats (100px, duas linhas de dois) e sem ele o
//! contentor mede 0; o `clear` seguinte desce para 170.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { width: 320px; background: #eee; margin-bottom: 20px; }
  .c span { display: block; float: left; width: 128px; height: 18px; margin: 16px; background: #fc0; }
  #cf::after { content: ""; display: block; clear: both; }
</style>
<div class="c" id="cf"><span>1</span><span>2</span><span>3</span><span id="f4">4</span></div>
<div class="c" id="sem"><span>1</span><span>2</span></div>
<div style="clear: both" id="fim">fim</div>"#;

#[test]
fn o_clearfix_em_after_contem_os_floats() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#cf"), (0.0, 0.0, 320.0, 100.0), "com clearfix: contem as duas linhas de floats");
    assert_eq!(r("#f4"), (176.0, 66.0, 128.0, 18.0));
    assert_eq!(r("#sem"), (0.0, 120.0, 320.0, 0.0), "sem clearfix: 0");
    assert_eq!(r("#fim").1, 170.0, "o clear seguinte desce ate ao fundo dos floats do #sem");
}
