//! Duas causas pequenas da triagem do WPT flexbox, contra o Blink (Edge 152,
//! 2026-09-04): a largura shrink-to-fit de um flex em COLUNA é o maior filho
//! e não a soma (`claude-flex-column-shrink-to-fit`), e um `<input>` item de
//! flex estica na altura com `align-items: stretch`
//! (`claude-flex-stretch-input-height`).

use crate::table::tests::{geometria, rect};

#[test]
fn a_coluna_flutuante_mede_o_maior_filho_e_nao_a_soma() {
    let html = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #coluna { display: flex; flex-direction: column; flex-wrap: wrap; float: left; }
  #coluna > div { flex: none; width: 80px; }
  #a { height: 24px; } #b { height: 16px; } #c { height: 32px; }
</style>
<div id="coluna"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#;
    let (dom, list) = geometria(html, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#coluna"), (0.0, 0.0, 80.0, 72.0), "largura = maior filho, altura = soma");
    assert_eq!(r("#c"), (0.0, 40.0, 80.0, 32.0));
}

#[test]
fn um_input_item_de_flex_estica_na_altura() {
    let html = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #f { display: flex; width: 300px; height: 80px; }
  #campo { flex: 1; margin: 0; padding: 0; border: 0; }
</style>
<div id="f"><input id="campo" type="range"></div>"#;
    let (dom, list) = geometria(html, 1280.0);
    let r = rect(&dom, &list, "#campo", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 300.0, 80.0), "flex-grow na largura, stretch na altura");
}
