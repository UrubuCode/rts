//! The `tests/css/claude-{float,absoluto}-*` fixtures against Edge/Blink
//! (`.esperado.json`, measured 2026-09-18): CSS 2.1 §9.2.1.1 splits an inline
//! around an IN-FLOW block box, and a float or an absolutely positioned box is
//! not in flow (§9.3). Splitting the inline around them opens anonymous boxes
//! Blink does not have — the text after them drops a line.

use super::*;
use crate::table::tests::geometria;

/// Corpus tolerance (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

/// The rect the bridge answers (`boundingRect`) and the one `.esperado.json`
/// measures — `rect_of`, not the hit-test table, which leaves out the blocks
/// that split an inline (`layout/rect_cliente.rs`).
fn rect(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, _n: usize) -> Rect {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
    list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"))
}

fn assert_rect(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, expected: (f32, f32, f32, f32)) {
    let r = rect(dom, list, sel, 0);
    let got = (r.x, r.y, r.w, r.h);
    let matches = (got.0 - expected.0).abs() <= TOL
        && (got.1 - expected.1).abs() <= TOL
        && (got.2 - expected.2).abs() <= TOL
        && (got.3 - expected.3).abs() <= TOL;
    assert!(matches, "{sel}: expected {expected:?} (Blink), got {got:?}");
}

/// The span's `x`/`y`/`h` — what says it stayed on one line. Its width depends
/// on the monospace text width, which this engine approximates.
fn assert_line(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, x: f32, y: f32, h: f32) {
    let r = rect(dom, list, sel, 0);
    assert!(
        (r.x - x).abs() <= TOL && (r.y - y).abs() <= TOL && (r.h - h).abs() <= TOL,
        "{sel}: expected x={x} y={y} h={h} (Blink), got x={} y={} h={}",
        r.x,
        r.y,
        r.h
    );
}

fn html(child_style: &str, id: &str, rest: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
  body {{ margin: 0; font: 16px/20px monospace; }}
  #quebra {{ background: #ff0; }}
  #{id} {{ {child_style} }}
  #bloco {{ height: 25px; background: #f0f; }}
  #seguinte {{ background: #00f; height: 10px; }}
</style></head><body>
  <div id="contentor"><span id="quebra">antes<div id="{id}"></div>{rest}</span></div>
  <div id="seguinte"></div>
</body></html>"#
    )
}

#[test]
fn a_float_inside_an_inline_does_not_split_it() {
    let src = html("float: left; width: 50px; height: 30px; background: #0f0;", "flutua", "depois");
    let (dom, list) = geometria(&src, 1280.0);
    assert_rect(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 20.0));
    assert_rect(&dom, &list, "#flutua", (0.0, 0.0, 50.0, 30.0));
    // One line, shortened around the float: it starts at x=50.
    assert_line(&dom, &list, "#quebra", 50.0, 0.0, 19.0);
    assert_rect(&dom, &list, "#seguinte", (0.0, 20.0, 1280.0, 10.0));
}

#[test]
fn an_absolute_box_inside_an_inline_does_not_split_it() {
    let src = html("position: absolute; width: 40px; height: 30px; background: #0f0;", "fora", "depois");
    let (dom, list) = geometria(&src, 1280.0);
    assert_rect(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 20.0));
    assert_line(&dom, &list, "#quebra", 0.0, 0.0, 19.0);
    // `#fora` is NOT asserted, and the fixture is in `esperado-a-falhar.txt`:
    // Blink gives it `(0,20)` — the static position of an absolute box that was
    // BLOCK-level is after the line box — and this engine gives `y≈1`, because
    // `posicao_estatica_bloco` takes the DOM `<span>` as the container and no
    // line box is kept to say where the line ends (the IFC lot).
    assert_rect(&dom, &list, "#seguinte", (0.0, 20.0, 1280.0, 10.0));
}

#[test]
fn only_the_in_flow_block_splits_and_the_float_stays_in_the_first_run() {
    let src = html(
        "float: left; width: 50px; height: 30px; background: #0f0;",
        "flutua",
        r#"meio<div id="bloco"></div>depois"#,
    );
    let (dom, list) = geometria(&src, 1280.0);
    assert_rect(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 65.0));
    assert_rect(&dom, &list, "#quebra", (0.0, 0.0, 1280.0, 64.0));
    assert_rect(&dom, &list, "#flutua", (0.0, 0.0, 50.0, 30.0));
    assert_rect(&dom, &list, "#bloco", (0.0, 20.0, 1280.0, 25.0));
    assert_rect(&dom, &list, "#seguinte", (0.0, 65.0, 1280.0, 10.0));
}

/// `tests/css/claude-float-a-meio-da-linha.html`: a DIRECT-child float in the
/// middle of text, left and right, and one that does not fit in the rest of
/// the line and goes to the top of the next (CSS 2.1 §9.5.1).
#[test]
fn a_float_mid_line_takes_that_lines_top_or_the_next_if_it_does_not_fit() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-float-a-meio-da-linha.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    assert_rect(&dom, &list, "#a", (0.0, 0.0, 1280.0, 20.0));
    assert_rect(&dom, &list, "#fa", (0.0, 0.0, 50.0, 30.0));
    assert_rect(&dom, &list, "#b", (0.0, 30.0, 1280.0, 20.0));
    assert_rect(&dom, &list, "#fb", (1220.0, 30.0, 60.0, 10.0));
    assert_rect(&dom, &list, "#c", (0.0, 60.0, 120.0, 40.0));
    assert_rect(&dom, &list, "#fc", (0.0, 80.0, 80.0, 10.0));
    assert_rect(&dom, &list, "#fim", (0.0, 110.0, 1280.0, 10.0));
}
