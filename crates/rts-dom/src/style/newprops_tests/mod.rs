//! Testes das propriedades acrescentadas ao vocabulário CSS: o shorthand
//! `background`, as bordas por lado, `outline`, `vertical-align`, `clear` e o
//! lote de texto/listas.
//!
//! Ficheiro próprio porque `style/tests.rs` já tem 587 linhas (o teto do
//! repositório é 500). Cada teste nomeia o COMPORTAMENTO que fixa e falharia sem
//! a propriedade — o de `background` pinta mesmo o fundo, que era o sintoma que
//! começou este trabalho.

use crate::layout::{ApproxMeasurer, DisplayItem, DisplayList, LayoutCtx, Rect, layout_document};
use crate::style::{BgRepeat, BgSize, BorderStyle, Dimension, parse_inline};

/// Layout determinístico (medidor aproximado, viewport fixo) — o mesmo arranjo
/// dos testes de `layout.rs`, para poder afirmar o que foi PINTADO.
///
/// Os testes inline usam `<em>`: o estilo POR TAG (`define_style`) e a tabela de
/// blocos são thread-locals partilhados entre testes da mesma thread, e outros
/// testes deste crate registam `a`, `p`, `center` e `div` — um teste que dependa
/// de uma dessas tags passa ou falha conforme a ORDEM em que o cargo os corre.
fn layout(html: &str, vw: f32) -> DisplayList {
    crate::block::define(
        "div",
        crate::block::BlockDef {
            display: 0,
            indent: 0.0,
            prefix: 0,
            flags: 0,
        },
    );
    // body{margin:0}: a folha de UA (lote I) dá 8px ao body, e este corpus
    // mede caixas a partir de (0,0)/largura cheia do viewport.
    let dom = crate::parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
    let ctx = LayoutCtx {
        viewport_w: vw,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    };
    layout_document(&dom, &ctx)
}

/// A lista PLANA, em coordenadas absolutas.
///
/// `list.items` traz só os itens do nível de topo: o que um filho de bloco pinta
/// vive no FRAGMENTO dele. E desde que o parser cria `<html>`/`<body>`
/// implícitos — como qualquer browser — nem o elemento escrito no fonte é filho
/// direto do `#document`, portanto `items` responde vazio e o assert acusava
/// "não pintou" numa página que pinta. As três tags são precisas: sem `<body>`
/// na árvore, uma regra `body{…}` não casava com elemento nenhum e TODA a
/// propriedade herdada declarada aí desaparecia em silêncio.
fn itens(list: &DisplayList) -> Vec<DisplayItem> {
    list.materialized()
}

/// A cor do primeiro `SolidRect` pintado (o fundo da 1ª caixa).
fn first_solid(list: &DisplayList) -> Option<(Rect, u32)> {
    itens(list).iter().find_map(|it| match it {
        DisplayItem::SolidRect { rect, color, .. } => Some((*rect, *color)),
        _ => None,
    })
}

mod lote_inicial;
mod line_height;
mod timing_e_logicas;
mod vocabulario;
mod raios_e_transform;
mod bordas_e_clip;
mod grid_lines;
mod pintura;
mod aliases_e_fecho;
