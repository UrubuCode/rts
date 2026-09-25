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
