//! `tests/css/claude-flex-cross-size-overflow.html`,
//! `claude-flex-wrap-stretch-linha.html` e
//! `claude-align-content-center-overflow.html` contra o Blink (Edge 152,
//! 2026-09-04): o cross-size de uma linha ÚNICA é sempre a altura DEFINIDA
//! do contentor (mesmo em overflow, com ou sem `flex-wrap`), e
//! `align-content` sem fallback `safe` deixa o espaço livre ficar NEGATIVO
//! quando as linhas transbordam (`flex_linhas.rs`).

use crate::table::tests::{geometria, rect};

#[test]
fn linha_unica_sem_wrap_usa_a_altura_do_contentor_mesmo_com_item_maior() {
    // Lote flex-cross-size: `container_cross_h > items_h` só usava a altura
    // do contentor quando ela era MAIOR que a do maior item — o oposto do
    // caso do `overflow`. `#grande` (height:128) transborda o contentor
    // (height:64) e `#pequeno` (sem height própria) estica contra o
    // CONTENTOR (64 - 8 de margin-bottom = 56), não contra `#grande`.
    const HTML: &str = r#"<style>
  .f { display: flex; width: 200px; height: 64px; background: #eee; }
  #grande { width: 40px; height: 128px; background: #06c; }
  #pequeno { width: 40px; margin-bottom: 8px; background: #0c0; }
</style>
<div class="f"><div id="grande"></div><div id="pequeno"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    assert_eq!(r(".f"), (0.0, 0.0, 200.0, 64.0));
    assert_eq!(r("#grande"), (0.0, 0.0, 40.0, 128.0), "transborda: altura própria, não a do contentor");
    assert_eq!(r("#pequeno"), (40.0, 0.0, 40.0, 56.0), "estica ao CONTENTOR (64-8), não ao #grande (128-8)");
}

#[test]
fn linha_unica_com_wrap_tambem_usa_a_altura_do_contentor() {
    // Mesma causa do teste anterior, mas por `flex-wrap:wrap` com UM item só
    // (uma única linha): `line_h` caía sempre no ramo `items_h` quando
    // `wrap=true`, mesmo com uma única linha — `#s1` esticava a 0 em vez de
    // aos 50px do contentor.
    const HTML: &str = r#"<style>
  .s { display: flex; flex-wrap: wrap; width: 200px; height: 50px; }
  .s > div { width: 80px; }
</style>
<div class="s"><div id="s1"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    assert_eq!(r("#s1"), (0.0, 0.0, 80.0, 50.0), "linha única com wrap: estica aos 50 do contentor, não a 0");
}

#[test]
fn multiplas_linhas_com_wrap_continuam_a_esticar_por_align_content_normal() {
    // O caso de VÁRIAS linhas já funcionava (lote da coluna,
    // `line_stretch_extra`) — este teste fixa que a extração para
    // `flex_linhas::distribuir_align_content` não regrediu: 2 linhas de 2
    // itens cada, altura do contentor 100, sem `height` própria nos itens
    // (natural 0 cada) — `align-content:normal` reparte os 100px igualmente,
    // 50 por linha.
    const HTML: &str = r#"<style>
  .w { display: flex; flex-wrap: wrap; width: 200px; height: 100px; }
  .w > div { width: 100px; }
</style>
<div class="w"><div id="w1"></div><div id="w2"></div><div id="w3"></div><div id="w4"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    assert_eq!(r("#w1"), (0.0, 0.0, 100.0, 50.0));
    assert_eq!(r("#w2"), (100.0, 0.0, 100.0, 50.0));
    assert_eq!(r("#w3"), (0.0, 50.0, 100.0, 50.0));
    assert_eq!(r("#w4"), (100.0, 50.0, 100.0, 50.0));
}

#[test]
fn align_content_center_deixa_as_linhas_transbordarem_simetricamente() {
    // `flex.rs` grampeava `free` a `≥0` ANTES de `align-content` decidir —
    // `center`/`flex-end` sem `safe` não têm fallback por omissão (Box
    // Alignment), então quando as linhas transbordam o `leading` tem de
    // ficar NEGATIVO. Contentor 256×64, 2 linhas de 128×40 cada (80 de
    // conteúdo) → free=-16, leading=-8: a 1ª linha sai a y=-8, a 2ª a y=32
    // (=-8+40) — o clamp antigo dava y=0/40.
    const HTML: &str = r#"<style>
  .c { display: flex; flex-wrap: wrap; align-content: center; width: 256px; height: 64px; }
  .c > div { width: 128px; height: 40px; }
</style>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| {
        let r = rect(&dom, &list, s, 0);
        (r.x, r.y, r.w, r.h)
    };
    assert_eq!(r("#i1"), (0.0, -8.0, 128.0, 40.0));
    assert_eq!(r("#i2"), (128.0, -8.0, 128.0, 40.0));
    assert_eq!(r("#i3"), (0.0, 32.0, 128.0, 40.0), "leading negativo: -8+40, não o 0+40 do clamp antigo");
    assert_eq!(r("#i4"), (128.0, 32.0, 128.0, 40.0));
}
