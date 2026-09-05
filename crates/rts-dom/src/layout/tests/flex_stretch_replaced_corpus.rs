//! Lote `flex-desvios-pequenos` (WPT `css/css-flexbox`): `align-items:
//! stretch` (o default) num flex-COLUMN não esticava a LARGURA de um
//! `<input type=checkbox|radio>` sem `width` declarado — ele ficava no
//! quadrado intrínseco de 13px (`layout/input.rs::CAIXA_DE_MARCA`) em vez de
//! encher o contentor, porque `coluna.rs` só impunha `forced_outer_w` a
//! `<img>` (comentário antigo: "`<table>`/`<input>` ficam de fora" — a
//! exclusão era total, quando só o CAMPO DE TEXTO comum precisa de ficar de
//! fora).
//!
//! Os números não vêm de uma medição no Edge/Chrome: são o próprio contrato
//! CSS que os reftests `stretch-flex-item-checkbox-input`/`-radio-input` do
//! WPT verificam — a REFERÊNCIA deles é `<input style="display:block;
//! width:50px;height:50px">`, um valor exacto que não depende de
//! aproximação de fonte nem de render de browser (só de o `width`/`height`
//! declarados serem respeitados).

use super::*;
use crate::dom::parse_html_to_dom;
use crate::table::tests::rect;

/// Cópia mínima do WPT `stretch-flex-item-checkbox-input.html`/
/// `stretch-flex-item-radio-input.html`: os dois casos (row com `width:100%`,
/// column com `height:100%`) no mesmo documento, com os dois tipos de input.
const HTML: &str = r#"<!DOCTYPE html>
<meta name="fixar-estilo-em" content="row-cb,row-radio,col-cb,col-radio">
<div style="display: flex; width: 50px; height: 50px;">
  <input id="row-cb" type="checkbox" style="width: 100%; margin: 0;">
</div>
<div style="display: flex; width: 50px; height: 50px;">
  <input id="row-radio" type="radio" style="width: 100%; margin: 0;">
</div>
<div style="display: flex; flex-direction: column; width: 50px; height: 50px;">
  <input id="col-cb" type="checkbox" style="height: 100%; margin: 0;">
</div>
<div style="display: flex; flex-direction: column; width: 50px; height: 50px;">
  <input id="col-radio" type="radio" style="height: 100%; margin: 0;">
</div>"#;

fn geometria(html: &str) -> (crate::Dom, DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

/// `flex-direction: row` (o default) já esticava certo — `<input>` sempre
/// teve o `forced_outer_w` do eixo horizontal em `flex.rs` (não é o que este
/// lote mexe); fica aqui para pinar que continua certo depois da mudança em
/// `coluna.rs`.
#[test]
fn checkbox_e_radio_esticam_a_largura_num_flex_row() {
    let (dom, list) = geometria(HTML);
    let r_cb = rect(&dom, &list, "#row-cb", 0);
    let r_radio = rect(&dom, &list, "#row-radio", 0);
    assert_eq!((r_cb.w, r_cb.h), (50.0, 50.0), "checkbox row");
    assert_eq!((r_radio.w, r_radio.h), (50.0, 50.0), "radio row");
}

/// O que este lote fecha: `align-items: stretch` num flex-COLUMN também
/// estica a LARGURA de um `<input type=checkbox|radio>` sem `width`
/// declarado — antes ficava em 13px (o quadrado intrínseco).
#[test]
fn checkbox_e_radio_esticam_a_largura_num_flex_column() {
    let (dom, list) = geometria(HTML);
    let r_cb = rect(&dom, &list, "#col-cb", 0);
    let r_radio = rect(&dom, &list, "#col-radio", 0);
    assert_eq!((r_cb.w, r_cb.h), (50.0, 50.0), "checkbox column");
    assert_eq!((r_radio.w, r_radio.h), (50.0, 50.0), "radio column");
}

/// Um `<input>` de texto comum não precisa de `forced_outer_w`: já se
/// enche sozinho (`medida_do_input` cai em `avail_w` quando não é
/// checkbox/radio) — continua a NÃO entrar em `precisa_de_forced_w_no_stretch`,
/// e o resultado tem de ser o mesmo.
#[test]
fn input_de_texto_continua_a_encher_sozinho_num_flex_column() {
    let html = r#"<div style="display: flex; flex-direction: column; width: 120px; height: 50px;">
  <input id="txt" type="text" style="margin: 0;">
</div>"#;
    let (dom, list) = geometria(html);
    let r = rect(&dom, &list, "#txt", 0);
    assert_eq!(r.w, 120.0, "input de texto ainda enche a coluna sozinho");
}
