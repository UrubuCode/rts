//! CORPUS DE REGRESSÃO: as três fixtures do lote TRANSFORMAÇÕES
//! (`crates/rts-dom/PLAN.md` §5.S, grupo "transformações") — `transform-origin`,
//! `matrix()`/`skewX`/`skewY`/`scale(2 eixos)`/composição, e "transform não
//! afeta o fluxo" — com os rects que o Edge (Blink) mediu, copiados de
//! `tests/css/claude-transform-*.esperado.json` (2026-09-04, 1280×800,
//! tolerância 1px — mesmo padrão de `inline_corpus.rs`). Copiados e não LIDOS:
//! ver o cabeçalho de `layout.rs` — este crate não tem parser de JSON.
//!
//! **Antes deste lote**, nenhuma das 13 asserções abaixo passava: `transform`
//! nunca tocava `list.node_rects` (decisão antiga de `relativo.rs`, revertida
//! aqui), a matriz só sabia compor translate/scale/rotate simples em torno do
//! CENTRO, e `transform-origin` era parseado e nunca lido.

use crate::table::tests::{geometria, rect};

/// Tolerância do corpus (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn afirma_rect(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, esperado: (f32, f32, f32, f32)) {
    let r = rect(dom, list, sel, 0);
    let got = (r.x, r.y, r.w, r.h);
    let bate = (got.0 - esperado.0).abs() <= TOL
        && (got.1 - esperado.1).abs() <= TOL
        && (got.2 - esperado.2).abs() <= TOL
        && (got.3 - esperado.3).abs() <= TOL;
    assert!(bate, "{sel}: esperado {esperado:?} (Chrome), obtido {got:?}");
}

const TRANSFORM_ORIGIN_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; }
  div { position: absolute; width: 100px; height: 100px; background: #ccc; transform: rotate(90deg); }
  #centro    { top: 0px;   left: 0px;   /* transform-origin: center (default) */ }
  #topo-esq  { top: 150px; left: 0px;   transform-origin: top left; }
  #meio-base { top: 300px; left: 0px;   transform-origin: 50% 100%; }
  #fixo      { top: 450px; left: 0px;   transform-origin: 10px 10px; }
</style></head>
<body>
  <div id="centro"></div>
  <div id="topo-esq"></div>
  <div id="meio-base"></div>
  <div id="fixo"></div>
</body>
</html>"#;

/// `claude-transform-origin.html`: `rotate(90deg)` numa caixa 100×100, com
/// `transform-origin` em `center` (default), `top left`, `50% 100%` e um par
/// de comprimentos fixos — a origem desloca o PONTO em volta do qual a caixa
/// roda, e portanto a bounding box que `getBoundingClientRect` mede.
#[test]
fn transform_origin_contra_o_chrome() {
    let (dom, list) = geometria(TRANSFORM_ORIGIN_HTML, 1280.0);
    afirma_rect(&dom, &list, "#centro", (0.0, 0.0, 100.0, 100.0));
    afirma_rect(&dom, &list, "#topo-esq", (-100.0, 150.0, 100.0, 100.0));
    afirma_rect(&dom, &list, "#meio-base", (50.0, 350.0, 100.0, 100.0));
    afirma_rect(&dom, &list, "#fixo", (-80.0, 450.0, 100.0, 100.0));
}

const SKEW_MATRIX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; }
  div { position: absolute; width: 100px; height: 50px; background: #ccc; }
  #skew-x   { top: 0px;   left: 0px;   transform: skewX(30deg); }
  #skew-y   { top: 100px; left: 0px;   transform: skewY(20deg); }
  #matriz   { top: 200px; left: 0px;   transform: matrix(1, 0, 0.5, 1, 0, 0); }
  #escala   { top: 300px; left: 0px;   transform: scale(2, 0.5); }
  #composta { top: 450px; left: 0px;   transform: translate(10px, 20px) rotate(45deg); }
</style></head>
<body>
  <div id="skew-x"></div>
  <div id="skew-y"></div>
  <div id="matriz"></div>
  <div id="escala"></div>
  <div id="composta"></div>
</body>
</html>"#;

/// `claude-transform-skew-matrix.html`: `skewX`/`skewY`, `matrix()` a 6
/// valores, `scale()` com dois eixos diferentes, e a COMPOSIÇÃO
/// `translate() rotate()` numa declaração só — a lista aplica-se da direita
/// para a esquerda ao ponto (CSS Transforms 1 §7).
#[test]
fn skew_matrix_contra_o_chrome() {
    let (dom, list) = geometria(SKEW_MATRIX_HTML, 1280.0);
    afirma_rect(&dom, &list, "#skew-x", (-14.43, 0.0, 128.87, 50.0));
    afirma_rect(&dom, &list, "#skew-y", (0.0, 81.8, 100.0, 86.4));
    afirma_rect(&dom, &list, "#matriz", (-12.5, 200.0, 125.0, 50.0));
    afirma_rect(&dom, &list, "#escala", (-50.0, 312.5, 200.0, 25.0));
    afirma_rect(&dom, &list, "#composta", (6.97, 441.97, 106.07, 106.07));
}

const NAO_AFETA_FLUXO_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; }
  div { width: 100px; height: 50px; background: #ccc; }
  #antes  { background: #aaa; }
  #rodado { transform: rotate(45deg) scale(2); background: #888; }
  #depois { background: #666; }
</style></head>
<body>
  <div id="antes"></div>
  <div id="rodado"></div>
  <div id="depois"></div>
</body>
</html>"#;

/// `claude-transform-nao-afeta-fluxo.html`: `transform` não participa no
/// fluxo — o irmão seguinte fica onde ficaria se `#rodado` não tivesse
/// transform nenhuma, mesmo a bounding box DELE saindo maior que a caixa
/// original (a caixa reservada no fluxo é a de ANTES da matriz).
#[test]
fn transform_nao_afeta_fluxo_contra_o_chrome() {
    let (dom, list) = geometria(NAO_AFETA_FLUXO_HTML, 1280.0);
    afirma_rect(&dom, &list, "#antes", (0.0, 0.0, 100.0, 50.0));
    afirma_rect(&dom, &list, "#rodado", (-56.07, -31.07, 212.13, 212.13));
    afirma_rect(&dom, &list, "#depois", (0.0, 100.0, 100.0, 50.0));
}
