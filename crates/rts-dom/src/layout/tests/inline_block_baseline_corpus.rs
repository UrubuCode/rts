//! `tests/css/claude-inline-block-baseline.html` against Edge/Blink
//! (`.esperado.json`, measured 2026-09-18): where an `inline-block` sits on a
//! line of text (CSS 2.1 §10.8.1). Its baseline is its last line box's, or its
//! bottom margin edge when it has none or clips (`overflow` not visible); the
//! line box holds the text's strut and the box on one baseline.

use super::*;
use crate::table::tests::geometria;

const TOL: f32 = 1.0;

/// Blink's numbers for each case, RELATIVE to the case's own container: its
/// height, and each child's `(dx, dy, w, h)` from the container's top-left.
///
/// Relative and not absolute, and the reason is measured: the strut's
/// descent here is 0.4px short of Blink's at 16px monospace (4.6 against 5 —
/// Blink rounds the font's ascent and descent to whole pixels, this engine's
/// calibrated metrics do not; the FM lot of PLAN §10). Per line that is inside
/// the corpus tolerance, but it accumulates down the page and the fourth case
/// is already 1.2px off in absolute `y`. Rounding the strut alone was TRIED and
/// reverted: the text's own top still uses the fractional metrics, and the two
/// disagreed by +0.5px per line instead. The fixture is in
/// `esperado-a-falhar.txt` for that drift; what this lot changes — where the
/// box and the text sit INSIDE each line — is pinned here.
const CASES: [(&str, f32, &[(&str, (f32, f32, f32, f32))]); 7] = [
    ("#a", 25.0, &[("#a1", (8.8, 0.0, 20.0, 20.0)), ("#a2", (28.8, 5.0, 8.8, 19.0))]),
    ("#b", 32.0, &[("#b1", (8.8, 0.0, 24.8, 32.0)), ("#b2", (33.59, 6.0, 8.8, 19.0))]),
    ("#c", 45.0, &[("#c1", (8.8, 0.0, 30.0, 40.0)), ("#c2", (38.8, 25.0, 8.8, 19.0))]),
    ("#d", 35.0, &[("#d1", (8.8, 0.0, 30.0, 30.0)), ("#d2", (38.8, 15.0, 8.8, 19.0))]),
    ("#e", 30.0, &[("#e1", (8.8, 0.0, 20.0, 30.0)), ("#e2", (28.8, 3.92, 8.8, 19.0))]),
    ("#f", 30.0, &[("#f1", (8.8, 0.0, 20.0, 30.0)), ("#f2", (28.8, 0.0, 8.8, 19.0))]),
    ("#g", 30.0, &[("#g1", (8.8, 0.0, 20.0, 30.0)), ("#g2", (28.8, 10.0, 8.8, 19.0))]),
];

#[test]
fn inline_block_sits_on_the_baseline_of_the_line() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-inline-block-baseline.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    let rect = |sel: &str| {
        let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
        list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"))
    };
    let mut wrong = Vec::new();
    for (caso, altura, filhos) in CASES {
        let c = rect(caso);
        if (c.h - altura).abs() > TOL {
            wrong.push(format!("{caso}.h: expected {altura}, got {}", c.h));
        }
        for &(sel, e) in filhos {
            let r = rect(sel);
            let got = (r.x - c.x, r.y - c.y, r.w, r.h);
            if !((got.0 - e.0).abs() <= TOL && (got.1 - e.1).abs() <= TOL && (got.2 - e.2).abs() <= TOL && (got.3 - e.3).abs() <= TOL) {
                wrong.push(format!("{sel} relative to {caso}: expected {e:?}, got {got:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "against Blink:
{}", wrong.join("
"));
}

/// The silent class this lot could have opened: an inline-block's baseline is
/// read from the lines its content RECORDS while being laid out, and a block
/// served from the fragment cache runs no flow. Without the fragment
/// re-announcing its last line, the second layout pass would find none in the
/// cached `<div>` and sit the box on its bottom edge — a different answer from
/// the same document, with nothing to say so.
#[test]
fn a_cached_block_inside_an_inline_block_keeps_its_baseline() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    #ib{display:inline-block;padding-bottom:30px;background:#0a0}</style>
    <div id="linha">x<span id="ib"><div>um</div><div id="dois">dois</div></span>y</div>
    <p id="outro">z</p>"#;
    let mut dom = crate::parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
    fn topo(dom: &crate::Dom, ctx: &LayoutCtx) -> f32 {
        let list = layout_document(dom, ctx);
        let idx = dom.resolve(dom.query("#linha").expect("#linha")).expect("live node");
        list.rect_of(idx).expect("geometry").h
    }
    let primeira = topo(&dom, &ctx);
    // What is compared is the LINE's height. The 70px box sits on its last
    // line ("dois"), so it hangs below the baseline and the line is exactly as
    // tall as the box; sitting on its bottom edge, the text's descent would
    // hang below the box and the line would be ~4.6px taller.
    assert!((primeira - 70.0).abs() < 0.5, "the box does not sit on its last line: line h={primeira}");
    let segunda = topo(&dom, &ctx);
    assert_eq!(primeira, segunda, "the cached pass changed the line");
    // A mutation ELSEWHERE stitches the ancestors and keeps this subtree cached.
    let outro = dom.query("#outro").expect("#outro");
    dom.set_text(outro, "zz");
    let terceira = topo(&dom, &ctx);
    assert_eq!(primeira, terceira, "the stitched pass changed the line");
}
