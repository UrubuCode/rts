//! As duas fixtures do lote R que ficaram a falhar (`crates/rts-dom/PLAN.md`
//! §0, linha R): `minmax(min-content, 200px)`/`fit-content(150px)` como
//! valores de `grid-template-columns`, e `repeat(auto-fill, minmax(a,b))`. O
//! HTML é o das fixtures, byte a byte, e os rects vêm de
//! `tests/css/claude-grid-minmax-intrinseco.esperado.json` e
//! `tests/css/claude-grid-auto-fill.esperado.json` (medidos no Blink,
//! 2026-09-04); ver `tests/css/README.md`, "O Chrome é a RÉGUA".
//!
//! `claude-grid-auto-tracks-conteudo` e `claude-grid-intrinseco` (que JÁ
//! passavam antes deste lote) não são reproduzidas aqui — continuam cobertas
//! por `grid_corpus.rs`/`grid_colocacao.rs` e pelo corpus do orquestrador;
//! este ficheiro só pina os DOIS desvios que este lote fecha.

use super::*;
use crate::table::tests::{geometria, rect};

/// Tolerância da régua do corpus (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn bate(got: Rect, x: f32, y: f32, w: f32, h: f32) -> bool {
    (got.x - x).abs() < TOL && (got.y - y).abs() < TOL && (got.w - w).abs() < TOL && (got.h - h).abs() < TOL
}

const MINMAX_INTRINSECO: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #grade {
    display: grid;
    grid-template-columns: minmax(min-content, 200px) fit-content(150px);
    width: 500px;
  }
  #grade > div { background: #eee; }
</style></head>
<body>
  <div id="grade">
    <div id="col-minmax">palavracompridasemespacos</div>
    <div id="col-fit">ab</div>
  </div>
</body>
</html>"#;

/// `minmax(min-content, 200px)`: a palavra sem espaços não quebra, o
/// min-content dela (219.92px, calibrado) EXCEDE o máximo declarado (200px)
/// — e é o mínimo que vence (spec §11.1), não o máximo. Um motor que trata
/// `minmax()` como `1fr` ou como um valor fixo dá 250px (metade de 500) ou
/// 200px aqui; nenhum bate no Chrome.
#[test]
fn minmax_com_minimo_intrinseco_maior_que_o_maximo_usa_o_minimo() {
    let (dom, list) = geometria(MINMAX_INTRINSECO, 1280.0);
    let r = rect(&dom, &list, "#col-minmax", 0);
    assert!(bate(r, 0.0, 0.0, 219.92, 20.0), "#col-minmax: {r:?}");
}

/// `fit-content(150px)`: `min(150px, max-content)` — "ab" não enche os
/// 150px pedidos, fica no seu max-content (17.59px). Um motor que trata
/// `fit-content()` como um valor fixo dá 150px aqui, onde o Chrome dá 17.59.
#[test]
fn fit_content_fica_no_max_content_quando_nao_enche_o_teto() {
    let (dom, list) = geometria(MINMAX_INTRINSECO, 1280.0);
    let r = rect(&dom, &list, "#col-fit", 0);
    assert!(bate(r, 219.92, 0.0, 17.59, 20.0), "#col-fit: {r:?}");
}

#[test]
fn grade_com_minmax_intrinseco_nao_estica_alem_do_que_as_trilhas_pedem() {
    // 500px de container, mas as duas trilhas juntas só pedem 237.5 — nenhuma
    // das duas tem um máximo "auto"/flexível para comer o resto (ao contrário
    // de `auto auto`, coberto por `grid_corpus.rs`), e o sobrante fica por
    // gastar. `#grade` continua a medir os 500px declarados.
    let (dom, list) = geometria(MINMAX_INTRINSECO, 1280.0);
    let r = rect(&dom, &list, "#grade", 0);
    assert!(bate(r, 0.0, 0.0, 500.0, 20.0), "#grade: {r:?}");
}

const AUTO_FILL: &str = r#"<!DOCTYPE html>
<html>
<head><style>
  body { margin: 0; }
  #pagina { display: grid; width: 620px;
            grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
            gap: 10px;
            background: #eee; }
  #pagina div { height: 60px; background: #cfc; }
</style></head>
<body>
  <div id="pagina">
    <div>1</div><div>2</div><div>3</div><div>4</div><div>5</div>
  </div>
</body>
</html>"#;

/// `repeat(auto-fill, minmax(150px, 1fr))` num container de 620px com 10px de
/// gap: cabem 3 repetições — `(620+10)/(150+10) = 3.9375`, arredondado para
/// baixo (CSS Grid 1 §7.2.3.3) — não 4, que é o que a divisão inteira ingénua
/// (sem o `+gap` dos dois lados) dava. As 5 divs enchem 2 linhas de 60px
/// (nenhuma trilha `fr` sobra por conta de um 4º/5º item que não cabe numa
/// coluna só), logo `#pagina` mede 620×130 (120 + 1 gap de linha).
#[test]
fn auto_fill_conta_repeticoes_pelo_espaco_disponivel() {
    let (dom, list) = geometria(AUTO_FILL, 1280.0);
    let r = rect(&dom, &list, "#pagina", 0);
    assert!(bate(r, 0.0, 0.0, 620.0, 130.0), "#pagina: {r:?}");
}
