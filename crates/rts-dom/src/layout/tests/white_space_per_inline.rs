//! `white-space` is inherited and applies to each inline box (CSS Text 3 §3),
//! not once per block: a `<span>` that declares it decides what happens to
//! ITS spaces, and the text around it keeps the container's value. The WPT
//! `css-text/white-space/*-051/052` reftests set it on a span inside a
//! normal container; these three pin the same thing on positions, which the
//! Ahem font makes exact (one character = one font-size).

use crate::table::tests::{geometria, rect};

/// Spaces inside a `pre` span survive; the ones outside it still collapse.
/// "a" + space + "  b  " (5ch) + space → the marker after it starts at 8ch.
#[test]
fn a_pre_span_keeps_its_spaces_and_only_its_spaces() {
    let html = r#"<style>div { font: 10px/1 Ahem; }</style>
<div>a   <span style="white-space:pre">  b  </span>   <span id=c>c</span></div>"#;
    let (dom, list) = geometria(html, 800.0);
    let c = rect(&dom, &list, "#c", 0);
    assert_eq!((c.x, c.y), (80.0, 0.0), "a␣ + ␣␣b␣␣ + ␣ = 8ch before c");
}

/// A `pre-wrap` span wraps at its own preserved spaces, which HANG at the
/// line end, while the text around it collapses. In 6ch, "aa b␣␣c" is 7ch:
/// the line breaks after the preserved spaces, and "c dd" starts line two.
/// Collapsed (the container's `normal`), "aa b c" fits in 6ch and only "dd"
/// would move down.
#[test]
fn a_pre_wrap_span_wraps_at_its_own_preserved_spaces() {
    let html = r#"<style>div { font: 10px/1 Ahem; width: 60px; }</style>
<div>aa    <span style="white-space:pre-wrap">b  c</span>    <span id=d>dd</span></div>"#;
    let (dom, list) = geometria(html, 800.0);
    let d = rect(&dom, &list, "#d", 0);
    assert_eq!((d.x, d.y), (20.0, 10.0), "line two is 'c dd'");
}

/// The reverse: a `normal` span inside a `<pre>` collapses its own spaces.
/// "x" + " a b " (5ch) → "y" at 6ch; kept as `pre` it would sit at 10ch.
#[test]
fn a_normal_span_inside_pre_collapses_its_own_spaces() {
    let html = r#"<style>pre { font: 10px/1 Ahem; margin: 0; }</style>
<pre>x<span style="white-space:normal">  a   b  </span><span id=y>y</span></pre>"#;
    let (dom, list) = geometria(html, 800.0);
    let y = rect(&dom, &list, "#y", 0);
    assert_eq!((y.x, y.y), (60.0, 0.0), "x + ␣a␣b␣ = 6ch before y");
}
