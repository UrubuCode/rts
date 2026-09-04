//! `tests/css/claude-flex-column-shrink.html`,
//! `claude-flex-coluna-shrink-overflow.html`,
//! `claude-flex-basis-percent-shrink-column.html`,
//! `claude-gap-row-percentual-eixo.html`,
//! `claude-flex-definite-min-height.html` e
//! `claude-flex-coluna-shrink-zero-e-min-content.html` contra o Blink (Edge
//! 152, 2026-09-04): `flex-basis`/`flex-shrink` no eixo de COLUNA (antes só
//! existiam no eixo horizontal), `row-gap` percentual contra a ALTURA do
//! contentor (nunca a largura, e "normal"/0 quando ela é indefinida),
//! `min-height` a contar como altura DEFINIDA para o stretch dos filhos flex
//! e o `height:%` dos netos, `flex-shrink:0` (não encolhe), `flex-basis:
//! content` (a base é o max-content) e `min-height: min-content` DECLARADO
//! (não some sob `overflow` não-visível, ao contrário do automático).

use crate::table::tests::{geometria, rect};

#[test]
fn coluna_com_altura_fixa_encolhe_os_itens_em_vez_de_transbordar() {
    // Lote flex-coluna-shrink: `layout_children_column` nunca lia
    // `flex-shrink` — 4 itens de 50px de altura num contentor de 100px
    // transbordavam (h=50 cada, y=0/50/100/150) em vez de encolher para 25.
    const HTML: &str = r#"<style>
  #f { display: flex; flex-flow: column nowrap; width: 100px; height: 100px; background: #eee; }
  #f > div { height: 50px; }
  #a { background: #fc0; }
  #b { background: #f0c; }
  #c { background: #0cf; }
  #d { background: #0c0; }
</style>
<div id="f">
  <div id="a"></div>
  <div id="b"></div>
  <div id="c"></div>
  <div id="d"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#f"), (0.0, 0.0, 100.0, 100.0));
    assert_eq!(r("#a"), (0.0, 0.0, 100.0, 25.0), "100/4 itens = 25 cada");
    assert_eq!(r("#b"), (0.0, 25.0, 100.0, 25.0));
    assert_eq!(r("#c"), (0.0, 50.0, 100.0, 25.0));
    assert_eq!(r("#d"), (0.0, 75.0, 100.0, 25.0));
}

#[test]
fn item_de_coluna_com_overflow_scroll_encolhe_ate_ao_espaco_do_contentor() {
    // `overflow-y:scroll` faz o `min-height:auto` resolver a 0 (Flexbox §4.5,
    // "automatic minimum size") — o único item, com `flex-grow:1` e conteúdo
    // de 200px, encolhe até aos 80px do contentor em vez de crescer para 200.
    const HTML: &str = r#"<style>
  #contentor { display: flex; flex-direction: column; width: 80px; height: 80px; }
  #item { flex-grow: 1; overflow-y: scroll; }
  #item > div { height: 200px; background: #06c; }
</style>
<div id="contentor"><div id="item"><div></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#contentor"), (0.0, 0.0, 80.0, 80.0));
    assert_eq!(r("#item"), (0.0, 0.0, 80.0, 80.0), "encolhe a 80, não cresce a 200");
}

#[test]
fn flex_basis_percentual_em_coluna_resolve_contra_a_altura_e_encolhe() {
    // `flex-basis:100%` num container de coluna não era lido — `#resto`
    // media só o conteúdo (vazio, 0) e `#topo` (height:20%) nunca encolhia. A
    // soma das bases (48+240=288) excede os 240 do contentor: cada um cede
    // proporcionalmente a `shrink×base` (shrink=1 nos dois, o default).
    const HTML: &str = r#"<style>
  #pai { position: relative; width: 320px; height: 240px; }
  #c { position: absolute; top: 0; bottom: 0; left: 0; right: 0; display: flex; flex-direction: column; background: red; }
  #topo { background: green; height: 20%; }
  #resto { background: blue; flex-basis: 100%; }
</style>
<div id="pai"><div id="c"><div id="topo"></div><div id="resto"></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#c"), (0.0, 0.0, 320.0, 240.0));
    assert_eq!(r("#topo"), (0.0, 0.0, 320.0, 40.0), "48 de base, encolhe 8 -> 40");
    assert_eq!(r("#resto"), (0.0, 40.0, 320.0, 200.0), "240 de base, encolhe 40 -> 200");
}

#[test]
fn row_gap_percentual_resolve_contra_a_altura_e_vira_normal_quando_indefinida() {
    // `row-gap` é sempre o eixo de BLOCO (aqui vertical nos dois casos,
    // mesmo com `flex-direction:row`): `#A` tem altura definida (200) e
    // `row-gap:10%` = 20 entre as duas linhas (o motor resolvia contra a
    // LARGURA, 64, dando 6.4). `#B` tem altura AUTO (indefinida) e
    // `row-gap:20%` vira `normal` (0) — CSS Align 3 §column-row-gap. Como
    // `#A` tem múltiplas linhas com altura definida, `align-content:normal`
    // (o inicial) estica-as: 2 linhas de 40 nos 200px de `#A` crescem para
    // 90 cada, e só depois entra o gap de 20 entre elas — por isso `#a2` sai
    // a 110 (=90+20), não a 60 (=40+20, o empacotamento simples).
    const HTML: &str = r#"<style>
  .a { display: flex; flex-wrap: wrap; width: 64px; height: 200px; row-gap: 10%; }
  .a > div { width: 48px; height: 40px; background: #0c0; }
  .b { display: flex; flex-wrap: wrap; width: 200px; row-gap: 20%; }
  .b > div { width: 120px; height: 40px; background: #c0f; }
</style>
<div class="a" id="A"><div id="a1"></div><div id="a2"></div></div>
<div class="b" id="B"><div id="b1"></div><div id="b2"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#A"), (0.0, 0.0, 64.0, 200.0));
    assert_eq!(r("#a1"), (0.0, 0.0, 48.0, 40.0));
    assert_eq!(r("#a2"), (0.0, 110.0, 48.0, 40.0), "linhas esticadas (90 cada) + gap 20% de 200 = 20");
    assert_eq!(r("#B"), (0.0, 200.0, 200.0, 80.0), "altura auto: soma sem gap (40+0+40)");
    assert_eq!(r("#b1"), (0.0, 200.0, 120.0, 40.0));
    assert_eq!(r("#b2"), (0.0, 240.0, 120.0, 40.0), "row-gap 20% de altura indefinida = normal (0)");
}

#[test]
fn min_height_conta_como_altura_definida_para_stretch_e_height_percentual() {
    // `avail_children` (bloco.rs) só olhava para `height`/`max-height`;
    // `min-height` resolvia e era descartado. `#ext` só tem `min-height`
    // (sem conteúdo que a alargue) — devia contar como definida para (a) o
    // `align-items:stretch` de `#item` e (b) o `height:100%` de `#neto`.
    const HTML: &str = r#"<style>
  #ext { display: flex; min-height: 100px; width: 200px; background: #eee; }
  #item { width: 100px; background: #fc0; }
  #neto { height: 100%; background: #0c0; }
</style>
<div id="ext">
  <div id="item">
    <div id="neto"></div>
  </div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#ext"), (0.0, 0.0, 200.0, 100.0));
    assert_eq!(r("#item"), (0.0, 0.0, 100.0, 100.0), "estica ao min-height do pai");
    assert_eq!(r("#neto"), (0.0, 0.0, 100.0, 100.0), "height:100% do item, não 0");
}

#[test]
fn flex_shrink_zero_flex_basis_content_e_min_height_min_content_nao_encolhem() {
    // RETRABALHO (2026-09-04): o build central mediu 2 reftests do WPT que
    // PASSAVAM caindo com o primeiro commit deste lote — os dois por
    // encolher em coluna o que a spec pede para não encolher.
    // `flexbox-flex-basis-content-004a`: `flex: 0 0 content` fazia
    // `apply_flex_shorthand` ler o 3º token não reconhecido ("content", sem
    // suporte a essa keyword) como basis AUSENTE, caindo no fallback de "um
    // número sem basis" (`Percent(0.0)`) — #a colapsava a 0 em vez de manter
    // os 60 do conteúdo (o `flex-shrink:0` já estava certo; a base é que
    // nascia errada). `flex-item-min-height-min-content-overflow`: a
    // keyword `min-content` em `min-height` não era parseada — caía no
    // automático de `min_main_auto`, que ZERA sob overflow não-visível
    // (`overflow:auto`), mas o piso aqui é DECLARADO (80) e não deve
    // desaparecer com o overflow.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 20px/20px monospace; }
  .col { display: flex; flex-direction: column; height: 40px; width: 100px; margin-bottom: 60px; align-items: flex-start; }
  #a { flex: 0 0 content; min-height: 0; background: #c00; }
  #b { overflow: auto; min-height: min-content; background: #0c0; }
  #c { flex: 0 1 auto; min-height: 0; background: #00c; }
</style>
<div class="col" id="ca"><div id="a">X<br>X<br>X</div></div>
<div class="col" id="cb"><div id="b">X<br>X<br>X<br>X</div></div>
<div class="col" id="cc"><div id="c">X<br>X<br>X</div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // A LARGURA é shrink-to-fit sobre "X" — o `ApproxMeasurer` (n×size×0.5)
    // e não o medidor real do Blink, então difere do `.esperado.json`
    // (11,0px) por calibração; é a ALTURA (por `line-height`, não por
    // largura de fonte) que fixa este lote, e essa bate exata nos três.
    assert_eq!(r("#a"), (0.0, 0.0, 9.2, 60.0), "flex-shrink:0 + flex-basis:content: não encolhe");
    assert_eq!(r("#b"), (0.0, 100.0, 9.2, 80.0), "min-height:min-content declarado sobrevive ao overflow:auto");
    assert_eq!(r("#c"), (0.0, 200.0, 9.2, 40.0), "flex:0 1 auto + min-height:0: encolhe até ao contentor (o caso que já tinha)");
}
