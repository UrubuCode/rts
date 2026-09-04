//! Dois comportamentos do lote de PINTURA (rotação exata + recorte de
//! `overflow`), pinados na `DisplayList` materializada — não em `node_rects`,
//! que `transform_corpus.rs` já cobre e este lote não mexeu.

use crate::layout::DisplayItem;
use crate::table::tests::geometria;

/// `transform: rotate(90deg)` emite `PushTransform`/`PopTransform` em volta
/// do `SolidRect`, com a matriz composta em torno de `transform-origin`
/// (default: centro) — não mais o `rect` mutado pela aproximação de norma de
/// coluna (a caixa continua com o TAMANHO ORIGINAL; quem pinta é que aplica a
/// matriz aos 4 cantos).
#[test]
fn rotacao_emite_push_transform_em_vez_de_mutar_o_rect() {
    let (_, list) = geometria(
        "<style>div{width:100px;height:50px;background:#f00;transform:rotate(90deg)}</style><div></div>",
        400.0,
    );
    let itens = list.materialized();

    let push = itens.iter().find_map(|it| match it {
        DisplayItem::PushTransform { mat } => Some(*mat),
        _ => None,
    });
    let solid = itens
        .iter()
        .find(|it| matches!(it, DisplayItem::SolidRect { .. }))
        .expect("o fundo continua um SolidRect");

    assert!(push.is_some(), "rotate(90deg) devia abrir um PushTransform: {itens:?}");
    assert!(
        itens.iter().any(|it| matches!(it, DisplayItem::PopTransform)),
        "todo PushTransform tem o PopTransform correspondente: {itens:?}"
    );
    let DisplayItem::SolidRect { rect, .. } = solid else { unreachable!() };
    // O RECT em si guarda o tamanho ORIGINAL (100×50) — é a matriz, não o
    // rect, que carrega a rotação. Mutar `w`/`h` pela norma das colunas (a
    // aproximação antiga) trocava isto por um quadrado ~75×75.
    assert!((rect.w - 100.0).abs() < 0.5 && (rect.h - 50.0).abs() < 0.5, "{rect:?}");

    // A matriz gira 90°: aplicada ao canto (1,0) relativo à origem, dá ~(0,1).
    let m = push.unwrap();
    let (dx, dy) = (m.a * 1.0 + m.c * 0.0, m.b * 1.0 + m.d * 0.0);
    assert!(dx.abs() < 0.01 && (dy - 1.0).abs() < 0.01, "matriz não é uma rotação de 90°: {m:?}");
}

/// `overflow:hidden` com um filho maior que a caixa: o `BeginClip` PRECISA
/// envolver o `SolidRect` do filho no `walk()` — pin da regressão em que
/// `filhos_antes` usava `list.children.len()` DEPOIS de o filho já ter sido
/// anexado, e o filho saía ANTES do clip (nunca recortado).
#[test]
fn overflow_hidden_recorta_o_filho_que_transborda() {
    let (_, list) = geometria(
        "<style>#caixa{width:50px;height:50px;overflow:hidden}#caixa>div{width:200px;height:200px;background:#00f}</style><div id=caixa><div></div></div>",
        400.0,
    );
    let itens = list.materialized();

    // Índice do BeginClip e do SolidRect do filho (300×300 → só o do filho,
    // não o de `#caixa` — que não tem `background`, então não emite fundo).
    let clip_at = itens
        .iter()
        .position(|it| matches!(it, DisplayItem::BeginClip { .. }))
        .expect("overflow:hidden abre um BeginClip");
    let child_at = itens
        .iter()
        // RGBA (R no byte mais significativo — ver `display.rs`): azul opaco.
        .position(|it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == 0x0000_FFFF));
    let child_at = child_at.expect("o filho pinta um SolidRect azul");

    assert!(
        child_at > clip_at,
        "o SolidRect do filho (índice {child_at}) tem de vir DEPOIS do BeginClip \
         (índice {clip_at}) — antes dele é como se o clip nunca o tivesse envolvido: {itens:?}"
    );
}
