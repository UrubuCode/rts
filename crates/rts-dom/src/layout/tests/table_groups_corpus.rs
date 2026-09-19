//! `tests/css/claude-tabela-cabecalho-rodape.html` against Edge/Blink
//! (`.esperado.json`, measured 2026-09-18): a header group is displayed
//! first and a footer group last, wherever they are in the source
//! (CSS 2.1 §17.2), for the HTML elements and for the `display` values.

use super::*;
use crate::table::tests::geometria;

const TOL: f32 = 1.0;

#[test]
fn header_group_first_and_footer_group_last() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-tabela-cabecalho-rodape.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    for (sel, e) in [
        ("#t1", (0.0, 0.0, 50.0, 80.0)),
        ("#h1", (0.0, 0.0, 50.0, 20.0)),
        ("#b1", (0.0, 20.0, 50.0, 40.0)),
        ("#cb1b", (0.0, 40.0, 50.0, 20.0)),
        ("#f1", (0.0, 60.0, 50.0, 20.0)),
        ("#t2", (0.0, 90.0, 50.0, 60.0)),
        ("#h2", (0.0, 90.0, 50.0, 20.0)),
        ("#b2", (0.0, 110.0, 50.0, 20.0)),
        ("#f2", (0.0, 130.0, 50.0, 20.0)),
        ("#fim", (0.0, 160.0, 1280.0, 10.0)),
    ] {
        let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
        let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
        let got = (r.x, r.y, r.w, r.h);
        let matches = (got.0 - e.0).abs() <= TOL && (got.1 - e.1).abs() <= TOL && (got.2 - e.2).abs() <= TOL && (got.3 - e.3).abs() <= TOL;
        assert!(matches, "{sel}: expected {e:?} (Blink), got {got:?}");
    }
}
