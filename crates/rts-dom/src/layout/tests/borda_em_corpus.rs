//! `tests/css/claude-borda-em.html` contra o Blink (Edge 152, 2026-09-04):
//! bordas em `em` resolvem contra a fonte (20px): `border: .3em` = 6 por lado
//! (112×62), `border-left: .5em` = 10 (110), `border-top-width: 1em` = 20
//! (100×70) e o caret só de bordas mede 12×6.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 20px/24px monospace; }
  div { width: 100px; height: 50px; background: #eee; margin-bottom: 10px; }
  #uni { border: .3em solid #c00; }
  #lado { border-left: .5em solid #0c0; }
  #long { border-style: solid; border-color: #00c; border-width: 0; border-top-width: 1em; }
  #caret { display: inline-block; width: 0; height: 0; border-top: .3em solid #000; border-right: .3em solid transparent; border-bottom: 0; border-left: .3em solid transparent; background: none; }
</style>
<div id="uni"></div>
<div id="lado"></div>
<div id="long"></div>
<div id="caret"></div>"#;

#[test]
fn bordas_em_em_resolvem_contra_a_fonte() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#uni"), (0.0, 0.0, 112.0, 62.0), "border: .3em a 20px = 6");
    assert_eq!(r("#lado"), (0.0, 72.0, 110.0, 50.0), "border-left: .5em = 10");
    assert_eq!(r("#long"), (0.0, 132.0, 100.0, 70.0), "border-top-width: 1em = 20");
    let c = r("#caret");
    assert_eq!((c.0, c.2, c.3), (0.0, 12.0, 6.0), "o caret so de bordas: 12x6");
    assert!((c.1 - 214.0).abs() <= 1.0, "o caret senta na baseline (Blink 214): {}", c.1);
}
