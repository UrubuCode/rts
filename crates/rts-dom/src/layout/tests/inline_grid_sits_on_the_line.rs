//! `display: inline-grid` is an INLINE-level box that establishes a grid
//! formatting context (CSS Display §2.2): it sits on the line beside text and
//! takes its shrink-to-fit width (CSS 2.1 §10.3.9, Grid §5.2), never the
//! containing block's. It was laid out block-level, 800 wide, because the
//! inline-level lists in the block flow named only inline-block/inline-flex.

use super::*;

fn colored(list: &DisplayList, want: u32) -> Rect {
    list.materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == want => Some(*rect),
            _ => None,
        })
        .expect("the box paints its background")
}

#[test]
fn a_bare_inline_grid_shrinks_to_its_child() {
    let list = layout(
        "<div style='display:inline-grid;background:#00f'><div style='width:100px;height:20px'></div></div>",
        800.0,
    );
    assert_eq!(colored(&list, 0x0000FFFF).w, 100.0, "shrink-to-fit, not the 800px line");
}

#[test]
fn an_inline_grid_sits_on_the_line_beside_text() {
    let list = layout(
        "<div>text <div style='display:inline-grid;background:#00f'>\
         <div style='width:100px;height:20px'></div></div> more</div>",
        800.0,
    );
    let r = colored(&list, 0x0000FFFF);
    assert_eq!(r.w, 100.0);
    assert!(r.x > 0.0, "after the word on the same line, not at the left edge: {r:?}");
    assert!(r.y < 20.0, "on the first line, not pushed below it: {r:?}");
}
