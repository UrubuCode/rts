//! `url(...)` no computed vem SEMPRE com aspas duplas — CSSOM, e medido no
//! Blink real (o corpus `claude-list-style-image` falhava exatamente nisto:
//! `url("data:…")` esperado, `url(data:…)` obtido). `list-style-image`,
//! `background-image` e `cursor` partilham `fmt_values::fmt_url`; um teste
//! por propriedade prova que os TRÊS usam a MESMA função e não três cópias
//! da mesma regra.

use super::*;

#[test]
fn list_style_image_ganha_aspas_duplas_mesmo_sem_declarar_nenhuma() {
    let s = parse_inline("list-style-image: url(bullet.png)");
    assert_eq!(s.get_property("list-style-image"), "url(\"bullet.png\")");
}

#[test]
fn list_style_image_normaliza_aspas_simples_para_duplas() {
    let s = parse_inline("list-style-image: url('bullet.png')");
    assert_eq!(s.get_property("list-style-image"), "url(\"bullet.png\")");
}

/// A fixture do corpus (`claude-list-style-image.html`) usa uma `data:` URL —
/// pina o mesmo caso com o `;`/`,` que uma URL comum não tem, que é onde um
/// corte por `,`/`;` ingênuo partiria a string ao meio.
#[test]
fn list_style_image_url_data_leva_aspas_inteira() {
    let s = parse_inline("list-style-image: url(data:image/png;base64,AAAA)");
    assert_eq!(
        s.get_property("list-style-image"),
        "url(\"data:image/png;base64,AAAA\")"
    );
}

#[test]
fn background_image_sem_gradiente_responde_url_com_aspas() {
    // Antes desta correção `background-image` sem gradiente não tinha braço
    // NENHUM em `get_property` — respondia "" mesmo com a propriedade
    // declarada. Este teste pina as duas metades: que responde, e que
    // responde com aspas.
    let s = parse_inline("background-image: url(fundo.png)");
    assert_eq!(s.get_property("background-image"), "url(\"fundo.png\")");
}

#[test]
fn cursor_com_url_leva_aspas_e_preserva_o_fallback() {
    let s = parse_inline("cursor: url(mao.png), pointer");
    assert_eq!(s.get_property("cursor"), "url(\"mao.png\"), pointer");
}

#[test]
fn cursor_sem_url_fica_como_estava() {
    let s = parse_inline("cursor: pointer");
    assert_eq!(s.get_property("cursor"), "pointer");
}

/// `mask-image` não tinha braço NENHUM em `get_property` antes desta correção
/// (achado ao procurar OUTROS sítios a serializar `url()` sem aspas, pelo
/// mesmo padrão de `bg_image`/`list_style_image`: um campo `String` cru) —
/// respondia sempre "" mesmo declarada. Pina as duas metades, como o teste
/// de `background-image` acima.
#[test]
fn mask_image_responde_url_com_aspas() {
    let s = parse_inline("mask-image: url(mascara.svg)");
    assert_eq!(s.get_property("mask-image"), "url(\"mascara.svg\")");
    let s = parse_inline("-webkit-mask-image: url(mascara.svg)");
    assert_eq!(s.get_property("-webkit-mask-image"), "url(\"mascara.svg\")");
}
