//! `position: relative` on table-internal boxes reaches the WHOLE box, not
//! only its own background — the geometry (`getBoundingClientRect`) shifts by
//! the same offset. Pinned against the shape of three WPT reftests
//! (`css-position/position-relative-table-{tbody,tr}-{left,top}.html`), which
//! shift a `<tbody>`/`<tr>` by `left:100px`/`top:100px` and check the CELL
//! ends up where an un-tabled `position:relative` box with the same offset
//! would (`position-relative-table-left-ref.html`,
//! `position-relative-table-top-ref.html`).
//!
//! Before this module, a row/row-group's own OFFSET was silently dropped —
//! `lay_out_grid` measured and painted `<tr>`/`<tbody>` directly, never
//! through `layout_block`, so nothing ever asked its `position`. The rect
//! below is what a reftest actually checks (`getBoundingClientRect`, not
//! pixels), which is why these are Rust tests and not another raster
//! comparison: a raster would still show red-over-green here for an
//! unrelated reason (a pre-existing stacking/paint-order bug between
//! `position:relative` and `position:absolute` siblings, reproduced with two
//! plain `<div>`s and no table at all — out of scope for this module).

use super::{geometria, rect};

const BASE: &str = "<style>table{border-collapse:collapse}td{padding:0}\
td>div{width:50px;height:50px}</style>";

/// A `<tbody>`'s own `position:relative;left:100px` moves the tbody's rect
/// AND its cell's rect by the same 100px — nothing about the row's own
/// layout (the grid, the column widths) changes.
#[test]
fn tbody_relative_left_shifts_row_and_cell() {
    let html = format!(
        "{BASE}<table><tbody style=\"position:relative;left:100px\">\
         <tr><td><div></div></td></tr></tbody></table>"
    );
    let (dom, list) = geometria(&html, 900.0);
    let tbody = rect(&dom, &list, "tbody", 0);
    let td = rect(&dom, &list, "td", 0);
    assert!((tbody.x - 100.0).abs() < 0.51, "tbody.x = {}", tbody.x);
    assert!((td.x - 100.0).abs() < 0.51, "td.x = {}", td.x);
}

/// Same shift, on a bare `<tr>` (no explicit row group) and the vertical
/// axis (`top`) instead of the horizontal one — CSS 2.1 §9.4.3 treats both
/// axes the same way, and the fix does not special-case either.
#[test]
fn tr_relative_top_shifts_row_and_cell() {
    let html = format!(
        "{BASE}<table><tr style=\"position:relative;top:100px\">\
         <td><div></div></td></tr></table>"
    );
    let (dom, list) = geometria(&html, 900.0);
    let tr = rect(&dom, &list, "tr", 0);
    let td = rect(&dom, &list, "td", 0);
    assert!((tr.y - 100.0).abs() < 0.51, "tr.y = {}", tr.y);
    assert!((td.y - 100.0).abs() < 0.51, "td.y = {}", td.y);
}

/// A `<thead>`'s offset composes with a `<tr>`'s own offset inside it —
/// nested `position:relative` on table parts adds up, the same way it does
/// on ordinary blocks (`layout/relativo.rs`'s own doc comment). Regression
/// guard for shifting the row's pieces in place and then re-walking the same
/// range for the group: the second walk must ADD to the first, not replace
/// it or skip it.
#[test]
fn thead_and_row_relative_offsets_compose() {
    let html = format!(
        "{BASE}<table><thead style=\"position:relative;left:20px\">\
         <tr style=\"position:relative;left:5px\"><td><div></div></td></tr>\
         </thead></table>"
    );
    let (dom, list) = geometria(&html, 900.0);
    let td = rect(&dom, &list, "td", 0);
    assert!((td.x - 25.0).abs() < 0.51, "td.x = {}", td.x);
}

/// A row/row-group with no `top`/`left`/`right`/`bottom` at all stays exactly
/// where the grid put it — `position:relative` alone is not itself an
/// offset, and the new code path must not walk the subtree (and pay its
/// cost) when there is nothing to move.
#[test]
fn relative_row_with_no_inset_does_not_move() {
    let html = format!(
        "{BASE}<table><tbody style=\"position:relative\">\
         <tr><td><div></div></td></tr></tbody></table>"
    );
    let (dom, list) = geometria(&html, 900.0);
    let td = rect(&dom, &list, "td", 0);
    assert!(td.x.abs() < 0.51 && td.y.abs() < 0.51, "td = {:?}", td);
}

/// `<td>` was already correct before this change (it goes through the
/// ordinary `layout_block`) — kept here as the control that pins it stays
/// so, now that rows/groups take a second code path to the same offset.
#[test]
fn td_relative_left_still_shifts_itself() {
    let html = format!(
        "{BASE}<table><tr><td style=\"position:relative;left:100px\"><div></div></td></tr></table>"
    );
    let (dom, list) = geometria(&html, 900.0);
    let td = rect(&dom, &list, "td", 0);
    assert!((td.x - 100.0).abs() < 0.51, "td.x = {}", td.x);
}
