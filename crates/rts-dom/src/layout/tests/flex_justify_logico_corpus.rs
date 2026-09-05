//! Os fixtures do lote `flex-justify-logico` contra o Blink (Edge 152,
//! 2026-09-04): `tests/css/claude-justify-start-end-row-reverse.html`,
//! `tests/css/claude-justify-left-column-reverse.html`,
//! `tests/css/claude-flex-column-rtl-cross-start.html` e — do retrabalho —
//! `tests/css/claude-rtl-filho-transborda.html`. O quarto teste
//! (`writing-mode:vertical-rl` não é espelhado) não vem de fixture: é a
//! régua do PRÓPRIO corte (o motor não faz layout vertical), pinada contra o
//! comportamento ANTES do `direction:rtl` existir — não contra o Blink, que
//! dispõe verticalmente algo que este motor nunca dispôs.

use crate::table::tests::{geometria, rect};

const HTML_START_END: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .row { display: flex; flex-direction: row-reverse; width: 320px; height: 40px; gap: 8px; }
  .row > div { width: 80px; height: 40px; }
  .row1 { justify-content: start; }
  .row2 { justify-content: end; margin-top: 8px; }
</style>
<div class="row row1"><div id="a1"></div><div id="a2"></div><div id="a3"></div></div>
<div class="row row2"><div id="b1"></div><div id="b2"></div><div id="b3"></div></div>"#;

/// `justify-content:start`/`end` são LÓGICOS (Box Alignment §8.1): fixos no
/// lado de início/fim do eixo inline, invariantes a `row-reverse` — ao
/// contrário de `flex-start`/`flex-end`, que são espelhados. `start` embala
/// sempre à ESQUERDA (mesmo em row-reverse); `end`, à DIREITA.
#[test]
fn justify_start_end_sao_logicos_e_invariantes_a_row_reverse() {
    let (dom, list) = geometria(HTML_START_END, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y) };
    assert_eq!(r("#a3"), (0.0, 0.0), "start: a3 encostado a esquerda");
    assert_eq!(r("#a2"), (88.0, 0.0));
    assert_eq!(r("#a1"), (176.0, 0.0));
    assert_eq!(r("#b3"), (64.0, 48.0), "end: b3 encostado a direita (free=64)");
    assert_eq!(r("#b2"), (152.0, 48.0));
    assert_eq!(r("#b1"), (240.0, 48.0));
}

const HTML_LEFT_COLUMN: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .col { display: flex; width: 100px; height: 320px; float: left; }
  .col > div { width: 100px; height: 80px; }
  .col1 { flex-direction: column; justify-content: left; }
  .col2 { flex-direction: column-reverse; justify-content: left; margin-left: 8px; }
</style>
<div class="col col1"><div id="p1"></div><div id="p2"></div><div id="p3"></div></div>
<div class="col col2"><div id="q1"></div><div id="q2"></div><div id="q3"></div></div>"#;

/// `justify-content:left`/`right` numa COLUNA não têm eixo (Box Alignment
/// §5.1: sem left/right no eixo principal vertical, colapsam em "início") e
/// embalam sempre no TOPO físico — `column-reverse` só inverte a ORDEM
/// visual dos itens, nunca o lado do empacotamento.
#[test]
fn justify_left_numa_coluna_embala_sempre_no_topo() {
    let (dom, list) = geometria(HTML_LEFT_COLUMN, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y) };
    assert_eq!(r("#p1"), (0.0, 0.0), "column: ordem normal, ja batia");
    assert_eq!(r("#p2"), (0.0, 80.0));
    assert_eq!(r("#p3"), (0.0, 160.0));
    assert_eq!(r("#q3"), (108.0, 0.0), "column-reverse: ordem visual invertida, TOPO");
    assert_eq!(r("#q2"), (108.0, 80.0));
    assert_eq!(r("#q1"), (108.0, 160.0));
}

const HTML_RTL_COLUMN: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #c { display: flex; flex-direction: column; direction: rtl; width: 320px; background: #eee; }
  #item { width: 120px; height: 40px; background: #0c0; }
</style>
<div id="c"><div id="item"></div></div>"#;

/// Numa flex-column, o eixo cruzado (X) É o eixo inline (Flexbox §4.1 +
/// Writing Modes) — `direction:rtl` faz o cross-start físico ser a borda
/// DIREITA. Um item não esticado (largura declarada, 120 de 320) encosta lá.
#[test]
fn direction_rtl_encosta_o_item_de_coluna_a_direita() {
    let (dom, list) = geometria(HTML_RTL_COLUMN, 1280.0);
    let c = rect(&dom, &list, "#c", 0);
    let item = rect(&dom, &list, "#item", 0);
    assert_eq!((c.x, c.y, c.w, c.h), (0.0, 0.0, 320.0, 40.0));
    assert_eq!(
        (item.x, item.y, item.w, item.h),
        (200.0, 0.0, 120.0, 40.0),
        "cross-start em rtl = borda direita: x = 320-120 = 200"
    );
}

const HTML_RTL_VERTICAL: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #c { display: flex; flex-direction: column; direction: rtl; writing-mode: vertical-rl; width: 320px; background: #eee; }
  #item { width: 120px; height: 40px; background: #0c0; }
</style>
<div id="c"><div id="item"></div></div>"#;

/// SUPERSEDIDO pelo lote `flex-writing-mode`: o corte antigo ("o motor não
/// faz layout de `writing-mode` vertical, então desliga o espelho") não é
/// mais verdade — `eixos_flex.rs` troca de eixo físico de verdade. Este
/// `#c` é `flex-direction:column`, cujo eixo PRINCIPAL é sempre o de BLOCO
/// (Flexbox §3); em `vertical-rl` o bloco é o eixo X, e corre RTL — pelo
/// PRÓPRIO `vertical-rl` (`eixos_flex::eixo_x_forward`), não pelo
/// `direction` do contentor (que só vale para o eixo INLINE, aqui o Y: o
/// `direction:rtl` deste teste não muda nada nesta caixa). Um único item
/// com `justify-content` por omissão (`flex-start`) fica no main-START, que
/// passou a ser a borda DIREITA: `x = 320 - 120 = 200` — a MESMA caixa que
/// `direction_rtl_encosta_o_item_de_coluna_a_direita` dá para
/// `direction:rtl` sozinho (sem `writing-mode`), só que por um mecanismo
/// diferente (eixo principal invertido, não um espelho do cruzado).
#[test]
fn writing_mode_vertical_rl_inverte_o_eixo_principal_de_uma_coluna() {
    let (dom, list) = geometria(HTML_RTL_VERTICAL, 1280.0);
    let item = rect(&dom, &list, "#item", 0);
    assert_eq!(
        (item.x, item.y, item.w, item.h),
        (200.0, 0.0, 120.0, 40.0),
        "vertical-rl: o eixo de bloco (X) corre RTL, item no main-start = borda direita"
    );
}

const HTML_RTL_TRANSBORDA: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #b { direction: rtl; width: 300px; border: 2px solid #000; overflow: auto; margin-bottom: 10px; }
  #bf { width: 500px; height: 30px; background: #ccc; }
  #f { direction: rtl; width: 300px; border: 2px solid #000; overflow: auto; display: flex; flex-direction: column; }
  #ff { width: 500px; height: 30px; background: #ccc; }
</style>
<div id="b"><div id="bf">bloco</div></div>
<div id="f"><div id="ff">flex</div></div>"#;

/// RETRABALHO (`claude-rtl-filho-transborda`, espelho do `overflow-top-left`
/// do WPT): com `direction:rtl`, um filho MAIS LARGO do que o contentor
/// transborda pela ESQUERDA (a margem direita encosta à direita do
/// content-box) — tanto num bloco normal (`rtl_bloco::margin_left_usado`,
/// CSS 2.1 §10.3.3) como num flex em coluna (`coluna_rtl::cross_x` com a
/// largura VERDADEIRA do item, não grampeada ao `content_w`). Os dois têm de
/// dar a MESMA caixa — é o mesmo `.column-wrapper`/`.row-wrapper` do WPT,
/// um bloco e o outro flex.
#[test]
fn direction_rtl_transborda_pela_esquerda_no_bloco_e_na_coluna() {
    let (dom, list) = geometria(HTML_RTL_TRANSBORDA, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#b"), (0.0, 0.0, 304.0, 34.0));
    assert_eq!(r("#bf"), (-198.0, 2.0, 500.0, 30.0), "bloco: margem direita encostada, transborda a esquerda");
    assert_eq!(r("#f"), (0.0, 44.0, 304.0, 34.0));
    assert_eq!(r("#ff"), (-198.0, 46.0, 500.0, 30.0), "flex coluna: MESMA caixa do bloco");
}
