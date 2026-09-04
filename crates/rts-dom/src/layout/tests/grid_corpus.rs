//! `tests/css/claude-grid-areas.html` contra o Chrome — pina os três desvios
//! que `docs/ui/css-implementation-gaps.md` §3.3 media e que a régua deste
//! corpus (`tests/css/README.md`) exige nunca se apagar: a row do meio de
//! `grid-template-rows: 60px 1fr 40px` media ALTURA ZERO e o rodapé, colocado
//! logo a seguir, subia para y=60 em vez de y=360.
//!
//! **Antes desta correção** (`crates/rts-dom/src/layout/grid.rs`,
//! `has_explicit_row_track`): `#pagina` e `#cabecalho` já batiam com o Chrome
//! — a 1ª row é `Fixed` e a resolução de colunas nunca teve o defeito (passa
//! por `resolve_tracks`, que sempre distinguiu `fr` de `Fixed`). `#lateral` e
//! `#corpo` tinham x/y/w certos e h=0; `#rodape` tinha x/w/h certos e y=60.
//! Depois da correção os cinco batem inteiros, a 1px.
//!
//! O HTML é o da fixture, byte a byte — nunca escrito de memória — e os
//! quatro números por `id` vêm de `tests/css/claude-grid-areas.esperado.json`
//! (medido no Chrome, 1280x800, 2026-08-18); ver `tests/css/README.md`,
//! "O Chrome é a RÉGUA".

use super::*;
use crate::table::tests::{geometria, rect};

const PAGINA: &str = r#"<!DOCTYPE html>
<!-- Fixa: `grid-template-areas` — o nome põe o item na região e um nome que
     abrange duas células faz o item ocupar as duas. É o mecanismo que um motor
     sem posicionamento explícito falha em silêncio, empilhando tudo em (1,1). -->
<html>
<head><style>
  body { margin: 0; }
  #pagina { display: grid; width: 600px; height: 400px;
            grid-template-columns: 150px 1fr;
            grid-template-rows: 60px 1fr 40px;
            grid-template-areas: "cabec cabec" "lado  corpo" "rodape rodape";
            background: #eee; }
  #cabecalho { grid-area: cabec; background: #fcc; }
  #lateral { grid-area: lado; background: #cfc; }
  #corpo { grid-area: corpo; background: #ccf; }
  #rodape { grid-area: rodape; background: #ffc; }
</style></head>
<body>
  <div id="pagina">
    <div id="cabecalho"></div><div id="lateral"></div>
    <div id="corpo"></div><div id="rodape"></div>
  </div>
</body>
</html>"#;

/// Tolerância da régua do corpus (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

fn rect_de(id: &str) -> Rect {
    let (dom, list) = geometria(PAGINA, 1280.0);
    rect(&dom, &list, &format!("#{id}"), 0)
}

fn bate(got: Rect, x: f32, y: f32, w: f32, h: f32) -> bool {
    (got.x - x).abs() < TOL && (got.y - y).abs() < TOL && (got.w - w).abs() < TOL && (got.h - h).abs() < TOL
}

#[test]
fn pagina_mede_600x400_e_ja_batia_antes_da_correcao() {
    let r = rect_de("pagina");
    assert!(bate(r, 0.0, 0.0, 600.0, 400.0), "#pagina: {r:?}");
}

#[test]
fn cabecalho_ocupa_a_primeira_row_fixa_e_ja_batia_antes_da_correcao() {
    let r = rect_de("cabecalho");
    assert!(bate(r, 0.0, 0.0, 600.0, 60.0), "#cabecalho: {r:?}");
}

#[test]
fn lateral_recebe_a_altura_da_row_1fr_do_meio() {
    // Antes: x/y/w já certos (0,60,150); h vinha 0 — a row do meio não recebia
    // espaço livre nenhum.
    let r = rect_de("lateral");
    assert!(bate(r, 0.0, 60.0, 150.0, 300.0), "#lateral: {r:?}");
}

#[test]
fn corpo_recebe_a_altura_da_row_1fr_do_meio() {
    // Mesma causa do #lateral: x/y/w já certos (150,60,450); h vinha 0.
    let r = rect_de("corpo");
    assert!(bate(r, 150.0, 60.0, 450.0, 300.0), "#corpo: {r:?}");
}

#[test]
fn rodape_desce_para_depois_da_row_do_meio() {
    // Antes: x/w/h já certos (0,600,40); y vinha 60 (logo após o cabeçalho, como
    // se a row do meio não tivesse sido dimensionada).
    let r = rect_de("rodape");
    assert!(bate(r, 0.0, 360.0, 600.0, 40.0), "#rodape: {r:?}");
}
