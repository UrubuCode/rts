//! The static position of an absolutely positioned child of a GRID container
//! (css-align-3 §4.4, Grid §9.2): aligned as the sole item of an area that is
//! the container's content box, by `align-self`/`justify-self` falling back
//! to `align-items`/`justify-items`, `normal` acting as `start`.
//!
//! The container is the one of WPT
//! `grid-abspos-staticpos-align-items-center-large-border-padding`: border
//! 23 (45 at the bottom), padding 13 (74 top, 42 bottom), content 100×500,
//! so the content box starts at (36, 97) with body margin 0. Its reference
//! puts a 50×100 box centred there at y 97 + 200.

use crate::table::tests::{geometria, rect};

fn place(grid_extra: &str, item_extra: &str) -> (f32, f32) {
    let html = format!(
        r#"<body style="margin:0"><div style="display:grid;padding:13px;padding-top:74px;padding-bottom:42px;border:23px solid black;border-bottom-width:45px;width:100px;height:500px;{grid_extra}">
<div id="alvo" style="position:absolute;width:50px;height:100px;{item_extra}"></div></div></body>"#
    );
    let (dom, list) = geometria(&html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    (r.x, r.y)
}

#[test]
fn no_alignment_puts_the_box_at_the_content_box_start() {
    assert_eq!(place("", ""), (36.0, 97.0));
}

#[test]
fn align_items_center_centres_in_the_content_box_not_the_padding_box() {
    assert_eq!(place("align-items:center", ""), (36.0, 297.0));
}

#[test]
fn items_end_in_both_axes_reach_the_content_box_end() {
    assert_eq!(
        place("align-items:end;justify-items:end", ""),
        (86.0, 497.0)
    );
}

#[test]
fn the_items_own_self_alignment_wins_over_the_containers() {
    assert_eq!(
        place(
            "align-items:center;justify-items:end",
            "align-self:start;justify-self:center"
        ),
        (61.0, 97.0)
    );
}

#[test]
fn rtl_starts_the_inline_axis_at_the_right_edge() {
    assert_eq!(place("direction:rtl", ""), (86.0, 97.0));
    assert_eq!(place("direction:rtl;justify-items:end", ""), (36.0, 97.0));
}

#[test]
fn debug_positioned_grid_items_009() {
    let html = r#"<style>
#grid {
  display: grid;
  grid: 150px 100px / 200px 300px;
  width: 550px;
  height: 400px;
  position: relative;
}
#grid > div {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  bottom: 0;
}
#firstItem { background: magenta; grid-column: 1 / 2; grid-row: 1 / 2; }
#secondItem { background: cyan; grid-column: 2 / 3; grid-row: 1 / 2; }
#thirdItem { background: yellow; grid-column: 1 / 2; grid-row: 2 / 3; }
#fourthItem { background: lime; grid-column: 2 / 3; grid-row: 2 / 3; }
</style>
<div id="grid">
  <div id="firstItem">First item</div>
  <div id="secondItem">Second item</div>
  <div id="thirdItem">Third item</div>
  <div id="fourthItem">Fourth item</div>
</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let r1 = rect(&dom, &list, "#firstItem", 0);
    let r2 = rect(&dom, &list, "#secondItem", 0);
    let r3 = rect(&dom, &list, "#thirdItem", 0);
    let r4 = rect(&dom, &list, "#fourthItem", 0);
    println!("r1: {r1:?}");
    println!("r2: {r2:?}");
    println!("r3: {r3:?}");
    println!("r4: {r4:?}");
    assert_eq!((r1.w, r1.h), (200.0, 150.0));
    assert_eq!((r2.w, r2.h), (300.0, 150.0));
    assert_eq!((r3.w, r3.h), (200.0, 100.0));
    assert_eq!((r4.w, r4.h), (300.0, 100.0));
}

#[test]
fn test_sized_to_grid_area_with_left() {
    let html = r#"<style>
#grid {
  display: grid;
  grid-template-columns: 100px 200px;
  grid-template-rows: 50px 150px;
  width: 500px;
  height: 300px;
  position: relative;
}
#item {
  position: absolute;
  grid-column: 1 / 2;
  grid-row: 1 / 2;
  width: 100%;
  height: 100%;
  left: 5px;
}
</style>
<div id="grid">
  <div id="item"></div>
</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#item", 0);
    println!("item rect: {r:?}");
    assert_eq!((r.x, r.y, r.w, r.h), (5.0, 0.0, 100.0, 50.0));
}
