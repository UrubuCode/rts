//! `tests/css/claude-tabela-anonima-de-fora.html` against Edge/Blink
//! (`.esperado.json`, measured 2026-09-18): CSS 2.1 §17.2.1 wraps table parts
//! whose parent is not a table in an ANONYMOUS table, and cells without a row
//! in an anonymous row. Each misparented cell used to be an ordinary block, so
//! the cells stacked at full width instead of sitting side by side in a
//! shrink-to-fit table.

use super::*;
use crate::table::tests::geometria;

/// Corpus tolerance (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn assert_rect(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str, expected: (f32, f32, f32, f32)) {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("live node");
    let r = list.rect_of(idx).unwrap_or_else(|| panic!("{sel} has no geometry"));
    let got = (r.x, r.y, r.w, r.h);
    let matches = (got.0 - expected.0).abs() <= TOL
        && (got.1 - expected.1).abs() <= TOL
        && (got.2 - expected.2).abs() <= TOL
        && (got.3 - expected.3).abs() <= TOL;
    assert!(matches, "{sel}: expected {expected:?} (Blink), got {got:?}");
}

#[test]
fn table_parts_without_a_table_get_an_anonymous_one() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-tabela-anonima-de-fora.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    for (sel, r) in [
        ("#soltas", (0.0, 0.0, 1280.0, 30.0)),
        ("#s1", (0.0, 0.0, 40.0, 30.0)),
        ("#s2", (40.0, 0.0, 60.0, 30.0)),
        ("#so-linha", (0.0, 40.0, 1280.0, 30.0)),
        ("#l", (0.0, 40.0, 100.0, 30.0)),
        ("#l1", (0.0, 40.0, 40.0, 30.0)),
        ("#l2", (40.0, 40.0, 60.0, 30.0)),
        ("#com-float", (0.0, 80.0, 1280.0, 0.0)),
        ("#flutua", (1180.0, 80.0, 100.0, 30.0)),
        ("#f1", (1180.0, 80.0, 40.0, 30.0)),
        ("#f2", (1220.0, 80.0, 60.0, 30.0)),
        ("#entre-texto", (0.0, 80.0, 1280.0, 70.0)),
        ("#t1", (0.0, 100.0, 40.0, 30.0)),
        ("#t2", (40.0, 100.0, 60.0, 30.0)),
        ("#espaco", (0.0, 160.0, 1280.0, 38.0)),
        ("#e1", (10.0, 164.0, 40.0, 30.0)),
        ("#e2", (60.0, 164.0, 60.0, 30.0)),
        ("#fim", (0.0, 208.0, 1280.0, 10.0)),
    ] {
        assert_rect(&dom, &list, sel, r);
    }
}

/// `tests/css/claude-linha-so-com-texto.html`: a `table-row` holding only
/// text, outside a table, gets an anonymous table AND an anonymous cell for
/// its text (§17.2.1 rules 2 and 3) — without the cell the row had zero height
/// and the next block moved up over it.
#[test]
fn a_row_holding_only_text_gets_an_anonymous_cell() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-linha-so-com-texto.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    assert_rect(&dom, &list, "#antes", (0.0, 0.0, 1280.0, 20.0));
    assert_rect(&dom, &list, "#linha", (0.0, 20.0, 87.97, 20.0));
    assert_rect(&dom, &list, "#depois", (0.0, 40.0, 1280.0, 10.0));
}
