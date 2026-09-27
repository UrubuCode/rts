//! `grid-area` with LINE NUMBERS places the item. It used to be dropped at
//! parse time, so the item fell to auto-placement.
//!
//! The first test is `css-grid/grid-items/grid-inline-z-axis-ordering-001`
//! with the Ahem glyphs replaced by 100px boxes: two items, both
//! `grid-area: 1 / 1`, must overlap in ONE cell, and the reference
//! (`reference/ref-filled-green-100px-square.xht`) is a single 100×100 square
//! — so the grid is 100 tall and both items sit at its origin.

use super::*;

fn rect(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str) -> crate::paint::Rect {
    let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
    list.geometry_now().rects[&idx]
}

fn run(html: &str) -> (crate::Dom, crate::paint::DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

#[test]
fn two_items_with_grid_area_1_1_share_the_first_cell() {
    let (dom, list) = run(
        "<style>body{margin:0} .c{grid-area:1 / 1;width:100px;height:100px}</style>\
         <div id=g style='display:inline-grid'>\
         <div id=a class=c></div><div id=b class=c></div></div>",
    );
    let (g, a, b) = (rect(&dom, &list, "#g"), rect(&dom, &list, "#a"), rect(&dom, &list, "#b"));
    assert_eq!((a.x, a.y), (b.x, b.y), "both in cell 1/1: {a:?} {b:?}");
    assert!((g.h - 100.0).abs() < 0.5, "one row of 100px, not two: {}", g.h);
}

#[test]
fn grid_area_four_numbers_is_row_start_col_start_row_end_col_end() {
    // 3×3 tracks of 3/14/3 × 2/20/2 px — `grid-abspos-staticpos-align-self-safe-001`'s
    // grid — with an IN-FLOW item on `2 / 2 / 3 / 3`: the middle cell.
    let (dom, list) = run(
        "<style>body{margin:0}</style>\
         <div id=g style='display:grid;grid:3px 14px 3px / 2px 20px 2px;width:24px'>\
         <div id=a style='grid-area:2 / 2 / 3 / 3'></div></div>",
    );
    let (g, a) = (rect(&dom, &list, "#g"), rect(&dom, &list, "#a"));
    assert_eq!((a.x - g.x, a.y - g.y, a.w, a.h), (2.0, 3.0, 20.0, 14.0), "{a:?}");
}

#[test]
fn implicit_area_line_names_place_on_that_area() {
    // `b-start / b-end` are the lines `grid-template-areas` names for area `b`.
    let (dom, list) = run(
        "<style>body{margin:0}</style>\
         <div style='display:grid;grid-template-columns:100px 50px;grid-template-areas:\"a b\"'>\
         <div id=x style='grid-column:b-start / b-end;grid-row:1'></div>\
         <div id=y style='grid-area:a'></div></div>",
    );
    let (x, y) = (rect(&dom, &list, "#x"), rect(&dom, &list, "#y"));
    assert_eq!((x.x, x.w), (100.0, 50.0), "{x:?}");
    assert_eq!((y.x, y.y, y.w), (0.0, 0.0, 100.0), "{y:?}");
}
