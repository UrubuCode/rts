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
