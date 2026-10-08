//! Lot WM-1: a vertical writing mode laid out in a rotated frame
//! (`layout/block/rotated.rs`). Every number below is derived by hand on an
//! 800 × 600 viewport, and the body's `writing-mode` reaches the root through
//! the HTML propagation (Writing Modes 4 §8), so the root is the frame: its
//! block axis starts at the viewport's right edge (`vertical-rl`) or left
//! edge (`vertical-lr`), and its inline size is the viewport's height.

use super::*;

fn rects(html: &str, ids: &[&str]) -> Vec<Rect> {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    ids.iter()
        .map(|id| {
            let n = dom.query(&format!("#{id}")).unwrap();
            bounding_rect(&dom, dom.resolve(n).unwrap(), &ctx).unwrap()
        })
        .collect()
}

const TWO_BLOCKS: &str = "<div id=a style='display:block;width:50px'></div>\
                          <div id=b style='display:block;width:50px'></div>";

/// Blocks stack from the RIGHT in `vertical-rl`: the first 50px-thick block
/// ends at the body's right edge, the second sits left of it, and each is as
/// tall as the body's 300px, which is its inline size.
#[test]
fn vertical_rl_stacks_blocks_leftwards_from_the_right_edge() {
    let html = format!(
        "<style>body{{margin:0;writing-mode:vertical-rl;height:300px}}</style><body id=body>{TWO_BLOCKS}</body>"
    );
    let r = rects(&html, &["body", "a", "b"]);
    let (body, a, b) = (r[0], r[1], r[2]);
    assert_eq!((body.x, body.y, body.w, body.h), (700.0, 0.0, 100.0, 300.0), "body {body:?}");
    assert_eq!((a.x, a.y, a.w, a.h), (body.x + body.w - 50.0, 0.0, 50.0, 300.0), "a {a:?}");
    assert_eq!((b.x, b.y, b.w, b.h), (a.x - 50.0, 0.0, 50.0, 300.0), "b {b:?}");
}

/// `vertical-lr` is the same stacking from the LEFT edge.
#[test]
fn vertical_lr_stacks_blocks_rightwards_from_the_left_edge() {
    let html = format!(
        "<style>body{{margin:0;writing-mode:vertical-lr;height:300px}}</style><body id=body>{TWO_BLOCKS}</body>"
    );
    let r = rects(&html, &["body", "a", "b"]);
    let (body, a, b) = (r[0], r[1], r[2]);
    assert_eq!((body.x, body.w, body.h), (0.0, 100.0, 300.0), "body {body:?}");
    assert_eq!((a.x, a.y, a.w, a.h), (0.0, 0.0, 50.0, 300.0), "a {a:?}");
    assert_eq!((b.x, b.y, b.w, b.h), (50.0, 0.0, 50.0, 300.0), "b {b:?}");
}

/// Ahem "XX" at 20px is a 20 × 40 column: two 1em squares stacked down the
/// line, at the right edge of the page. The run is painted sideways, pivoted
/// at the page point its own top-left lands on — the right edge, top.
#[test]
fn ahem_text_in_vertical_rl_is_a_column_one_em_wide() {
    let html = "<style>body{margin:0;writing-mode:vertical-rl}</style>\
                <div style='display:block;font:20px/1 Ahem'><span id=t>XX</span></div>";
    let r = rects(html, &["t"])[0];
    assert_eq!((r.x, r.y, r.w, r.h), (780.0, 0.0, 20.0, 40.0), "{r:?}");

    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let texts: Vec<_> = layout_document(&dom, &ctx)
        .materialized()
        .into_iter()
        .filter_map(|it| match it {
            DisplayItem::Text { x, y, text, orientation, .. } => Some((x, y, text.to_string(), orientation)),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec![(800.0, 0.0, "XX".to_string(), crate::paint::Orientation::SidewaysRl)]);
}

/// The author's physical sides land where a vertical mode puts them:
/// `margin-top` is inline-start (the box starts 10px down, and its inline
/// size loses those 10px), `margin-right` is block-start (7px in from the
/// right, where the stacking begins).
#[test]
fn physical_margins_become_inline_start_and_block_start() {
    let html = "<style>body{margin:0;writing-mode:vertical-rl}</style>\
                <div id=a style='display:block;width:50px;margin-top:10px;margin-right:7px'></div>";
    let a = rects(html, &["a"])[0];
    assert_eq!((a.x, a.y, a.w, a.h), (800.0 - 7.0 - 50.0, 10.0, 50.0, 590.0), "{a:?}");
}

/// A horizontal page never enters a frame: its display list is the one the
/// same page gives with the vertical box removed from consideration — here,
/// a `writing-mode: horizontal-tb` stated explicitly answers exactly what the
/// initial value does.
#[test]
fn a_horizontal_page_is_laid_out_as_before() {
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let plain = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{TWO_BLOCKS}"));
    let stated = parse_html_to_dom(&format!("<style>body{{margin:0;writing-mode:horizontal-tb}}</style>{TWO_BLOCKS}"));
    assert_eq!(layout_document(&plain, &ctx).materialized(), layout_document(&stated, &ctx).materialized());
}

/// Lot WM-2: an orthogonal flow (horizontal-tb inside vertical-rl).
/// The outer box has definite block size 160px (10em).
/// The inner horizontal box sizes against it and shrinks to fit.
#[test]
fn orthogonal_horizontal_inside_vertical_fixed() {
    let html = "<style>body{margin:0}</style>\
                <div id=outer style='writing-mode:vertical-rl; width:10em; border:3px solid; background:red;'>\
                  <div id=inner style='writing-mode:horizontal-tb; padding:3px; background:yellow;'>\
                    This text should be inside a box with a yellow background and a black border. There should be no red.\
                  </div>\
                </div>";
    let r = rects(html, &["outer", "inner"]);
    let (outer, inner) = (r[0], r[1]);
    assert_eq!((outer.w, outer.h), (166.0, 102.0), "outer {outer:?}");
    assert_eq!((inner.w, inner.h), (160.0, 96.0), "inner {inner:?}");
    assert_eq!((inner.x, inner.y), (outer.x + 3.0, outer.y + 3.0), "inner offset");
}

/// Lot WM-3: static position of absolute box inside vertical-rl line.
#[test]
fn vertical_rl_static_position_anchor_rotated() {
    let html = "<style>body{margin:0;writing-mode:vertical-rl;font:16px/1 Ahem}\
                .container{position:relative;width:100px;height:400px;border:3px solid;padding:2px}\
                .abs{position:absolute;display:inline;width:20px;height:20px}\
                </style>\
                <div class=container id=c>\
                  <span id=s>XX<span class=abs id=a></span></span>\
                </div>";
    let r = rects(html, &["c", "s", "a"]);
    let (c, _s, a) = (r[0], r[1], r[2]);
    // Container c has border 3 + padding 2 = 5px frame.
    // Content box starts at right edge: 800 - 5 = 795.
    // Text XX has 2 glyphs of 16px along the line (inline axis, physical Y: 5 + 32 = 37).
    // In vertical-rl, line right edge is 795, so a.x = 795 - 20 = 775.
    assert_eq!((c.x, c.y, c.w, c.h), (800.0 - 110.0, 0.0, 110.0, 410.0), "container {c:?}");
    assert_eq!((a.x, a.y, a.w, a.h), (775.0, 37.0, 20.0, 20.0), "abs {a:?}");
}
