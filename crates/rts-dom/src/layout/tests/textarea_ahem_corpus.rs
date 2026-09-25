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
fn textarea_com_font_family_ahem_pinta_texto_como_ahem() {
    // Conteúdo, não `value=`: HTML §4.10.11 não dá a um `<textarea>` esse
    // atributo — a fonte do valor por omissão é o texto FILHO da tag (ver
    // `Dom::input_value`/`textarea_raw_value` em `dom/formulario.rs`). Este
    // teste usava `value="abc"` antes de esse ramo existir, o que passava
    // por acidente — nenhum atributo desses é lido num `<textarea>` real.
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
        .expect("esperava um DisplayItem::Text com o valor do textarea");
    assert!(is_ahem, "textarea com font-family: Ahem devia pintar como Ahem");
}

/// Sem Ahem na família computada, o bit continua `false` — a mesma pergunta
/// respondida na outra direção, para não fixar um `true` sempre.
#[test]
fn textarea_sem_ahem_continua_mascarado() {
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
        .expect("esperava um DisplayItem::Text com o valor do textarea");
    assert!(!is_ahem, "textarea sem Ahem na familia nao devia pintar como Ahem");
}

/// Uma quebra de linha logo após `<textarea>` é uma convenção de marcação
/// (alinhar a indentação do HTML) e não faz parte do valor — a mesma regra
/// que o HTML Standard dá ao parser. Quebras SUBSEQUENTES ficam: o texto tem
/// duas linhas reais e o valor deve preservar a segunda quebra.
#[test]
fn textarea_remove_so_a_quebra_de_linha_inicial() {
    let list = layout("<textarea>\nlinha1\nlinha2</textarea>", 400.0);
    let texto = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .expect("esperava um DisplayItem::Text com o conteudo do textarea");
    assert_eq!(texto, "linha1\nlinha2");
}

/// Não há atributo `value` num `<textarea>` de verdade, mas se um autor o
/// escrever de qualquer forma, o CONTEÚDO ainda vence — a spec não dá a esse
/// atributo papel nenhum no valor por omissão. O que vence sobre os dois é
/// SEMPRE um valor digitado (`input_values`), que nenhum dos dois testes
/// acima passou por não ter foco/digitação nenhuma.
#[test]
fn textarea_com_atributo_value_espurio_ainda_le_o_conteudo() {
    let list = layout(r#"<textarea value="ignorado">abc</textarea>"#, 400.0);
    let achou_conteudo = list.materialized().iter().any(|it| {
        matches!(it, DisplayItem::Text { text, .. } if text.as_ref() == "abc")
    });
    let achou_atributo = list.materialized().iter().any(|it| {
        matches!(it, DisplayItem::Text { text, .. } if text.as_ref() == "ignorado")
    });
    assert!(achou_conteudo, "o CONTEUDO do textarea devia vencer, mesmo com um value= espurio");
    assert!(!achou_atributo, "o atributo value= nao deveria ser lido num textarea");
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
fn textarea_com_width_em_ch_e_familia_ahem_usa_1ch_1em() {
    const HTML: &str = r#"<textarea id="ta" style="
        font-family: Ahem; font-size: 20px; width: 4ch;
        margin: 0; padding: 0; border: none;
    ">XX    XX</textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    assert!(
        (r.w - 80.0).abs() < 0.5,
        "largura de <textarea> com width:4ch em Ahem devia ser 80 (4 * 20px * 1em/ch), foi {}",
        r.w
    );
}

/// Cause 4 in isolation: a plain `width: 100px` (no `ch`, no Ahem) must not
/// carry the UA's default 2px border reservation once `border: none`
/// overrides it — pins the fix on its own axis, independent of cause 3.
#[test]
fn textarea_com_border_none_nao_reserva_a_borda_da_ua() {
    const HTML: &str =
        r#"<textarea id="ta" style="width: 100px; margin: 0; padding: 0; border: none;"></textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    assert!(
        (r.w - 100.0).abs() < 0.5,
        "largura com border:none nao devia reservar a borda default da UA (2px por lado): foi {}",
        r.w
    );
}

/// The same markup without Ahem takes `ch` from THAT family's own `0`
/// advance (the `ch` lot, CSS Values 4 §6.1.1) — the other direction of the
/// same question, so the fix does not hardcode `1ch = 1em` for every font.
#[test]
fn textarea_com_width_em_ch_sem_ahem_usa_o_avanco_do_zero_da_familia() {
    const HTML: &str = r#"<textarea id="ta" style="
        font-family: Arial; font-size: 20px; width: 4ch;
        margin: 0; padding: 0; border: none;
    ">XX    XX</textarea>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#ta", 0);
    let esperado = 4.0 * 20.0 * crate::layout::fonte_metricas::FontMetricsModel::ch_advance_em(Some("Arial"));
    assert!(
        (r.w - esperado).abs() < 0.5,
        "largura de <textarea> com width:4ch em Arial devia ser 4 x o avanco do 0 da Arial (~{}), foi {}",
        esperado,
        r.w
    );
}
