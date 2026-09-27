//! WPT `css/css-flexbox/balance/balance-negative-margin-001.html` (referência:
//! um quadrado verde 100×100 preenchido, `overflow:clip`, sem vermelho — o
//! `#flex` de fundo vermelho só aparece onde nada pinta por cima).
//!
//! Regrediu em `e83b34321` (a árvore de caixas passou a ser a fonte da
//! iteração de itens flex): o nó de COMENTÁRIO do WPT original, entre `#a` e
//! `#b`, parou de contar como item (a árvore de caixas não gera caixa para
//! ele, correto — BT-1). Sem ele, `#a`+`#b` decidem a quebra sozinhos, e a
//! soma que decide isso usava o `min_main` de `#b` CRU — a "specified size
//! suggestion" outer (§4.5), que inclui a margem negativa e por isso é
//! NEGATIVO de propósito — sem o piso que CSS Flexbox 2 §algo-line-break pede
//! ("floor the outer hypothetical main size... at zero", só sob `balance`), a segunda linha
//! saía diferente e `#b` deixava de cobrir o resto do contentor. O fix mora
//! em `flex_limites::hypothetical_for_wrap`; este teste pina a geometria
//! (não só "sem vermelho") para não regredir em silêncio de novo.
use crate::table::tests::{geometria, rect};

#[test]
fn item_com_margem_negativa_nao_abre_segunda_linha_vazia() {
    const HTML: &str = r#"<style>
#flex {
  display: flex;
  flex-wrap: wrap;
  flex-wrap: balance;
  width: 100px;
  height: 100px;
  background: red;
  overflow: clip;
}
#flex > div {
  height: 50px;
  background: green;
  flex-grow: 1;
}
</style>
<div id="flex">
  <div id="a" style="width: 150px;"></div>
  <!-- Without clamping this item would "fit" on the first line, (e.g. see result with just flex-wrap:wrap). -->
  <div id="b" style="width: 0px; margin-left: -50px; margin-right: -50px;"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    let (ax, ay, aw, ah) = r("#a");
    let (bx, by, bw, bh) = r("#b");
    // `#a` (150px, sozinho já maior que o contentor) fica na 1a linha,
    // `#b` (margem negativa) na 2a — cada linha quebra SOZINHA, mas nenhuma
    // fica vazia: com `flex-grow:1` e só um item por linha, o grow dá a cada
    // um TODO o espaço livre da sua própria linha, cobrindo o contentor
    // inteiro em cada uma (`#a`: 0..100; `#b`, com a margem esquerda de
    // -50px somada ao main já crescido, pinta -50..150) — é isso que faz o
    // quadrado sair TODO verde apesar de haver 2 linhas (o `#flex` vermelho
    // de fundo nunca aparece por baixo, mesmo sob `overflow:clip`). O
    // defeito era `#b` herdar o `min_main` NEGATIVO (a "specified size
    // suggestion" outer, §4.5, que inclui a margem) na CONTA DE QUEBRA em
    // vez de um piso de zero (`flex_limites::hypothetical_for_wrap`) — sem
    // ele, um nó de COMENTÁRIO entre `#a` e `#b` (como no WPT original)
    // deixava de contar como item quando a árvore de caixas passou a ser a
    // fonte da iteração (`e83b34321`), e a soma sem piso da decisão de
    // quebra passava a produzir uma linha diferente onde `#b` NÃO cobre o
    // resto — o rasterizador confirma isso no corpus real (ver o comentário
    // do módulo).
    assert_eq!((ax, ay, aw, ah), (0.0, 0.0, 100.0, 50.0), "#a sozinho na 1a linha cresce até cobri-la");
    assert_eq!((bx, by, bw, bh), (-50.0, 50.0, 200.0, 50.0), "#b sozinho na 2a linha cresce até cobri-la, apesar do min_main negativo");
}

/// O contraponto: com `wrap` SIMPLES não há piso, e o próprio WPT o diz no
/// comentário da fixture ("Without clamping this item would fit on the first
/// line, e.g. see result with just flex-wrap:wrap"). `#b` soma -100 à linha
/// de `#a` (150 - 100 = 50 <= 100) e fica nela; o espaço livre (50) divide-se
/// pelos dois `flex-grow:1`. Pisar em zero também aqui mudaria o `wrap`
/// normal de um lado ao outro para ganhar um teste de `balance`.
#[test]
fn wrap_simples_nao_pisa_a_margem_negativa_na_quebra() {
    const HTML: &str = r#"<style>
#flex { display: flex; flex-wrap: wrap; width: 100px; height: 100px; }
#flex > div { height: 50px; flex-grow: 1; }
</style>
<div id="flex">
  <div id="a" style="width: 150px;"></div>
  <div id="b" style="width: 0px; margin-left: -50px; margin-right: -50px;"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert_eq!((a.x, a.y, a.w, a.h), (0.0, 0.0, 175.0, 50.0), "#a divide o espaço livre com #b na MESMA linha");
    assert_eq!((b.y, b.w), (0.0, 25.0), "#b cabe na 1a linha: sem piso, o seu tamanho outer é -100");
}
