//! O fundo de um elemento SUBSTITUÍDO sem pixels (`claude-object-fit`): o
//! Blink pinta o `background` de um `<img>` cuja imagem não carregou, na caixa
//! que `width`/`height` já fixam. Aqui a régua de pintura via 0 itens.

use crate::layout::DisplayItem;
use crate::table::tests::geometria;

#[test]
fn img_sem_pixels_pinta_o_fundo_na_caixa_declarada() {
    let (_, l) = geometria(
        r#"<style>body{margin:0}img{display:block;width:100px;height:50px;background:#eee}</style>
        <img src="data:image/png;base64,iVBORw0KGgo=">"#,
        600.0,
    );
    let fundo = l.materialized().iter().find_map(|i| match i {
        DisplayItem::SolidRect { rect, color, .. } if rect.w == 100.0 && rect.h == 50.0 => Some(*color),
        _ => None,
    });
    assert_eq!(fundo, Some(0xEEEEEEFF), "o #eee do <img> devia estar na lista");
}
