//! Regressão do lote `safe`: `align-items: safe center`/`safe end` deixaram
//! de cair no fallback `start` quando o item TRANSBORDA a linha — em
//! `coluna.rs::align_offset`, `SafeCenter` partilhava o `free / 2.0`
//! incondicional de `Center`, i.e. centrava com um offset NEGATIVO em vez de
//! recuar para o topo. Geometria pura (sem texto), então a mesma conta que a
//! spec faz (CSS Box Alignment §4.4) é a que o Chrome desenha.
//!
//! `#item` (100px) é maior do que a linha (50px) nas duas fixtures — o caso
//! em que `safe` e `unsafe` divergem.

use crate::table::tests::{geometria, rect};

/// `align-items:safe center` num flex ROW (eixo cruzado = altura): item maior
/// do que a linha recua para o TOPO (offset 0), não centra com metade
/// negativa. Antes deste lote: `y=75` (centrado sobre o transbordo); Chrome/
/// spec: `y=100` (o topo da linha, que é o topo do content do container).
#[test]
fn safe_center_row_recua_para_o_topo_quando_o_item_transborda() {
    const HTML: &str = r#"<div style="display:flex;align-items:safe center;height:50px;margin-top:100px;width:200px">
  <div id="item" style="width:50px;height:100px;background:lime"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y), (0.0, 100.0), "#item: {r:?}");
}

/// Guarda: `align-items:center` (SEM `safe`) continua a centrar com offset
/// negativo — o comportamento `unsafe` que Flexbox usa por omissão não pode
/// ter sido tocado por esta correcção.
#[test]
fn center_sem_safe_continua_unsafe() {
    const HTML: &str = r#"<div style="display:flex;align-items:center;height:50px;margin-top:100px;width:200px">
  <div id="item" style="width:50px;height:100px;background:lime"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y), (0.0, 75.0), "#item: {r:?}");
}

/// `align-items:safe center` num flex COLUMN (eixo cruzado = largura): mesma
/// regra, eixo trocado. Container de 50px de largura, item de 100px de
/// largura — recua para `x=0` (o `left` do content) em vez de `x=-25`.
#[test]
fn safe_center_column_recua_para_a_esquerda_quando_o_item_transborda() {
    const HTML: &str = r#"<div style="display:flex;flex-direction:column;align-items:safe center;width:50px;margin-left:100px;height:200px">
  <div id="item" style="height:50px;width:100px;background:lime"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!(r.x, 100.0, "#item: {r:?}");
}
