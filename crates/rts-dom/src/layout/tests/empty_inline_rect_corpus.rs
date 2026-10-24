//! `tests/css/claude-margem-atraves-de-inline-vazio.html` against Edge/Blink:
//! an inline whose whole content COLLAPSES AWAY still has a position.
//!
//! `<span> </span>` between two blocks makes no line box (CSS 2.1 §9.4.2), so
//! the margins around it collapse through — which this engine already did. What
//! it did NOT do is give the span a rect: it owned no piece of any line, so
//! `getBoundingClientRect` answered y = 0 where Blink answers 0x0 at the line's
//! place (y = 140). The empty `<span></span>` of `#s1` was right all along, and
//! that asymmetry is what named the gap: the question is whether CONTENT is
//! left, not whether a run was produced.

use super::*;
use crate::table::tests::geometria;

const TOL: f32 = 1.0;

#[test]
fn an_inline_whose_content_all_collapses_still_has_a_zero_sized_rect() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-margem-atraves-de-inline-vazio.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    let mut wrong = Vec::new();
    for (sel, e) in [
        // the empty span: margins collapse through, 0x0 at the collapse point
        ("#c1", (0.0, 0.0, 200.0, 50.0)),
        ("#s1", (0.0, 30.0, 0.0, 0.0)),
        ("#b1", (0.0, 40.0, 200.0, 10.0)),
        // one collapsible space: the same, and the case this test was added for
        ("#c2", (0.0, 110.0, 200.0, 50.0)),
        ("#s2", (0.0, 140.0, 0.0, 0.0)),
        ("#b2", (0.0, 150.0, 200.0, 10.0)),
        // `x` DOES make a line, so the margins must not collapse through it
        ("#c3", (0.0, 220.0, 200.0, 90.0)),
        ("#s3", (0.0, 250.0, 8.8, 19.0)),
        ("#b3", (0.0, 300.0, 200.0, 10.0)),
    ] {
        let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
        let r = list
            .rect_of(idx)
            .unwrap_or_else(|| panic!("{sel} has no geometry"));
        let got = (r.x, r.y, r.w, r.h);
        if !((got.0 - e.0).abs() <= TOL
            && (got.1 - e.1).abs() <= TOL
            && (got.2 - e.2).abs() <= TOL
            && (got.3 - e.3).abs() <= TOL)
        {
            wrong.push(format!("{sel}: expected {e:?}, got {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "against Blink:\n{}", wrong.join("\n"));
}
