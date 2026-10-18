//! As duas fixtures de posição do corpus CSS (`tests/css/`), pinadas aqui
//! contra os rects que o Chrome mediu — o HTML é uma cópia EXACTA do
//! ficheiro (não um resumo), porque este teste corre sobre `rts-dom` isolado,
//! sem o corredor `examples/claude-css-runner.ts` que lê o `.html`/`.esperado.json`
//! do disco.
//!
//! Antes deste lote: os TRÊS mecanismos abaixo eram os seis desvios medidos
//! em `docs/ui/css-implementation-gaps.md` §3.2 — `#relativo.{x,y}`,
//! `#esticado.{w,h}`, `#irmao-normal.y` e `#meio.y`. Nenhum id de nenhuma das
//! duas fixtures tinha um teste Rust a piná-lo antes deste ficheiro: os únicos
//! testes de `position` em `layout/tests/` eram os de `absolute`/`fixed`/
//! `float`/`clear`/hit-test/`z-index` de `posicionado.rs`, e nenhum cobria
//! `relative` nem o `stretch` do `absolute`.

use super::*;
use crate::dom::parse_html_to_dom;
use crate::table::tests::rect;

/// Cópia exacta de `tests/css/claude-position-relative.html`.
const RELATIVO: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta name="fixar-estilo-em" content="relativo">
<meta name="fixar-estilo" content="position">
<style>
  body { margin: 0; }
  div { width: 100px; height: 40px; }
  #estatico { background: #fcc; }
  #relativo { position: relative; top: 15px; left: 30px; background: #cfc; }
  #seguinte { background: #ccf; }
</style></head>
<body>
  <div id="estatico"></div>
  <div id="relativo"></div>
  <div id="seguinte"></div>
</body>
</html>"#;

/// Cópia exacta de `tests/css/claude-position-absolute.html`.
const ABSOLUTO: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta name="fixar-estilo-em" content="ancora,canto,esticado">
<meta name="fixar-estilo" content="position">
<style>
  body { margin: 0; }
  #ancora { position: relative; width: 400px; height: 300px; margin: 50px; background: #eee; }
  #meio { width: 200px; height: 200px; margin: 20px; background: #ddd; }
  #canto { position: absolute; top: 10px; right: 10px; width: 60px; height: 30px; background: #fcc; }
  #esticado { position: absolute; top: 0; left: 0; right: 0; bottom: 0; background: #cfc; }
  #fora-do-fluxo { position: absolute; top: 5px; left: 5px; width: 500px; height: 500px; background: #ccf; }
  #irmao-normal { width: 90px; height: 25px; background: #ffc; }
</style></head>
<body>
  <div id="ancora">
    <div id="meio"><div id="canto"></div></div>
    <div id="irmao-normal"></div>
  </div>
  <div id="raiz-abs" style="position: relative; width: 200px; height: 100px; background: #efe;">
    <div id="esticado"></div>
  </div>
</body>
</html>"#;

/// Um pixel de tolerância — a mesma régua do corpus CSS
/// (`scripts/css_fixtures.sh`, tolerância default) e de `colapso.rs`.
const TOL: f32 = 1.0;

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOL
}

/// Layout a 1280×800 — a mesma janela em que `tests/css/*.esperado.json` foi
/// medido no Chrome. `geometria` (de `table::tests`) fixa a altura em 600; as
/// duas fixtures aqui não usam `vh` nem `height:100%`, então a diferença é
/// invisível no resultado — mas o número que este teste afirma medir é
/// 1280×800, e é esse que a chamada abaixo faz de facto.
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

/// Compara o rect de `#id` com `(x, y, w, h)` do Chrome, a 1px.
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

/// `position:relative` desloca a PINTURA sem tirar a caixa do fluxo: o
/// deslocamento (`top:15px;left:30px`) move `#relativo`, e `#seguinte` fica
/// exactamente onde ficaria se `#relativo` não tivesse offset nenhum — a
/// prova de que o espaço reservado no fluxo não mudou.
///
/// NOVO: `#relativo.x` e `#relativo.y` eram os dois desvios de
/// `claude-position-relative.html` no inventário (§3.2) e nenhum teste Rust os
/// pinava. `#estatico` e `#seguinte.y` já respondiam certo antes deste lote —
/// entram como GUARDA de que deslocar `#relativo` não desloca quem está à
/// volta dele.
#[test]
fn relativo_desloca_a_pintura_sem_tirar_do_fluxo() {
    let (dom, list) = geometria_800(RELATIVO);
    afirma_rect(&dom, &list, "estatico", (0.0, 0.0, 100.0, 40.0));
    afirma_rect(&dom, &list, "relativo", (30.0, 55.0, 100.0, 40.0));
    afirma_rect(&dom, &list, "seguinte", (0.0, 80.0, 100.0, 40.0));
}

/// As seis geometrias de `claude-position-absolute.html`: `absolute` mede-se
/// contra o ancestral POSITIONED mais próximo (`#canto`/`#esticado` contra
/// `#ancora`/`#raiz-abs`, não o `body`); um `absolute` com os DOIS offsets de
/// um eixo e a dimensão auto ESTICA até ao containing block (`#esticado`); e
/// a margem-top de `#meio` — primeiro filho de `#ancora`, que não tem borda
/// nem padding — COLAPSA com a margem própria de `#ancora` em vez de somar
/// (`#meio.y == #ancora.y`), o que também corrige `#irmao-normal.y`, que
/// segue do fundo de `#meio`.
///
/// NOVO: `#esticado.w`, `#esticado.h`, `#irmao-normal.y` e `#meio.y` eram os
/// quatro desvios de `claude-position-absolute.html` no inventário (§3.2) e
/// nenhum teste Rust os pinava. `#ancora`, `#canto` e `#raiz-abs` já
/// respondiam certo antes deste lote — entram como GUARDA, para que uma
/// correcção futura do stretch ou do colapso não os mexa sem que um teste
/// avise.
#[test]
fn absoluto_estica_e_o_relativo_ancestral_colapsa_com_o_primeiro_filho() {
    let (dom, list) = geometria_800(ABSOLUTO);
    afirma_rect(&dom, &list, "ancora", (50.0, 50.0, 400.0, 300.0));
    afirma_rect(&dom, &list, "meio", (70.0, 50.0, 200.0, 200.0));
    afirma_rect(&dom, &list, "canto", (380.0, 60.0, 60.0, 30.0));
    afirma_rect(&dom, &list, "esticado", (0.0, 400.0, 200.0, 100.0));
    afirma_rect(&dom, &list, "irmao-normal", (50.0, 270.0, 90.0, 25.0));
    afirma_rect(&dom, &list, "raiz-abs", (0.0, 400.0, 200.0, 100.0));
}

/// An out-of-flow box centred by a flex container's `justify-content:center`
/// must not bleed outside its containing block. A 100px-wide absolute box
/// inside a 40px inner flex (`justify-content:center`) would centre to a
/// negative x (`+(40-100)/2` off the inner flex's start); css-align-3
/// §abspos-sizing keeps a centred abspos box within the CB, so the start edge
/// is clamped to the CB start. WPT `css-position/position-absolute-center-001`.
const ABSPOS_FLEX_CENTER: &str = r#"<!DOCTYPE html>
<div id="cb" style="position:relative;width:100px;height:100px;display:flex;flex-direction:row">
  <div id="inner" style="display:flex;justify-content:center;margin:10px;height:100px;flex-grow:1">
    <div id="abs" style="position:absolute;top:0;width:100px;height:100px"></div>
  </div>
  <div style="height:100px;flex-grow:1"></div>
</div>"#;

#[test]
fn abspos_centrado_em_flex_nao_sai_do_containing_block() {
    let (dom, list) = geometria_800(ABSPOS_FLEX_CENTER);
    // O bloco-container está em x=8 (margem do body); o abs de 100px, centrado
    // no inner flex de 40px, cairia em x=-12 — fica preso à borda do CB (x=8).
    afirma_rect(&dom, &list, "abs", (8.0, 8.0, 100.0, 100.0));
}

/// A plain block flex item sized by its content takes the width of its widest
/// LINE, not its widest child: three inline `<canvas>`es side by side make the
/// item 150 wide, not 50 (one). Before, an imposed flex main height was fed
/// back through the children's ratios and the item reported one child's width
/// (WPT `css-flexbox/flexbox-flex-basis-content-004a`); the non-row transfer
/// now sums an inline run instead of taking its max.
const FLEX_COL_INLINE_CANVASES: &str = r#"<!DOCTYPE html>
<div style="display:flex;flex-direction:column;align-items:flex-start;height:1px">
  <div id="item"><canvas width="50" height="10"></canvas><canvas width="50" height="10"></canvas><canvas width="50" height="10"></canvas></div>
</div>"#;

#[test]
fn item_de_flex_coluna_mede_a_linha_inteira_de_canvases_inline() {
    let (dom, list) = geometria_800(FLEX_COL_INLINE_CANVASES);
    let r = rect(&dom, &list, "#item", 0);
    assert!(
        perto(r.w, 150.0),
        "#item: largura {:?}, esperado 150 (3 canvases de 50 lado a lado)",
        r.w
    );
}
