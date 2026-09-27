//! The static position of an INLINE-level absolute box met by the block flow
//! beside a float (CSS 2.1 §10.3.7/§10.6.4): where it would have been as an
//! inline in flow, i.e. on a line SHORTENED by the float, placed by the line's
//! `text-align`, and under `direction: rtl` with its right edge on that point.
//!
//! Shape of WPT `css-position/static-position/inline-level-absolute-in-block-level-context-*`
//! without the `transform` those tests use to cancel the alignment: a 100px
//! container, a 50px-tall in-flow block, a 50x50 float, then the absolute box.
//! The reference paints a 100x100 green square, so the float and the box share
//! the band `y = 50..100` and the numbers below are what the free band gives.

use super::*;

fn blue(list: &DisplayList) -> Rect {
    list.materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
            _ => None,
        })
        .expect("the absolute box paints blue")
}

fn abs_beside_float(direction: &str, align: &str, float: &str) -> Rect {
    let html = format!(
        "<div style='position:relative;width:100px;height:100px;direction:{direction};text-align:{align}'>\
         <div style='height:50px'></div>\
         <div style='float:{float};width:50px;height:50px'></div>\
         <div style='display:inline;position:absolute;width:50px;height:50px;background:#00f'></div>\
         </div>"
    );
    blue(&layout(&html, 600.0))
}

#[test]
fn ltr_line_starts_after_a_left_float() {
    let r = abs_beside_float("ltr", "left", "left");
    assert_eq!((r.x, r.y), (50.0, 50.0), "beside the float, not below it nor at the edge");
}

#[test]
fn ltr_center_and_right_align_inside_the_shortened_band() {
    assert_eq!(abs_beside_float("ltr", "center", "left").x, 75.0);
    assert_eq!(abs_beside_float("ltr", "right", "left").x, 100.0);
}

#[test]
fn ltr_line_before_a_right_float_starts_at_the_content_edge() {
    assert_eq!(abs_beside_float("ltr", "left", "right").x, 0.0);
}

#[test]
fn rtl_puts_the_right_edge_on_the_line_position() {
    // band 50..100, right-aligned: the box's right edge at 100.
    assert_eq!(abs_beside_float("rtl", "right", "left").x, 50.0);
    // band 0..50, centred at 25: right edge at 25.
    assert_eq!(abs_beside_float("rtl", "center", "right").x, -25.0);
    // band 0..50, left-aligned: right edge at 0.
    assert_eq!(abs_beside_float("rtl", "left", "right").x, -50.0);
}
