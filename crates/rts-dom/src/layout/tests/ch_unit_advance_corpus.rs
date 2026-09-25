//! `ch` (CSS Values 4 §6.1.1) is the advance of the "0" (U+0030) glyph of the
//! element's FIRST AVAILABLE FONT — the same advance the text measurer sums
//! from `fonte_avancos`/`fonte_metricas`, not `MONO_ADVANCE`'s single number
//! calibrated for Consolas alone. A `width: 4ch` box used to be 70.3744px
//! against 70.375px of the text "123 " it was meant to fit exactly, because
//! `resolve_family` multiplied by the decimal-truncated 0.5498 instead of the
//! table's own `1126/2048`; and Arial's "0" (0.556em) used to get Consolas'
//! fraction too, since `MONO_ADVANCE` was the ONLY number `Ch` ever read.

use super::*;

#[test]
fn ch_of_a_monospace_block_matches_the_table_advance_of_zero_exactly() {
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style><div id=\"t\" style=\"font-family:monospace;font-size:16px;width:4ch\">x</div>",
    );
    let t = dom.query("#t").unwrap();
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();

    // The table's own advance of "0" in Consolas, not `MONO_ADVANCE`
    // (0.5498): `crates/rts-dom/src/layout/fonte_avancos.rs` carries the raw
    // `hmtx` unit and `fonte_metricas.rs::CONSOLAS` states it as `1126/2048`.
    let zero_em = crate::layout::fonte_metricas::FontMetricsModel::ch_advance_em(Some("monospace"));
    let expected = 4.0 * 16.0 * zero_em;
    assert!(
        (rect.w - expected).abs() < 0.01,
        "w={} expected {expected} (4 × 16 × {zero_em}, not 4×16×MONO_ADVANCE={})",
        rect.w,
        4.0 * 16.0 * crate::style::MONO_ADVANCE
    );
}

/// A proportional family's `ch` is the advance of ITS "0" — Arial's, not
/// Consolas' `MONO_ADVANCE` fraction, which `resolve_family` used to answer
/// for every family alike because `Ch` never carried a font question at all.
#[test]
fn ch_of_a_proportional_block_uses_its_own_familys_zero_not_mono_advance() {
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = parse_html_to_dom(
        "<style>body{margin:0}</style><div id=\"t\" style=\"font-family:Arial;font-size:16px;width:4ch\">x</div>",
    );
    let t = dom.query("#t").unwrap();
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();

    let zero_em = crate::layout::fonte_metricas::FontMetricsModel::ch_advance_em(Some("Arial"));
    // Arial's "0" is NOT the Consolas fraction MONO_ADVANCE resolved into.
    assert!(
        (zero_em - crate::style::MONO_ADVANCE).abs() > 0.001,
        "Arial's zero_em ({zero_em}) should differ from MONO_ADVANCE ({})",
        crate::style::MONO_ADVANCE
    );
    let expected = 4.0 * 16.0 * zero_em;
    assert!(
        (rect.w - expected).abs() < 0.01,
        "w={} expected {expected} (Arial's own \"0\" advance, not MONO_ADVANCE)",
        rect.w
    );
}
