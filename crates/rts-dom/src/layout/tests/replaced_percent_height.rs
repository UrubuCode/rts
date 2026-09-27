//! `height: %` on a REPLACED element in inline flow, and the order of the
//! aspect-ratio transfer against `min-`/`max-` (CSS 2.1 §10.5, §10.4; CSS
//! Sizing 4 §5).
//!
//! Every expected number was measured in Blink (Edge 152 headless,
//! `getBoundingClientRect`, 2026-09-26) on the same markup.

use crate::table::tests::{geometria, rect};

/// A definite containing-block height is the basis: 50% of 200px is 100px.
/// The `width` attribute is a presentational `width:40px`, so the width stays
/// 40 rather than following the 2:1 ratio.
#[test]
fn percent_height_of_img_resolves_against_a_definite_block_height() {
    let html = r#"<body style="margin:0"><div style="height:200px"><img id="alvo" width="40" height="20" style="height:50%"></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (40.0, 100.0));
}

/// An `auto`-height containing block is indefinite, so the percentage
/// computes to `auto` and the image keeps its intrinsic height.
#[test]
fn percent_height_of_img_in_an_auto_height_block_is_the_intrinsic_height() {
    let html = r#"<body style="margin:0"><div><img id="alvo" width="40" height="20" style="height:50%"></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (40.0, 20.0));
}

/// With the height definite and the width `auto`, the height is clamped in
/// ITS axis first (200 → 150 by `max-height`), then transferred through the
/// 2:1 ratio (300), then clamped in the other axis (120 by `max-width`).
/// Clamping the width first and letting `max-height` rescale it afterwards
/// gave 90×150 — the order defect this pins.
#[test]
fn ratio_transfer_clamps_the_source_axis_before_the_transferred_one() {
    let html = r#"<body style="margin:0"><div style="height:200px"><canvas id="alvo" width="32" height="16" style="height:100%;max-height:150px;max-width:120px"></canvas></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (120.0, 150.0));
}
