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
    (got.x - x).abs() < TOL
        && (got.y - y).abs() < TOL
        && (got.w - w).abs() < TOL
        && (got.h - h).abs() < TOL
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

#[test]
fn grid_item_overflow_auto_max_height_percentage() {
    let html = r#"<style>
.webconsole-app { display: grid; height: 200px; }
.sidebar { display: flex; flex: 1; flex-direction: row; height: 100%; background: blue; }
.controlled { display: flex; overflow: auto; }
.sidebar-wrapper { display: grid; grid-template-columns: 100px; grid-template-rows: 50px 1fr; overflow: hidden; }
.sidebar-contents { grid-row: 2 / 3; overflow: auto; max-height: 100%; }
</style>
<div class="webconsole-app" id="app">
  <div class="sidebar" id="sb">
    <div class="controlled" id="ctrl">
      <div class="sidebar-wrapper" id="wrap">
        <div></div>
        <div class="sidebar-contents" id="cont">
          <div style="height:400px; background: grey" id="grey"></div>
        </div>
      </div>
    </div>
  </div>
</div>"#;
    let p_test = "C:/Users/nexga/Documents/wpt-corpus/css/css-grid/grid-items/grid-item-overflow-auto-max-height-percentage.html";
    let p_ref = "C:/Users/nexga/Documents/wpt-corpus/css/css-grid/grid-items/grid-item-overflow-auto-max-height-percentage-ref.html";
    if let (Ok(ht), Ok(hr)) = (std::fs::read_to_string(p_test), std::fs::read_to_string(p_ref)) {
        let (dt, lt) = geometria(&ht, 1280.0);
        let (dr, lr) = geometria(&hr, 1280.0);
        let r_app_t = rect(&dt, &lt, ".webconsole-app", 0);
        let r_app_r = rect(&dr, &lr, ".webconsole-app", 0);
        assert_eq!(r_app_t.h, 200.0);
        assert_eq!(r_app_r.h, 450.0);
        for sel in [".sidebar", ".controlled", ".sidebar-wrapper", ".sidebar-contents"] {
            let rt = rect(&dt, &lt, sel, 0);
            let rr = rect(&dr, &lr, sel, 0);
            assert_eq!(rt, rr, "{sel}");
        }
    }
}

#[test]
fn grid_size_with_orthogonal_child_001() {
    let html = r#"<style>
.grid { display: inline-grid; grid-template-rows: 100px; background-color: green; }
.item { writing-mode: vertical-rl; inline-size: 100%; block-size: 50%; }
.item > span { display: inline-block; inline-size: 40px; block-size: 100px; }
</style>
<div class="grid"><div class="item"><span></span><span></span></div></div>"#;
    let (dom, list) = geometria(html, 1280.0);
    let r_grid = rect(&dom, &list, ".grid", 0);
    assert_eq!(r_grid.w, 100.0);
    assert_eq!(r_grid.h, 100.0);
}

#[test]
fn grid_size_with_orthogonal_child_002() {
    let html = r#"<style>
.grid { display: inline-grid; grid-template-rows: 100px; background-color: green; }
.item { writing-mode: vertical-lr; }
.item > span { display: inline-block; inline-size: 100px; block-size: 50px; }
</style>
<div class="grid"><div class="item"><span></span><span></span></div></div>"#;
    let (dom, list) = geometria(html, 1280.0);
    let r_grid = rect(&dom, &list, ".grid", 0);
    assert_eq!(r_grid.w, 100.0);
    assert_eq!(r_grid.h, 100.0);
}

#[test]
fn debug_percentage_sizes_001_to_003() {
    for num in ["001", "002", "003"] {
        let test_path = format!("C:/Users/nexga/Documents/wpt-corpus/css/css-grid/grid-items/grid-item-percentage-sizes-{num}.html");
        let ref_path = format!("C:/Users/nexga/Documents/wpt-corpus/css/css-grid/grid-items/grid-item-percentage-sizes-{num}-ref.html");
        let (html_test, html_ref) = match (std::fs::read_to_string(&test_path), std::fs::read_to_string(&ref_path)) {
            (Ok(t), Ok(r)) => (t, r),
            _ => continue,
        };
        let (dom_t, list_t) = geometria(&html_test, 1280.0);
        let (dom_r, list_r) = geometria(&html_ref, 1280.0);
        let items_t = dom_t.query_all(".item");
        let items_r = dom_r.query_all(".item");
        assert_eq!(items_t.len(), items_r.len());
        for i in 0..items_t.len() {
            let rt = rect(&dom_t, &list_t, ".item", i);
            let rr = rect(&dom_r, &list_r, ".item", i);
            assert_eq!(rt, rr, "mismatch in test {num} item {i}");
        }
    }
}

/// CSS Grid §11.5: under a MIN-content constraint (`width:min-content`) a
/// flexible track spanned by an item keeps its base and is NOT grown by the
/// item's min-content — the item overflows instead. A `1fr 30px` grid holding
/// one `grid-column:1/span 2` item with a 300px filler is 30px of CONTENT wide
/// (the fixed track only), not 280 (WPT `css-grid/grid-flex-spanning-items-001`).
const SPANNING_FLEX_MIN: &str = r#"<!DOCTYPE html>
<div id="mygrid" style="display:grid;grid-template-columns:1fr 30px;border:10px solid fuchsia;width:min-content">
  <div id="item" style="grid-column:1 / span 2"><div style="width:300px;height:50px;background:aqua"></div></div>
</div>"#;

#[test]
fn grid_min_content_nao_cresce_track_flexivel_com_item_spanning() {
    let (dom, list) = geometria(SPANNING_FLEX_MIN, 1280.0);
    let g = rect(&dom, &list, "#mygrid", 0);
    // Border-box: 30px de conteúdo + 2×10 de borda = 50; a 1fr fica em 0 e o
    // filler de 300px transborda a caixa.
    assert!(
        (g.w - 50.0).abs() < 1.0,
        "#mygrid largura {:?}, esperado 50 (30 conteúdo + 20 borda; 1fr não cresce)",
        g.w
    );
}

/// CSS Grid §11.5.1: a FIXED min track sizing function (`minmax(0px, 500px)`,
/// `minmax(0px, auto)`) is not an intrinsic min, so the item's content does NOT
/// raise the track's base. With a DEFINITE grid height the base is that min and
/// the grow pass distributes the container's space up to the max — a track in a
/// 100px grid holding a 200px item is 100px, with the item overflowing, not
/// 200px (WPT `css-grid/grid-minimum-size-grid-items-011`/`-012`).
const MINMAX_FIXED_MIN: &str = r#"<!DOCTYPE html>
<div id="g" style="display:grid;width:100px;height:100px;grid:minmax(0px,500px)/minmax(0px,500px)">
  <div id="gi"><div style="width:200px;height:200px"></div></div>
</div>"#;

#[test]
fn grid_minmax_min_fixo_nao_cresce_a_track_com_container_definido() {
    let (dom, list) = geometria(MINMAX_FIXED_MIN, 1280.0);
    let g = rect(&dom, &list, "#gi", 0);
    assert!(
        (g.w - 100.0).abs() < 1.0 && (g.h - 100.0).abs() < 1.0,
        "#gi {:?}, esperado 100×100 (track fica no container, item de 200px transborda)",
        (g.w, g.h)
    );
}
