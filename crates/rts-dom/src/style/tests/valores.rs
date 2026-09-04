//! Parse de valores: tipografia, `z-index`, `aspect-ratio`, `transform`, efeitos de texto, grid, `calc()` e `opacity`
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

#[test]
fn parses_typography() {
    let c = parse_inline("color:#ff0000; font-size:18px; font-weight:bold; font-style:italic");
    assert_eq!(c.color, Some(0xFF0000FF));
    assert_eq!(c.font_size, Some(Dimension::Px(18.0)));
    assert_eq!(c.bold, Some(true));
    assert_eq!(c.italic, Some(true));
}

#[test]
fn parses_z_index() {
    assert_eq!(parse_inline("z-index: 10").z_index, Some(10));
    assert_eq!(parse_inline("z-index: -1").z_index, Some(-1));
    assert_eq!(parse_inline("z-index: auto").z_index, None);
}

#[test]
fn parses_aspect_ratio() {
    assert_eq!(
        parse_inline("aspect-ratio: 16 / 9").aspect_ratio,
        Some(16.0 / 9.0)
    );
    assert_eq!(parse_inline("aspect-ratio: 1 / 1").aspect_ratio, Some(1.0));
    assert_eq!(parse_inline("aspect-ratio: 1.5").aspect_ratio, Some(1.5));
    assert_eq!(parse_inline("aspect-ratio: auto").aspect_ratio, None);
}

#[test]
fn parses_transform() {
    use crate::layout::TransformOp;
    let t = parse_inline("transform: translate(10px, -20px) scale(1.5) rotate(45deg)")
        .transform
        .unwrap();
    let ops: Vec<TransformOp> = t.ops.iter().collect();
    assert_eq!(
        ops,
        vec![
            TransformOp::Translate { tx: 10.0, ty: -20.0, tx_pct: 0.0, ty_pct: 0.0 },
            TransformOp::Scale { sx: 1.5, sy: 1.5 },
            TransformOp::Rotate { deg: 45.0 },
        ]
    );
    // translate(-50%, -50%) → frações.
    let c = parse_inline("transform: translate(-50%, -50%)")
        .transform
        .unwrap();
    assert_eq!(
        c.ops.iter().next(),
        Some(TransformOp::Translate { tx: 0.0, ty: 0.0, tx_pct: -0.5, ty_pct: -0.5 })
    );
    // translateX/Y e scaleX/Y isolados.
    let x = parse_inline("transform: translateX(8px)")
        .transform
        .unwrap();
    assert_eq!(
        x.ops.iter().next(),
        Some(TransformOp::Translate { tx: 8.0, ty: 0.0, tx_pct: 0.0, ty_pct: 0.0 })
    );
    let s = parse_inline("transform: scaleY(2)").transform.unwrap();
    assert_eq!(s.ops.iter().next(), Some(TransformOp::Scale { sx: 1.0, sy: 2.0 }));
    // none / desconhecido.
    assert!(parse_inline("transform: none").transform.is_none());
}

#[test]
fn parses_text_effects() {
    use crate::style::values::TextDecoration;
    let a = parse_inline("letter-spacing: 2px; text-decoration: underline");
    assert_eq!(a.letter_spacing, Some(2.0));
    assert_eq!(a.text_decoration, Some(TextDecoration::Underline));
    // `normal` = 0; shorthand com cor/estilo → pega a keyword de linha.
    assert_eq!(
        parse_inline("letter-spacing: normal").letter_spacing,
        Some(0.0)
    );
    assert_eq!(
        parse_inline("text-decoration: line-through dotted red").text_decoration,
        Some(TextDecoration::LineThrough)
    );
    assert_eq!(
        parse_inline("text-decoration-line: overline").text_decoration,
        Some(TextDecoration::Overline)
    );
}

#[test]
fn parses_grid() {
    use crate::style::DisplayKind;
    // display:grid + grid-template-columns via repeat() e via lista de trilhas.
    let a = parse_inline("display:grid; grid-template-columns: repeat(3, minmax(0, 1fr))");
    assert_eq!(a.display, Some(DisplayKind::Grid));
    assert_eq!(a.grid_columns, Some(3));
    let b = parse_inline("grid-template-columns: 1fr 1fr 1fr 1fr");
    assert_eq!(b.grid_columns, Some(4));
    let c = parse_inline("grid-template-columns: 200px 200px");
    assert_eq!(c.grid_columns, Some(2));
}

#[test]
fn parses_calc_in_edges() {
    use crate::style::{CalcLen, Dimension, Side};
    // `padding: calc(0.25rem * 4)` — o espaço interno NÃO pode quebrar o shorthand.
    let c = parse_inline("padding: calc(0.25rem * 4)");
    match c.padding.top {
        Side::Len(Dimension::Calc(CalcLen { rem, .. })) => assert_eq!(rem, 1.0),
        other => panic!("esperava calc 1rem, veio {other:?}"),
    }
    // props lógicas do Tailwind: padding-inline (left+right) e padding-block (top+bottom).
    let pi = parse_inline("padding-inline: 12px");
    assert_eq!(pi.padding.left, Side::Len(Dimension::Px(12.0)));
    assert_eq!(pi.padding.right, Side::Len(Dimension::Px(12.0)));
    assert_eq!(pi.padding.top, Side::Unset);
    let pb = parse_inline("padding-block: 8px");
    assert_eq!(pb.padding.top, Side::Len(Dimension::Px(8.0)));
    assert_eq!(pb.padding.bottom, Side::Len(Dimension::Px(8.0)));
}

#[test]
fn parses_opacity() {
    assert_eq!(parse_inline("opacity:0.5").opacity, Some(0.5));
    assert_eq!(parse_inline("opacity:1").opacity, Some(1.0));
    assert_eq!(parse_inline("opacity:0").opacity, Some(0.0));
    // clampa fora do intervalo [0,1], como o browser.
    assert_eq!(parse_inline("opacity:1.5").opacity, Some(1.0));
    assert_eq!(parse_inline("opacity:-0.2").opacity, Some(0.0));
    // valor inválido → None (default = opaco).
    assert_eq!(parse_inline("opacity:nope").opacity, None);
    // via cascade de autor.
    let mut sheet = Stylesheet::new();
    sheet.append_css(".fade{opacity:0.3}");
    assert_eq!(
        sheet.computed_for("div", None, &["fade"]).normal.opacity,
        Some(0.3)
    );
}
