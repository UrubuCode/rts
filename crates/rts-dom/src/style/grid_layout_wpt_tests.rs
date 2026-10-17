// Tests running grid layout-algorithm WPT fixtures.

#[test]
fn test_flex_sizing_rows_indefinite_height() {
    let html = r#"<!DOCTYPE HTML>
<style>
.grid {
  display: inline-grid;
  position: relative;
  grid-template-columns: minmax(0, .5fr);
  grid-template-rows: minmax(0, .5fr);
}
.item {
  width: 200px;
  height: 200px;
  background: linear-gradient(green, green) no-repeat;
  background-size: 100px 100px;
}
.abspos {
  grid-area: 1 / 1 / 2 / 2;
  position: absolute;
  width: 200%;
  height: 200%;
  background: red;
  z-index: -1;
}
</style>
<div class="grid">
  <div class="item"></div>
  <div class="abspos"></div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let grid_node = dom.resolve(divs[0]).unwrap();
    let grid_rect = list.rect_of(grid_node).unwrap();
    let abspos_rect = list.rect_of(dom.resolve(divs[2]).unwrap()).unwrap();
    let cols = list.grid_column_tracks.get(&grid_node).unwrap();
    let rows = list.grid_row_tracks.get(&grid_node).unwrap();
    assert_eq!(cols.as_slice(), &[50.0]);
    assert_eq!(rows.as_slice(), &[50.0]);
    assert_eq!(grid_rect.w, 50.0);
    assert_eq!(grid_rect.h, 50.0);
    assert_eq!(abspos_rect.w, 100.0);
    assert_eq!(abspos_rect.h, 100.0);
}

