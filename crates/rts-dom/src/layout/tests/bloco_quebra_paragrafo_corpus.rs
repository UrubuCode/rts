//! `tests/css/claude-bloco-quebra-inline-dentro-de-paragrafo.html` contra o
//! Edge/Blink (`.esperado.json`, medido 2026-09-18) — a régua do fix de
//! "close a p element" (WHATWG §13.2.6.4.7): um `<div>` dentro de um `<span>`
//! dentro de um `<p>` fecha o `<p>` (e o `<span>`, como colateral) na
//! ABERTURA do `<div>`, e o `</p>` que sobra no fim do documento é órfão —
//! insere um `<p>` vazio, não é ignorado.
//!
//! Antes deste fix o parser só olhava o TOPO da pilha de abertos para decidir
//! se fechava um `<p>`, então via `span` (não `p`) e nunca disparava: o
//! `<div>` nascia ANINHADO, `#paragrafo` virava uma única caixa de bloco com
//! altura de várias linhas (medido ~70px) e `#bloco` ficava deslocado (y≈36)
//! em vez de abaixo do parágrafo inteiro. Com o `<div>` extraído como IRMÃO
//! do `<p>` (a árvore certa), o `<p>` volta a ser fluxo inline simples — uma
//! linha — e não depende de nenhuma quebra de caixa anónima bloco-em-inline.

use super::*;
use crate::table::tests::{geometria, rect};

/// Tolerância do corpus (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn afirma_yh(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, y: f32, h: f32) {
    let r = rect(dom, list, sel, 0);
    assert!(
        (r.y - y).abs() <= TOL && (r.h - h).abs() <= TOL,
        "{sel}: esperado y={y} h={h} (Blink), obtido y={} h={}",
        r.y,
        r.h
    );
}

fn afirma_rect(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, esperado: (f32, f32, f32, f32)) {
    let r = rect(dom, list, sel, 0);
    let got = (r.x, r.y, r.w, r.h);
    let bate = (got.0 - esperado.0).abs() <= TOL
        && (got.1 - esperado.1).abs() <= TOL
        && (got.2 - esperado.2).abs() <= TOL
        && (got.3 - esperado.3).abs() <= TOL;
    assert!(bate, "{sel}: esperado {esperado:?} (Blink/Chrome), obtido {got:?}");
}

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #quebra { background: #ff0; border: 2px solid #f00; }
  #bloco { background: #0f0; height: 30px; }
  #depois_p { background: #00f; height: 10px; }
</style></head>
<body>
  <p id="paragrafo">irmao antes <span id="quebra">antes<div id="bloco"></div>depois</span> irmao depois</p>
  <div id="depois_p"></div>
</body>
</html>"#;

#[test]
fn paragrafo_com_bloco_dentro_de_span_contra_o_blink() {
    let (dom, list) = geometria(HTML, 1280.0);
    // #paragrafo: (0,16,1280,20) — UMA linha, porque o <div> não está mais
    // dentro dele (o parser agora fecha o <p> na abertura do <div>).
    afirma_rect(&dom, &list, "#paragrafo", (0.0, 16.0, 1280.0, 20.0));
    // #bloco: (0,52,1280,30) — bloco de largura total, abaixo do parágrafo.
    afirma_rect(&dom, &list, "#bloco", (0.0, 52.0, 1280.0, 30.0));
    // #depois_p: (0,118,1280,10) — abaixo do bloco, não colado no parágrafo.
    afirma_rect(&dom, &list, "#depois_p", (0.0, 118.0, 1280.0, 10.0));
    // #quebra (span "antes"): a Blink mede (105.56,14,47.98,23). O x/w depende
    // do medidor de largura de texto monoespaçado deste motor, que já se
    // sabe divergir por sub-pixel de fonte a fonte (ver `tests/css/README.md`
    // sobre a tolerância) — aqui afirma-se só y/h, que não dependem disso.
    afirma_yh(&dom, &list, "#quebra", 14.0, 23.0);
}
