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

/// `font-family: Ahem` on the textarea itself must flip `is_ahem` to `true`
/// on the emitted `Text` item — the bug's exact repro.
#[test]
fn textarea_com_font_family_ahem_pinta_texto_como_ahem() {
    // `value=` and not a text child: `Dom::input_value` (`dom/formulario.rs`)
    // reads the attribute, never the node's text content.
    let list = layout(
        r#"<textarea value="abc" style="font-family: Ahem"></textarea>"#,
        400.0,
    );
    let is_ahem = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, is_ahem, .. } if text.as_ref() == "abc" => Some(*is_ahem),
            _ => None,
        })
        .expect("esperava um DisplayItem::Text com o valor do textarea");
    assert!(is_ahem, "textarea com font-family: Ahem devia pintar como Ahem");
}

/// Sem Ahem na família computada, o bit continua `false` — a mesma pergunta
/// respondida na outra direção, para não fixar um `true` sempre.
#[test]
fn textarea_sem_ahem_continua_mascarado() {
    let list = layout(
        r#"<textarea value="abc" style="font-family: Arial"></textarea>"#,
        400.0,
    );
    let is_ahem = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, is_ahem, .. } if text.as_ref() == "abc" => Some(*is_ahem),
            _ => None,
        })
        .expect("esperava um DisplayItem::Text com o valor do textarea");
    assert!(!is_ahem, "textarea sem Ahem na familia nao devia pintar como Ahem");
}
