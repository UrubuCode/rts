use super::*;
use crate::table::tests::rect;

#[test]
fn content_basis_in_a_column_ignores_the_items_height() {
    let dom = parse_html_to_dom(include_str!(
        "../../../../../tests/css/claude-flex-basis-content-coluna.html"
    ));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let item = rect(&dom, &list, "#item", 0);
    assert!((item.h - 14.0).abs() <= 1.0, "#item: {item:?}, Blink: 14px");
}
