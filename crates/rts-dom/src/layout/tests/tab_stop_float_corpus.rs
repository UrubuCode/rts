//! CSS Text 3 §4.2: a tab stop is measured from "the start edge of the line
//! box's containing block" — the block's CONTENT edge — never from where the
//! line happens to start after a float shortens it. In the shape of WPT's
//! `css/css-text/white-space/tab-stop-with-float-001.html` (web-platform-tests,
//! 3-Clause BSD licence; this markup is written fresh, not copied — see
//! `THIRD-PARTY-NOTICES.md`): a left float narrows a `white-space: pre` line,
//! and the `\t` inside it must still land on the stop counted from the
//! container's edge, not from the float's right edge.

use crate::paint::DisplayItem;
use crate::table::tests::geometria;

/// A 20px-wide left float sits at the container's left edge — not a multiple
/// of the 40px tab stop's HALF (so the rounding rule of `avanco_tab` never
/// triggers, keeping the tab an exact multiple of the 10px space and this
/// test a clean check on X ALONE). The `pre` line reads "X\tY" at 10px Ahem
/// with `tab-size: 4`, so stops fall at content-edge x = 0, 40, 80…
///
/// "X" occupies the line's first 10px (x=20..30). Measured from the content
/// edge, the tab must advance from x=30 to the next stop, x=40 — 10px of tab,
/// landing "Y" at x=40. Measured from the line's own start (the bug this
/// pins, `pos` was `cur_w` alone), the tab would advance from a LOCAL position
/// of 10px to x=40's local equivalent, landing "Y" at x=20+10+30=60.
#[test]
fn a_tab_after_a_left_float_is_measured_from_the_content_edge() {
    let html = r#"<style>
body { margin: 0; }
#f { float: left; width: 20px; height: 20px; background: #fcc; }
#test { font: 10px/1 Ahem; white-space: pre; tab-size: 4; }
</style>
<div id="f"></div><div id="test">X&#x09;Y</div>"#;
    let (_, list) = geometria(html, 400.0);
    let itens = list.materialized();
    let (x, text) = itens
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, x, .. } if text.contains('Y') => Some((*x, text.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text item holding 'Y' in {itens:?}"));
    // Every Ahem glyph — a letter or a tab's expansion, drawn as spaces — is
    // exactly `size` (10px) wide, so the chars before 'Y' give its offset.
    let chars_before_y = text.find('Y').expect("segment must contain 'Y'");
    let y_x = x + chars_before_y as f32 * 10.0;
    assert_eq!(
        y_x, 40.0,
        "the tab stop after the float's band must be counted from the content \
         edge (x=0 → stops at 0,40,80…), not from the line's own start at x=20 \
         (which would give x=60): text={text:?} item.x={x}"
    );
}
