//! `tests/css/claude-svg-atributo-em.html` contra o Blink (Edge 152,
//! 2026-09-04): `width`/`height` de um `<svg>` são comprimentos CSS — `1em`
//! = 16, `24` = 24px, `50%` = metade do contentor de 200.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>body{margin:0;font:16px/20px monospace}#c{width:200px}svg{display:block;margin-bottom:4px}</style>
<div id="c">
<svg id="em" width="1em" height="1em" viewBox="0 0 16 16"></svg>
<svg id="px" width="24" height="24" viewBox="0 0 16 16"></svg>
<svg id="pct" width="50%" height="40" viewBox="0 0 16 16"></svg>
</div>"#;

#[test]
fn atributos_do_svg_sao_comprimentos_css() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#em"), (0.0, 0.0, 16.0, 16.0), "1em = font-size");
    assert_eq!(r("#px"), (0.0, 20.0, 24.0, 24.0), "sem unidade = px");
    assert_eq!(r("#pct"), (0.0, 48.0, 100.0, 40.0), "50% do contentor de 200");
}
