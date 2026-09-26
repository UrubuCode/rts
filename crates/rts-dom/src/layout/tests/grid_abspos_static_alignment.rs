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
    assert_eq!(place("align-items:end;justify-items:end", ""), (86.0, 497.0));
}

#[test]
fn the_items_own_self_alignment_wins_over_the_containers() {
    assert_eq!(place("align-items:center;justify-items:end", "align-self:start;justify-self:center"), (61.0, 97.0));
}

#[test]
fn rtl_starts_the_inline_axis_at_the_right_edge() {
    assert_eq!(place("direction:rtl", ""), (86.0, 97.0));
    assert_eq!(place("direction:rtl;justify-items:end", ""), (36.0, 97.0));
}