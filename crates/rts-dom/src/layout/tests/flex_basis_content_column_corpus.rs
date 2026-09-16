use super::*;
use crate::table::tests::rect;

#[test]
fn content_basis_in_a_column_ignores_the_items_height() {
    let dom = parse_html_to_dom(include_str!(
        "../../../../../tests/css/claude-flex-basis-content-coluna.html"
    ));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let item = rect(&dom, &list, "#item", 0);
    assert!((item.h - 14.0).abs() <= 1.0, "#item: {item:?}, Blink: 14px");
}

/// O outro lado da mesma condição, e a razão de ela ser estreita: SEM `height`
/// declarado não há nada para ignorar, e a medida natural do item já é a do
/// conteúdo. Somar os filhos um sobre o outro daria 40px aos dois — três
/// inline-blocks ficam na MESMA linha e três floats ficam lado a lado.
/// Dois dos seis casos de `flexbox-flex-basis-content-004a` (WPT), que uma
/// primeira versão desta correcção custou para ganhar o teste acima.
#[test]
fn content_basis_without_a_declared_height_keeps_the_natural_measure() {
    let dom = parse_html_to_dom(include_str!(
        "../../../../../tests/css/claude-flex-basis-content-coluna-sem-height.html"
    ));
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let linha = rect(&dom, &list, "#linha", 0);
    assert!((linha.h - 22.0).abs() <= 1.0, "#linha: {linha:?}, Blink: 22px");
    let flutuantes = rect(&dom, &list, "#flutuantes", 0);
    assert!(
        (flutuantes.h - 16.0).abs() <= 1.0,
        "#flutuantes: {flutuantes:?}, Blink: 16px"
    );
}
