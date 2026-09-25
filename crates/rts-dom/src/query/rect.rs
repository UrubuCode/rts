//! O rect que o DOM pede de um nó — `getBoundingClientRect` — e porque ele não
//! é o `Geometry::rects` do mesmo nó.
//!
//! CSSOM View define o bounding rect como a união dos `getClientRects`, e para
//! um inline partido por um bloco (CSS 2.1 §9.2.1.1) o Blink inclui nesses
//! client rects o BLOCO que o partiu: a caixa do `<div>` é descendente do
//! `<span>` no DOM. Medido no Edge 153 (`tests/css/claude-bloco-*.esperado.json`):
//! `<span>antes<div style="height:30px"></div>depois</span>` responde
//! `(0,0,1280,69)`, e um `<span>` cujo único filho é o bloco responde o rect do
//! bloco — não `0×0`, embora a partição o tenha consumido todo e ele não tenha
//! caixa nenhuma.
//!
//! **Porque não se mete o bloco em `Geometry::rects`**, que seria uma linha em
//! `geometry_now`: essa tabela é também a do HIT-TEST. A ordem de hit de um
//! inline partido é `span(fragmento 1), div, span(fragmento 2)`, e o hit-test
//! procura do fim para o princípio — com o bloco dentro do rect do span, o
//! segundo fragmento ganhava o clique em cima do `<div>` inteiro, onde o Chrome
//! acerta o `<div>`. O rect do DOM e o rect do hit-test respondem perguntas
//! diferentes, e só o primeiro é a união dos client rects.
//!
//! A PINTURA também não muda: o fundo e a borda do `<span>` continuam só nos
//! seus fragmentos inline, que é o que o §9.2.1.1 pede e o que se pinta.
//!
//! Moved from `layout/rect_cliente.rs` on 2026-09-25 (PQ-A3); `DisplayList::rect_of` came from `layout/display.rs`. Nothing in it changed.

use crate::dom::NodeIdx;
use crate::paint::list::{DisplayList, Rect};

impl DisplayList {
    /// `getBoundingClientRect` — NÃO é só `Geometry::rects`; ver o topo deste ficheiro.
    pub fn rect_of(&self, node: NodeIdx) -> Option<Rect> {
        rect_cliente(self, node)
    }
}

/// A união das caixas de `node` e das dos blocos em fluxo que o partiram, ou
/// `None` quando nenhuma das duas existe (texto não desenhado, `display:none`,
/// um nó ainda não layoutado).
pub(super) fn rect_cliente(list: &DisplayList, node: NodeIdx) -> Option<Rect> {
    let g = list.geometry();
    let mut acc = g.rects.get(&node).copied();
    for &bloco in list.tree.blocks_splitting(node) {
        let Some(rect) = list.tree.node_of(bloco).and_then(|n| g.rects.get(&n)) else {
            continue;
        };
        acc = Some(acc.map_or(*rect, |a| a.union(*rect)));
    }
    acc
}
