use super::*;
use crate::table::tests::rect;

/// `<canvas>` é inline por natureza, como o `<img>`: dois irmãos ficam LADO A
/// LADO e a caixa conta a própria borda. Enquanto `is_block_level` o forçava a
/// bloco — para que houvesse quem emitisse a superfície — eles empilhavam-se, e
/// a medida ignorava a borda (15×10 onde o Blink dá 17×12).
#[test]
fn canvas_is_an_inline_atom_like_an_image() {
    let dom = parse_html_to_dom(include_str!("../../../../../tests/css/claude-canvas-inline.html"));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let cv = rect(&dom, &list, "#cv", 0);
    assert!(
        (cv.w - 17.0).abs() <= 1.0 && (cv.h - 12.0).abs() <= 1.0,
        "#cv: {cv:?}, Blink: 17x12 (a borda conta)"
    );
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert!(
        (a.y - b.y).abs() <= 1.0 && b.x > a.x,
        "dois canvas irmãos ficam na MESMA linha: a={a:?} b={b:?}, Blink: y igual, x 0 e 17"
    );
}

/// Uma percentagem de largura contra uma base INDEFINIDA computa a `auto`. Um
/// item shrink-to-fit mede o conteúdo com largura infinita, e `100%` de
/// infinito era infinito: a borda do item saía com `w: inf` e o rasterizador
/// ficava 65 segundos a percorrê-la (`intrinsic-percent-replaced-019`, WPT,
/// que por isso deixou de rasterizar de todo).
///
/// O que se afirma é um LIMITE e não um número: o Blink dá 22 a este item e o
/// motor dá 24, porque ele resolve a percentagem uma segunda vez contra a
/// largura já decidida do item. Essa diferença é outro lote; a finitude é o
/// que este garante, e um valor exacto aqui fixaria por engano o que ainda
/// está por decidir.
#[test]
fn a_percentage_width_with_no_basis_does_not_become_infinite() {
    let dom = parse_html_to_dom(include_str!("../../../../../tests/css/claude-canvas-inline.html"));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let caixa = rect(&dom, &list, ".caixa", 0);
    assert!(
        caixa.w.is_finite() && caixa.w < 100.0,
        "o item shrink-to-fit encolhe ao conteúdo: {caixa:?}, Blink: 22x28"
    );
}
