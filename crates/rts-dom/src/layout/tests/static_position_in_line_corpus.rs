//! `tests/css/claude-absoluto-posicao-estatica-na-linha.html` (and the older
//! `claude-absoluto-dentro-do-inline-nao-parte.html`) against Edge/Blink: the
//! static position of an absolutely positioned box that appears in the middle
//! of a line. Block-level before blockification: below that line, at the
//! flow's start edge. Inline-level: where it appears, at the line's top.

use super::*;

const TOL: f32 = 1.0;

/// Blink's rects of the five absolutely positioned boxes of the fixture.
const BLINK: [(&str, (f32, f32, f32, f32)); 5] = [
    ("#a1", (0.0, 20.0, 30.0, 10.0)),
    ("#a2", (43.98, 60.0, 30.0, 10.0)),
    ("#a3", (0.0, 140.0, 30.0, 10.0)),
    ("#a4", (0.0, 220.0, 30.0, 10.0)),
    ("#a5", (27.59, 260.0, 30.0, 10.0)),
];

fn fixture() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-absoluto-posicao-estatica-na-linha.html"
    ))
    .expect("fixture")
}

fn wrong(dom: &crate::Dom, ctx: &LayoutCtx) -> Vec<String> {
    let list = layout_document(dom, ctx);
    BLINK
        .iter()
        .filter_map(|&(sel, e)| {
            let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
            let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
            let got = (r.x, r.y, r.w, r.h);
            let ok = (got.0 - e.0).abs() <= TOL && (got.1 - e.1).abs() <= TOL && (got.2 - e.2).abs() <= TOL && (got.3 - e.3).abs() <= TOL;
            (!ok).then(|| format!("{sel}: expected {e:?}, got {got:?}"))
        })
        .collect()
}

#[test]
fn an_absolute_box_mid_line_takes_the_static_position_blink_gives_it() {
    let dom = crate::parse_html_to_dom(&fixture());
    let ctx = LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
    let w = wrong(&dom, &ctx);
    assert!(w.is_empty(), "against Blink:\n{}", w.join("\n"));
}

/// The silent class: the position is RECORDED by the inline flow, and a block
/// served from the fragment cache runs no flow. The same document laid out a
/// second time (cached) and a third (stitched around a mutation elsewhere)
/// has to answer what the first pass answered.
#[test]
fn the_static_position_survives_the_fragment_cache() {
    let html = fixture().replace("</body>", "<p id=\"outro\">z</p></body>");
    let mut dom = crate::parse_html_to_dom(&html);
    let ctx = LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
    for passagem in ["fresh", "cached"] {
        let w = wrong(&dom, &ctx);
        assert!(w.is_empty(), "{passagem} pass:\n{}", w.join("\n"));
    }
    let outro = dom.query("#outro").expect("#outro");
    dom.set_text(outro, "zz");
    let w = wrong(&dom, &ctx);
    assert!(w.is_empty(), "stitched pass:\n{}", w.join("\n"));
}
