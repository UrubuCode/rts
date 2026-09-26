//! The intrinsic width of an inline formatting context asks EACH text run's
//! own `white-space` (CSS Text 3 §4.1, CSS Sizing 3 §4.1.1), not the
//! container's: a forced break inside a `pre` span splits the max-content
//! line, its preserved spaces keep their width, and a `normal` span inside a
//! `<pre>` collapses its own. Ahem at 10px: every glyph, space included, is
//! 10px, so each expected width is a character count.

use crate::table::tests::{geometria, rect};

fn width_of(body: &str) -> f32 {
    let html = format!(
        "<style>#t {{ position: absolute; left: 0; top: 0; font: 10px/1 Ahem; margin: 0; }}</style>{body}"
    );
    let (dom, list) = geometria(&html, 800.0);
    rect(&dom, &list, "#t", 0).w
}

#[test]
fn a_forced_break_inside_a_pre_span_splits_the_max_content_line() {
    // Lines "a B" and "C d": 30, not the single line "a B C d" (70).
    let w = width_of("<div id=t>a <span style=\"white-space:pre\">B&#10;C</span> d</div>");
    assert_eq!(w, 30.0);
}

#[test]
fn preserved_leading_spaces_of_a_pre_span_keep_their_width() {
    let w = width_of("<div id=t><span style=\"white-space:pre\">  x</span></div>");
    assert_eq!(w, 30.0);
}

#[test]
fn a_normal_span_inside_pre_collapses_its_own_spaces() {
    // "a" + " b " collapsed + "c" = "a b c".
    let w = width_of("<pre id=t>a<span style=\"white-space:normal\">   b   </span>c</pre>");
    assert_eq!(w, 50.0);
}

#[test]
fn a_space_between_two_inlines_counts_once() {
    // The space ends the first run and the second run's leading space
    // collapses into it (§4.1.1 phase I across the boundary): "ab cd".
    let w = width_of("<div id=t><span>ab </span> cd</div>");
    assert_eq!(w, 50.0);
}

#[test]
fn tab_size_of_the_span_sets_its_tab_stop() {
    // A tab at column 0 advances to the first stop, 4 spaces, then "x".
    let w = width_of("<div id=t><span style=\"white-space:pre;tab-size:4\">&#9;x</span></div>");
    assert_eq!(w, 50.0);
}

#[test]
fn min_content_of_a_pre_text_is_its_widest_forced_line() {
    // `min-width: min-content` asks the min-content walk (`table::min_content`):
    // "AB\nCDE" in `pre` never soft-wraps, so the floor is the widest line,
    // 30, not the whole text joined (60).
    let w = width_of("<div id=t style=\"width:0;min-width:min-content;white-space:pre\">AB&#10;CDE</div>");
    assert_eq!(w, 30.0);
}

#[test]
fn a_space_after_a_float_that_opens_the_line_has_no_width() {
    // WPT css/CSS2/floats/intrinsic-size-float-and-line.html: the float is not
    // line content, so the newline after it is at the line's start and phase II
    // removes it (CSS Text 3 §4.1.3): 100 + 100, not 100 + space + 100.
    let w = width_of(
        "<div id=t><div style=\"float:right;width:100px;height:20px\"></div>\n<div style=\"display:inline-block;width:100px;height:20px\"></div></div>",
    );
    assert_eq!(w, 200.0);
}

#[test]
fn leading_and_trailing_collapsible_spaces_have_no_width() {
    let w = width_of("<div id=t>\n<span>ab</span>\n</div>");
    assert_eq!(w, 20.0);
}

#[test]
fn newlines_between_floats_add_no_width() {
    // The float-based reference of css-flexbox/flexbox-flex-wrap-horiz-002.html.
    let f = "<div style=\"float:left;width:20px;height:20px\"></div>";
    let w = width_of(&format!("<div id=t>\n{f}\n{f}\n{f}\n{f}\n{f}\n</div>"));
    assert_eq!(w, 100.0);
}
