//! As formas de cor, os shorthands de margin/padding/border, e a propriedade desconhecida
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

#[test]
fn color_forms() {
    assert_eq!(parse_color("#f00"), Some(0xFF0000FF));
    assert_eq!(parse_color("#00ff00"), Some(0x00FF00FF));
    assert_eq!(parse_color("rgb(10, 20, 30)"), Some(0x0A141EFF));
    assert_eq!(parse_color("blue"), Some(0x0000FFFF));
    assert_eq!(parse_color("nope"), None);
    // oklch (paleta do Tailwind v4): branco e preto puros são exatos; uma cor
    // qualquer só precisa cair perto do sRGB esperado (a conversão é aproximada).
    assert_eq!(parse_color("oklch(1 0 0)"), Some(0xFFFFFFFF)); // branco
    assert_eq!(parse_color("oklch(0 0 0)"), Some(0x000000FF)); // preto
    // oklch(0.628 0.2577 29.23) ≈ vermelho sRGB puro. Checa que R domina.
    let red = parse_color("oklch(0.628 0.2577 29.23)").unwrap();
    let r = (red >> 24) & 0xFF;
    let g = (red >> 16) & 0xFF;
    assert!(r > 200 && g < 80, "oklch vermelho deu {red:#010x}");
    // com alpha via `/`.
    let a = parse_color("oklch(1 0 0 / 0.5)").unwrap();
    assert_eq!(a & 0xFF, 0x80);
}

#[test]
fn margin_padding_shorthand() {
    // 1 valor: todos os lados.
    let c = parse_inline("padding: 10px");
    assert_eq!(c.padding, Edges::all(Side::px_len(10.0)));
    // 2 valores: vertical | horizontal.
    let c = parse_inline("margin: 10px 20px");
    assert_eq!(c.margin.top, Side::px_len(10.0));
    assert_eq!(c.margin.bottom, Side::px_len(10.0));
    assert_eq!(c.margin.left, Side::px_len(20.0));
    assert_eq!(c.margin.right, Side::px_len(20.0));
    // 3 valores: top | horizontal | bottom.
    let c = parse_inline("padding: 1px 2px 3px");
    assert_eq!(c.padding.top, Side::px_len(1.0));
    assert_eq!(c.padding.right, Side::px_len(2.0));
    assert_eq!(c.padding.left, Side::px_len(2.0));
    assert_eq!(c.padding.bottom, Side::px_len(3.0));
    // 4 valores: top right bottom left (horário).
    let c = parse_inline("margin: 1px 2px 3px 4px");
    assert_eq!(c.margin.top, Side::px_len(1.0));
    assert_eq!(c.margin.right, Side::px_len(2.0));
    assert_eq!(c.margin.bottom, Side::px_len(3.0));
    assert_eq!(c.margin.left, Side::px_len(4.0));
}

#[test]
fn margin_padding_longhand_e_auto() {
    // por-lado.
    let c = parse_inline("padding-left: 12px; margin-top: 8px");
    assert_eq!(c.padding.left, Side::px_len(12.0));
    assert_eq!(c.margin.top, Side::px_len(8.0));
    assert_eq!(c.padding.top, Side::Unset); // outros lados Unset
    // margin: 0 auto (centralização) — left/right auto.
    let c = parse_inline("margin: 0 auto");
    assert_eq!(c.margin.top, Side::px_len(0.0));
    assert!(c.margin.left.is_auto());
    assert!(c.margin.right.is_auto());
    // padding NÃO aceita auto (vira Unset).
    assert_eq!(parse_inline("padding: auto").padding.left, Side::Unset);
    // margin negativo permitido.
    assert_eq!(
        parse_inline("margin-top: -5px").margin.top,
        Side::px_len(-5.0)
    );
    // longhand VENCE o shorthand na cascade (merge_over por lado).
    let mut base = parse_inline("padding: 10px");
    base.merge_over(&parse_inline("padding-left: 30px"));
    assert_eq!(base.padding.left, Side::px_len(30.0));
    assert_eq!(base.padding.top, Side::px_len(10.0)); // os outros mantêm
}

#[test]
fn border_shorthand() {
    // border: width style color — qualquer ordem.
    let c = parse_inline("border: 2px solid #ff0000");
    assert_eq!(c.border_width, Some(2.0));
    assert_eq!(c.border_style, Some(BorderStyle::Solid));
    assert_eq!(c.border_color, Some(0xFF0000FF));
    // ordem trocada.
    let c2 = parse_inline("border: red solid 3px");
    assert_eq!(c2.border_width, Some(3.0));
    assert_eq!(c2.border_style, Some(BorderStyle::Solid));
    assert_eq!(c2.border_color, Some(0xFF0000FF));
    // keyword de largura.
    assert_eq!(
        parse_inline("border: thin dashed blue").border_width,
        Some(1.0)
    );
    // border-style isolado.
    assert_eq!(
        parse_inline("border-style: dotted").border_style,
        Some(BorderStyle::Dotted)
    );
}

#[test]
fn border_sem_style_nao_e_visivel() {
    // border-width sem border-style → o default é none → NÃO pinta (fiel ao CSS).
    let c = parse_inline("border-width: 2px; border-color: red");
    assert_eq!(c.border_width, Some(2.0));
    // sem border-style declarado: o campo fica None (o render trata como invisível).
    assert_eq!(c.border_style, None);
    // is_visible: none/hidden não pintam, solid/dashed/dotted/double pintam.
    assert!(BorderStyle::Solid.is_visible());
    assert!(BorderStyle::Dashed.is_visible());
    assert!(!BorderStyle::None.is_visible());
    assert!(!BorderStyle::Hidden.is_visible());
}

#[test]
fn color_alpha_hex() {
    // #rgba e #rrggbbaa (com alpha).
    assert_eq!(parse_color("#F09F"), Some(0xFF0099FF)); // nibbles expandidos
    assert_eq!(parse_color("#FF009980"), Some(0xFF009980)); // 8 díg
    assert_eq!(parse_color("#0000"), Some(0x00000000)); // transparente
}

#[test]
fn color_rgba_e_moderno() {
    // rgba legado (vírgula + alpha).
    assert_eq!(parse_color("rgba(255, 0, 153, 0.5)"), Some(0xFF009980));
    // moderno: espaço + / alpha.
    assert_eq!(parse_color("rgb(255 0 153)"), Some(0xFF0099FF));
    assert_eq!(parse_color("rgb(255 0 153 / 50%)"), Some(0xFF009980));
    // canais em %.
    assert_eq!(parse_color("rgb(100% 0% 60%)"), Some(0xFF0099FF));
}

#[test]
fn color_hsl() {
    // hsl básicos (vértices do círculo).
    assert_eq!(parse_color("hsl(0 100% 50%)"), Some(0xFF0000FF)); // vermelho
    assert_eq!(parse_color("hsl(120, 100%, 50%)"), Some(0x00FF00FF)); // verde
    assert_eq!(parse_color("hsl(240 100% 50%)"), Some(0x0000FFFF)); // azul
    // cinza (s=0).
    assert_eq!(parse_color("hsl(0 0% 50%)"), Some(0x808080FF));
    // com alpha.
    assert_eq!(parse_color("hsl(0 100% 50% / 50%)"), Some(0xFF000080));
}

#[test]
fn background_color() {
    let c = parse_inline("background-color: #112233");
    assert_eq!(c.bg, Some(0x112233FF));
    assert_eq!(c.color, None);
}

#[test]
fn ignores_unknown() {
    let c = parse_inline("font-size:bogus; unknown:1; font-weight:300");
    assert_eq!(c.font_size, None);
    assert_eq!(c.bold, Some(false));
}
