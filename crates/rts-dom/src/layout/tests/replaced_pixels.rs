//! Um `<img>` com pixels guardados NO documento (`set_pixel_data`, o caminho
//! da `data:` URL do lote V-img) tem tamanho natural e pinta `Pixels` —
//! `tests/css/claude-img-natural.html` no Blink: 4×2 sem atributos, 40×20 com
//! `width: 40px` (a razão mantém-se), 8×8 quando os atributos mandam.

use crate::layout::DisplayItem;
use crate::table::tests::{geometria_com, rect};

const HTML: &str = r#"<style>body{margin:0;font:16px/20px monospace}img{display:block}#so-largura{width:40px}</style>
<img id="natural" src="x.png"><img id="so-largura" src="x.png"><img id="atributos" width="8" height="8" src="x.png">"#;

#[test]
fn img_com_pixels_no_documento_mede_o_natural_e_pinta_pixels() {
    let (dom, list) = geometria_com(HTML, 1280.0, |d| {
        for sel in ["#natural", "#so-largura", "#atributos"] {
            let id = d.query(sel).expect(sel);
            d.set_pixel_data(id, vec![200, 30, 30, 255].repeat(8), 4, 2);
        }
    });
    let n = rect(&dom, &list, "#natural", 0);
    let s = rect(&dom, &list, "#so-largura", 0);
    let a = rect(&dom, &list, "#atributos", 0);
    assert_eq!((n.w, n.h), (4.0, 2.0), "tamanho natural do PNG");
    assert_eq!((s.w, s.h), (40.0, 20.0), "width sozinho mantém a razão");
    assert_eq!((a.w, a.h), (8.0, 8.0), "os atributos vencem o natural");
    assert_eq!((n.y, s.y, a.y), (0.0, 2.0, 22.0), "empilhados como blocos");
    let pixels = list.materialized().iter().filter(|i| matches!(i, DisplayItem::Pixels { .. })).count();
    assert_eq!(pixels, 3, "um item Pixels por imagem");
}
