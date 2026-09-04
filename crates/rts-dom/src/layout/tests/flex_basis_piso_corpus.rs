//! `tests/css/claude-flex-basis-zero-min-content.html` e
//! `claude-flex-basis-unitless-invalido.html` contra o Blink (Edge 152,
//! 2026-09-04) — duas causas independentes do balde "flex-basis-piso".
//!
//! Causa 1 — `flex-basis: 0`/`0%` SEM `flex-grow` não colapsa a 0: o piso
//! automático de min-content (Flexbox §4.5) clampa o hypothetical main size
//! ANTES de resolver grow/shrink (§9.7 passo 2), não só durante o
//! encolhimento. `#a`/`#b`/`#c` variam só o texto — a largura final tem de
//! acompanhar o CONTEÚDO. As larguras são monospace 16px: o `ApproxMeasurer`
//! usa `MONO_ADVANCE` calibrado contra o Blink (0,5498), por isso os números
//! batem a menos de meio pixel — a tolerância é a mesma do corpus real.
//!
//! Causa 2 — um 3º token NUMÉRICO SEM UNIDADE e diferente de zero no
//! shorthand `flex` (`flex: 0 0 4`) não é um `<length>` válido (CSS Values
//! §6.1: só o zero literal dispensa unidade); o shorthand inteiro cai e
//! grow/shrink/basis ficam nos iniciais (0/1/auto) — `#a` usa o seu
//! `width:80px` próprio, não os "4px" que a leitura ingénua do "4" dava.
//!
//! Achado do orquestrador ao correr o release: o piso da causa 1, aplicado a
//! TODO item, deixava um `width:100%; aspect-ratio:1/1` cujo filho TAMBÉM
//! mede a `100%` subir para o seu min-content GIGANTE (o filho mede-se à
//! custa de um pai ainda sem largura) — muito acima do `width` pedido, e o
//! `claude-raster` encravava num canvas desse tamanho
//! (`flex-aspect-ratio-resize-001` do WPT). Fix: o piso automático nunca
//! ultrapassa a "specified size suggestion" (Flexbox §4.5) — o `width` do
//! item, quando definido; `min_automatico` em `flex_limites.rs`.

use crate::table::tests::{geometria, rect};

fn perto(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

const ZERO_MIN_CONTENT: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .f { display: flex; width: 400px; height: 40px; background: #eee; }
  .f > div { background: #fc0; margin: 0 8px; white-space: nowrap; }
  #a { flex: 0 1 0; }
  #b { flex: 0 2 0; }
  #c { flex: 0 2 0%; }
</style>
<div class="f">
  <div id="a">wwwwww</div>
  <div id="b">ww</div>
  <div id="c">wwwwwwwwww</div>
</div>"#;

#[test]
fn flex_basis_zero_sem_grow_fica_no_min_content_nao_em_zero() {
    let (dom, list) = geometria(ZERO_MIN_CONTENT, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    let c = rect(&dom, &list, "#c", 0);
    // Determinístico independentemente do medidor: nenhuma largura colapsa a
    // 0, e a ordem acompanha o conteúdo (mais 'w's = mais largo).
    assert!(
        a.w > 0.0 && b.w > 0.0 && c.w > 0.0,
        "nenhum item pode colapsar a 0: a={} b={} c={}",
        a.w,
        b.w,
        c.w
    );
    assert!(
        c.w > a.w && a.w > b.w,
        "largura acompanha o conteúdo: a={} b={} c={}",
        a.w,
        b.w,
        c.w
    );
    // Blink: a=52.78 b=17.59 c=87.97 (monospace 16px). O ApproxMeasurer usa
    // o mesmo avanço calibrado (MONO_ADVANCE=0.5498) — bate a <0.1px.
    assert!(perto(a.w, 52.78, 0.1), "#a: Blink 52.78, obtido {}", a.w);
    assert!(perto(b.w, 17.59, 0.1), "#b: Blink 17.59, obtido {}", b.w);
    assert!(perto(c.w, 87.97, 0.1), "#c: Blink 87.97, obtido {}", c.w);
    // Altura esticada (align-items:stretch default) ao height:40 do contentor.
    assert_eq!((a.y, a.h), (0.0, 40.0));
    assert_eq!((b.y, b.h), (0.0, 40.0));
    assert_eq!((c.y, c.h), (0.0, 40.0));
    // x: margem 8px de cada lado, sem gap declarado.
    assert!(perto(a.x, 8.0, 0.1), "#a.x: Blink 8, obtido {}", a.x);
    assert!(
        perto(b.x, a.x + a.w + 16.0, 0.1),
        "#b.x: margem 8+8 depois de #a"
    );
    assert!(
        perto(c.x, b.x + b.w + 16.0, 0.1),
        "#c.x: margem 8+8 depois de #b"
    );
}

const UNITLESS_BASIS_INVALIDO: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #container { display: flex; width: 320px; height: 40px; background: #eee; }
  #a { width: 80px; flex: 0 0 4; background: #fc0; }
  #b { width: 80px; background: #0cf; }
</style>
<div id="container"><div id="a"></div><div id="b"></div></div>"#;

#[test]
fn flex_basis_unitless_nao_zero_invalida_o_shorthand_inteiro() {
    let (dom, list) = geometria(UNITLESS_BASIS_INVALIDO, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    // `flex: 0 0 4` inválido → grow:0 shrink:1(inicial) basis:auto → usa o
    // width:80px PRÓPRIO, não os "4px" da leitura ingénua do "4" como basis.
    assert_eq!(
        (a.x, a.y, a.w, a.h),
        (0.0, 0.0, 80.0, 40.0),
        "Blink: shorthand cai, usa width:80px"
    );
    assert_eq!(
        (b.x, b.y, b.w, b.h),
        (80.0, 0.0, 80.0, 40.0),
        "Blink: #b sem flex, width:80px"
    );
}

// O WPT `css/css-flexbox/flex-aspect-ratio-resize-001.html`, sem o
// `<script>` (este corpus não o executa): o Blink sem script dá 500×500 —
// o que este teste fixa é só o invariante que faltava, "o piso nunca
// ultrapassa a base especificada", não o número exacto de um reftest.
const ASPECT_RATIO_WIDTH_CAP: &str = r#"<style>
  body { margin: 0; }
  #container { display: flex; width: 500px; background: red; }
  #wrapper { width: 100%; aspect-ratio: 1 / 1; background: green; }
  .image { display: block; width: 100%; height: 100%; }
</style>
<div id="container">
  <div id="wrapper">
    <img class="image">
  </div>
</div>"#;

#[test]
fn item_com_aspect_ratio_e_filho_a_100pct_nao_excede_a_base_especificada() {
    let (dom, list) = geometria(ASPECT_RATIO_WIDTH_CAP, 1280.0);
    let wrapper = rect(&dom, &list, "#wrapper", 0);
    // O invariante do fix: o piso automático (min-content) nunca ultrapassa
    // a "specified size suggestion" — aqui os 500px do `width:100%` do
    // `#container`. Sem o teto, o min-content do `<img>` a 100% (medido à
    // custa de um pai sem largura própria ainda) erguia o item bem acima
    // disto — e o `main` gigante que sobrava fazia o raster tentar um
    // canvas do mesmo tamanho (era esta chamada que nunca devolvia).
    assert_eq!(wrapper.w, 500.0, "o piso nunca ultrapassa o width:100% especificado");
    // aspect-ratio:1/1 sobre uma largura SÃ dá uma altura SÃ, nunca gigante.
    assert!(wrapper.h < 1000.0, "altura explode se o piso não for capado: {}", wrapper.h);
}
