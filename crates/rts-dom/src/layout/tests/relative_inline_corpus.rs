//! `tests/css/claude-relativo-em-inline.html` against Edge/Blink:
//! `position: relative` on an INLINE box shifts the box's own fragments — and
//! those of the inlines inside it — while nothing around it reflows and the
//! line keeps its size (CSS 2.1 §9.4.3).

use super::*;
use crate::table::tests::geometria;

const TOL: f32 = 1.0;

#[test]
fn a_relative_inline_moves_and_nothing_around_it_reflows() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-relativo-em-inline.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    let mut wrong = Vec::new();
    for (sel, e) in [
        ("#c1", (0.0, 0.0, 1280.0, 20.0)),
        ("#r1", (65.19, 5.0, 26.39, 19.0)),
        ("#d1", (61.58, 0.0, 26.39, 19.0)),
        ("#r2", (23.98, 40.0, 26.39, 19.0)),
        ("#d2", (70.38, 40.0, 26.39, 19.0)),
        ("#c3", (0.0, 80.0, 1280.0, 20.0)),
        ("#r3", (43.19, 76.0, 35.19, 19.0)),
        ("#n3", (60.78, 76.0, 17.59, 19.0)),
        ("#d3", (70.38, 80.0, 26.39, 19.0)),
        ("#c4", (0.0, 120.0, 100.0, 60.0)),
        ("#r4", (10.0, 123.0, 79.17, 39.0)),
        ("#d4", (0.0, 160.0, 26.39, 19.0)),
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

/// The TEXT moves with the box, and so does its painted background: a shifted
/// rect over unmoved glyphs would satisfy every geometry number above.
#[test]
fn the_glyphs_and_the_surface_move_with_the_box() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    #r{position:relative;left:30px;top:5px;background:#ff0}</style>
    <div>aaa <span id="r">bbb</span></div>"#;
    let (_, list) = geometria(html, 1280.0);
    let itens = list.materialized();
    let texto = itens.iter().find_map(|i| match i {
        DisplayItem::Text { x, y, text, .. } if &**text == "bbb" => Some((*x, *y)),
        _ => None,
    });
    let fundo = itens.iter().find_map(|i| match i {
        DisplayItem::SolidRect { rect, color, .. } if *color == 0xFFFF00FF => Some((rect.x, rect.y)),
        _ => None,
    });
    let natural_x = 4.0 * 16.0 * crate::style::MONO_ADVANCE;
    let (tx, ty) = texto.expect("the text of the span");
    assert!((tx - (natural_x + 30.0)).abs() < 0.5 && (ty - 5.0).abs() < 1.0, "text at ({tx}, {ty})");
    let (fx, fy) = fundo.expect("the background of the span");
    assert!((fx - (natural_x + 30.0)).abs() < 0.5 && (fy - 5.0).abs() < 1.0, "background at ({fx}, {fy})");
}
