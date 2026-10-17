// Tests running grid items WPT fixtures against references or expected positions.

#[test]
fn test_grid_item_inline_contribution_001() {
    let html = r#"<!DOCTYPE html>
<div style="display: inline-grid; background: green; height: 100px; grid-template-rows: 50px;">
  <div style="height: 100%;">
    <canvas width="20" height="10" style="height: 100%;">
  </div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let grid_rect = list.rect_of(dom.resolve(divs[0]).unwrap()).unwrap();
    let child_rect = list.rect_of(dom.resolve(divs[1]).unwrap()).unwrap();
    let canvases = dom.query_all("canvas");
    let canvas_rect = list.rect_of(dom.resolve(canvases[0]).unwrap()).unwrap();
    assert_eq!(grid_rect.w, 100.0, "grid should be 100px wide");
    assert_eq!(grid_rect.h, 100.0, "grid should be 100px tall");
    assert_eq!(child_rect.w, 100.0, "child div should be 100px wide");
    assert_eq!(child_rect.h, 50.0, "child div should be 50px tall");
    assert_eq!(canvas_rect.w, 100.0, "canvas should be 100px wide");
    assert_eq!(canvas_rect.h, 50.0, "canvas should be 50px tall");
}

#[test]
fn test_grid_item_inline_contribution_003() {
    let html = r#"<!DOCTYPE html>
<div style="display: grid; width: 0; grid-template: auto / auto auto;">
  <div style="background: green;">
    <canvas width="10" height="10" style="height: 100%;">
  </div>
  <div>
    <div style="height: 100px;"></div>
  </div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let green_div_node = dom.resolve(divs[1]).unwrap();
    let green_div = list.rect_of(green_div_node).unwrap();
    let canvases = dom.query_all("canvas");
    let canvas_rect = list.rect_of(dom.resolve(canvases[0]).unwrap()).unwrap();
    let grid_node = dom.resolve(divs[0]).unwrap();
    let col_tracks = list.grid_column_tracks.get(&grid_node);
    assert_eq!(col_tracks, Some(&vec![100.0, 0.0]), "col tracks should be 100px and 0px");
    assert_eq!(green_div.w, 100.0, "green div should be 100px wide");
    assert_eq!(green_div.h, 100.0, "green div should be 100px tall");
    assert_eq!(canvas_rect.w, 100.0, "canvas should be 100px wide");
    assert_eq!(canvas_rect.h, 100.0, "canvas should be 100px tall");
}

#[test]
fn test_grid_item_inline_contribution_005() {
    let html = r#"<!DOCTYPE html>
<div style="display: grid; width: 0; grid-template: auto / auto auto;">
  <div style="grid-row: 1 / 2; background: green;">
    <canvas width="10" height="10" style="height: 100%;">
  </div>
  <div style="grid-row: 1 / 2;">
    <div style="height: 100px;"></div>
  </div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let green_div_node = dom.resolve(divs[1]).unwrap();
    let green_div = list.rect_of(green_div_node).unwrap();
    let canvases = dom.query_all("canvas");
    let canvas_rect = list.rect_of(dom.resolve(canvases[0]).unwrap()).unwrap();
    let grid_node = dom.resolve(divs[0]).unwrap();
    let col_tracks = list.grid_column_tracks.get(&grid_node);
    assert_eq!(col_tracks, Some(&vec![100.0, 0.0]), "col tracks should be 100px and 0px");
    assert_eq!(green_div.w, 100.0, "green div should be 100px wide");
    assert_eq!(green_div.h, 100.0, "green div should be 100px tall");
    assert_eq!(canvas_rect.w, 100.0, "canvas should be 100px wide");
    assert_eq!(canvas_rect.h, 100.0, "canvas should be 100px tall");
}

#[test]
fn test_aspect_ratio_005() {
    let html = r#"<!DOCTYPE html>
<div id="g" style="display: inline-grid; grid-template-rows: 50px 50px; background: red; height: 100px;">
  <div id="a" style="grid-row: 1; background: green; aspect-ratio: 2/1; height: 100%;"></div>
  <div id="b" style="grid-row: 2; background: green;"></div>
</div>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let a = crate::table::tests::rect(&dom, &list, "#a", 0);
    let b = crate::table::tests::rect(&dom, &list, "#b", 0);
    let g = crate::table::tests::rect(&dom, &list, "#g", 0);
    let _g_node = dom.resolve(dom.query_all("#g")[0]).unwrap();
    assert_eq!((g.w, g.h), (100.0, 100.0));
    assert_eq!((a.w, a.h), (100.0, 50.0));
    assert_eq!((b.w, b.h), (100.0, 50.0));
}

#[test]
fn test_aspect_ratio_004() {
    let html = r#"<!DOCTYPE html>
<div id="g" style="display: inline-grid; grid-template-rows: 100px; background: green;">
  <img id="img" src="support/50x50-green.png" style="align-self: stretch; opacity: 0;">
</div>"#;
    let mut dom = crate::parse_html_to_dom(html);
    let img_id = dom.query_all("#img")[0];
    dom.set_pixel_data(img_id, vec![0; 50 * 50 * 4], 50, 50);
    let img_node = dom.resolve(img_id).unwrap();
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let g = list.rect_of(dom.resolve(dom.query_all("#g")[0]).unwrap()).unwrap();
    let img = list.rect_of(img_node).unwrap();
    assert_eq!((g.w, g.h), (100.0, 100.0));
    assert_eq!((img.w, img.h), (100.0, 100.0));
}

#[test]
fn test_grid_items_001() {
    let html = r#"<!DOCTYPE html>
<div id="g1" style="display: grid; font: 25px/1 Ahem; color: green; grid-template-columns: auto auto; justify-content: start; align-content: start;">
  <div>it</div>
  em
  <div>it</div>
  em
</div>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let g1 = crate::table::tests::rect(&dom, &list, "#g1", 0);
    let g1_node = dom.resolve(dom.query_all("#g1")[0]).unwrap();
    let col_tracks = list.grid_column_tracks.get(&g1_node);
    assert_eq!(col_tracks, Some(&vec![50.0, 50.0]));
    assert_eq!((g1.w, g1.h), (800.0, 50.0));
}

#[test]
fn test_grid_img_item_percent_max_height_001() {
    let html = r#"<!DOCTYPE html>
<div id="g" style="display: grid; grid-template-rows: 200px;">
  <img id="img" src="support/200x200-green.png" style="max-height: 50%;">
</div>"#;
    let mut dom = crate::parse_html_to_dom(html);
    let img_id = dom.query_all("#img")[0];
    dom.set_pixel_data(img_id, vec![0; 200 * 200 * 4], 200, 200);
    let img_node = dom.resolve(img_id).unwrap();
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let img = list.rect_of(img_node).unwrap();
    assert_eq!((img.w, img.h), (100.0, 100.0));
}

#[test]
fn test_grid_item_containing_block_004() {
    let html = r#"<!DOCTYPE html>
<style>
    body { margin: 8px; }
    p { margin-top: 16px; margin-bottom: 16px; font-size: 16px; }
    #grid {
        display: grid;
        position: absolute;
        height: 200px;
        width: 200px;
        grid-template-rows: 1fr 1fr;
        grid-template-columns: 1fr 1fr;
    }
    #test-item { width: 100%; height: 100%; }
</style>
<p>Test</p>
<div id="grid">
    <div id="test-item"></div>
</div>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let grid = crate::table::tests::rect(&dom, &list, "#grid", 0);
    assert_eq!(grid.x, 8.0);
    assert!(grid.y >= 40.0, "grid.y should account for p bottom margin, got {}", grid.y);
    assert_eq!((grid.w, grid.h), (200.0, 200.0));
}

#[test]
fn test_debug_minmax_img_002() {
    let html_t = r#"<!DOCTYPE html>
<style>
.grid {
    display: inline-grid;
    grid-template-columns: minmax(auto, 0);
    grid-template-rows: minmax(auto, 0);
    border: 5px solid goldenrod;
}
.spacer{
    height: 10px;
}
img {
    border: 2px solid indigo;
}
.stretch {
    align-self: stretch;
    justify-self: stretch;
}
.start {
    align-self: start;
    justify-self: start;
}
</style>
<div class="grid">
  <img src="support/100x100-green.png">
</div>
<div class="spacer"></div>
<div class="grid">
  <img class="stretch" src="support/100x100-green.png">
</div>
<div class="spacer"></div>
<div class="grid">
  <img class="start" src="support/100x100-green.png">
</div>"#;

    let html_r = r#"<!DOCTYPE html>
<style>
.grid {
    display: inline-grid;
    grid-template-columns: 4px;
    grid-template-rows: 4px;
    border: 5px solid goldenrod;
}
.spacer{
    height: 10px;
}
.img{
    background-image: url("support/100x100-green.png");
    width: 100px;
    height: 100px;
}
.img, .empty{
    border: 2px solid indigo;
}
</style>
<div class="grid">
  <div class="img"></div>
</div>
<div class="spacer"></div>
<div class="grid">
  <div class="empty"></div>
</div>
<div class="spacer"></div>
<div class="grid">
  <div class="img"></div>
</div>"#;

    let mut dom_t = crate::parse_html_to_dom(html_t);
    for img in dom_t.query_all("img") {
        dom_t.set_pixel_data(img, vec![0; 100 * 100 * 4], 100, 100);
    }
    let mut dom_r = crate::parse_html_to_dom(html_r);

    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);

    let grids_t: Vec<_> = dom_t.query_all(".grid").into_iter().map(|g| list_t.rect_of(dom_t.resolve(g).unwrap()).unwrap()).collect();
    let grids_r: Vec<_> = dom_r.query_all(".grid").into_iter().map(|g| list_r.rect_of(dom_r.resolve(g).unwrap()).unwrap()).collect();

    let imgs_t: Vec<_> = dom_t.query_all("img").into_iter().map(|g| list_t.rect_of(dom_t.resolve(g).unwrap()).unwrap()).collect();
    let imgs_r: Vec<_> = dom_r.query_all(".img, .empty").into_iter().map(|g| list_r.rect_of(dom_r.resolve(g).unwrap()).unwrap()).collect();

    assert_eq!(grids_t, grids_r);
    assert_eq!(imgs_t, imgs_r);
}

#[test]
fn test_grid_inline_items_001() {
    let html = r#"<!DOCTYPE html>
<style>
  body { margin: 8px; }
  p { margin: 16px 0; font-size: 16px; }
  #reference-overlapped-red {
    position: absolute;
    background-color: red;
    width: 100px;
    height: 100px;
    z-index: -1;
  }
  .test-inline-grid-overlapping-green {
    display: inline-grid;
    font: 25px/1 Ahem;
    color: green;
  }
</style>
<p>Test</p>
<div id="reference-overlapped-red"></div>
<div class="test-inline-grid-overlapping-green">
  <div>it</div>
  em
  <div>it</div>
  em
</div><div class="test-inline-grid-overlapping-green">
  it
  <span>em</span>
  it
  <span>em</span>
</div>"#;

    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let red = crate::table::tests::rect(&dom, &list, "#reference-overlapped-red", 0);
    let g1 = crate::table::tests::rect(&dom, &list, ".test-inline-grid-overlapping-green", 0);
    let g2 = crate::table::tests::rect(&dom, &list, ".test-inline-grid-overlapping-green", 1);
    assert_eq!(g1.y, red.y);
    assert_eq!(g2.y, red.y);
    assert_eq!(g1.x, red.x);
    assert_eq!(g2.x, g1.x + g1.w);
    assert_eq!(g1.w + g2.w, red.w);
    assert_eq!(g1.h, red.h);
    assert_eq!(g2.h, red.h);
}

#[test]
fn test_grid_inline_items_002() {
    let html = r#"<!DOCTYPE html>
<style>
  body { margin: 8px; }
  p { margin: 16px 0; font-size: 16px; }
  #reference-overlapped-red {
    position: absolute;
    background-color: red;
    width: 100px;
    height: 100px;
    z-index: -1;
  }
  .test-inline-grid-overlapping-green {
    display: inline-grid;
    font: 25px/1 Ahem;
    color: green;
  }
</style>
<p>Test</p>
<div id="reference-overlapped-red"></div>
<div class="test-inline-grid-overlapping-green">
  <div>
    <div>it</div>
    em
  </div>
  <div>
    it
    <div>em</div>
  </div>
</div><div class="test-inline-grid-overlapping-green">
  <span>
    <span>i</span>t
  </span>
  <span>
    e<span>m</span>
  </span>
  <span>
    <span>i</span>t
  </span>
  <span>
    e<span>m</span>
  </span>
</div>"#;

    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let red = crate::table::tests::rect(&dom, &list, "#reference-overlapped-red", 0);
    let g1 = crate::table::tests::rect(&dom, &list, ".test-inline-grid-overlapping-green", 0);
    let g2 = crate::table::tests::rect(&dom, &list, ".test-inline-grid-overlapping-green", 1);
    assert_eq!(g1.y, red.y);
    assert_eq!(g2.y, red.y);
    assert_eq!(g1.x, red.x);
    assert_eq!(g2.x, g1.x + g1.w);
    assert_eq!(g1.w + g2.w, red.w);
    assert_eq!(g1.h, red.h);
    assert_eq!(g2.h, red.h);
}

#[test]
fn test_grid_minimum_size_grid_items_017() {
    let html = r#"<!DOCTYPE html>
<style>
#reference-overlapped-green {
    position: absolute;
    background-color: green;
    width: 100px;
    height: 100px;
    z-index: -1;
}
#constrained-grid {
    display: grid;
    width: 10px;
    height: 10px;
    grid: minmax(auto, 0px) / minmax(auto, 0px);
}
#test-grid-item-overlapping-red {
    background-color: red;
}
#content-100x100 {
    width: 100px;
    height: 100px;
}
</style>
<div id="reference-overlapped-green"></div>
<div id="constrained-grid">
    <div id="test-grid-item-overlapping-red">
        <div id="content-100x100"></div>
    </div>
</div>"#;

    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let item = crate::table::tests::rect(&dom, &list, "#test-grid-item-overlapping-red", 0);
    assert_eq!((item.w, item.h), (0.0, 0.0));
}

#[test]
fn test_named_grid_lines_001() {
    let html = r#"<!DOCTYPE html>
<style>
.grid {
    display: inline-grid;
    background: red;
    grid-template-columns: [C] 25px [C] 25px [C] 25px [C] 25px;
    grid-auto-rows: 50px;
    grid-template-areas: "A1 A2 A3 A4"
                         ".  A2 A3 A4";
}
.grid > div { background: green; }
</style>
<div class="grid">
    <div id="d1" style="grid-area: A1"></div>
    <div id="d2" style="grid-area: A2"></div>
    <div id="d3" style="grid-area: A3"></div>
    <div id="d4" style="grid-area: A4"></div>
    <div id="d5" style="grid-column: C; grid-row: 2;"></div>
</div>"#;

    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let g = crate::table::tests::rect(&dom, &list, ".grid", 0);
    let d5 = crate::table::tests::rect(&dom, &list, "#d5", 0);
    assert_eq!((g.w, g.h), (100.0, 100.0));
    assert_eq!((d5.x, d5.y, d5.w, d5.h), (0.0, 50.0, 25.0, 50.0));
}
