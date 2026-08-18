//! Os testes de COMPORTAMENTO do `display: list-item` — o marcador, a
//! numeração e o recuo da lista.
//!
//! Vivem ao lado dos da tabela e não dentro do `listitem.rs` porque partilham os
//! ajudantes (`geometria`/`rect`/`textos`), que montam um documento inteiro e
//! leem a geometria de um seletor: uma segunda cópia deles seria a duplicação
//! que o resto do crate evita.

use super::tests::{geometria, rect, textos};
use crate::layout::{DisplayItem, Rect};

#[test]
fn ol_numera_os_itens_a_partir_de_um() {
    let (dom, list) = geometria("<ol><li>um</li><li>dois</li><li>três</li></ol>", 800.0);
    let t = textos(&list);
    for esperado in ["1.", "2.", "3."] {
        assert!(t.iter().any(|s| s == esperado), "faltou o marcador {esperado} em {t:?}");
    }
    // O marcador do primeiro item fica à esquerda do content-box dele.
    let li = rect(&dom, &list, "li", 0);
    let x_marcador = list
        .materialized()
        .iter()
        .find_map(|i| match i {
            DisplayItem::Text { x, text, .. } if &**text == "1." => Some(*x),
            _ => None,
        })
        .expect("marcador 1.");
    assert!(x_marcador < li.x, "marcador em {x_marcador}, item em {}", li.x);
}

#[test]
fn ol_com_start_comeca_no_numero_pedido() {
    let (_, list) = geometria("<ol start=\"5\"><li>a</li><li>b</li></ol>", 800.0);
    let t = textos(&list);
    assert!(t.iter().any(|s| s == "5."), "{t:?}");
    assert!(t.iter().any(|s| s == "6."), "{t:?}");
}

/// `list-style: none` não gera marcador nenhum — o caso mais comum numa página
#[test]
fn list_style_none_nao_gera_marcador() {
    let (_, list) = geometria(
        "<ul style=\"list-style:none\"><li>a</li><li>b</li></ul>",
        800.0,
    );
    // Nenhum bullet: os únicos rects sólidos possíveis viriam de fundos, que
    // este markup não tem.
    let bullets = list
        .materialized()
        .iter()
        .filter(|i| matches!(i, DisplayItem::SolidRect { .. }))
        .count();
    assert_eq!(bullets, 0, "list-style:none desenhou {bullets} marcadores");
}

#[test]
fn ul_desenha_um_bullet_por_item_dentro_do_recuo() {
    let (dom, list) = geometria("<ul><li>a</li><li>b</li></ul>", 800.0);
    let bullets: Vec<Rect> = list
        .materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bullets.len(), 2, "esperados 2 bullets, vieram {}", bullets.len());
    let ul = rect(&dom, &list, "ul", 0);
    let li = rect(&dom, &list, "li", 0);
    for b in &bullets {
        assert!(b.x + b.w <= li.x + 0.5, "bullet invade o texto: {} vs {}", b.x + b.w, li.x);
        assert!(b.x >= ul.x - 0.5, "bullet fora da caixa da lista");
    }
}

/// Um `<li>` que o autor virou `display:flex` deixa de ser item de lista: não
/// ganha marcador e não conta para a numeração dos irmãos.
#[test]
fn li_com_display_trocado_nao_e_mais_item_de_lista() {
    let (_, list) = geometria(
        "<ol><li style=\"display:flex\">a</li><li>b</li></ol>",
        800.0,
    );
    let t = textos(&list);
    assert!(!t.iter().any(|s| s == "2."), "o `flex` não devia contar: {t:?}");
    assert!(t.iter().any(|s| s == "1."), "o item que sobrou é o 1: {t:?}");
}

/// `list-style-position: inside` põe o marcador DENTRO da caixa de conteúdo, e
#[test]
fn list_style_position_muda_o_lado_do_marcador_e_nao_a_caixa() {
    let (d1, l1) = geometria("<ul><li>a</li></ul>", 800.0);
    let (d2, l2) = geometria(
        "<ul style=\"list-style-position:inside\"><li>a</li></ul>",
        800.0,
    );
    let bullet = |l: &crate::layout::DisplayList| {
        l.materialized()
            .iter()
            .find_map(|i| match i {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("bullet")
    };
    let fora = bullet(&l1);
    let dentro = bullet(&l2);
    let li_fora = rect(&d1, &l1, "li", 0);
    let li_dentro = rect(&d2, &l2, "li", 0);

    assert!(fora.x + fora.w <= li_fora.x + 0.5, "outside devia ficar fora do conteúdo");
    assert!(dentro.x >= li_dentro.x - 0.5, "inside devia ficar dentro do conteúdo");
    // A caixa do item é a MESMA nos dois: o marcador nunca ocupa espaço de fluxo.
    assert!((li_fora.w - li_dentro.w).abs() < 0.5, "a caixa mudou: {} vs {}", li_fora.w, li_dentro.w);
}
