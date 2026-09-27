//! A altura de conteúdo de um item de coluna (`min-height:auto`, Flexbox
//! §4.5) medida pela ÁRVORE DE CAIXAS e não pelos filhos do DOM.
//!
//! `coluna_shrink::content_height_without_height` andava `dom.node(item)
//! .children` e media cada filho com `measure_block(filho, None)`. Depois da
//! partição bloco-em-inline (CSS 2.1 §9.2.1.1) um filho do DOM pode não ter
//! caixa nenhuma (o `<span>` que só envolve um bloco: as caixas do bloco
//! sobem para o item) ou ter várias (o `<span>` com texto dos dois lados do
//! bloco). No primeiro caso o layout de bloco corria sem árvore e o caminho
//! rápido do cache de fragmentos fazia `expect` da caixa do `<canvas>`; no
//! segundo, `unica_caixa_do_no` recusava escolher um fragmento. Os dois eram
//! pânicos, e o primeiro é o WPT `css-flexbox/percentage-heights-023`.

use crate::table::tests::{geometria, rect};

/// O HTML exacto de `percentage-heights-023` (sem o `<p>`). O Chrome dá ao
/// item 200 de altura e ao `<canvas>` 200×200: `height:100%` resolve contra
/// os 200 do item. Antes deste lote: pânico em `vertical.rs` ("um
/// filho-cacheado tem uma caixa").
///
/// A altura do `<canvas>` NÃO se afirma aqui: este motor dá-lhe 400 mesmo sem
/// o `<span>` à volta (um `<canvas style="display:block;height:100%">` num
/// bloco de 200 sai com o natural), porque a percentagem de altura de um
/// replaced não tem base (não há `avail_h` que chegue a `replaced_inline_size`)
/// — um defeito aberto, anterior e independente deste. Afirmar 400 pinaria o erro; afirmar 200 não é deste
/// lote.
#[test]
fn span_que_so_envolve_um_bloco_nao_derruba_a_altura_de_conteudo_do_item() {
    let (dom, list) = geometria(
        r#"<style>
#flex-container { display: flex; flex-direction: column; }
#flex-item { width: max-content; height: 200px; background: red; }
#target { display: block; height: 100%; background: green; }
</style>
<div id="flex-container">
  <div id="flex-item">
    <span id="inline">
      <canvas id="target" width="400" height="400"></canvas>
    </span>
  </div>
</div>"#,
        800.0,
    );
    let item = rect(&dom, &list, "#flex-item", 0);
    assert_eq!(item.h, 200.0, "{item:?}");
    let canvas = rect(&dom, &list, "#target", 0);
    assert_eq!((canvas.x, canvas.y), (item.x, item.y), "{canvas:?} vs {item:?}");
}

/// Um `<span>` partido com TEXTO dos dois lados do bloco: a altura do
/// conteúdo são as três caixas que a partição produz, empilhadas — uma
/// linha, o bloco, outra linha: 20 + 30 + 20 = 70. O contentor de 20px
/// obriga o item (`height:200px`) a encolher, e o piso automático é o menor
/// entre os 200 declarados e esses 70 (Flexbox §4.5): o item sai com 70.
/// Antes deste lote: pânico em `unica_caixa_do_no` (o `<span>` tem duas).
#[test]
fn span_partido_com_texto_dos_dois_lados_soma_as_duas_linhas_e_o_bloco() {
    let (dom, list) = geometria(
        r#"<style>
#c { display: flex; flex-direction: column; height: 20px; width: 300px; }
#item { height: 200px; font-size: 16px; line-height: 20px; }
#b { height: 30px; }
</style>
<div id="c"><div id="item"><span>aaa<div id="b"></div>ccc</span></div></div>"#,
        800.0,
    );
    let item = rect(&dom, &list, "#item", 0);
    assert_eq!(item.h, 70.0, "{item:?}");
}

/// `display:contents` não é modelado por este motor (`style/parse` devolve
/// `None` e o elemento fica com o display da tag), por isso o invólucro é um
/// bloco e a pergunta que isto fixa é a mesma do Chrome por outro caminho:
/// a altura de conteúdo atravessa o invólucro e chega ao filho de 30px. O
/// item de `height:50px` num contentor de 20px pára em min(50, 30) = 30.
/// Passava ANTES deste lote também — o invólucro tem a sua caixa —, e fica
/// como guarda de que a descida pela árvore não perdeu o caso comum.
#[test]
fn involucro_display_contents_deixa_passar_a_altura_do_filho() {
    let (dom, list) = geometria(
        r#"<style>
#c { display: flex; flex-direction: column; height: 20px; width: 300px; }
#item { height: 50px; }
#w { display: contents; }
#f { height: 30px; }
</style>
<div id="c"><div id="item"><div id="w"><div id="f"></div></div></div></div>"#,
        800.0,
    );
    let item = rect(&dom, &list, "#item", 0);
    assert_eq!(item.h, 30.0, "{item:?}");
}

