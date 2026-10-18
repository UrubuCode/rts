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

/// A BLOCK child that breaks a relative inline ancestor moves by the inline's
/// offset too (WPT `position-relative-001`): a `<span top:100% left:100%>`
/// (100% of the 100×100 owning block = +100,+100) around a green block that
/// is itself `relative -100px,-100px` lands the block back at the block's
/// origin. Before the fix only the block's own offset applied, so it sat at
/// (-100,-100) from where it should be.
#[test]
fn a_block_breaking_a_relative_inline_moves_by_the_inline_offset() {
    let html = r#"<body style="margin:0"><div style="width:100px;height:100px">
  <span style="position:relative;top:100%;left:100%">
    <div id="alvo" style="width:100px;height:100px;position:relative;top:-100px;left:-100px"></div>
  </span></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.x, r.y), (0.0, 0.0));
}

/// Guard: a plain in-flow block whose parent is a block is NOT touched by the
/// inline-offset shift (the walk stops at the first block), and an OUT-OF-FLOW
/// block under a relative inline keeps taking its position from the static
/// position, not this shift (covered together with the nested-percentage test
/// above, which uses a `position:fixed` target).
#[test]
fn a_plain_block_child_is_unaffected_by_the_inline_offset_shift() {
    let html = r#"<body style="margin:0"><div style="width:200px">
  <div style="position:relative;top:30px;left:40px"><div id="alvo" style="width:20px;height:20px"></div></div>
</div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.x, r.y), (40.0, 30.0));
}

/// A relative BLOCK's percentage inset is `auto` (no shift) when the owning
/// block's height is INDEFINITE — a `min-height`-only parent is NOT definite
/// (CSS 2.1 §10.5). WPT `position-relative-006` (`top:-10000%`) and `-007`
/// (`top:calc(10px + 10%)`, the whole calc goes auto because the percentage
/// cannot resolve) both expect the green block at the parent's origin.
/// `min-height` was being treated as a definite height for ALL children, so
/// the inset resolved against it; now it only counts inside a flex/grid
/// container (Flexbox §4.1), never a plain block.
#[test]
fn relative_block_percent_inset_is_auto_under_indefinite_min_height() {
    for top in ["top:-10000%", "top:calc(10px + 10%)"] {
        let html = format!(
            "<body style=\"margin:0\"><div style=\"width:100px;min-height:100px\">\
             <div id=\"alvo\" style=\"width:100px;height:100px;position:relative;{top}\"></div></div></body>"
        );
        let (dom, list) = geometria(&html, 800.0);
        assert_eq!(rect(&dom, &list, "#alvo", 0).y, 0.0, "{top} devia ser auto (altura indefinida)");
    }
}

/// `height:%` of a child resolves to `auto` under a plain BLOCK parent whose
/// height is only a `min-height` (not definite, CSS 2.1 §10.5) — but STILL
/// resolves inside a flex container, where `min-height` is definite
/// (Flexbox §4.1, pinned separately by `flex_coluna_shrink_corpus`).
#[test]
fn child_height_percent_under_min_height_only_block_is_auto_but_flex_keeps_it() {
    let block = r#"<body style="margin:0"><div style="width:100px;min-height:100px"><div id="alvo" style="width:100px;height:50%"></div></div></body>"#;
    let (dom, list) = geometria(block, 800.0);
    assert_eq!(rect(&dom, &list, "#alvo", 0).h, 0.0, "bloco: height:% vira auto sob min-height");
    let flex = r#"<body style="margin:0"><div style="display:flex;width:100px;min-height:100px"><div id="alvo" style="width:100px;height:50%"></div></div></body>"#;
    let (dom, list) = geometria(flex, 800.0);
    assert_eq!(rect(&dom, &list, "#alvo", 0).h, 50.0, "flex: min-height continua definida (§4.1)");
}
