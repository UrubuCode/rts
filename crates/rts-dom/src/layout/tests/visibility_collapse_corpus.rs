//! As três fixtures do lote `visibility-collapse` (`tests/css/`), pinadas
//! aqui contra o Chrome — cópia EXACTA de cada `.html` (sem o comentário
//! descritivo do topo), porque este teste corre sobre `rts-dom` isolado, sem
//! o corredor `examples/claude-css-runner.ts`.
//!
//! `claude-flex-visibility-collapse.esperado.json` foi medido no Edge 152 e
//! **reconfirmado num Chrome 152 real** via `chrome-devtools` (2026-09-04, a
//! mesma sessão): nas três fixtures o item `visibility:collapse` continua a
//! ocupar a largura cheia no eixo principal, a contar para o wrap e a manter
//! os dois `gap` — inclusive no reftest OFICIAL do WPT (`gap-collapse.html`),
//! medido do mesmo jeito nesse Chrome e que também não fecha nele. Por isso
//! este lote NÃO implementa CSS Flexbox §algo-visibility (o item sairia do
//! eixo principal/wrap/gap, com um "strut" de cross-size) — implementá-lo
//! divergiria da régua medida (PLAN.md §1: "a régua é o Chrome"), sem nenhuma
//! fixture a pedi-lo. `Visibility::Collapse` existe só para que
//! `getComputedStyle` responda `"collapse"` (CSS2 §11.2 — fora de tabelas o
//! valor USADO é `hidden`, mesmo efeito de layout, string diferente).

use super::*;
use crate::dom::parse_html_to_dom;
use crate::table::tests::rect;

/// Cópia exacta de `tests/css/claude-flex-visibility-collapse.html`: um item
/// `visibility:collapse` (`#a`) num flex-row de linha única com `flex:1
/// auto` — participa da distribuição de `flex-grow` como um item comum.
const FLEX_GROW: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta name="fixar-estilo-em" content="cont,a,b">
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #cont { display: flex; width: 100px; height: 40px; background: #eee; }
  #a { width: 20px; height: 40px; background: #f0f; visibility: collapse; flex: 1 auto; }
  #b { width: 20px; height: 20px; background: #0c0; flex: 1 auto; }
</style></head>
<body>
  <div id="cont"><div id="a"></div><div id="b"></div></div>
</body>
</html>"#;

/// Cópia exacta de `tests/css/claude-visibility-collapse-flex-gap.html`:
/// `#b` (o item do meio) é `visibility:collapse` num flex com `gap:16px`.
const GAP: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta name="fixar-estilo-em" content="a,c,d">
<meta name="fixar-estilo" content="visibility,gap">
<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; gap: 16px; }
  .f > div { width: 48px; height: 40px; background: #06c; }
  #b { visibility: collapse; }
</style></head>
<body>
  <div class="f"><div id="a"></div><div id="b"></div><div id="c"></div><div id="d"></div></div>
</body>
</html>"#;

/// Cópia exacta de `tests/css/claude-visibility-collapse-flex-wrap.html`:
/// `#i2` é `visibility:collapse` num contentor de 320px com `flex-wrap:wrap`
/// e quatro itens de 160px (cabem exactamente dois por linha).
const WRAP: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta name="fixar-estilo-em" content="i1,i3,i4">
<meta name="fixar-estilo" content="visibility,flex-wrap">
<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; flex-wrap: wrap; width: 320px; }
  .f > div { width: 160px; height: 40px; }
  #i2 { visibility: collapse; }
</style></head>
<body>
<div class="f"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>
</body></html>"#;

/// Um pixel de tolerância — a mesma régua do corpus CSS.
const TOL: f32 = 1.0;

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOL
}

/// Layout a 1280×800 — a mesma janela em que os três `.esperado.json` foram
/// medidos (`"viewport": [1280, 800]`).
fn geometria_800(html: &str) -> (crate::Dom, DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

#[track_caller]
fn afirma_rect(dom: &crate::Dom, list: &DisplayList, id: &str, esperado: (f32, f32, f32, f32)) {
    let r = rect(dom, list, &format!("#{id}"), 0);
    let (ex, ey, ew, eh) = esperado;
    assert!(
        perto(r.x, ex) && perto(r.y, ey) && perto(r.w, ew) && perto(r.h, eh),
        "#{id}: obtido {:?}, esperado {:?} (Chrome)",
        (r.x, r.y, r.w, r.h),
        esperado
    );
}

#[track_caller]
fn afirma_visibility(dom: &crate::Dom, id: &str, esperado: &str) {
    let node = dom.query(&format!("#{id}")).unwrap_or_else(|| panic!("sem #{id}"));
    let css = dom.computed_style(node).expect("css");
    assert_eq!(
        css.computed_value("visibility", None),
        esperado,
        "#{id}: getComputedStyle().visibility"
    );
}

/// Um item `visibility:collapse` com `flex:1 auto` continua a competir pela
/// distribuição do `flex-grow` como um item comum: `#a` e `#b` crescem os
/// dois de 20 para 50 (não `#b` sozinho para 100, que seria o resultado de
/// implementar CSS Flexbox §algo-visibility). Medido no Edge 152 e
/// reconfirmado num Chrome 152 real — o Chrome não aplica a exclusão da spec
/// aqui, e `getComputedStyle().visibility` continua a distinguir `collapse`
/// de `hidden` mesmo sem efeito de layout.
#[test]
fn item_collapsado_continua_na_distribuicao_do_flex_grow() {
    let (dom, list) = geometria_800(FLEX_GROW);
    afirma_rect(&dom, &list, "cont", (0.0, 0.0, 100.0, 40.0));
    afirma_rect(&dom, &list, "a", (0.0, 0.0, 50.0, 40.0));
    afirma_rect(&dom, &list, "b", (50.0, 0.0, 50.0, 20.0));
    afirma_visibility(&dom, "a", "collapse");
    afirma_visibility(&dom, "b", "visible");
}

/// O item colapsado (`#b`) mantém a sua largura cheia e os DOIS `gap` que o
/// rodeiam: `#c` sai a 128 (=48+16+48+16), não a 64 (48+16, que seria o
/// resultado de "o colapsado não deixa gap duplo" — o que o WPT
/// `gap-collapse.html` pede e o Chrome medido não faz).
#[test]
fn item_collapsado_mantem_os_dois_gaps() {
    let (dom, list) = geometria_800(GAP);
    afirma_rect(&dom, &list, "a", (0.0, 0.0, 48.0, 40.0));
    afirma_rect(&dom, &list, "b", (64.0, 0.0, 48.0, 40.0));
    afirma_rect(&dom, &list, "c", (128.0, 0.0, 48.0, 40.0));
    afirma_rect(&dom, &list, "d", (192.0, 0.0, 48.0, 40.0));
    afirma_visibility(&dom, "b", "collapse");
}

/// O item colapsado (`#i2`) continua a contar para o cálculo do wrap: com
/// dois itens de 160px por linha num contentor de 320px, `#i2` ocupa a vaga
/// da linha 1 e `#i3` quebra para a linha 2 — não fica ao lado de `#i1`, que
/// seria o resultado de excluí-lo do wrap.
#[test]
fn item_collapsado_conta_para_o_wrap() {
    let (dom, list) = geometria_800(WRAP);
    afirma_rect(&dom, &list, "i1", (0.0, 0.0, 160.0, 40.0));
    afirma_rect(&dom, &list, "i2", (160.0, 0.0, 160.0, 40.0));
    afirma_rect(&dom, &list, "i3", (0.0, 40.0, 160.0, 40.0));
    afirma_rect(&dom, &list, "i4", (160.0, 40.0, 160.0, 40.0));
    afirma_visibility(&dom, "i2", "collapse");
}

/// Fora de flex, `collapse` é `hidden` (CSS2 §11.2): mesmo efeito de layout
/// (nenhum — a caixa continua a ocupar espaço), só a string de
/// `getComputedStyle` muda.
#[test]
fn fora_de_flex_collapse_nao_afeta_o_layout_como_hidden() {
    let html = r#"<style>body{margin:0}div{width:50px;height:20px}</style>
<div id="a" style="visibility:hidden"></div>
<div id="b" style="visibility:collapse"></div>
<div id="c"></div>"#;
    let (dom, list) = geometria_800(html);
    afirma_rect(&dom, &list, "a", (0.0, 0.0, 50.0, 20.0));
    afirma_rect(&dom, &list, "b", (0.0, 20.0, 50.0, 20.0));
    afirma_rect(&dom, &list, "c", (0.0, 40.0, 50.0, 20.0));
    afirma_visibility(&dom, "a", "hidden");
    afirma_visibility(&dom, "b", "collapse");
    afirma_visibility(&dom, "c", "visible");
}
