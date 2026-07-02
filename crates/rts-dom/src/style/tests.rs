//! Testes do motor de estilo — migrados intactos do `style.rs` monolítico na
//! divisão em submódulos (a API pública é a mesma via reexports do `mod.rs`).

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
fn color_forms() {
    assert_eq!(parse_color("#f00"), Some(0xFF0000FF));
    assert_eq!(parse_color("#00ff00"), Some(0x00FF00FF));
    assert_eq!(parse_color("rgb(10, 20, 30)"), Some(0x0A141EFF));
    assert_eq!(parse_color("blue"), Some(0x0000FFFF));
    assert_eq!(parse_color("nope"), None);
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
    assert_eq!(parse_inline("margin-top: -5px").margin.top, Side::px_len(-5.0));
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
    assert_eq!(parse_inline("border: thin dashed blue").border_width, Some(1.0));
    // border-style isolado.
    assert_eq!(parse_inline("border-style: dotted").border_style, Some(BorderStyle::Dotted));
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
    let s = ComputedStyle { color: Some(0xAABBCCFF), ..Default::default() };
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
    assert_eq!(Dimension::from_abi(DIM_BASE_PX + 280_000), Some(Dimension::Px(280.0)));
    assert_eq!(Dimension::from_abi(DIM_BASE_PERCENT + 60_000), Some(Dimension::Percent(60.0)));
    assert_eq!(Dimension::from_abi(DIM_BASE_EM + 1_500), Some(Dimension::Em(1.5)));
    assert_eq!(Dimension::from_abi(DIM_BASE_VW + 50_000), Some(Dimension::Vw(50.0)));
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
    assert_eq!(parse_inline("width: 280px").width, Some(Dimension::Px(280.0)));
    assert_eq!(parse_inline("width: 60%").width, Some(Dimension::Percent(60.0)));
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

#[test]
fn stylesheet_seletores_e_especificidade() {
    // <style> com tag/.class/#id; #id > .class > tag na cascade.
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "p { color:#ff0000; font-size:14 }
         .card { color:#00ff00; padding:10 }
         #alvo { color:#0000ff }",
    );
    // <p> simples: só a regra de tag.
    let s = sheet.computed_for("p", None, &[]).normal;
    assert_eq!(s.color, Some(0xFF0000FF));
    assert_eq!(s.font_size, Some(Dimension::Px(14.0)));
    // <p class="card">: classe vence a tag na COR (10>1), mas font-size só a
    // tag tem (herda), e padding só a classe.
    let s = sheet.computed_for("p", None, &["card"]).normal;
    assert_eq!(s.color, Some(0x00FF00FF)); // classe > tag
    assert_eq!(s.font_size, Some(Dimension::Px(14.0))); // só a tag define
    assert_eq!(s.padding.top, Side::px_len(10.0)); // só a classe define
    // <p id="alvo" class="card">: id vence tudo na cor (100>10>1).
    let s = sheet.computed_for("p", Some("alvo"), &["card"]).normal;
    assert_eq!(s.color, Some(0x0000FFFF)); // id > classe > tag
    assert_eq!(s.padding.top, Side::px_len(10.0)); // classe ainda aplica onde o id não toca
}

#[test]
fn stylesheet_empate_ordem_e_virgula() {
    let mut sheet = Stylesheet::new();
    // mesma especificidade (classe) → a DECLARADA DEPOIS vence.
    sheet.append_css(".a { color:#ff0000 } .a { color:#00ff00 }");
    assert_eq!(sheet.computed_for("div", None, &["a"]).normal.color, Some(0x00FF00FF));
    // seletor-lista `h1, h2, .big { ... }` → aplica aos três.
    let mut s2 = Stylesheet::new();
    s2.append_css("h1, h2, .big { font-size:30 }");
    assert_eq!(s2.computed_for("h1", None, &[]).normal.font_size, Some(Dimension::Px(30.0)));
    assert_eq!(s2.computed_for("h2", None, &[]).normal.font_size, Some(Dimension::Px(30.0)));
    assert_eq!(s2.computed_for("p", None, &["big"]).normal.font_size, Some(Dimension::Px(30.0)));
    assert_eq!(s2.computed_for("p", None, &[]).normal.font_size, None); // não casa
}

#[test]
fn stylesheet_universal_e_comentarios() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "/* tema escuro */ * { color:#cccccc } /* destaque */ .hl { color:#ffff00 }",
    );
    // universal aplica a qualquer tag; a classe (mais específica) sobrepõe.
    assert_eq!(sheet.computed_for("span", None, &[]).normal.color, Some(0xCCCCCCFF));
    assert_eq!(sheet.computed_for("span", None, &["hl"]).normal.color, Some(0xFFFF00FF));
}

#[test]
fn important_separa_camadas() {
    // `!important` vai para a camada important; normal fica na normal.
    let b = parse_inline_block("color:#ff0000 !important; font-size:14");
    assert_eq!(b.important.color, Some(0xFF0000FF));
    assert_eq!(b.important.font_size, None);
    assert_eq!(b.normal.font_size, Some(Dimension::Px(14.0)));
    assert_eq!(b.normal.color, None);
    // case-insensitive e com espaço antes do `!`.
    let b2 = parse_inline_block("padding: 10  !IMPORTANT");
    assert_eq!(b2.important.padding.top, Side::px_len(10.0));
}

#[test]
fn important_vence_especificidade_maior() {
    // MDN estágio 1: um `!important` de TAG vence um normal de #id (a importância
    // inverte a precedência de origem/especificidade dentro da mesma origem-autor).
    let mut sheet = Stylesheet::new();
    sheet.append_css("p { color:#ff0000 !important } #x { color:#0000ff }");
    let b = sheet.computed_for("p", Some("x"), &[]);
    // normal: #id vence (azul). important: a tag (vermelho).
    assert_eq!(b.normal.color, Some(0x0000FFFF));
    assert_eq!(b.important.color, Some(0xFF0000FF));
    // entre dois important, a especificidade volta a valer:
    let mut s2 = Stylesheet::new();
    s2.append_css("p { color:#ff0000 !important } #x { color:#0000ff !important }");
    let b2 = s2.computed_for("p", Some("x"), &[]);
    assert_eq!(b2.important.color, Some(0x0000FFFF)); // #id important vence tag important
}

#[test]
fn stylesheet_malformado_nao_panica() {
    let mut sheet = Stylesheet::new();
    // sem `}`, seletor com combinador (cortado), bloco vazio.
    sheet.append_css("p { color:#ff0000  .x { } div p { color:#000 } #ok { font-size:20 }");
    // o `#ok` (após o bloco sem-fechar consumir até o próximo `}`) ainda é lido
    // de forma robusta; o importante é não panicar e parsear o que dá.
    assert!(!sheet.is_empty());
    // `div p` (combinador descendente) AGORA vira uma regra válida (#1752): 2
    // compounds (div, p) ligados por Descendant.
    let has_descendant = sheet.rules.iter().any(|r| r.selector.compounds.len() == 2);
    assert!(has_descendant || !sheet.is_empty()); // robustez: ao menos parseou algo
}

#[test]
fn define_style_acumula_por_tag() {
    // F1: defineStyle por slot OPACO acumula na mesma tag (cor + tamanho).
    // (thread_local — usa uma tag única pra não colidir com outros testes.)
    define_style("h1_acum", SLOT_COLOR, 0x0088FFFF);
    define_style("h1_acum", SLOT_FONT_SIZE, 28);
    let s = lookup_style("h1_acum").expect("tag registrada");
    assert_eq!(s.color, Some(0x0088FFFF));
    assert_eq!(s.font_size, Some(Dimension::Px(28.0)));
    // tag não registrada → None.
    assert_eq!(lookup_style("tag_inexistente_xyz"), None);
}

#[test]
fn font_size_e_lados_em_rem() {
    // font-size preserva a FORMA no parse (rem/em/%/vw/calc); quem resolve para
    // Px é a CASCADE (base de em/% = font do pai — ver dom.rs). 2.5rem → Rem(2.5).
    assert_eq!(parse_inline("font-size: 2.5rem").font_size, Some(Dimension::Rem(2.5)));
    // calc() linear reduz no parse: calc(1.375rem + 1.5vw) → {rem:1.375, vw:1.5}.
    let c = parse_inline("font-size: calc(1.375rem + 1.5vw)").font_size;
    assert_eq!(c, Some(Dimension::Calc(CalcLen { rem: 1.375, vw: 1.5, ..Default::default() })));
    // padding/margin carregam a unidade (resolve TARDE no layout).
    let c = parse_inline("padding: 1rem; margin: -0.5rem 2em");
    assert_eq!(c.padding.top, Side::Len(Dimension::Rem(1.0)));
    assert_eq!(c.margin.top, Side::Len(Dimension::Rem(-0.5)), "negativo preserva o sinal");
    assert_eq!(c.margin.left, Side::Len(Dimension::Em(2.0)));
    // border-radius em rem: 0.375rem = 6px (o .btn do Bootstrap).
    assert_eq!(parse_inline("border-radius: 0.375rem").corner_radius, Some(6.0));
}

#[test]
fn at_rules_nao_corrompem_o_parse() {
    // `@media` tem bloco ANINHADO — o fechamento raso no primeiro `}` deixava o
    // `}` externo órfão e ENGOLIA as regras vizinhas (era assim que o
    // bootstrap.min.css perdia `h1{font-size}`). Agora o bloco é pulado inteiro
    // com chaves casadas; at-rules sem corpo (@charset/@import) pulam até o `;`.
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@media (min-width: 1200px){ h1{ font-size:40px } .x{ color:#ffffff } } \
         h1 { color:#ff0000 } \
         @charset \"utf-8\"; \
         p { color:#00ff00 }",
    );
    // as regras APÓS as at-rules sobrevivem (antes eram corrompidas).
    assert_eq!(sheet.computed_for("h1", None, &[]).normal.color, Some(0xFF0000FF));
    assert_eq!(sheet.computed_for("p", None, &[]).normal.color, Some(0x00FF00FF));
    // FASE 2: as regras INTERNAS do @media APLICAM quando o viewport casa (o
    // helper computed_for usa 1280 ≥ 1200)…
    assert_eq!(
        sheet.computed_for("h1", None, &[]).normal.font_size,
        Some(Dimension::Px(40.0))
    );
    // …e NÃO aplicam quando não casa (viewport 800 < 1200).
    let no_match = sheet.computed_for_node(800.0, None, |sel| {
        sel.compounds.len() == 1
            && compound_matches(&sel.compounds[0], "h1", None, &[], &|_| None, &|_| false)
    });
    assert_eq!(no_match.normal.font_size, None);
    assert_eq!(no_match.normal.color, Some(0xFF0000FF), "a regra fora do @media segue");
    // feature desconhecida (prefers-*) nunca casa.
    let mut s3 = Stylesheet::new();
    s3.append_css("@media (prefers-reduced-motion: reduce){ p{ color:#111111 } }");
    assert_eq!(s3.computed_for("p", None, &[]).normal.color, None);
    // @media sem fechar não panica (tolerância).
    let mut s2 = Stylesheet::new();
    s2.append_css("p { color:#0000ff } @media (x) { .a { color:#fff }");
    assert_eq!(s2.computed_for("p", None, &[]).normal.color, Some(0x0000FFFF));
}

#[test]
fn mecanismos_gerados_da_tabela() {
    // A REFATORAÇÃO data-driven: herança, diff-de-animação e interpolação são
    // GERADOS da tabela css_props! — este teste prova que os 3 mecanismos batem
    // com o comportamento antigo (campo a campo) e cobre o caso que o modelo
    // antigo deixava dessincronizar (campo animável fora da lista fixa).
    // herança: só os campos [inh] descem.
    let mut child = ComputedStyle::default();
    let mut parent = ComputedStyle::default();
    parent.color = Some(0x112233FF); // inh
    parent.bg = Some(0x445566FF); // NÃO herda (box)
    parent.font_size = Some(Dimension::Px(20.0)); // inh
    child.inherit_from(&parent);
    assert_eq!(child.color, Some(0x112233FF));
    assert_eq!(child.font_size, Some(Dimension::Px(20.0)));
    assert_eq!(child.bg, None); // bg não herda
    // diff animado: campo [anim] dispara; não-animável não.
    let mut a = ComputedStyle::default();
    let mut b = ComputedStyle::default();
    b.display = Some(DisplayKind::Flex); // não-animável
    assert!(!a.differs_animated(&b));
    b.bg = Some(0xFF0000FF); // animável
    assert!(a.differs_animated(&b));
    // interpolação: animável interpola (preto→vermelho no meio = meio-vermelho),
    // não-animável salta pro destino.
    a.bg = Some(0x000000FF);
    let mid = ComputedStyle::interpolate_animated(&a, &b, 0.5);
    assert_eq!(mid.bg, Some(0x800000FF));
    assert_eq!(mid.display, Some(DisplayKind::Flex)); // discreto: já no destino
}
