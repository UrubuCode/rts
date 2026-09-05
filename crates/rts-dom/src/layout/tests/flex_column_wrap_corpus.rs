//! `tests/css/claude-flex-column-wrap-multicoluna.html` contra o Blink (Edge
//! 152, 2026-09-04): `flex-direction:column` + `flex-wrap:wrap` quebra em
//! VÁRIAS COLUNAS quando a altura EXPLÍCITA do contentor não comporta todos
//! os itens — corte que estava documentado em `bloco.rs` sem implementação
//! (a mesma causa vista por cinco agentes na triagem do WPT flexbox, baldes
//! "align", "outros-2", "outros-3", "gap-overflow" e "shrink"). O `x=640` da
//! 2ª coluna é `align-content:normal` (não declarado) a esticar as DUAS
//! colunas para preencherem os 1280px do `body` (nem `.c` nem os `<div>`
//! declaram `width`) — não `justify-content` nem `align-items`, que também
//! não estão declarados.

use crate::table::tests::{geometria, rect};

#[test]
fn coluna_quebra_em_duas_colunas_quando_a_altura_nao_chega() {
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { display: flex; flex-direction: column; flex-wrap: wrap; height: 160px; }
  .c > div { width: 100px; height: 80px; }
</style>
<body>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>
</body></html>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // Medido no Blink (`tests/css/claude-flex-column-wrap-multicoluna.esperado.json`):
    // 2 itens de 80px cabem nos 160px do contentor (a 3ª unidade — 3×80=240 —
    // não cabe), então a 3ª e a 4ª abrem uma 2ª coluna. `align-content:normal`
    // estica as duas colunas (100px cada, naturais) para os 1280px do `body`
    // — daí o x=640 da 2ª coluna, não 100 (encostada) nem outro valor.
    assert_eq!(r("#i1"), (0.0, 0.0, 100.0, 80.0));
    assert_eq!(r("#i2"), (0.0, 80.0, 100.0, 80.0));
    assert_eq!(r("#i3"), (640.0, 0.0, 100.0, 80.0), "2a coluna: align-content:normal estica p/ 1280/2");
    assert_eq!(r("#i4"), (640.0, 80.0, 100.0, 80.0));
}

/// `column-reverse` + `wrap-reverse` juntos — combinação que nenhuma fixture
/// medida deste lote cobre sozinha. Os NÚMEROS abaixo são DERIVADOS do
/// algoritmo (`coluna_wrap.rs`: agrupa na ordem do documento, só a POSIÇÃO
/// dentro de cada coluna espelha com `column-reverse`, e `wrap-reverse` troca
/// a ORDEM DAS COLUNAS) — **não foram medidos num browser real**. A ORDEM dos
/// itens (não os px) foi conferida contra a referência real do WPT
/// `flexbox_flow-column-reverse-wrap-reverse-ref.html` (que usa floats
/// reordenados "four,two,three,one" para simular o mesmo resultado): coluna
/// esquerda = four(topo)/three(fundo), coluna direita = two(topo)/one(fundo)
/// — a mesma relação que dá aqui i4/i3 à esquerda e i2/i1 à direita.
#[test]
fn coluna_reversa_e_wrap_reverso_juntos_derivado_da_spec_nao_medido() {
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { display: flex; flex-direction: column-reverse; flex-wrap: wrap-reverse; height: 160px; }
  .c > div { width: 100px; height: 80px; }
</style>
<body>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>
</body></html>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // Agrupamento (ordem do documento, ANTES de qualquer reversão): coluna A
    // = [i1,i2] (160=2×80 cabe nos 160 do contentor), coluna B = [i3,i4].
    // `column-reverse` espelha a posição DENTRO de cada coluna: A vira
    // (i2 no topo, i1 no fundo), B vira (i4 no topo, i3 no fundo).
    // `wrap-reverse` troca a ORDEM DAS COLUNAS no eixo cruzado: B (a 2a
    // calculada) vai para a ESQUERDA (x=0), A vai para a DIREITA (x=640) —
    // o mesmo "cross-start↔cross-end" que `align-content:normal` já estica
    // para 640 cada, como no primeiro teste.
    assert_eq!(r("#i4"), (0.0, 0.0, 100.0, 80.0), "coluna B, invertida: i4 no topo");
    assert_eq!(r("#i3"), (0.0, 80.0, 100.0, 80.0), "coluna B, invertida: i3 no fundo");
    assert_eq!(r("#i2"), (640.0, 0.0, 100.0, 80.0), "coluna A, invertida: i2 no topo");
    assert_eq!(r("#i1"), (640.0, 80.0, 100.0, 80.0), "coluna A, invertida: i1 no fundo");
}

/// `min-height:0` NÃO é um limiar de wrap — regressão apanhada pela régua
/// central contra o WPT `flexbox-flex-basis-content-004a`/`-004b`, medida com
/// `claude-paint-dump` sobre o reftest e a sua referência (`.item{border:2px
/// solid teal;float:left}` + o MESMO `.innerFlex`/`innerItem` do teste — a
/// referência usa float só na metade DE FORA; a metade `flex-wrap:wrap` de
/// DENTRO é a mesma marcação nos dois lados). `.item` (`flex:0 0 content;
/// min-height:0`) É ele próprio `display:flex;flex-direction:column;
/// flex-wrap:wrap`, sem `height` — `avail_children` (bloco.rs) valia
/// `Some(0.0)` só por causa do `min-height:0` (achado do lote
/// `flex-coluna-shrink`: min-height conta como altura definida para
/// stretch/`height:%`), e um limiar de wrap de 0 abre uma coluna nova a
/// cada item (3 itens, 3 colunas de 1, lado a lado) em vez de os empilhar
/// numa pilha vertical de altura automática. `bloco.rs::wrap_definite_h`
/// (só `height`/`max-height`) corrige, sem tocar em `avail_children` (que
/// continua a servir stretch/`height:%`/gap% como antes).
#[test]
fn min_height_zero_no_item_nao_e_limiar_de_wrap_para_o_seu_proprio_column_wrap() {
    const HTML: &str = r#"<style>
  body { margin: 0; }
  .container { display: flex; flex-direction: column; align-items: flex-start; height: 1px; }
  .item { flex: 0 0 content; min-height: 0; border: 2px solid teal; }
  .innerFlex { display: flex; flex-direction: column; }
  innerItem { background: salmon; border: 1px solid gray; height: 10px; width: 15px; flex: none; }
</style>
<div class="container">
  <div class="item innerFlex" style="flex-wrap: wrap">
    <innerItem></innerItem>
    <innerItem></innerItem>
    <innerItem></innerItem>
  </div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // Medido com `claude-paint-dump` (motor próprio) contra a referência do
    // WPT (mesma marcação interna, floats só na metade de fora) — os 3
    // `innerItem` numa PILHA VERTICAL de uma coluna só, `.item` a conter os
    // três (2px de borda de cada lado): antes do fix saíam a (2,2)/(2,2)/(2,2)
    // (uma coluna por item, `.item` com h=16 em vez de 40).
    assert_eq!(r(".item"), (0.0, 0.0, 55.0, 40.0), ".item empilha em 1 coluna, nao 3 colunas de 1");
    assert_eq!(r("innerItem"), (2.0, 2.0, 17.0, 12.0));
    assert_eq!(rect(&dom, &list, "innerItem", 1).y, 14.0, "2o item por baixo do 1o, nao ao lado");
    assert_eq!(rect(&dom, &list, "innerItem", 2).y, 26.0, "3o item por baixo do 2o");
}
