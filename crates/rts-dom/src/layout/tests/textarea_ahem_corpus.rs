//! `<textarea>`'s own `DisplayItem::Text` (`layout/input.rs`) hardcoded
//! `is_ahem: false`, regardless of the computed `font-family` — found by the
//! 2026-09-25 `white-space` triage (cause 2). That masked the text out of
//! every reftest comparison that puts Ahem on a `<textarea>` (the raster
//! backend paints Ahem glyphs as solid boxes and everything else as a mask),
//! so ~20 `css-text/white-space` `textarea-*` reftests compared nothing where
//! they should compare text.
//!
//! Pins the fix: the textarea's `Text` item follows the SAME predicate as
//! every other emitter (`fonte_metricas::usa_ahem`, driven by the node's own
//! `font_family`), not a hardcoded value.

use super::*;
use crate::table::tests::{geometria, rect};

/// `font-family: Ahem` on the textarea itself must flip `is_ahem` to `true`
/// on the emitted `Text` item — the bug's exact repro.
#[test]
fn textarea_with_font_family_ahem_paints_text_as_ahem() {
    // Content, not `value=`: HTML §4.10.11 gives a `<textarea>` no such
    // attribute — the default value's source is the tag's CHILD text (see
    // `Dom::input_value`/`textarea_raw_value` in `dom/formulario.rs`). This
    // test used `value="abc"` before that branch existed, which passed by
    // accident — no such attribute is read on a real `<textarea>`.
    let list = layout(
        r#"<textarea style="font-family: Ahem">abc</textarea>"#,
        400.0,
    );
    let is_ahem = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, is_ahem, .. } if text.as_ref() == "abc" => Some(*is_ahem),
            _ => None,
        })
        .expect("expected a DisplayItem::Text with the textarea's value");
    assert!(is_ahem, "a textarea with font-family: Ahem should paint as Ahem");
}

/// With no Ahem in the computed family, the bit stays `false` — the same
/// question answered in the other direction, so a `true` is not always fixed.
#[test]
fn textarea_without_ahem_stays_masked() {
    let list = layout(
        r#"<textarea style="font-family: Arial">abc</textarea>"#,
        400.0,
    );
    let is_ahem = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, is_ahem, .. } if text.as_ref() == "abc" => Some(*is_ahem),
            _ => None,
        })
        .expect("expected a DisplayItem::Text with the textarea's value");
    assert!(!is_ahem, "a textarea without Ahem in the family should not paint as Ahem");
}

/// A line break right after `<textarea>` is a markup convention (aligning
/// HTML indentation) and is not part of the value — the same rule the HTML
/// Standard gives the parser. SUBSEQUENT breaks stay: the text has two real
/// lines and the value must preserve the second break.
#[test]
fn textarea_strips_only_the_leading_line_break() {
    let list = layout("<textarea>\nline1\nline2</textarea>", 400.0);
    let text = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .expect("expected a DisplayItem::Text with the textarea's content");
    assert_eq!(text, "line1\nline2");
}

/// There is no `value` attribute on a real `<textarea>`, but if an author
/// writes one anyway, the CONTENT still wins — the spec gives that attribute
/// no role in the default value. What wins over both is ALWAYS a typed value
/// (`input_values`), which neither test above went through since neither has
/// focus or typing.
#[test]
fn textarea_with_a_spurious_value_attribute_still_reads_the_content() {
    let list = layout(r#"<textarea value="ignored">abc</textarea>"#, 400.0);
    let found_content = list.materialized().iter().any(|it| {
        matches!(it, DisplayItem::Text { text, .. } if text.as_ref() == "abc")
    });
    let found_attribute = list.materialized().iter().any(|it| {
        matches!(it, DisplayItem::Text { text, .. } if text.as_ref() == "ignored")
    });
    assert!(found_content, "the textarea's CONTENT should win, even with a spurious value=");
    assert!(!found_attribute, "the value= attribute should not be read on a textarea");
}

/// Cause 3 of the same triage, and a fourth found while pinning it.
///
/// 3: `medida_do_input` resolved a declared `width: Nch` with the generic
/// `Dimension::resolve` (`MONO_ADVANCE` = 0.5498 em/ch, calibrated against a
/// real monospace font), not `resolve_family` — the one that answers
/// `1ch = 1em` for Ahem, by construction of the font (every glyph, including
/// the "0" that defines `ch`, advances exactly 1em).
///
/// 4: `medida_do_input` reserved `css.border_width` unconditionally, never
/// checking `css.border_style` — so a `border: none` override (explicit
/// author CSS, not just an unset property) still reserved the UA sheet's 2px
/// per side (`input:not(...), textarea { border-width: 2px; }` in
/// `style/ua.css`). Painting already got this right
/// (`style::borders::resolved_sides` zeroes the USED width whenever the
/// style is not visible, CSS2.1 §border-width); this scalar reservation for
/// LAYOUT did not.
///
/// WPT's `css-text/white-space/textarea-pre-wrap-001..007` all declare
/// `font-family: Ahem; font-size: 20px; width: 4ch` with `padding: 0;
/// border: none`, so the ruler is the CSS itself: 4 * 20px = 80px, exactly —
/// no frame to add, since padding and border are both explicitly zeroed.
/// Before this fix the same markup measured 4 * 20 * 0.5498 + 2 * 2 =~ 48px
/// (cause 3 alone) — the "~50px" the task described.
#[test]
fn textarea_with_width_in_ch_and_ahem_family_uses_1ch_1em() {
    const HTML: &str = r#"<textarea id="ta" style="
        font-family: Ahem; font-size: 20px; width: 4ch;
        margin: 0; padding: 0; border: none;
    ">XX    XX</textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    assert!(
        (r.w - 80.0).abs() < 0.5,
        "the width of a <textarea> with width:4ch in Ahem should be 80 (4 * 20px * 1em/ch), was {}",
        r.w
    );
}

/// Cause 4 in isolation: a plain `width: 100px` (no `ch`, no Ahem) must not
/// carry the UA's default 2px border reservation once `border: none`
/// overrides it — pins the fix on its own axis, independent of cause 3.
#[test]
fn textarea_with_border_none_does_not_reserve_the_ua_border() {
    const HTML: &str =
        r#"<textarea id="ta" style="width: 100px; margin: 0; padding: 0; border: none;"></textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    assert!(
        (r.w - 100.0).abs() < 0.5,
        "the width with border:none should not reserve the UA's default border (2px per side): was {}",
        r.w
    );
}

/// The same markup without Ahem takes `ch` from THAT family's own `0`
/// advance (the `ch` lot, CSS Values 4 §6.1.1) — the other direction of the
/// same question, so the fix does not hardcode `1ch = 1em` for every font.
#[test]
fn textarea_with_width_in_ch_without_ahem_uses_the_familys_zero_advance() {
    const HTML: &str = r#"<textarea id="ta" style="
        font-family: Arial; font-size: 20px; width: 4ch;
        margin: 0; padding: 0; border: none;
    ">XX    XX</textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    let expected = 4.0 * 20.0 * crate::layout::measure::font_metrics::FontMetricsModel::ch_advance_em(Some("Arial"));
    assert!(
        (r.w - expected).abs() < 0.5,
        "the width of a <textarea> with width:4ch in Arial should be 4 times Arial's 0 advance (~{}), was {}",
        expected,
        r.w
    );
}
