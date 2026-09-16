use super::*;
use crate::table::tests::rect;

#[test]
fn fractional_growth_and_frozen_limits_match_blink() {
    let dom = parse_html_to_dom(include_str!(
        "../../../../../tests/css/claude-flex-grow-fracionario.html"
    ));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    // Edge 153, measured with css_fixtures_medir_edge.mjs at 1280×800.
    for (id, expected) in [
        ("r1", [40.0, 0.0, 60.0, 20.0]),
        ("r2", [100.0, 0.0, 60.0, 20.0]),
        ("c1", [0.0, 60.0, 20.0, 60.0]),
        ("c2", [0.0, 120.0, 20.0, 60.0]),
        ("w1", [0.0, 260.0, 20.0, 60.0]),
        ("w2", [0.0, 320.0, 20.0, 60.0]),
        ("cap", [0.0, 420.0, 0.0, 20.0]),
        ("rest", [0.0, 420.0, 200.0, 20.0]),
        ("cap2", [65.0, 440.0, 20.0, 20.0]),
        ("rest2", [85.0, 440.0, 50.0, 20.0]),
        ("minimum", [0.0, 460.0, 7.11, 18.0]),
        ("remaining", [7.11, 460.0, 292.89, 18.0]),
        ("shrink1", [0.0, 478.0, 100.02, 0.0]),
        ("shrink2a", [0.0, 478.0, 50.0, 0.0]),
        ("shrink2b", [50.0, 478.0, 50.0, 0.0]),
        ("infinite", [0.0, 478.0, 100.0, 0.0]),
        ("finite", [100.0, 478.0, 0.0, 0.0]),
        ("indefinite-basis", [0.0, 478.0, 100.0, 100.0]),
    ] {
        let r = rect(&dom, &list, &format!("#{id}"), 0);
        let actual = [r.x, r.y, r.w, r.h];
        assert!(actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= 1.0),
            "#{id}: {actual:?}, Blink: {expected:?}");
    }
}
