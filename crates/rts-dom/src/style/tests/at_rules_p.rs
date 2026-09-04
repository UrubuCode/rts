//! Testes do lote P (§5.P): a gramática de `@media`, `@property` e o
//! contrato de ordenação que `@import` depende de (a expansão em si é da
//! FACHADA — `window.ts::__expandInlineStyleImports` — porque este crate não
//! tem I/O; ver a nota de decisão no ficheiro).

use super::*;

fn ctx(w: f32, h: f32) -> crate::style::MediaContext {
    crate::style::MediaContext { width: w, height: h, ..Default::default() }
}

// ── `@media`: cada feature, `not`, listas ───────────────────────────────────

#[test]
fn media_min_max_width_continuam_a_funcionar() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (min-width: 400px) and (max-width: 800px) { #a { color: red; } }");
    assert_eq!(sheet.rules.iter().filter(|r| !r.is_ua).count(), 1);
    let rule = sheet.rules.iter().find(|r| !r.is_ua).unwrap();
    let media = rule.media.as_ref().unwrap();
    assert!(media.matches(&ctx(600.0, 100.0)));
    assert!(!media.matches(&ctx(200.0, 100.0)));
    assert!(!media.matches(&ctx(900.0, 100.0)));
}

#[test]
fn media_intervalo_400_a_800_e_equivalente_ao_and_de_min_max() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (400px <= width <= 800px) { #a { color: red; } }");
    let rule = sheet.rules.iter().find(|r| !r.is_ua).unwrap();
    let media = rule.media.as_ref().unwrap();
    assert!(media.matches(&ctx(400.0, 0.0)));
    assert!(media.matches(&ctx(800.0, 0.0)));
    assert!(!media.matches(&ctx(399.0, 0.0)));
    assert!(!media.matches(&ctx(801.0, 0.0)));
}

#[test]
fn media_orientation_landscape_e_portrait() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (orientation: landscape) { #a { color: red; } }");
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    assert!(media.matches(&ctx(1280.0, 800.0))); // landscape (w >= h)
    assert!(!media.matches(&ctx(400.0, 800.0))); // portrait
}

#[test]
fn media_not_screen_nega_a_query_inteira() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media not screen and (max-width: 100px) { #a { color: red; } }");
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    // a 1280px, `max-width:100px` é falso → `not` inverte para verdadeiro.
    assert!(media.matches(&ctx(1280.0, 0.0)));
    // a 50px, `max-width:100px` é verdadeiro → `not` inverte para falso.
    assert!(!media.matches(&ctx(50.0, 0.0)));
}

#[test]
fn media_lista_por_virgula_e_or() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (min-width: 9999px), (max-width: 2000px) { #a { color: red; } }");
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    // a segunda query da lista casa; a OR inteira casa.
    assert!(media.matches(&ctx(1280.0, 0.0)));
    // nenhuma das duas casa.
    assert!(!media.matches(&ctx(5000.0, 0.0)));
}

#[test]
fn media_prefers_color_scheme_le_o_host() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (prefers-color-scheme: dark) { #a { color: red; } }");
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    let mut c = ctx(1280.0, 800.0);
    assert!(!media.matches(&c)); // default do host é Light
    c.prefers_color_scheme = crate::style::PrefersColorScheme::Dark;
    assert!(media.matches(&c));
}

#[test]
fn media_prefers_reduced_motion_le_o_host() {
    let mut sheet = Stylesheet::new();
    sheet.append_css("@media (prefers-reduced-motion: reduce) { #a { color: red; } }");
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    let mut c = ctx(1280.0, 800.0);
    assert!(!media.matches(&c));
    c.prefers_reduced_motion = true;
    assert!(media.matches(&c));
}

#[test]
fn media_aninhado_combina_por_and() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@media (min-width: 400px) { @media (max-width: 800px) { #a { color: red; } } }",
    );
    let media = sheet.rules.iter().find(|r| !r.is_ua).unwrap().media.clone().unwrap();
    assert!(media.matches(&ctx(600.0, 0.0)));
    assert!(!media.matches(&ctx(200.0, 0.0)));
    assert!(!media.matches(&ctx(900.0, 0.0)));
}

// ── `@property`: `initial-value` e `inherits` ───────────────────────────────

// `Stylesheet::computed_for` (o helper sem árvore usado nos outros testes
// deste ficheiro) documenta de propósito "sem vars — pendentes com var() não
// resolvem aqui" (`sheet.rs`, pré-existente a este lote): passa `None` a
// `declarations_from`, então uma declaração PENDENTE (qualquer `var(...)`)
// nunca é aplicada por ali, `@property` presente ou não. O primeiro
// commit deste teste usava esse helper e nunca tinha corrido de verdade — só
// a integração é que rodou `cargo test` e achou. `seed_defaults` só é
// alcançável pelo caminho REAL, `dom/cascade.rs`, que resolve `var()` por
// elemento — então o teste passa a exercer esse caminho, com uma página de
// verdade, como `afirmacoes_tests`/`auditoria_lote_b` já fazem para os
// outros cantos de `@media`/cascade.
#[test]
fn property_initial_value_preenche_var_sem_declaracao() {
    let dom = crate::dom::parse_html_to_dom(
        "<html><head><style>\
           @property --cor { syntax: \"<color>\"; inherits: true; initial-value: rgb(1,2,3); }\
           #a { background-color: var(--cor); }\
         </style></head><body><div id=\"a\"></div></body></html>",
    );
    let id = dom.query("#a").expect("#a existe");
    assert_eq!(
        dom.computed_property(id, "background-color"),
        "rgb(1, 2, 3)",
        "var(--cor) sem declaração alcançável tem de resolver para o initial-value"
    );
}

#[test]
fn property_inherits_false_e_reconhecido_no_registo() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(
        "@property --acento { syntax: \"<color>\"; inherits: false; initial-value: rgb(0,0,255); }",
    );
    assert!(!sheet.properties_registry().inherits("--acento"));
    let entry = sheet.registered_property("--acento").unwrap();
    assert_eq!(entry.initial_value.as_deref(), Some("rgb(0,0,255)"));
}

// ── at-rules reconhecidas e IGNORADAS de propósito (item 5) ─────────────────

// Lê o contador `css_at_rules_ignoradas`, que só existe com a feature: sem
// ela o `bump!` não expande e o teste falhava no job dom-tests do CI.
#[cfg(feature = "metrics")]
#[test]
fn at_rules_sem_pre_requisito_ficam_fora_da_cascade_mas_sao_contadas() {
    let mut sheet = Stylesheet::new();
    let antes = crate::metrics::snapshot().css_at_rules_ignoradas;
    sheet.append_css(
        "@container (min-width: 400px) { #a { color: red; } }
         @scope (main) { #b { color: red; } }
         @font-face { font-family: X; src: url(x.woff2); }
         @page { margin: 1in; }",
    );
    let depois = crate::metrics::snapshot().css_at_rules_ignoradas;
    assert_eq!(depois - antes, 4);
    // nenhuma virou `Rule` — a cascade não as aplica.
    assert_eq!(sheet.rules.iter().filter(|r| !r.is_ua).count(), 0);
}

// ── `@import`: o contrato de ORDEM que a expansão da fachada depende ───────
//
// `@import` em si não é lowered pelo Rust (decisão do lote P, §5.P item 3: a
// resolução — I/O, `fs`/`fetch` — fica em `window.ts`, que reescreve o texto
// do `<style>` ANTES da primeira cascade). O que este teste prova é o lado
// Rust do contrato: depois da fachada ter colado o texto importado ANTES do
// resto (é o que `__inlineImports` faz — copia até o `@import`, insere o
// conteúdo, continua), a regra da PÁGINA declarada depois vence por ORDEM em
// empate de especificidade — sem isto, "a regra do import entra antes"
// seria uma promessa vã.
#[test]
fn regra_da_pagina_vence_a_importada_por_ordem_apos_a_expansao_textual() {
    let mut sheet = Stylesheet::new();
    // simula o que `__inlineImports` produz: o conteúdo da folha importada
    // colado ANTES do resto do texto do `<style>`.
    sheet.append_css(
        "#alvo { background-color: rgb(255,0,0); }\n\
         #so-importado { background-color: rgb(0,128,0); }\n\
         #alvo { background-color: rgb(0,0,255); }",
    );
    assert_eq!(
        sheet.computed_for("div", Some("alvo"), &[]).normal.bg,
        Some(0x0000ffff)
    );
    assert_eq!(
        sheet.computed_for("div", Some("so-importado"), &[]).normal.bg,
        Some(0x008000ff)
    );
}
