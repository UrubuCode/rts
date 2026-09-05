//! Reftests do WPT `css/css-flexbox/` (2026-09-05) — o lote
//! `flex-min-auto-content`: `min-width`/`min-height: auto` (o mínimo
//! automático, Flexbox §4.5) num item flex, nos dois eixos.
//!
//! `min-width:auto`/`min-height:auto` resolvem para o MENOR entre a
//! "specified size suggestion" (`width`/`height`, quando `flex-basis` é
//! `auto`) e a "content size suggestion" (o min-content, ou — no eixo de
//! coluna, sem quebra de palavra — a altura natural dos filhos) — e o
//! `max-width`/`max-height` COMPUTADO entra nessa conta como um TECTO do
//! candidato de conteúdo, não só como um clamp por cima do resultado final
//! (`flexbox-min-width-auto-001` blocos 4-6, WPT: sem isto, um item com
//! `max-width` menor do que o seu conteúdo saía no CONTEÚDO, não no tecto).
//! O automático inteiro desliga (vira 0) sob `overflow` não visível em
//! QUALQUER dos dois eixos — CSS Overflow 3 promove `overflow-x:visible` a
//! `auto` sempre que `overflow-y` não é visível, e vice-versa
//! (`flexbox-min-width-auto-003`/`-004`, WPT).

use crate::table::tests::{geometria, rect};

/// `min-width:auto` (eixo de linha) com um `max-width` menor do que o
/// min-content: o automático tem de sair no TECTO (54 = 50 de `max-width` +
/// 4 de borda), não no conteúdo (84 = 80 do filho + 4) — `flexbox-min-
/// width-auto-001` blocos 4-6 (WPT).
#[test]
fn min_width_auto_e_o_menor_entre_max_width_e_o_conteudo() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; width: 1px; height: 40px; background: #eee; }
      #item { border: 2px solid purple; max-width: 50px; background: #0c0; }
      #item > div { width: 80px; height: 10px; }
    </style>
    <div class="f"><div id="item"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 54.0, 40.0), "max-width vence o conteúdo: {r:?}");
}

/// `overflow-x` NÃO visível desliga `min-width:auto` inteiro — o item
/// encolhe até ao próprio contentor (30), não ao seu min-content (84).
/// `flexbox-min-width-auto-003` (WPT).
#[test]
fn overflow_x_nao_visivel_desliga_o_automatico() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; width: 30px; height: 40px; background: #eee; }
      #item { overflow-x: hidden; border: 2px solid purple; background: #0c0; }
      #item > div { width: 80px; height: 10px; }
    </style>
    <div class="f"><div id="item"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 30.0, 40.0), "encolhe ao contentor: {r:?}");
}

/// `overflow-y` não visível força `overflow-x` a computar como não-visível
/// TAMBÉM (CSS Overflow 3 §overflow-properties) — mesmo resultado do teste
/// acima, mas via o eixo CRUZADO. `flexbox-min-width-auto-004` (WPT).
#[test]
fn overflow_y_nao_visivel_desliga_o_automatico_de_min_width() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; width: 30px; height: 40px; background: #eee; }
      #item { overflow-y: auto; border: 2px solid purple; background: #0c0; }
      #item > div { width: 80px; height: 10px; }
    </style>
    <div class="f"><div id="item"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 30.0, 40.0), "overflow-y arrasta overflow-x: {r:?}");
}

/// `min-height:auto` (eixo de coluna), sem razão de aspeto: o automático é
/// o MENOR entre `height` (a specified size suggestion, 54) e o conteúdo
/// dos filhos IGNORANDO esse `height` (84) — antes deste lote devolvia 0
/// sempre que `height` estava declarado sem razão de aspeto, e o item
/// colapsava à borda nua. `flexbox-min-height-auto-001` bloco 1 (WPT).
#[test]
fn min_height_auto_e_o_menor_entre_height_e_o_conteudo() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; flex-direction: column; height: 1px; width: 40px; background: #eee; }
      #item { border: 2px solid purple; background: #0c0; }
      #item > div { width: 10px; height: 80px; }
    </style>
    <div class="f"><div id="item" style="height: 50px;"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 40.0, 54.0), "height (54) é o menor: {r:?}");
}

/// O espelho do teste acima: com `height` MAIOR do que o conteúdo dos
/// filhos, o automático fica no CONTEÚDO (84), não no `height` declarado
/// (104) — `flexbox-min-height-auto-001` bloco 7 (WPT).
#[test]
fn min_height_auto_fica_no_conteudo_quando_height_e_maior() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; flex-direction: column; height: 1px; width: 40px; background: #eee; }
      #item { border: 2px solid purple; background: #0c0; }
      #item > div { width: 10px; height: 80px; }
    </style>
    <div class="f"><div id="item" style="height: 100px;"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 40.0, 84.0), "o conteúdo (84) é o menor: {r:?}");
}

/// `min-width: max-content` num item flex — a mesma palavra-chave que
/// `min-content` já resolvia (`limites_do_item`), agora também para
/// `max-content`, partilhada com `bloco.rs` via `intrinseco_min_max`
/// (`flex-item-content-is-min-width-max-content`, WPT — só a parte do item
/// em SI; a de um DESCENDENTE dele fica por fazer, ver o PLAN).
#[test]
fn min_width_max_content_no_proprio_item_flex() {
    const HTML: &str = r#"<style>
      body { margin: 0; }
      .f { display: flex; width: 1px; height: 40px; background: #eee; }
      #item { min-width: max-content; border: 2px solid purple; background: #0c0; }
      #item > div { width: 90px; height: 10px; }
    </style>
    <div class="f"><div id="item"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 94.0, 40.0), "min-width:max-content vence o encolhimento: {r:?}");
}
