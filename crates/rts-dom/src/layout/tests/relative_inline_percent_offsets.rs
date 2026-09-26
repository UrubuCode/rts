//! Percentage offsets of a `position: relative` INLINE, and the out-of-flow
//! boxes inside inlines that make no line (CSS 2.1 §9.4.3, §10.1, §10.3.7,
//! §10.6.4).
//!
//! The expected numbers come from the WPT references: `position-relative-003`
//! and `nested-inline-abspos-child[-with-siblings]` all expect one 100×100
//! green square exactly over the block that holds the inlines.

use crate::table::tests::{geometria, rect};

/// Two nested relative spans — `100%` then `-100px`, on both axes — around a
/// `position: fixed` box. The percentages resolve against the 100×100 block
/// that owns the line, so the chain cancels and the fixed box sits at the
/// block's origin. Against the viewport (the old base) `top: 100%` was 600px;
/// and the spans make no line, so the fixed box had no static position at all
/// and was drawn at (0, 0).
#[test]
fn nested_relative_inline_percentages_resolve_against_the_owning_block() {
    let html = r#"<body style="margin:0"><div style="height:30px"></div>
<div style="width:100px;height:100px">
  <span style="position:relative;top:100%;left:100%">
    <span style="position:relative;top:-100px;left:-100px">
      <div id="alvo" style="width:100px;height:100px;position:fixed"></div>
    </span>
  </span>
</div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 30.0, 100.0, 100.0));
}

/// An absolutely positioned box whose containing block is a relative inline
/// with nothing else in it: the inline establishes the block where it would
/// have started, not the viewport. `top: 0; left: 0` lands on the owning
/// block's content origin.
#[test]
fn a_positioned_inline_with_no_line_is_still_the_containing_block() {
    let html = r#"<body style="margin:0"><div style="height:30px"></div>
<div><span><span></span><span><span class="p" style="position:relative">
  <div id="alvo" style="display:inline-block;position:absolute;top:0;left:0;width:100px;height:100px"></div>
</span></span></span></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.x, r.y), (0.0, 30.0));
}

/// A span with text: `top: 50%` in a 100px-high block moves it by 50, and
/// `left: 10%` of the 200px width by 20 — the block's axes, not the
/// viewport's (800×600 here, which gave 300 and 80).
#[test]
fn relative_inline_percent_top_and_left_use_the_block_height_and_width() {
    let still = r#"<body style="margin:0"><div style="width:200px;height:100px"><span id="alvo" style="position:relative">x</span></div></body>"#;
    let moved = r#"<body style="margin:0"><div style="width:200px;height:100px"><span id="alvo" style="position:relative;top:50%;left:10%">x</span></div></body>"#;
    let (dom, list) = geometria(still, 800.0);
    let a = rect(&dom, &list, "#alvo", 0);
    let (dom, list) = geometria(moved, 800.0);
    let b = rect(&dom, &list, "#alvo", 0);
    assert_eq!((b.x - a.x, b.y - a.y), (20.0, 50.0));
}

/// In an `auto`-height block the percentage `top` computes to `auto`, so the
/// span stays put — and a `bottom` beside it then applies (Blink), where the
/// old `0px` for the percentage won over it.
#[test]
fn relative_inline_percent_top_in_an_auto_height_block_is_auto() {
    let still = r#"<body style="margin:0"><div><span id="alvo" style="position:relative">x</span></div></body>"#;
    let pct = r#"<body style="margin:0"><div><span id="alvo" style="position:relative;top:50%">x</span></div></body>"#;
    let with_bottom = r#"<body style="margin:0"><div><span id="alvo" style="position:relative;top:50%;bottom:10px">x</span></div></body>"#;
    let y = |html| {
        let (dom, list) = geometria(html, 800.0);
        rect(&dom, &list, "#alvo", 0).y
    };
    let base = y(still);
    assert_eq!(y(pct) - base, 0.0);
    assert_eq!(y(with_bottom) - base, -10.0);
}
