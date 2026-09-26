//! `aspect-ratio` on a grid item whose percentage height resolves against a
//! FIXED row: the height transfers through the ratio to the item's width, to
//! the column it sits in, and to the shrink-to-fit width of the grid around it.
//!
//! The markup is WPT `css-grid/grid-items/aspect-ratio-001`, `-003` and `-005`,
//! with the `inline-grid` replaced by a floated `grid`: both shrink-to-fit
//! (CSS 2.1 §10.3.5 / §10.3.9), and `inline-grid` is still laid out as a
//! block-level box by this engine (see the lot's report). Every expected number
//! is the reference's (`ref-filled-green-100px-square-only`: one 100×100 square).

use crate::table::tests::{geometria, rect};

/// `aspect-ratio-001`: `height:100%` of a 100px row is 100, and 1/1 makes the
/// width 100 — the shrink-to-fit grid wraps it.
#[test]
fn percent_height_in_a_fixed_row_transfers_to_the_width_of_item_and_grid() {
    let html = r#"<body style="margin:0"><div id="g" style="display:grid;float:left;grid-template-rows:100px"><div id="i" style="aspect-ratio:1/1;height:100%"></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let i = rect(&dom, &list, "#i", 0);
    let g = rect(&dom, &list, "#g", 0);
    assert_eq!((i.w, i.h), (100.0, 100.0));
    assert_eq!((g.w, g.h), (100.0, 100.0));
}

/// `aspect-ratio-003`: 50% of the row is 50, and 2/1 makes it 100 wide.
#[test]
fn a_wide_ratio_doubles_the_percentage_height_into_the_width() {
    let html = r#"<body style="margin:0"><div id="g" style="display:grid;float:left;grid-template-rows:100px"><div id="i" style="aspect-ratio:2/1;height:50%"></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let i = rect(&dom, &list, "#i", 0);
    let g = rect(&dom, &list, "#g", 0);
    assert_eq!((i.w, i.h), (100.0, 50.0));
    assert_eq!((g.w, g.h), (100.0, 100.0));
}

/// `aspect-ratio-005`: the column the ratio item widens is shared with the
/// item of the second row, which stretches to it.
#[test]
fn the_transferred_width_sizes_the_column_for_the_other_rows_too() {
    let html = r#"<body style="margin:0"><div id="g" style="display:grid;float:left;grid-template-rows:50px 50px;height:100px"><div id="a" style="grid-row:1;aspect-ratio:2/1;height:100%"></div><div id="b" style="grid-row:2"></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert_eq!((a.w, a.h), (100.0, 50.0));
    assert_eq!((b.w, b.h), (100.0, 50.0));
}

/// Grid §6.6.1: only `normal` yields to the ratio; a DECLARED
/// `justify-self: stretch` still fills the 100px column
/// (`css-sizing/aspect-ratio/grid-aspect-ratio-037`).
#[test]
fn a_declared_stretch_still_wins_over_the_ratio() {
    let html = r#"<body style="margin:0"><div style="display:grid;grid-template:100px / 100px"><div id="i" style="aspect-ratio:1/2;height:100%;justify-self:stretch"></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let i = rect(&dom, &list, "#i", 0);
    assert_eq!((i.w, i.h), (100.0, 100.0));
}
