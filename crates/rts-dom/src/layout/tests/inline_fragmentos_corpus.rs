//! CORPUS DE REGRESSÃO do lote S-inline: os rects exatos do Blink/Chrome para
//! `claude-sel-has.html` e `claude-display-basico.html`
//! (`tests/css/claude-{sel-has,display-basico}.esperado.json`), copiados e
//! não lidos — o mesmo motivo de `inline_corpus.rs`: este crate não tem
//! dependência de parser de JSON.
//!
//! As duas fixtures fixam a MESMA pergunta por dois lados: `sel-has` pina o
//! caso que FALHA hoje (um `<span>` sem `display` declarado, com `height` e
//! `background`, tem de medir 0×0 — hoje mede uma linha de largura total);
//! `display-basico` pina o caso que já PASSA e não pode regredir (um inline
//! com `display:inline` explícito e `width`/`height`/`background` ignora as
//! dimensões, mede o TEXTO, e o `inline-block` a seguir fica na mesma linha).
//! Os três remendos revertidos em `feat/dom-lote-s-texto`
//! (`8803c326`/`2bb6a680`/`b997aa85`) tinham sempre um dos dois a falhar — é
//! por isso que as duas entram no MESMO ficheiro em vez de um lote cada.

use super::*;
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
    assert!(bate, "{sel}: esperado {esperado:?} (Blink/Chrome), obtido {got:?}");
}

const SEL_HAS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; }
  div, span { height: 20px; background-color: #ffffff; }
  .card:has(.erro) { background-color: #ff0000; }
  .box:has(> .img) { background-color: #ff0000; }
  .rotulo:has(+ .obrigatorio) { background-color: #ff0000; }
</style>
</head>
<body>
  <div class="card" id="card-com-erro"><span class="erro"></span></div>
  <div class="card" id="card-sem-erro"><span></span></div>

  <div class="box" id="box-filho"><span class="img"></span></div>
  <div class="box" id="box-neto"><span><span class="img"></span></span></div>

  <span class="rotulo" id="rotulo-com"></span><span class="obrigatorio"></span>
  <span class="rotulo" id="rotulo-sem"></span><span></span>
</body>
</html>"#;

/// `claude-sel-has.html` contra o Blink (`.esperado.json`, medido
/// 2026-09-04): os quatro `<div>` (tag block, sem `display` declarado)
/// respeitam a `height`/`background` da regra `div, span`; os dois `<span>`
/// (`#rotulo-com`/`#rotulo-sem`) — mesma regra, mesma `height`, mesmo
/// `background`, SEM `display` declarado — não têm caixa nenhuma: 0×0, na
/// posição de linha onde fluiriam. É a diferença entre um `<span>` sem nada
/// dentro (nenhum átomo na linha) e um `<div>`, que é bloco por default de
/// tag e por isso nunca passa pela pergunta "é inline?".
#[test]
fn sel_has_contra_o_blink() {
    let (dom, list) = geometria(SEL_HAS_HTML, 1280.0);
    afirma_rect(&dom, &list, "#card-com-erro", (0.0, 0.0, 1280.0, 20.0));
    afirma_rect(&dom, &list, "#card-sem-erro", (0.0, 20.0, 1280.0, 20.0));
    afirma_rect(&dom, &list, "#box-filho", (0.0, 40.0, 1280.0, 20.0));
    afirma_rect(&dom, &list, "#box-neto", (0.0, 60.0, 1280.0, 20.0));
    afirma_rect(&dom, &list, "#rotulo-com", (0.0, 80.0, 0.0, 0.0));
    afirma_rect(&dom, &list, "#rotulo-sem", (0.0, 80.0, 0.0, 0.0));
}

const DISPLAY_BASICO_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #bloco { display: block; width: 100px; height: 30px; background: #fcc; }
  #em-linha { display: inline; width: 300px; height: 300px; background: #cfc; }
  #em-linha-bloco { display: inline-block; width: 120px; height: 40px; background: #ccf; }
  #invisivel { display: none; width: 500px; height: 500px; }
  #depois-do-none { width: 80px; height: 25px; background: #ffc; }
</style></head>
<body>
  <div id="bloco"></div>
  <span id="em-linha">abc</span><span id="em-linha-bloco"></span>
  <div id="invisivel"></div>
  <div id="depois-do-none"></div>
</body>
</html>"#;

/// `claude-display-basico.html` contra o Chrome (`.esperado.json`, medido
/// 2026-08-18): `#em-linha` (`display:inline` explícito, `width`/`height`
/// declaradas) ignora as duas e mede o TEXTO (`abc`, shrink-to-fit);
/// `#em-linha-bloco` (`inline-block`) respeita `width`/`height` e fica na
/// MESMA linha, à direita de `#em-linha` (x=26.39 — o fim do texto). Este
/// teste já passava antes do lote S-inline; fica aqui para que a mesma
/// suite pine as duas metades da pergunta "é bloco?".
///
/// `#invisivel` (`display:none`) fica FORA desta asserção: este motor não
/// regista geometria nenhuma para um nó `display:none` (nem 0×0 — a chave
/// simplesmente não existe em `node_rects`), o que já é o resultado visível
/// certo (`bloco.rs::hide_display_none_nao_pinta`) e não é uma pergunta que
/// este lote — sobre caixas INLINE — responda.
#[test]
fn display_basico_contra_o_chrome() {
    let (dom, list) = geometria(DISPLAY_BASICO_HTML, 1280.0);
    afirma_rect(&dom, &list, "#bloco", (0.0, 0.0, 100.0, 30.0));
    afirma_rect(&dom, &list, "#em-linha", (0.0, 55.0, 26.39, 19.0));
    afirma_rect(&dom, &list, "#em-linha-bloco", (26.39, 30.0, 120.0, 40.0));
    afirma_rect(&dom, &list, "#depois-do-none", (0.0, 75.0, 80.0, 25.0));
}
