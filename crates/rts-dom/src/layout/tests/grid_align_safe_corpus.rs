//! Guarda de integração para `grid.rs::cell_align_offset` (a unidade real do
//! bug e o fix estão pinados por `grid.rs::tests`, no próprio ficheiro — este
//! corpus só verifica que uma página real ainda centra um item de grid que
//! CABE na célula, tanto com `center` como com `safe center`; o caso que
//! DIVERGE entre `safe`/`unsafe` (item maior do que a célula) não é
//! observável por uma página real hoje: `layout_children_grid` encolhe um
//! item não-`stretch` para `nat_w.min(cell_w)` ANTES de `cell_align_offset`
//! o ver, então `free` nunca é negativo neste caminho — ver a nota em
//! `grid.rs::cell_align_offset`.

use crate::table::tests::{geometria, rect};

#[test]
fn safe_center_centra_quando_o_item_cabe_na_celula() {
    const HTML: &str = r#"<div style="display:grid;grid-template-columns:100px;justify-items:safe center">
  <div id="item" style="width:40px;height:20px;background:lime"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 400.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!(r.x, 30.0, "#item: {r:?}");
}
