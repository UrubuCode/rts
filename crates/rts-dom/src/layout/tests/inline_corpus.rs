//! CORPUS DE REGRESSÃO: as três fixtures da Prioridade D
//! (`docs/ui/css-implementation-gaps.md` §3.4) — `vertical-align`,
//! `white-space` e a herança de `line-height` via `font` — com os rects que o
//! Chrome mediu, copiados de `tests/css/claude-*.esperado.json` (2026-08-18,
//! 1280×800, tolerância 1px). Copiados e não LIDOS: o `Cargo.toml` deste
//! crate não tem dependência nenhuma (nem parser de JSON) por desenho — ver o
//! cabeçalho de `layout.rs` — e este teste pina o NÚMERO, não relê o ficheiro
//! que já serve a suite TS (`scripts/css_fixtures.sh`).
//!
//! **Antes desta correção**, das 22 asserções abaixo, só as seguintes já
//! passavam (o resto de cada fixture estava mudo, sem afirmação nenhuma
//! antes): em `vertical-align`, `#linha`/`#topo`/`#fundo` (o `top`/`bottom`
//! da corrida de inline-blocks já acertavam, por o envelope antigo —
//! `max(alturas)` — coincidir com o novo neste caso); em `white-space`,
//! `#normal`/`#sem-quebra`; em `text-align`, todos exceto `#herdado-pai`. Os
//! outros nove (`#base`/`#meio`/`#sub`/`#super`/`#texto-topo`/`#pre`/
//! `#pre-wrap`) são os que este lote fecha.

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
    assert!(bate, "{sel}: esperado {esperado:?} (Chrome), obtido {got:?}");
}

const VERTICAL_ALIGN_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #linha { width: 600px; height: 100px; background: #eee; font-size: 20px; }
  #linha span { display: inline-block; width: 40px; background: #fcc; }
  #base { height: 20px; vertical-align: baseline; }
  #topo { height: 30px; vertical-align: top; }
  #meio { height: 40px; vertical-align: middle; }
  #fundo { height: 50px; vertical-align: bottom; }
  #texto-topo { height: 25px; vertical-align: text-top; }
  #super { height: 20px; vertical-align: super; }
  #sub { height: 20px; vertical-align: sub; }
</style></head>
<body>
  <div id="linha"><span id="base"></span><span id="topo"></span><span id="meio"></span><span id="fundo"></span><span id="texto-topo"></span><span id="super"></span><span id="sub"></span></div>
</body>
</html>"#;

/// `claude-vertical-align.html`: os sete `inline-block` de alturas diferentes
/// numa linha só, alinhados pelos sete valores baseline-family + top/bottom.
#[test]
fn vertical_align_contra_o_chrome() {
    let (dom, list) = geometria(VERTICAL_ALIGN_HTML, 1280.0);
    afirma_rect(&dom, &list, "#linha", (0.0, 0.0, 600.0, 100.0));
    afirma_rect(&dom, &list, "#base", (0.0, 14.91, 40.0, 20.0));
    afirma_rect(&dom, &list, "#topo", (40.0, 0.0, 40.0, 30.0));
    afirma_rect(&dom, &list, "#meio", (80.0, 10.0, 40.0, 40.0));
    afirma_rect(&dom, &list, "#fundo", (120.0, 0.0, 40.0, 50.0));
    afirma_rect(&dom, &list, "#texto-topo", (160.0, 16.91, 40.0, 25.0));
    afirma_rect(&dom, &list, "#super", (200.0, 7.25, 40.0, 20.0));
    afirma_rect(&dom, &list, "#sub", (240.0, 19.91, 40.0, 20.0));
}

const WHITE_SPACE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  div { width: 150px; background: #eee; margin-bottom: 5px; }
  #normal { white-space: normal; }
  #sem-quebra { white-space: nowrap; }
  #pre { white-space: pre; }
  #pre-wrap { white-space: pre-wrap; }
</style></head>
<body>
  <div id="normal">um dois tres quatro cinco seis</div>
  <div id="sem-quebra">um dois tres quatro cinco seis</div>
  <div id="pre">um dois tres
quatro cinco seis</div>
  <div id="pre-wrap">um dois tres quatro cinco seis</div>
</body>
</html>"#;

/// `claude-white-space.html`: `normal` colapsa e quebra; `nowrap` colapsa sem
/// quebrar; `pre` preserva o `\n` LITERAL como quebra forçada e não quebra
/// por largura; `pre-wrap` faz as duas coisas.
#[test]
fn white_space_contra_o_chrome() {
    let (dom, list) = geometria(WHITE_SPACE_HTML, 1280.0);
    afirma_rect(&dom, &list, "#normal", (0.0, 0.0, 150.0, 40.0));
    afirma_rect(&dom, &list, "#sem-quebra", (0.0, 45.0, 150.0, 20.0));
    afirma_rect(&dom, &list, "#pre", (0.0, 70.0, 150.0, 40.0));
    afirma_rect(&dom, &list, "#pre-wrap", (0.0, 115.0, 150.0, 40.0));
}

const TEXT_ALIGN_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  div.caixa { width: 400px; height: 40px; background: #eee; margin-bottom: 5px; }
  #esquerda { text-align: left; }
  #centro { text-align: center; }
  #direita { text-align: right; }
  #herdado-pai { text-align: right; width: 400px; background: #ddd; }
  #bloco-filho { width: 100px; height: 20px; background: #fcc; }
  span { background: #cfc; }
</style></head>
<body>
  <div class="caixa" id="esquerda"><span id="se">abc</span></div>
  <div class="caixa" id="centro"><span id="sc">abc</span></div>
  <div class="caixa" id="direita"><span id="sd">abc</span></div>
  <div id="herdado-pai"><div id="bloco-filho"></div><span id="sh">abc</span></div>
</body>
</html>"#;

/// `claude-text-align.html`: `text-align` alinha o conteúdo EM LINHA (não a
/// caixa dentro do pai), e a altura auto de `#herdado-pai` — um bloco seguido
/// de um `<span>` — soma a altura do bloco à do line-height HERDADO via
/// `font` do `body` (não ao `normal` do medidor).
#[test]
fn text_align_contra_o_chrome() {
    let (dom, list) = geometria(TEXT_ALIGN_HTML, 1280.0);
    afirma_rect(&dom, &list, "#esquerda", (0.0, 0.0, 400.0, 40.0));
    afirma_rect(&dom, &list, "#centro", (0.0, 45.0, 400.0, 40.0));
    afirma_rect(&dom, &list, "#direita", (0.0, 90.0, 400.0, 40.0));
    afirma_rect(&dom, &list, "#se", (0.0, 0.0, 26.39, 19.0));
    afirma_rect(&dom, &list, "#sc", (186.8, 45.0, 26.39, 19.0));
    afirma_rect(&dom, &list, "#sd", (373.61, 90.0, 26.39, 19.0));
    afirma_rect(&dom, &list, "#bloco-filho", (0.0, 135.0, 100.0, 20.0));
    // ⚠️ AINDA FALHA (não fechado por este lote — ver o relatório): a altura
    // de `#herdado-pai` devia somar 20 (bloco-filho) + 20 (line-height
    // herdado de `body`) = 40; o motor devolve 38 (usa o `normal` do
    // medidor, 18, em vez do declarado). Investigado sem resolver: a cascade
    // (`inherit_from`/`apply_font_shorthand`) reproduz `font-size` corretamente
    // para o MESMO elemento pela MESMA herança (`#sh` mede 26.39 de largura,
    // provando font-size:16 herdado) — a causa exata do line-height ficar
    // para trás fica por determinar sem um binário para instrumentar.
    afirma_rect(&dom, &list, "#herdado-pai", (0.0, 135.0, 400.0, 40.0));
    afirma_rect(&dom, &list, "#sh", (373.61, 155.0, 26.39, 19.0));
}
