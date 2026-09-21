//! `tests/css/claude-absoluto-posicao-estatica-linha-vazia.html` against
//! Edge/Blink: a block-level absolute box with NOTHING before it on its line
//! goes at the line's top — the line it would have split off is zero-height
//! (CSS 2.1 §9.4.2). Found as three WPT reftests the first version of the rule
//! lost (`CSS2/abspos/static-inside-inline-001`/`-003`,
//! `css-flexbox/flexbox-min-width-auto-005`).

use super::*;
use crate::table::tests::geometria;

const TOL: f32 = 1.0;

#[test]
fn a_block_level_absolute_box_first_on_its_line_sits_at_the_lines_top() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-absoluto-posicao-estatica-linha-vazia.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    let mut wrong = Vec::new();
    for (sel, e) in [
        ("#c1", (0.0, 0.0, 1280.0, 60.0)),
        ("#a1", (0.0, 0.0, 30.0, 10.0)),
        ("#a2", (0.0, 100.0, 30.0, 10.0)),
        ("#a3", (0.0, 200.0, 30.0, 10.0)),
        ("#c4", (0.0, 300.0, 1280.0, 40.0)),
        ("#a4", (0.0, 320.0, 30.0, 10.0)),
        ("#c5", (0.0, 380.0, 1280.0, 60.0)),
        ("#a5", (0.0, 420.0, 30.0, 10.0)),
        ("#d5", (0.0, 420.0, 1280.0, 20.0)),
        ("#a6", (0.0, 480.0, 30.0, 10.0)),
    ] {
        let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
        let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
        let got = (r.x, r.y, r.w, r.h);
        if !((got.0 - e.0).abs() <= TOL && (got.1 - e.1).abs() <= TOL && (got.2 - e.2).abs() <= TOL && (got.3 - e.3).abs() <= TOL) {
            wrong.push(format!("{sel}: expected {e:?}, got {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "against Blink:\n{}", wrong.join("\n"));
}
