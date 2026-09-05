//! Lote `flex-scroll-overflow`: `overflow:hidden`/`clip` num `flex-wrap:wrap`
//! não deve mudar ONDE a linha quebra — só CORTA visualmente o que
//! transborda depois (WPT `flexbox-overflow-horiz-004`/`-005`). A largura
//! "não comprimida" que o container ganha quando `overflow_x` não é
//! `visible` (#1744, `WhatsApp`) continua a valer para o orçamento de
//! grow/shrink; o que muda é que ela NUNCA decide o ponto de quebra de uma
//! linha `wrap` quando o eixo não é genuinamente rolável (`auto`/`scroll`) —
//! `overflow_viewport::scroll_children_width` é o ponto único da distinção.

use crate::table::tests::{geometria, rect};

/// Duas linhas (não uma) num `flex-wrap:wrap` com `overflow:hidden` e um
/// item que não encolhe: a largura intrínseca (sem comprimir) do item maior
/// não deve alargar o ponto de quebra além da largura DECLARADA do
/// container — `hidden` não rola, não há "deixar transbordar" que valha a
/// pena aqui, e a segunda linha (o `smallItem`) precisa existir para o
/// `align-content` ter DUAS linhas para distribuir.
#[test]
fn overflow_hidden_em_wrap_nao_alarga_o_ponto_de_quebra() {
    const HTML: &str = r#"<style>
  .c {
    display: flex;
    flex-wrap: wrap;
    overflow: hidden;
    width: 70px;
    height: 70px;
  }
  .big { width: 72px; height: 20px; flex: none; }
  .small { width: 20px; height: 20px; }
</style>
<div class="c">
  <div class="big" id="big"></div>
  <div class="small" id="small"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 200.0);
    let big = rect(&dom, &list, "#big", 0);
    let small = rect(&dom, &list, "#small", 0);
    // Duas linhas: o `small` fica ABAIXO do `big`, não ao lado — se o
    // ponto de quebra tivesse sido alargado pela largura intrínseca
    // (72+20=92 > 70), as duas caberiam numa só linha e `small.y` seria
    // igual a `big.y` (o defeito que este teste fixa: era exatamente isso).
    assert!(
        small.y > big.y,
        "o item pequeno devia estar numa 2ª linha, abaixo do grande (y={} vs y={}) — \
         indica que o wrap não quebrou na largura declarada",
        small.y, big.y,
    );
}

/// O mesmo HTML, mas com `overflow` NÃO declarado (`visible`): o
/// comportamento de quebra tem de ser IDÊNTICO ao de cima — `overflow`
/// nunca decide layout, só pintura (CSS 2.1 §11.1.1) — e é essa igualdade
/// que o WPT mede por auto-consistência (reftest `hidden` vs uma referência
/// cujo único objetivo é imitar o `visible`).
#[test]
fn overflow_visible_e_hidden_quebram_a_mesma_linha() {
    let render = |overflow: &str| {
        let html = format!(
            r#"<style>
  .c {{
    display: flex;
    flex-wrap: wrap;
    {overflow}
    width: 70px;
    height: 70px;
  }}
  .big {{ width: 72px; height: 20px; flex: none; }}
  .small {{ width: 20px; height: 20px; }}
</style>
<div class="c">
  <div class="big" id="big"></div>
  <div class="small" id="small"></div>
</div>"#
        );
        let (dom, list) = geometria(&html, 200.0);
        let big = rect(&dom, &list, "#big", 0);
        let small = rect(&dom, &list, "#small", 0);
        (big.y, small.y)
    };
    let (big_y_visible, small_y_visible) = render("");
    let (big_y_hidden, small_y_hidden) = render("overflow: hidden;");
    assert_eq!(big_y_visible, big_y_hidden);
    assert_eq!(small_y_visible, small_y_hidden);
}
