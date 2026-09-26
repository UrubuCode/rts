//! `white-space: break-spaces` against three WPT reftests of
//! `css/css-text/white-space/` (web-platform-tests, 3-Clause BSD licence; the
//! HTML below is the test's own markup, trimmed of its `<meta>`/`<link>`
//! header). Each test hides a red decoy behind a green Ahem box that must wrap
//! into an EXACT number of lines, so the height of `.test`/`#test` is the
//! whole verdict: CSS Text 3 §4.1.3 puts a soft wrap opportunity after EVERY
//! preserved space and tab, and preserved spaces under `break-spaces` take
//! room even at the end of a line.

use crate::table::tests::{geometria, rect};

/// `break-spaces-004`: a leading space in a 2ch box is a line of its own —
/// the opportunity is AFTER the space, and "XX" no longer fits beside it.
#[test]
fn leading_preserved_space_takes_its_own_line() {
    let html = r#"<style>
div { font: 20px/1 Ahem; }
.test { color: green; width: 2ch; white-space: break-spaces; word-break: break-word; }
</style>
<div class="test"> XX</div>"#;
    let (dom, list) = geometria(html, 800.0);
    assert_eq!(rect(&dom, &list, ".test", 0).h, 40.0, "' ' / 'XX': two lines");
}

/// `break-spaces-005`: 88 preserved spaces between two words in a 10ch box
/// fill ten lines — none of them collapses and each one may end a line.
#[test]
fn a_run_of_preserved_spaces_wraps_space_by_space() {
    let spaces = " ".repeat(88);
    let html = format!(
        r#"<style>
div {{ font: 10px/1 Ahem; }}
.test {{ color: green; width: 100px; white-space: break-spaces; }}
</style>
<div class="test">XXXX{spaces}XXXX</div>"#
    );
    let (dom, list) = geometria(&html, 800.0);
    assert_eq!(rect(&dom, &list, ".test", 0).h, 100.0, "4+6 / 8×10 / 2+4: ten lines");
}

/// `break-spaces-tab-003`: in a 1ch box every tab is ONE unit with an
/// opportunity after it — "X⇥" / ⇥ / ⇥ / ⇥ / ⇥ / "X" is six lines. Breaking
/// inside a tab (as if it were its expanded spaces) gives dozens.
#[test]
fn each_preserved_tab_is_one_wrap_unit() {
    let html = r#"<style>
div { font: 20px/1 Ahem; }
#test { white-space: break-spaces; color: green; width: 1ch; }
</style>
<div id=test>X&#x09;&#x09;&#x09;&#x09;&#x09;X</div>"#;
    let (dom, list) = geometria(html, 800.0);
    assert_eq!(rect(&dom, &list, "#test", 0).h, 120.0, "six lines");
}
