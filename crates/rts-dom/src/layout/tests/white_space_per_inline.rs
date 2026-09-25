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

/// `white-space: nowrap` on a `<span>` glues only ITS OWN spaces — there is
/// no soft-wrap opportunity between "bb", "cc" and "dd" — while the text
/// around the span still wraps normally on the container's `normal`. Width
/// is 6ch; "bb cc dd" alone is 8ch, wider than the whole line, so per CSS2.1
/// the unbreakable run overflows its line rather than being split: "bb" and
/// "dd" must land on the SAME line even though that line is wider than the
/// container.
#[test]
fn a_nowrap_span_glues_only_its_own_spaces() {
    let html = r#"<style>div { font: 10px/1 Ahem; width: 60px; }</style>
<div>aa <span style="white-space:nowrap"><span id=bb>bb</span> cc <span id=dd>dd</span></span> ee</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let bb = rect(&dom, &list, "#bb", 0);
    let dd = rect(&dom, &list, "#dd", 0);
    assert_eq!(bb.y, dd.y, "the nowrap span's own text never breaks between its spaces");
}

/// The container-wide `nowrap` shortcut in `linha.rs` used to force an
/// INFINITE line width for the whole flow whenever the CONTAINER's
/// `white-space` was `nowrap`/`pre`, which made a `normal` span placed
/// inside a `<pre>` unable to wrap at all — the line box it needed a finite
/// width for never existed. Width is 7ch; "aaaa bb" (7ch) fits the first
/// line exactly, and the `normal` span's own trailing space is where it
/// wraps: "cc" does not fit in what is left and moves to a second line,
/// exactly as it would outside the `<pre>`.
#[test]
fn a_normal_span_inside_pre_still_wraps() {
    let html = r#"<style>pre { font: 10px/1 Ahem; width: 70px; margin: 0; }</style>
<pre>aaaa <span style="white-space:normal">bb <span id=cc>cc</span></span></pre>"#;
    let (dom, list) = geometria(html, 800.0);
    let cc = rect(&dom, &list, "#cc", 0);
    assert_eq!(cc.y, 10.0, "'cc' wraps to the second line inside the normal span");
}

/// CSS Text 3 §4.1.3 phase II: a collapsible space at the START of a line is
/// removed regardless of `white-space` — `nowrap` withholds soft-wrap
/// OPPORTUNITIES, it does not stop a space from collapsing. Regression pin
/// for the `claude-word-spacing.html` fixture (Blink-measured
/// `.esperado.json` beside it): `body{white-space:nowrap}` with two
/// `inline-block`s separated by `<br>` — the newline after `<br>` is a
/// collapsible space at the start of the second box's content and must not
/// be materialised as a glued piece (it measured 8.8px wide — one space —
/// before this fix, instead of 0).
#[test]
fn a_collapsible_space_at_line_start_still_collapses_under_nowrap() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace;white-space:nowrap}
    div{display:inline-block}</style>
    <div id=largo style="display:inline-block">um dois tres</div><br>
    <div id=apertado>um dois tres</div>"#;
    let (dom, list) = geometria(html, 2000.0);
    let apertado = rect(&dom, &list, "#apertado", 0);
    assert_eq!(apertado.x, 0.0, "the newline after <br> collapses at the line start under nowrap, got x={}", apertado.x);
}

/// The other end of the same rule, without the `<br>`: leading whitespace
/// inside a single `nowrap` element still collapses away, so the first
/// glyph sits at x=0 rather than after a glued space.
#[test]
fn leading_collapsible_space_collapses_under_nowrap_single_run() {
    let html = r#"<style>div { font: 10px/1 Ahem; white-space: nowrap; }</style>
<div>  <span id=a>a</span> b</div>"#;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "#a", 0);
    assert_eq!(a.x, 0.0, "leading collapsible space under nowrap must collapse, got x={}", a.x);
}

/// WPT `CSS2/floats/float-nowrap-4.html`: a `float` anchored mid-line inside a
/// `nowrap` SPAN, whose own leading whitespace follows a WRAPPING run's
/// trailing space ("Some " + the span's own leading "\n    " before the
/// float). CSS Text 3 §4.1.1 collapses adjacent whitespace to ONE space
/// regardless of how many runs/elements it spans; `glue_space!`'s
/// `pending_space` from "Some "'s trailing space and the span's own glued
/// leading space used to BOTH count (`glued_space_absorbs_pending`,
/// `preserved_spaces.rs`), landing the float's anchor a whole line down
/// (`before` in `float_in_line::where_it_landed` came out one space too wide
/// to fit beside "Some" in the 10ch line, so the float missed line 0 and fell
/// to line 1). With the fix, "Some" (4ch) + ONE space (1ch) + the float
/// (5ch) = 10ch fits exactly, and the float stays on line 0 — `#f`'s y is
/// the line's top, not one line-height down.
#[test]
fn float_anchored_after_a_nowrap_spans_own_leading_space_stays_on_line_zero() {
    // Pixel widths, not `ch`: "Some" is 4 Ahem glyphs (40px) + ONE collapsed
    // space (10px) + the float (50px) = 100px, exactly the line's width — it
    // fits. The double-counted bug added a second 10px space (110px), which
    // does not, so this shape distinguishes fixed from broken where a `ch`
    // width does not (the float's `ch` resolution left enough slack either
    // way in the fixture this pins).
    let html = r#"<style>div { width: 100px; font: 10px/1 Ahem; }
    .f { float: right; width: 50px; height: 50px; }</style>
<div>Some
<span style="white-space:nowrap">
<span id=f class=f></span> text that overflows my parent.
</span></div>"#;
    let (dom, list) = geometria(html, 800.0);
    let f = rect(&dom, &list, "#f", 0);
    assert_eq!(f.y, 0.0, "the float must stay on the FIRST line (beside 'Some'), got y={}", f.y);
}
