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

use crate::paint::Rect;
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

#[test]
fn test_background_001_geometry() {
    let html = r#"<style>
.grid {
  display: grid;
  grid-template-columns: 100px 200px;
  grid-template-rows: 50px 150px;
  width: 500px;
  height: 300px;
  border: 5px solid black;
  margin: 30px;
  padding: 15px;
  position: relative;
}
.container { position: relative; float: left; }
.absolute { position: absolute; }
.onlyFirstRowOnlyFirstColumn { grid-column: 1 / 2; grid-row: 1 / 2; }
.onlyFirstRowOnlySecondColumn { grid-column: 2 / 3; grid-row: 1 / 2; }
.onlySecondRowOnlyFirstColumn { grid-column: 1 / 2; grid-row: 2 / 3; }
.onlySecondRowOnlySecondColumn { grid-column: 2 / 3; grid-row: 2 / 3; }
.lengthSize { width: 50px; height: 20px; }
.percentageSize { width: 50%; height: 20%; }
.offsetsSize { left: 25px; right: 30px; top: 35px; bottom: 40px; }
</style>
<p>The test passes if you see 4 green boxes and no red.</p>
<div class="container">
  <div id="r1" class="absolute red" style="left: 55px; top: 50px; width: 100px; height: 50px"></div>
  <div id="r2" class="absolute red" style="left: 290px; top: 50px; width: 50px; height: 20px"></div>
  <div id="r3" class="absolute red" style="left: 50px; top: 115px; width: 50px; height: 30px"></div>
  <div id="r4" class="absolute red" style="left: 175px; top: 135px; width: 145px; height: 75px"></div>
  <div class="grid">
    <div id="g1" class="absolute onlyFirstRowOnlyFirstColumn" style="width:100%;height:100%;left: 5px;"></div>
    <div id="g2" class="absolute onlyFirstRowOnlySecondColumn lengthSize" style="right: 10px;"></div>
    <div id="g3" class="absolute onlySecondRowOnlyFirstColumn percentageSize" style="top: 15px;"></div>
    <div id="g4" class="absolute onlySecondRowOnlySecondColumn offsetsSize"></div>
  </div>
</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let r1 = rect(&dom, &list, "#r1", 0);
    let g1 = rect(&dom, &list, "#g1", 0);
    assert_eq!(r1, g1);
    let r2 = rect(&dom, &list, "#r2", 0);
    let g2 = rect(&dom, &list, "#g2", 0);
    assert_eq!(r2, g2);
    let r3 = rect(&dom, &list, "#r3", 0);
    let g3 = rect(&dom, &list, "#g3", 0);
    assert_eq!(r3, g3);
    let r4 = rect(&dom, &list, "#r4", 0);
    let g4 = rect(&dom, &list, "#g4", 0);
    assert_eq!(r4, g4);
}

#[test]
fn test_negative_indices_003_geometry() {
    let html = r#"<style>
  #grid {
    display: grid;
    margin: 4px;
    padding: 10px;
    width: 500px;
    height: 130px;
    position: relative;
  }
  #absolute {
    position: absolute;
    width: 100%;
    height: 100%;
    grid-column: -3 / span 6;
    grid-row: 1 / 2;
    background-color: lightblue;
  }
  #item {
    grid-column: -5 / span 8;
    background-color: hotpink;
  }
</style>
<div id="grid">
  <div id="absolute"></div>
  <div id="item"></div>
</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let r_item = rect(&dom, &list, "#item", 0);
    let r_abs = rect(&dom, &list, "#absolute", 0);
    assert_eq!(r_item, Rect::new(14.0, 14.0, 500.0, 130.0));
    assert_eq!(r_abs, Rect::new(139.0, 14.0, 375.0, 130.0));
}

#[test]
fn test_018_abspos_clamped_container() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\positioned-grid-items-018.html",
    ).unwrap();
    let ref_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\reference\ref-filled-green-100px-square-only.html",
    ).unwrap();
    let (d_t, l_t) = geometria(&test_html, 800.0);
    let (d_r, l_r) = geometria(&ref_html, 800.0);
    let abs_node = d_t.query("div > div").unwrap();
    let ref_div = d_r.query("div").unwrap();
    assert_eq!(
        l_t.rect_of(d_t.resolve(abs_node).unwrap()),
        l_r.rect_of(d_r.resolve(ref_div).unwrap()),
    );
}

#[test]
fn test_orthogonal_001() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\orthogonal-positioned-grid-items-001.html",
    ).unwrap();
    let ref_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\orthogonal-positioned-grid-items-001-ref.html",
    ).unwrap();
    let (d_t, l_t) = geometria(&test_html, 800.0);
    let (d_r, l_r) = geometria(&ref_html, 800.0);
    for id in ["#firstItem", "#secondItem", "#thirdItem", "#fourthItem"] {
        let n_t = d_t.query(id).unwrap();
        let n_r = d_r.query(id).unwrap();
        let r_t = l_t.rect_of(d_t.resolve(n_t).unwrap());
        let r_r = l_r.rect_of(d_r.resolve(n_r).unwrap());
        assert_eq!(r_t, r_r, "mismatch in {id}");
    }
}

#[test]
fn test_sizing_001() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\positioned-grid-items-sizing-001.html",
    ).unwrap();
    let ref_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\positioned-grid-items-sizing-001-ref.html",
    ).unwrap();
    let grid_css = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\support\grid.css",
    ).unwrap();
    let mut d_t = crate::parse_html_to_dom(&test_html);
    d_t.add_stylesheet(&grid_css);
    let mut d_r = crate::parse_html_to_dom(&ref_html);
    d_r.add_stylesheet(&grid_css);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::measure::text_measurer::ApproxMeasurer,
    };
    let l_t = crate::layout::layout_document(&d_t, &ctx);
    let l_r = crate::layout::layout_document(&d_r, &ctx);
    let g_t = d_t.query_all(".grid");
    let g_r = d_r.query_all(".grid");
    println!("Grid counts: test={}, ref={}", g_t.len(), g_r.len());
    for (gi, (&nt, &nr)) in g_t.iter().zip(g_r.iter()).enumerate() {
        let it = d_t.resolve(nt).unwrap();
        let ir = d_r.resolve(nr).unwrap();
        let rt = l_t.rect_of(it);
        let rr = l_r.rect_of(ir);
        println!("grid {gi}: test={rt:?}, ref={rr:?}");
        for (ci, &child_t) in d_t.node(it).children.iter().enumerate() {
            if !matches!(d_t.node(child_t).kind, crate::dom::NodeKind::Element { .. }) { continue; }
            let child_r = d_r.node(ir).children[ci];
            let crt = l_t.rect_of(child_t);
            let crr = l_r.rect_of(child_r);
            if crt != crr {
                println!("  child {ci}: test={crt:?}, ref={crr:?}");
            }
        }
    }
}

#[test]
fn test_descendant_001() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\descendant-static-position-001.html",
    ).unwrap();
    let ref_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\descendant-static-position-001-ref.html",
    ).unwrap();
    let (d_t, l_t) = geometria(&test_html, 800.0);
    let (d_r, l_r) = geometria(&ref_html, 800.0);
    let cnt_t = d_t.query_all(".content");
    let cnt_r = d_r.query_all(".content");
    assert_eq!(cnt_t.len(), cnt_r.len());
    for (&nt, &nr) in cnt_t.iter().zip(cnt_r.iter()) {
        let it = d_t.resolve(nt).unwrap();
        let ir = d_r.resolve(nr).unwrap();
        assert_eq!(l_t.rect_of(it), l_r.rect_of(ir));
    }
}

#[test]
fn test_descendant_002() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\descendant-static-position-002.html",
    ).unwrap();
    let ref_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\descendant-static-position-002-ref.html",
    ).unwrap();
    let (d_t, l_t) = geometria(&test_html, 800.0);
    let (d_r, l_r) = geometria(&ref_html, 800.0);
    let cnt_t = d_t.query_all(".content");
    let cnt_r = d_r.query_all(".content");
    assert_eq!(cnt_t.len(), cnt_r.len());
    for (&nt, &nr) in cnt_t.iter().take(8).zip(cnt_r.iter().take(8)) {
        let it = d_t.resolve(nt).unwrap();
        let ir = d_r.resolve(nr).unwrap();
        assert_eq!(l_t.rect_of(it), l_r.rect_of(ir));
    }
}

#[test]
fn test_bg_rtl_001() {
    let test_html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\abspos\grid-positioned-items-background-rtl-001.html",
    ).unwrap();
    let grid_css = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\support\grid.css",
    ).unwrap();
    let mut d_t = crate::parse_html_to_dom(&test_html);
    d_t.add_stylesheet(&grid_css);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::measure::text_measurer::ApproxMeasurer,
    };
    let l_t = crate::layout::layout_document(&d_t, &ctx);
    let items = d_t.query_all(".grid > div");
    let reds = d_t.query_all(".red");
    println!("items: {}, reds: {}", items.len(), reds.len());
    for (i, (&it_node, &red_node)) in items.iter().zip(reds.iter()).enumerate() {
        let it = d_t.resolve(it_node).unwrap();
        let red = d_t.resolve(red_node).unwrap();
        let r_it = l_t.rect_of(it);
        let r_red = l_t.rect_of(red);
        assert_eq!(r_it, r_red, "item {i} mismatch");
    }
}
