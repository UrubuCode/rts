//! `tests/css/claude-inline-block-nowrap.html` contra o Blink (Edge 152,
//! 2026-09-04): uma corrida de inline-blocks respeita o `white-space` do
//! contentor — `nowrap` não quebra (o quarto a x=288, a transbordar), `normal`
//! quebra (o terceiro a y=178, segunda linha).

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { width: 192px; height: 128px; background: #eee; margin-bottom: 10px; }
  #nowrap { white-space: nowrap; }
  .c span { display: inline-block; width: 96px; height: 40px; background: #fc0; }
</style>
<div class="c" id="nowrap"><span id="nw1">one</span><span>two</span><span>three</span><span id="nw4">four</span></div>
<div class="c" id="quebra"><span>one</span><span>two</span><span id="q3">three</span><span>four</span></div>"#;

#[test]
fn a_corrida_de_inline_blocks_respeita_o_white_space() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#nw1"), (0.0, 0.0, 96.0, 40.0));
    assert_eq!(r("#nw4"), (288.0, 0.0, 96.0, 40.0), "nowrap: o quarto transborda à direita");
    assert_eq!(r("#q3"), (0.0, 178.0, 96.0, 40.0), "normal: o terceiro desce de linha");
}
