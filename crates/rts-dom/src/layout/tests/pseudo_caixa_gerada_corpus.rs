//! `tests/css/claude-pseudo-caixa-gerada.html` against Edge 153 (Blink,
//! `.esperado.json`, measured 2026-09-18): a `::before`/`::after` is a
//! generated BOX (CSS 2.1 §12.1), not a run of bare text. A pseudo has no rect
//! of its own to ask for, so each case asserts what the generated box does to
//! the elements AROUND it — where the next one starts and how tall the line is.
//!
//! The HTML is read from the fixture itself, so this test and the corpus pin
//! the same bytes.

use crate::table::tests::geometria;

/// Corpus tolerance (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn fixture() -> (crate::Dom, crate::paint::DisplayList) {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-pseudo-caixa-gerada.html"
    ))
    .expect("the BT-5 ruler fixture is in tests/css");
    geometria(&src, 1280.0)
}

fn afirma(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, esperado: (f32, f32, f32, f32)) {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
    let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
    let got = (r.x, r.y, r.w, r.h);
    let bate = (got.0 - esperado.0).abs() <= TOL
        && (got.1 - esperado.1).abs() <= TOL
        && (got.2 - esperado.2).abs() <= TOL
        && (got.3 - esperado.3).abs() <= TOL;
    assert!(bate, "{sel}: expected {esperado:?} (Blink), got {got:?}");
}

/// An `inline-block` pseudo is an atomic box of its own `width`×`height`: it
/// pushes the text 50px right and, sitting on the baseline, grows the line.
#[test]
fn inline_block_pseudo_takes_its_declared_size_on_the_line() {
    let (dom, list) = fixture();
    afirma(&dom, &list, "#p1", (0.0, 0.0, 1280.0, 35.0));
    afirma(&dom, &list, "#p1t", (50.0, 15.0, 17.59, 19.0));
}

/// An `inline` pseudo with horizontal padding and a left border takes that
/// space on the line, inside the span that generates it (the span's rect
/// grows by 23px on each of the two spans).
#[test]
fn inline_pseudo_padding_and_border_take_space_inside_its_element() {
    let (dom, list) = fixture();
    afirma(&dom, &list, "#p2", (0.0, 45.0, 1280.0, 20.0));
    afirma(&dom, &list, "#p2s", (8.8, 45.0, 40.59, 19.0));
    afirma(&dom, &list, "#p2n", (49.39, 45.0, 40.59, 19.0));
}

/// The horizontal half of an axis-by-axis assertion: `x` and `w`.
fn afirma_x(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, x: f32, w: f32) {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
    let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
    assert!(
        (r.x - x).abs() <= TOL && (r.w - w).abs() <= TOL,
        "{sel}: expected x={x} w={w} (Blink), got x={} w={}",
        r.x,
        r.w
    );
}

fn y_h(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str) -> (f32, f32) {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
    let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
    (r.y, r.h)
}

/// An empty `inline-block` `::after` with a left margin: its margin box
/// (5 + 20) is part of the span's width and pushes the next span — and, on
/// the VERTICAL axis, its bottom margin edge sits on the baseline (CSS 2.1
/// §10.8.1, no line box of its own) so the strut's descent hangs below it:
/// Blink's line grows to 25 (not the empty box's own 20), and the spans it
/// widens sit at y=80. This is the BT-5 ruler's last axis (`linha_baseline.rs`
/// `ascent_do_gerado`, which the atom now goes through the same
/// `alinhamento_vertical::Envelope` a real `inline-block` uses).
#[test]
fn inline_block_after_with_margin_widens_its_element() {
    let (dom, list) = fixture();
    afirma_x(&dom, &list, "#p3s", 0.0, 42.59);
    afirma_x(&dom, &list, "#p3n", 42.59, 8.8);
    let (p3_y, p3_h) = y_h(&dom, &list, "#p3");
    assert!((p3_h - 25.0).abs() <= TOL, "#p3: expected h=25 (Blink), got {p3_h}");
    let (p3s_y, _) = y_h(&dom, &list, "#p3s");
    assert!((p3s_y - p3_y - 5.0).abs() <= TOL, "#p3s: expected y=80 (Blink, 5px below #p3's top), got {p3s_y}");
}

/// A block pseudo 40px wide wraps "aaa bbb ccc ddd" to four lines: 80px of
/// generated box above the real content. Asserted RELATIVE to `#p4`, whose
/// own `y` inherits the 5px `#p3` is short of (above).
#[test]
fn block_pseudo_text_wraps_at_its_content_width() {
    let (dom, list) = fixture();
    let (p4_y, p4_h) = y_h(&dom, &list, "#p4");
    assert!((p4_h - 100.0).abs() <= TOL, "#p4: expected h=100 (Blink), got {p4_h}");
    let (t_y, _) = y_h(&dom, &list, "#p4t");
    assert!(
        (t_y - p4_y - 80.0).abs() <= TOL,
        "#p4t: expected 80px below #p4 (Blink 190-110), got {}",
        t_y - p4_y
    );
    afirma_x(&dom, &list, "#p4t", 0.0, 26.39);
}

/// An `inline-block` pseudo shrinks to its text and adds padding and border
/// around it: 6+2 on each side of one 8.8px glyph. On the vertical axis it
/// has its OWN line box (CSS 2.1 §10.8.1), so its baseline is its text's, not
/// its bottom margin edge: Blink's line grows to 32 (the box's padding+border
/// above and below its one line of text) and its text sits at y=226 — 6px
/// below `#p5`'s top, its own padding-top.
#[test]
fn inline_block_pseudo_shrinks_to_its_text_plus_padding_and_border() {
    let (dom, list) = fixture();
    afirma_x(&dom, &list, "#p5t", 24.8, 17.59);
    let (p5_y, p5_h) = y_h(&dom, &list, "#p5");
    assert!((p5_h - 32.0).abs() <= TOL, "#p5: expected h=32 (Blink), got {p5_h}");
    let (p5t_y, _) = y_h(&dom, &list, "#p5t");
    assert!((p5t_y - p5_y - 6.0).abs() <= TOL, "#p5t: expected y=226 (Blink, 6px below #p5's top), got {p5t_y}");
}

/// The surface of an `inline` pseudo is painted behind its text and spans
/// the generated box: from its border edge (the span's left, 8.8) across
/// 3 (border) + 10 + 8.8 ("x") + 10 = 31.8, with the left border on it.
#[test]
fn inline_pseudo_surface_is_painted_across_the_generated_box() {
    let (_, list) = fixture();
    let rects: Vec<_> = list
        .materialized()
        .iter()
        .filter_map(|it| match it {
            crate::paint::DisplayItem::SolidRect { rect, color, .. } => Some((*rect, *color)),
            _ => None,
        })
        .collect();
    let fundo = rects
        .iter()
        .find(|(r, c)| *c == 0xFFFF00FF && (r.x - 8.8).abs() <= TOL)
        .unwrap_or_else(|| panic!("no #ff0 surface for #p2s::before in {rects:?}"));
    assert!((fundo.0.w - 31.8).abs() <= TOL, "surface width {}", fundo.0.w);
    assert!(
        rects.iter().any(|(r, c)| *c == 0xAA0000FF && (r.x - 8.8).abs() <= TOL && (r.w - 3.0).abs() <= TOL),
        "no 3px left border for #p2s::before in {rects:?}"
    );
}
