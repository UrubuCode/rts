//! Lote `largura-intrinseca-de-controlos` (2026-09-05): `intrinsic_content_width`
//! / `replaced_inline_size` (`layout/medida.rs`) não sabiam medir um controlo
//! de formulário — só texto, tabela, flex e bloco-por-filhos — e por isso um
//! `<input>` sem `width` num contentor flex (`flex-basis:auto`, o default)
//! caía no ramo de bloco vazio: largura 0. Era o achado de
//! `flex-vertical-align-effect` (WPT), diagnosticado antes deste lote.
//!
//! Os números vêm de `tests/css/claude-controlos-tamanho-natural`, medida no
//! Edge (Blink) headless — `LARGURA_CAMPO_TEXTO`/`LARGURA_TEXTAREA`
//! (`layout/input.rs`) são essa régua. Aqui verifica-se que a mesma resposta
//! chega pelo caminho FLEX (`flex-basis:auto`) e não só pelo bloco comum, que
//! já a tinha antes deste lote (via `layout_input`, chamado diretamente).

use crate::table::tests::{geometria, rect};

#[test]
fn input_texto_solto_mede_a_largura_por_omissao_em_fluxo_normal() {
    // Regressão: confirma que `LARGURA_CAMPO_TEXTO` (169, o novo valor
    // calibrado) não partiu o caminho de bloco comum, que já chamava
    // `layout_input`/`medida_do_input` antes deste lote.
    const HTML: &str = r#"<input id="txt" type="text">"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#txt", 0);
    assert!((r.w - 177.0).abs() < 0.5, "largura por omissão do campo: {}", r.w);
}

#[test]
fn input_texto_num_flex_basis_auto_nao_encolhe_a_quase_zero() {
    // O BUG: antes deste lote, `intrinsic_content_width` não sabia medir um
    // `<input>` e a *flex base size* de `flex-basis:auto` caía a ~0 (o
    // achado original media 4×19 em vez de ~177×21).
    const HTML: &str = r#"<style>#f { display: flex; }</style>
<div id="f"><input id="txt" type="text"></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#txt", 0);
    assert!(
        (r.w - 177.0).abs() < 0.5,
        "flex-basis:auto de um <input> devia medir a largura natural (~177), não quase-zero: {}",
        r.w
    );
}

#[test]
fn checkbox_e_radio_continuam_treze_por_treze_num_flex() {
    // Regressão pedida pelo agente vizinho (lote `stretch-flex-item-*-input`):
    // este lote NÃO muda o valor que `medida_do_input`/`CAIXA_DE_MARCA`
    // devolvem para checkbox/radio — só lhes dá uma resposta em
    // `intrinsic_content_width` também, pela MESMA função.
    const HTML: &str = r#"<style>#f { display: flex; }</style>
<div id="f"><input id="c" type="checkbox"><input id="r" type="radio"></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let c = rect(&dom, &list, "#c", 0);
    let r = rect(&dom, &list, "#r", 0);
    assert!((c.w - 13.0).abs() < 0.1 && (c.h - 13.0).abs() < 0.1, "checkbox: {:?}", c);
    assert!((r.w - 13.0).abs() < 0.1 && (r.h - 13.0).abs() < 0.1, "radio: {:?}", r);
}

#[test]
fn select_sem_opcoes_mede_a_largura_da_seta_do_dropdown_num_flex() {
    // 22 é a largura mais pequena que o Chrome ainda desenha para um
    // `<select>` vazio (a seta) — medido no mesmo corpus. Só a LARGURA: a
    // altura de um `<select>` solto (fora do stretch cruzado de um flex) é
    // um mecanismo diferente (`bloco.rs` não roteia `select` por
    // `layout_input`, só `input`/`textarea` — `is_text_input_tag`,
    // `layout/pintura.rs:255`) e fica de fora deste lote — dito em
    // "o que NÃO verifiquei" no relatório.
    const HTML: &str = r#"<style>#f { display: flex; }</style>
<div id="f"><select id="s"></select></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let s = rect(&dom, &list, "#s", 0);
    assert!((s.w - 22.0).abs() < 0.5, "select vazio: {:?}", s);
}

#[test]
fn textarea_sem_cols_mede_as_duas_linhas_por_omissao_num_flex() {
    // `rows` por omissão é 2 (HTML Standard §4.10.11) — 30 = 2×15 de
    // conteúdo + 6 de frame = 36, medido no mesmo corpus.
    const HTML: &str = r#"<style>#f { display: flex; }</style>
<div id="f"><textarea id="t"></textarea></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let t = rect(&dom, &list, "#t", 0);
    assert!((t.w - 168.0).abs() < 0.5, "largura da textarea: {}", t.w);
    assert!((t.h - 36.0).abs() < 0.5, "altura da textarea (2 linhas): {}", t.h);
}
