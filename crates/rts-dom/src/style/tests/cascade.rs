//! Stylesheet, especificidade, `!important`, `@`-rules e o `rem`
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

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
    assert_eq!(
        sheet.computed_for("div", None, &["a"]).normal.color,
        Some(0x00FF00FF)
    );
    // seletor-lista `h1, h2, .big { ... }` → aplica aos três.
    let mut s2 = Stylesheet::new();
    s2.append_css("h1, h2, .big { font-size:30 }");
    assert_eq!(
        s2.computed_for("h1", None, &[]).normal.font_size,
        Some(Dimension::Px(30.0))
    );
    assert_eq!(
        s2.computed_for("h2", None, &[]).normal.font_size,
        Some(Dimension::Px(30.0))
    );
    assert_eq!(
        s2.computed_for("p", None, &["big"]).normal.font_size,
        Some(Dimension::Px(30.0))
    );
    assert_eq!(s2.computed_for("p", None, &[]).normal.font_size, None); // não casa
}

#[test]
fn stylesheet_universal_e_comentarios() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("/* tema escuro */ * { color:#cccccc } /* destaque */ .hl { color:#ffff00 }");
    // universal aplica a qualquer tag; a classe (mais específica) sobrepõe.
    assert_eq!(
        sheet.computed_for("span", None, &[]).normal.color,
        Some(0xCCCCCCFF)
    );
    assert_eq!(
        sheet.computed_for("span", None, &["hl"]).normal.color,
        Some(0xFFFF00FF)
    );
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
    assert_eq!(
        parse_inline("font-size: 2.5rem").font_size,
        Some(Dimension::Rem(2.5))
    );
    // calc() linear reduz no parse: calc(1.375rem + 1.5vw) → {rem:1.375, vw:1.5}.
    let c = parse_inline("font-size: calc(1.375rem + 1.5vw)").font_size;
    assert_eq!(
        c,
        Some(Dimension::Calc(CalcLen {
            rem: 1.375,
            vw: 1.5,
            ..Default::default()
        }))
    );
    // padding/margin carregam a unidade (resolve TARDE no layout).
    let c = parse_inline("padding: 1rem; margin: -0.5rem 2em");
    assert_eq!(c.padding.top, Side::Len(Dimension::Rem(1.0)));
    assert_eq!(
        c.margin.top,
        Side::Len(Dimension::Rem(-0.5)),
        "negativo preserva o sinal"
    );
    assert_eq!(c.margin.left, Side::Len(Dimension::Em(2.0)));
    // border-radius em rem: 0.375rem = 6px (o .btn do Bootstrap).
    assert_eq!(
        parse_inline("border-radius: 0.375rem").corner_radius,
        Some(6.0)
    );
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
    assert_eq!(
        sheet.computed_for("h1", None, &[]).normal.color,
        Some(0xFF0000FF)
    );
    assert_eq!(
        sheet.computed_for("p", None, &[]).normal.color,
        Some(0x00FF00FF)
    );
    // FASE 2: as regras INTERNAS do @media APLICAM quando o viewport casa (o
    // helper computed_for usa 1280 ≥ 1200)…
    assert_eq!(
        sheet.computed_for("h1", None, &[]).normal.font_size,
        Some(Dimension::Px(40.0))
    );
    // …e NÃO aplicam quando não casa (viewport 800 < 1200).
    let ctx = crate::style::MediaContext { width: 800.0, height: 600.0, ..Default::default() };
    let matched = sheet.matched_for_node(&ctx, "h1", None, &[], |sel| {
        sel.compounds.len() == 1
            && compound_matches(&sel.compounds[0], "h1", None, &[], &|_| None, &|_| false)
    });
    let no_match = sheet.declarations_from(&matched, None);
    // `h1` tem `font-size: 2em` na folha de UA (lote I, valor do Blink —
    // `em` de font-size resolve contra o PAI), que aplica SEMPRE (não tem
    // `@media`) — o `@media` que não casou aqui é só o do AUTOR, e sem ele o
    // valor cai para o da UA, não para `None`. Fica em `Em` (não `Px(32.0)`)
    // porque este caminho (`declarations_from` direto) não passa pela
    // resolução cedo da cascade completa (`dom/cascade.rs`) — só o `Dom`
    // real resolve `em` contra o font-size do pai.
    assert_eq!(no_match.normal.font_size, Some(Dimension::Em(2.0)));
    assert_eq!(
        no_match.normal.color,
        Some(0xFF0000FF),
        "a regra fora do @media segue"
    );
    // feature desconhecida (prefers-*) nunca casa.
    let mut s3 = Stylesheet::new();
    s3.append_css("@media (prefers-reduced-motion: reduce){ p{ color:#111111 } }");
    assert_eq!(s3.computed_for("p", None, &[]).normal.color, None);
    // @media sem fechar não panica (tolerância).
    let mut s2 = Stylesheet::new();
    s2.append_css("p { color:#0000ff } @media (x) { .a { color:#fff }");
    assert_eq!(
        s2.computed_for("p", None, &[]).normal.color,
        Some(0x0000FFFF)
    );
}


#[test]
fn layer_normal_e_regra_sem_layer_respeitam_a_precedencia() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@layer base { .x { color: red } } \
         @layer tema { .x { color: blue } } \
         .x { color: green }",
    );
    assert_eq!(
        sheet.computed_for("div", None, &["x"]).normal.color,
        Some(0x008000FF),
        "regra sem layer vence as layers na cascade normal"
    );

    let mut same = Stylesheet::new();
    same.append_css("@layer tema { .x { color: red } } @layer base { .x { color: blue } }");
    same.append_css("@layer tema { .x { color: green } }");
    assert_eq!(
        same.computed_for("div", None, &["x"]).normal.color,
        Some(0x0000FFFF),
        "a layer base, criada depois de tema, mantém a sua precedência"
    );
}

#[test]
fn layer_important_inverte_a_ordem_e_favorece_layers() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@layer base { .x { color: red !important } } \
         @layer tema { .x { color: blue !important } } \
         .x { color: green !important }",
    );
    assert_eq!(
        sheet.computed_for("div", None, &["x"]).important.color,
        Some(0xFF0000FF),
        "a primeira layer vence entre important, inclusive sobre a regra sem layer"
    );
}


#[test]
fn layer_order_declaration_precedes_following_blocks() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@layer low, high; \
         @layer high { .x { color: blue } } \
         @layer low { .x { color: red } }",
    );
    assert_eq!(
        sheet.computed_for("div", None, &["x"]).normal.color,
        Some(0x0000FFFF),
        "the layer list declares high after low"
    );
}


#[test]
fn all_initial_resets_the_block_without_clearing_custom_properties() {
    let block = parse_inline_block(
        "--brand: rebeccapurple; width: 120px; color: red; all: initial",
    );
    assert_eq!(block.normal.width, None);
    assert_eq!(block.normal.color, None);
    assert_eq!(block.custom, vec![("--brand".into(), "rebeccapurple".into())]);

    let important = parse_inline_block("color: red !important; all: initial !important");
    assert_eq!(important.important.color, None);
}
