//! Os testes de COMPORTAMENTO da tabela e do item de lista: cada um nomeia o
//! que fixa sobre a página, não a função que chama.
//!
//! Todos correm sobre o [`crate::layout::ApproxMeasurer`], cujo texto mede
//! `n * tamanho * 0.5`. Isso é o que torna as posições previsíveis a olho: uma
//! célula com "aa" a 16px quer 16px de conteúdo. Um medidor real daria números
//! diferentes e o teste passaria a afirmar coisas sobre a fonte.

use crate::layout::{ApproxMeasurer, DisplayItem, LayoutCtx, Rect, layout_document};
use crate::parse_html_to_dom;

pub(crate) fn geometria(html: &str, largura: f32) -> (crate::Dom, crate::layout::DisplayList) {
    // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este corpus de
    // testes mede coordenadas a partir de (0,0), como o corpus real faz.
    let dom = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
    let ctx = LayoutCtx {
        viewport_w: largura,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

/// Como [`geometria`], mas com uma passagem sobre o `Dom` ANTES do layout —
/// para o que só a ponte faz numa página real (pixels de um `<img>`, valor de
/// um `<input>`), sem trazer a ponte para os testes do crate.
pub(crate) fn geometria_com(
    html: &str,
    largura: f32,
    prepara: impl FnOnce(&mut crate::Dom),
) -> (crate::Dom, crate::layout::DisplayList) {
    let mut dom = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
    prepara(&mut dom);
    let ctx = LayoutCtx {
        viewport_w: largura,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

/// O rect do n-ésimo elemento que casa com o seletor.
pub(crate) fn rect(
    dom: &crate::Dom,
    list: &crate::layout::DisplayList,
    sel: &str,
    n: usize,
) -> Rect {
    let ids = dom.query_all(sel);
    let id = ids.get(n).unwrap_or_else(|| panic!("sem {sel}[{n}]"));
    let idx = dom.resolve(*id).expect("nó vivo");
    *list
        .geometry_now()
        .rects
        .get(&idx)
        .unwrap_or_else(|| panic!("{sel}[{n}] sem geometria"))
}

/// Os textos emitidos na display list, na ordem de pintura.
pub(crate) fn textos(list: &crate::layout::DisplayList) -> Vec<String> {
    list.materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

mod classes;
mod grade;
mod regras;
mod trilhas;
