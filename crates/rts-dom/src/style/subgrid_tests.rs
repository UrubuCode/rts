use crate::style::fmt_values::fmt_grid_template;
use crate::style::{Dimension, GridTemplate, GridTrack, SubgridLine};

#[test]
fn test_parse_grid_template_none() {
    assert_eq!(GridTemplate::parse("none"), Some(GridTemplate::None));
    assert_eq!(fmt_grid_template(Some(&GridTemplate::None)), "none");
}

#[test]
fn test_parse_grid_template_tracks_with_names() {
    let t = GridTemplate::parse("[first] 100px [main] 200px [last]").unwrap();
    if let GridTemplate::Tracks(tl) = &t {
        assert_eq!(tl.entries.len(), 2);
        assert_eq!(tl.entries[0].0, vec!["first"]);
        assert_eq!(tl.entries[0].1, GridTrack::Fixed(Dimension::Px(100.0)));
        assert_eq!(tl.entries[1].0, vec!["main"]);
        assert_eq!(tl.entries[1].1, GridTrack::Fixed(Dimension::Px(200.0)));
        assert_eq!(tl.trailing, vec!["last"]);
    } else {
        panic!("expected Tracks");
    }
    assert_eq!(
        fmt_grid_template(Some(&t)),
        "[first] 100px [main] 200px [last]"
    );
}

#[test]
fn test_parse_grid_template_subgrid() {
    let s = GridTemplate::parse("subgrid").unwrap();
    assert_eq!(s, GridTemplate::Subgrid(Vec::new()));
    assert_eq!(fmt_grid_template(Some(&s)), "subgrid");

    let s2 = GridTemplate::parse("subgrid [a] [b c]").unwrap();
    assert_eq!(
        s2,
        GridTemplate::Subgrid(vec![
            SubgridLine::Line(vec!["a".to_string()]),
            SubgridLine::Line(vec!["b".to_string(), "c".to_string()]),
        ])
    );
    assert_eq!(fmt_grid_template(Some(&s2)), "subgrid [a] [b c]");
}

#[test]
fn test_parse_grid_template_subgrid_repeat() {
    let s = GridTemplate::parse("subgrid repeat(auto-fill, [x])").unwrap();
    assert_eq!(
        s,
        GridTemplate::Subgrid(vec![SubgridLine::AutoFill(vec![vec!["x".to_string()]])])
    );
    assert_eq!(
        fmt_grid_template(Some(&s)),
        "subgrid repeat(auto-fill, [x])"
    );
}

#[test]
fn test_invalid_subgrid_idents() {
    assert!(GridTemplate::parse("subgrid [none]").is_none());
    assert!(GridTemplate::parse("subgrid [subgrid]").is_none());
    assert!(GridTemplate::parse("subgrid [auto]").is_none());
    assert!(GridTemplate::parse("subgrid [inherit]").is_none());
    assert!(GridTemplate::parse("subgrid [1a]").is_none());
}

#[test]
fn test_auto_repeat_line_merging() {
    let t = GridTemplate::parse("[x] repeat(2, [a] 50px [b]) [z]").unwrap();
    if let GridTemplate::Tracks(tl) = t {
        let (tracks, _collapsible, lines) =
            crate::layout::grid::tracks::expand_auto_repeats(&tl, 200.0, 10.0, None);
        assert_eq!(tracks.len(), 2);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], vec!["x", "a"]);
        assert_eq!(lines[1], vec!["b", "a"]);
        assert_eq!(lines[2], vec!["b", "z"]);
    } else {
        panic!("expected Tracks");
    }
}

#[test]
fn test_wpt_grid_placement_002() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
.grid {
  display: grid;
  width: 100px;
  height: 100px;
  background: red;
  grid-template-columns: [area-start] repeat(auto-fill, 10px) [area-end];
  grid-template-rows: [area-start] repeat(auto-fill, 10px [area-start]) [area-end];
}
.grid > div {
  grid-area: area;
  background: green;
}
</style>
</head>
<body>
<div class="grid">
  <div id="target"></div>
</div>
</body>
</html>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let r = crate::table::tests::rect(&dom, &list, "#target", 0);
    eprintln!("TARGET RECT: {:?}", r);
    assert_eq!(r.w, 100.0);
    assert_eq!(r.h, 100.0);
}

#[test]
fn test_computed_subgrid_on_block() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
#target {
  display: block;
  grid-template-columns: subgrid [a] [b];
}
</style>
</head>
<body>
  <div id="target"></div>
</body>
</html>"#;
    let (dom, _list) = crate::table::tests::geometria(html, 800.0);
    let target = dom.query("#target").unwrap();
    let val = dom.computed_property(target, "grid-template-columns");
    assert_eq!(val, "subgrid [a] [b]");
}

#[test]
fn test_computed_subgrid_invalid_on_grid_without_grid_parent() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
#parent {
  display: block;
}
#target {
  display: grid;
  grid-template-columns: subgrid [a] [b];
}
</style>
</head>
<body>
  <div id="parent">
    <div id="target"></div>
  </div>
</body>
</html>"#;
    let (dom, _list) = crate::table::tests::geometria(html, 800.0);
    let target = dom.query("#target").unwrap();
    let val = dom.computed_property(target, "grid-template-columns");
    assert_eq!(val, "none");
}

#[test]
fn test_standalone_axis_size() {
    let html = r#"<!DOCTYPE html>
<style>
.grid {
  display: grid;
  grid-template-rows: 100px;
}
.subgrid {
  display: grid;
  background: red;
  width: min-content;
  grid-template-rows: subgrid;
}
.item {
  background: green;
  width: 100px;
}
</style>
<div class="grid">
  <div class="subgrid">
    <div class="item"></div>
  </div>
</div>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let subgrid = dom.resolve(dom.query(".subgrid").unwrap()).unwrap();
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let rect = list.rect_of(subgrid).expect("subgrid rect");
    assert_eq!(rect.w, 100.0);
}
#[test]
fn test_grid_gap_smaller() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-smaller-001.html",
    )
    .unwrap();
    let (dom, list) = crate::table::tests::geometria(&html, 800.0);
    for class in &[".a", ".b", ".c", ".d", ".e", ".f", ".g", ".h", ".i"] {
        if let Some(node) = dom.query(class) {
            if let Some(r) = list.rect_of(dom.resolve(node).unwrap()) {
                println!("{}: x={}, y={}, w={}, h={}", class, r.x, r.y, r.w, r.h);
            }
        }
    }
}

#[test]
fn test_grid_gap_001() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-001.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let html_ref = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-001-ref.html",
    )
    .unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);
    for class in &[".wrapper", ".a", ".b", ".c", ".d", ".h", ".i"] {
        let node = dom.query(class).expect("test node");
        let node_ref = dom_ref.query(class).expect("ref node");
        let r = list.rect_of(dom.resolve(node).unwrap()).unwrap();
        let r_ref = list_ref
            .rect_of(dom_ref.resolve(node_ref).unwrap())
            .unwrap();
        assert!(
            (r.x - r_ref.x).abs() < 0.01,
            "{}: x {} vs {}",
            class,
            r.x,
            r_ref.x
        );
        assert!(
            (r.y - r_ref.y).abs() < 0.01,
            "{}: y {} vs {}",
            class,
            r.y,
            r_ref.y
        );
        assert!(
            (r.w - r_ref.w).abs() < 0.01,
            "{}: w {} vs {}",
            class,
            r.w,
            r_ref.w
        );
        assert!(
            (r.h - r_ref.h).abs() < 0.01,
            "{}: h {} vs {}",
            class,
            r.h,
            r_ref.h
        );
    }
}

#[test]
fn test_subgrid_stretch() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-stretch.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    for (i, elem) in dom.query_all(".subgrid").iter().enumerate() {
        let node = dom.resolve(*elem).unwrap();
        let r = list.rect_of(node).unwrap();
        assert_eq!(r.w, 100.0, "subgrid {} width must be 100", i);
        assert_eq!(r.h, 100.0, "subgrid {} height must be 100", i);
    }
}

#[test]
fn test_grid_gap_007() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-007.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let html_ref = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-007-ref.html",
    )
    .unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);

    println!("--- TEST ---");
    for (i, elem) in dom.query_all(".grid, span, div").iter().enumerate() {
        let node = dom.resolve(*elem).unwrap();
        if let Some(r) = list.rect_of(node) {
            println!("test item {}: {:?} -> {:?}", i, dom.node(node).kind, r);
        }
    }
    println!("--- REF ---");
    for (i, elem) in dom_ref.query_all(".grid, span, div").iter().enumerate() {
        let node = dom_ref.resolve(*elem).unwrap();
        if let Some(r) = list_ref.rect_of(node) {
            println!("ref item {}: {:?} -> {:?}", i, dom_ref.node(node).kind, r);
        }
    }
}

#[test]
fn test_subgrid_item_block_size_001() {
    let html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-item-block-size-001.html").unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let html_ref = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-item-block-size-001-ref.html").unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);

    let test_rects: Vec<_> = dom
        .query_all(".card, item, h2, .inner, footer")
        .into_iter()
        .map(|e| list.rect_of(dom.resolve(e).unwrap()).unwrap())
        .collect();
    assert_eq!(test_rects.len(), 11);
    assert_eq!(
        test_rects[0],
        crate::paint::Rect {
            x: 0.0,
            y: 0.0,
            w: 173.33333,
            h: 276.0
        }
    );
    assert_eq!(
        test_rects[1],
        crate::paint::Rect {
            x: 0.0,
            y: 0.0,
            w: 173.33333,
            h: 28.0
        }
    );
    assert_eq!(
        test_rects[2],
        crate::paint::Rect {
            x: 0.0,
            y: 68.0,
            w: 173.33333,
            h: 132.0
        }
    );
    assert_eq!(
        test_rects[3],
        crate::paint::Rect {
            x: 0.0,
            y: 240.0,
            w: 173.33333,
            h: 36.0
        }
    );
    assert_eq!(
        test_rects[4],
        crate::paint::Rect {
            x: 213.33333,
            y: 0.0,
            w: 173.33333,
            h: 28.0
        }
    );
    assert_eq!(
        test_rects[5],
        crate::paint::Rect {
            x: 213.33333,
            y: 68.0,
            w: 173.33333,
            h: 132.0
        }
    );
    assert_eq!(
        test_rects[6],
        crate::paint::Rect {
            x: 213.33333,
            y: 240.0,
            w: 173.33333,
            h: 36.0
        }
    );
    assert_eq!(
        test_rects[7],
        crate::paint::Rect {
            x: 426.66666,
            y: 0.0,
            w: 173.33333,
            h: 276.0
        }
    );
    assert_eq!(
        test_rects[8],
        crate::paint::Rect {
            x: 426.66666,
            y: 0.0,
            w: 173.33333,
            h: 28.0
        }
    );
    assert_eq!(
        test_rects[9],
        crate::paint::Rect {
            x: 426.66666,
            y: 68.0,
            w: 173.33333,
            h: 132.0
        }
    );
    assert_eq!(
        test_rects[10],
        crate::paint::Rect {
            x: 426.66666,
            y: 240.0,
            w: 173.33333,
            h: 36.0
        }
    );
}


