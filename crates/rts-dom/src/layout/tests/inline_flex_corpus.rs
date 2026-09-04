//! `tests/css/claude-flex-inline-flex-inline-level.html`,
//! `tests/css/claude-inline-flex-outer-display.html` e
//! `tests/css/claude-inline-flex-wrap.html` contra o Blink (Edge 152,
//! 2026-09-04): `display:inline-flex` é flex por DENTRO e inline-level por
//! FORA (CSS Display Module 3 §2.3-2.4) — flui na linha do pai lado a lado
//! com irmãos, e sem `width` encolhe ao conteúdo (shrink-to-fit, Flexbox
//! §9.9) em vez de tomar a largura do bloco inteiro.
//!
//! O terceiro teste fecha o RETRABALHO: `DisplayKind::InlineFlexWrap` — o
//! primeiro corte ("`flex-wrap:wrap` num `inline-flex` não quebra") derrubava
//! `gap-006-ltr/-rtl/-lr/-rl` do WPT flexbox, que passavam ANTES do lote
//! `inline-flex` (como bloco flex com wrap) e caíram quando o outer-display
//! passou a inline sem o wrap ir atrás.

use crate::table::tests::{geometria, rect};

const INLINE_LEVEL_HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #linha { width: 400px; }
  .f { display: inline-flex; vertical-align: top; background: #eee; }
  #a > div { width: 20px; height: 24px; background: #0c0; }
  #b { width: 64px; height: 24px; background: #c0f; }
</style>
<div id="linha"><span class="f" id="a"><div></div><div></div></span><span class="f" id="b"></span></div>"#;

#[test]
fn inline_flex_sem_width_encolhe_e_flui_na_linha_do_pai() {
    let (dom, list) = geometria(INLINE_LEVEL_HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    // #linha: uma só linha, altura = a do item mais alto (24, não a
    // line-height de 20 do `font` shorthand).
    assert_eq!(r("#linha"), (0.0, 0.0, 400.0, 24.0), "#linha é uma linha só");
    // #a: SEM `width` — shrink-to-fit = soma dos dois filhos de 20px, sem
    // gap. Antes desta correção saía w:400 (a largura do bloco inteiro).
    assert_eq!(r("#a"), (0.0, 0.0, 40.0, 24.0), "inline-flex sem width encolhe ao conteúdo");
    // #b: width:64 declarada, imediatamente a seguir a #a NA MESMA linha —
    // antes saía em x:0,y:24 (linha própria, por baixo).
    assert_eq!(r("#b"), (40.0, 0.0, 64.0, 24.0), "inline-flex flui ao lado do irmão, não empilha");
}

const OUTER_DISPLAY_HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: inline-flex; width: 64px; height: 64px; background: #36c; vertical-align: top; }
</style>
<div id="a" class="f"></div><div id="b" class="f"></div><div id="c" class="f"></div>"#;

#[test]
fn inline_flex_com_width_fica_lado_a_lado_como_inline_block() {
    let (dom, list) = geometria(OUTER_DISPLAY_HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    // Três `<div>` — bloco por TAG default — com `display:inline-flex`: o
    // outer-display do CSS vence a tag, como já vencia para `inline-block`.
    // Antes saíam em y:0, y:70, y:140 (um por linha, cada um empurrando o
    // seguinte); o Blink põe os três lado a lado.
    assert_eq!(r("#a"), (0.0, 0.0, 64.0, 64.0));
    assert_eq!(r("#b"), (64.0, 0.0, 64.0, 64.0));
    assert_eq!(r("#c"), (128.0, 0.0, 64.0, 64.0));
}

const WRAP_HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #f { display: inline-flex; flex-wrap: wrap; width: 200px; gap: 10px; background: #eee; vertical-align: top; }
  #f > div { width: 80px; height: 30px; background: #c00; }
</style>
<div id="f"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#;

#[test]
fn inline_flex_com_flex_wrap_quebra_como_flex_de_bloco() {
    let (dom, list) = geometria(WRAP_HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    // #f: width:200 declarada, gap:10, 2 linhas de 30 + 1 gap = 70. Antes do
    // InlineFlexWrap saía tudo numa linha só (a/b/c a x:0/90/180, #f a
    // w:200,h:30) — era o corte que derrubou os 4 `gap-006-*` do WPT.
    assert_eq!(r("#f"), (0.0, 0.0, 200.0, 70.0), "duas linhas: 30+10+30=70");
    // #a, #b: cabem juntos (80+10+80=170 ≤ 200) — primeira linha.
    assert_eq!(r("#a"), (0.0, 0.0, 80.0, 30.0));
    assert_eq!(r("#b"), (90.0, 0.0, 80.0, 30.0));
    // #c: 90+80+10+80=260 > 200 não cabe — quebra para a segunda linha.
    assert_eq!(r("#c"), (0.0, 40.0, 80.0, 30.0), "quebra para a 2ª linha (30+10 abaixo da 1ª)");
}
