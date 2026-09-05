//! Lote `flex-writing-mode`: os eixos lógicos do flex sob `writing-mode`
//! (`eixos_flex.rs`). As expectativas vêm dos reftests WPT
//! `flexbox-writing-mode-002` e
//! `flexbox_writing_mode_vertical_lays_out_contents_from_top_to_bottom`
//! (`css/css-flexbox`), cada um confirmado pixel a pixel contra a sua
//! referência com `claude-raster` antes deste ficheiro nascer — ver o
//! relatório do lote em `crates/rts-dom/PLAN.md` §0.

use crate::table::tests::{geometria, rect};

const HTML_ROW_VERTICAL_RL: &str = r#"<style>
  body { margin: 0; }
  .c { display: flex; flex-flow: row wrap; width: 40px; height: 30px; writing-mode: vertical-rl; direction: ltr; }
  .c > * { width: 20px; height: 15px; }
</style>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>"#;

/// `flex-direction:row` (o default) num contentor `vertical-rl`: o eixo
/// PRINCIPAL passa a ser o Y físico (a spec chama-lhe inline, e em
/// `vertical-rl` o inline é vertical top-to-bottom) — os 4 itens de 15px de
/// altura enchem os 30px de altura do contentor em DUAS linhas de 2, não
/// numa fila horizontal de 4. O eixo CRUZADO (X, físico) é o de BLOCO, que em
/// `vertical-rl` corre da DIREITA para a ESQUERDA independente de
/// `direction` — a primeira linha (i1, i2) fica encostada à borda direita
/// (x=20), a segunda (i3, i4) à esquerda (x=0). `flexbox-writing-mode-002`
/// do WPT confirma exatamente esta grelha contra o Blink.
#[test]
fn row_num_contentor_vertical_rl_corre_no_eixo_y_com_cruzado_da_direita() {
    let (dom, list) = geometria(HTML_ROW_VERTICAL_RL, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#i1"), (20.0, 0.0, 20.0, 15.0), "1a linha, topo, cruzado a direita");
    assert_eq!(r("#i2"), (20.0, 15.0, 20.0, 15.0), "1a linha, fundo");
    assert_eq!(r("#i3"), (0.0, 0.0, 20.0, 15.0), "2a linha, topo, cruzado a esquerda");
    assert_eq!(r("#i4"), (0.0, 15.0, 20.0, 15.0), "2a linha, fundo");
}

const HTML_ROW_REVERSE_WRAP: &str = r#"<style>
  body { margin: 0; }
  .c { display: flex; flex-flow: row-reverse wrap; width: 40px; height: 30px; }
  .c > * { width: 20px; height: 15px; }
</style>
<div class="c"><div id="j1"></div><div id="j2"></div><div id="j3"></div><div id="j4"></div></div>"#;

/// ACHADO do lote (não é sobre `writing-mode`, mas as fixtures da família
/// "CMYK" do WPT o expuseram): `row-reverse` invertia a lista ANTES de
/// agrupar em linhas, o que muda QUAIS itens partilham linha — j1/j2 (que
/// cabem juntos, 15+15=30) saíam separados em linhas diferentes. O
/// agrupamento é sempre pela ordem do documento (como `coluna_wrap.rs` já
/// fazia para `column-reverse`); só a ORDEM VISUAL dentro de cada linha já
/// formada inverte. `flexbox-writing-mode-001` do WPT (o baixo puro
/// `horizontal-tb`/`ltr` da família) já falhava por isto, sem nada de
/// `writing-mode` envolvido.
#[test]
fn row_reverse_com_wrap_agrupa_pela_ordem_do_documento() {
    let (dom, list) = geometria(HTML_ROW_REVERSE_WRAP, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y) };
    assert_eq!(r("#j2"), (0.0, 0.0), "linha 1 (j1,j2): j2 no main-start invertido = esquerda");
    assert_eq!(r("#j1"), (20.0, 0.0));
    assert_eq!(r("#j4"), (0.0, 15.0), "linha 2 (j3,j4): mesma inversao");
    assert_eq!(r("#j3"), (20.0, 15.0));
}

const HTML_VERTICAL_LAYS_OUT: &str = r#"<style>
  body { margin: 0; }
  .c { display: flex; flex-wrap: wrap; align-content: flex-start; writing-mode: vertical-rl; width: 200px; height: 200px; }
  .c > * { width: 100px; height: 100px; }
</style>
<div class="c"><div id="one"></div><div id="two"></div><div id="three"></div><div id="four"></div></div>"#;

/// `flexbox_writing_mode_vertical_lays_out_contents_from_top_to_bottom` do
/// WPT: uma flex ROW `vertical-rl` de 200×200 com 4 itens de 100×100 — main
/// (Y) enche 2 por linha (100+100=200), cruzado (X) corre da direita
/// (linha 1: `one`,`two`) para a esquerda (linha 2: `three`,`four`),
/// confirmado pixel a pixel contra a referência do WPT (posicionada por
/// `top`/`right` absolutos).
#[test]
fn row_vertical_rl_dispoe_de_cima_para_baixo_com_linhas_da_direita_para_a_esquerda() {
    let (dom, list) = geometria(HTML_VERTICAL_LAYS_OUT, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y) };
    assert_eq!(r("#one"), (100.0, 0.0));
    assert_eq!(r("#two"), (100.0, 100.0));
    assert_eq!(r("#three"), (0.0, 0.0));
    assert_eq!(r("#four"), (0.0, 100.0));
}
