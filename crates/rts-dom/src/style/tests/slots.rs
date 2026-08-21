//! Os slots da ABI e a resolução de `Dimension`
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

#[test]
fn apply_slot_opaco() {
    // SLOT opaco (invariante 4): nenhum nome CSS, só índice + valor.
    let mut s = ComputedStyle::default();
    s.apply_slot(SLOT_COLOR, 0x0088FFFF);
    s.apply_slot(SLOT_FONT_SIZE, 28);
    s.apply_slot(SLOT_BG, 0x111111FF);
    assert_eq!(s.color, Some(0x0088FFFF));
    assert_eq!(s.font_size, Some(Dimension::Px(28.0)));
    assert_eq!(s.bg, Some(0x111111FF));
}

#[test]
fn apply_slot_desconhecido_e_invalido_ignora() {
    let mut s = ComputedStyle::default();
    s.apply_slot(999, 123); // slot inexistente
    s.apply_slot(SLOT_FONT_SIZE, 0); // tamanho 0 inválido
    s.apply_slot(SLOT_FONT_SIZE, -5); // negativo inválido
    assert_eq!(s, ComputedStyle::default());
}

#[test]
fn egui_free_garantia() {
    // Documenta a invariante F0(d): este módulo não nomeia tipos do egui.
    // A cor é u32; o teste compila SÓ se ComputedStyle for egui-free.
    let s = ComputedStyle {
        color: Some(0xAABBCCFF),
        ..Default::default()
    };
    let _raw: Option<u32> = s.color; // se fosse Color32, isto não compilaria.
}

#[test]
fn box_model_slots() {
    // F2: slots de caixa (padding/margin/border/raio) via apply_slot opaco.
    let mut s = ComputedStyle::default();
    assert!(!s.has_box()); // vazio: sem caixa.
    s.apply_slot(SLOT_PADDING, 8);
    s.apply_slot(SLOT_MARGIN, 4);
    s.apply_slot(SLOT_BORDER_WIDTH, 2);
    s.apply_slot(SLOT_BORDER_COLOR, 0xFF0000FF);
    s.apply_slot(SLOT_CORNER_RADIUS, 6);
    s.apply_slot(SLOT_BG, 0x222222FF);
    assert_eq!(s.padding.top, Side::px_len(8.0));
    assert_eq!(s.margin.top, Side::px_len(4.0));
    assert_eq!(s.border_width, Some(2.0));
    assert_eq!(s.border_color, Some(0xFF0000FF));
    assert_eq!(s.corner_radius, Some(6.0));
    assert_eq!(s.bg, Some(0x222222FF));
    assert!(s.has_box());
}

#[test]
fn box_slots_negativos_ignorados() {
    let mut s = ComputedStyle::default();
    s.apply_slot(SLOT_PADDING, -3); // negativo não faz sentido numa caixa
    s.apply_slot(SLOT_CORNER_RADIUS, -1);
    assert_eq!(s.padding.top, Side::Unset);
    assert_eq!(s.corner_radius, None);
}

#[test]
fn has_box_so_com_texto_e_false() {
    // só cor/tamanho de texto NÃO conta como caixa (não vira egui::Frame).
    let mut s = ComputedStyle::default();
    s.apply_slot(SLOT_COLOR, 0xFFFFFFFF);
    s.apply_slot(SLOT_FONT_SIZE, 18);
    assert!(!s.has_box());
}

#[test]
fn dimension_abi_roundtrip() {
    // F2: a codificação ABI por FAIXAS (px/%/em/rem/vw/vh) é reversível — o que
    // o TS empacota o Rust decodifica e re-empacota igual. Auto = -1.
    for d in [
        Dimension::Auto,
        Dimension::Px(280.5),
        Dimension::Percent(60.0),
        Dimension::Em(1.5),
        Dimension::Rem(2.0),
        Dimension::Vw(50.0),
        Dimension::Vh(80.0),
    ] {
        assert_eq!(Dimension::from_abi(d.to_abi()), Some(d), "roundtrip {d:?}");
    }
    // contrato concreto das bases (valor × 1000 dentro da faixa):
    assert_eq!(Dimension::from_abi(-1), Some(Dimension::Auto));
    assert_eq!(
        Dimension::from_abi(DIM_BASE_PX + 280_000),
        Some(Dimension::Px(280.0))
    );
    assert_eq!(
        Dimension::from_abi(DIM_BASE_PERCENT + 60_000),
        Some(Dimension::Percent(60.0))
    );
    assert_eq!(
        Dimension::from_abi(DIM_BASE_EM + 1_500),
        Some(Dimension::Em(1.5))
    );
    assert_eq!(
        Dimension::from_abi(DIM_BASE_VW + 50_000),
        Some(Dimension::Vw(50.0))
    );
}

#[test]
fn dimension_resolve() {
    // F2: resolução TARDE contra o contexto do render (eixo por unidade).
    let ctx = ResolveCtx {
        parent_content_w: 400.0,
        node_font_size: 16.0,
        root_font_size: 20.0,
        viewport_w: 1000.0,
        viewport_h: 800.0,
    };
    assert_eq!(Dimension::Px(120.0).resolve(&ctx), Some(120.0));
    assert_eq!(Dimension::Percent(50.0).resolve(&ctx), Some(200.0)); // 50% de 400
    assert_eq!(Dimension::Em(2.0).resolve(&ctx), Some(32.0)); // 2 × 16
    assert_eq!(Dimension::Rem(2.0).resolve(&ctx), Some(40.0)); // 2 × 20
    assert_eq!(Dimension::Vw(10.0).resolve(&ctx), Some(100.0)); // 10% de 1000
    assert_eq!(Dimension::Vh(25.0).resolve(&ctx), Some(200.0)); // 25% de 800
    assert_eq!(Dimension::Auto.resolve(&ctx), None); // layout decide
}

#[test]
fn width_slot_e_parse() {
    // via SLOT opaco (defineStyle): faixa por unidade.
    let mut s = ComputedStyle::default();
    s.apply_slot(SLOT_WIDTH, DIM_BASE_PERCENT + 50_000); // 50%
    assert_eq!(s.width, Some(Dimension::Percent(50.0)));
    assert!(s.has_box()); // width sozinho já é "caixa" (vira Frame com max_width).
    s.apply_slot(SLOT_WIDTH, DIM_BASE_PX + 320_000); // sobrescreve com px
    assert_eq!(s.width, Some(Dimension::Px(320.0)));
    // via style="" inline: TODAS as unidades.
    assert_eq!(parse_inline("width: 280").width, Some(Dimension::Px(280.0)));
    assert_eq!(
        parse_inline("width: 280px").width,
        Some(Dimension::Px(280.0))
    );
    assert_eq!(
        parse_inline("width: 60%").width,
        Some(Dimension::Percent(60.0))
    );
    assert_eq!(parse_inline("width: 1.5em").width, Some(Dimension::Em(1.5)));
    assert_eq!(parse_inline("width: 2rem").width, Some(Dimension::Rem(2.0)));
    assert_eq!(parse_inline("width: 50vw").width, Some(Dimension::Vw(50.0)));
    assert_eq!(parse_inline("width: 80vh").width, Some(Dimension::Vh(80.0)));
    assert_eq!(parse_inline("width: auto").width, Some(Dimension::Auto));
    // box props inline (F2): padding/margin/border/raio.
    let c = parse_inline("padding: 12; margin: 6; border-width: 2; border-radius: 8");
    assert_eq!(c.padding.top, Side::px_len(12.0));
    assert_eq!(c.margin.top, Side::px_len(6.0));
    assert_eq!(c.border_width, Some(2.0));
    assert_eq!(c.corner_radius, Some(8.0));
}
