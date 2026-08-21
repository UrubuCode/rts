//! O PARSER de um `<style>`: regras, at-rules, `@keyframes` e os comentários
//!
//! Extraído de `stylesheet.rs` sem alterar uma linha.

use super::*;

/// Parseia o corpo de um `<style>` numa lista de [`Rule`] (sem `order`, que o
/// `Stylesheet::append_css` atribui). Robusto: comentários `/* */` são removidos;
/// regras malformadas (sem `{`/`}`, seletor desconhecido) são puladas sem panicar;
/// `a, b { ... }` vira uma regra por seletor (mesmas declarações).
pub fn parse_rules(css: &str) -> Vec<Rule> {
    let _phase = crate::metrics::phases::scope("css-parse-rules");
    let css = {
        let _strip = crate::metrics::phases::scope("css-strip-comments");
        strip_css_comments(css)
    };
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // AT-RULES: blocos ANINHADOS (`@media (...) { .x { … } }`) — o fechamento
        // raso no primeiro `}` CORROMPIA o parse (o `}` órfão engolia as regras
        // vizinhas do bootstrap.min.css). `@media` agora é AVALIADO (fase 2): a
        // condição vira [`MediaQuery`] e as regras INTERNAS entram com ela anexada
        // (a cascade filtra pelo viewport). Os demais at-rules com bloco
        // (`@supports`/`@font-face`/…) são pulados com chaves casadas;
        // `@import`/`@charset` (sem corpo) pulam até o `;`. `@keyframes` nunca
        // chega aqui (extraído antes em append_css).
        let ws = css[i..]
            .find(|c: char| !c.is_whitespace())
            .map(|r| i + r)
            .unwrap_or(css.len());
        if css[ws..].starts_with('@') {
            let brace_rel = css[ws..].find('{');
            let semi_rel = css[ws..].find(';');
            match (brace_rel, semi_rel) {
                // `;` antes do `{` (ou sem bloco): at-rule sem corpo → pula até o `;`.
                (None, Some(s)) => i = ws + s + 1,
                (Some(b), Some(s)) if s < b => i = ws + s + 1,
                // com bloco: @media parseia o corpo recursivo; o resto pula casado.
                (Some(b), _) => {
                    let body_start = ws + b + 1;
                    match find_matching_brace(&css[body_start..]) {
                        Some(end) => {
                            let header = &css[ws..ws + b];
                            let inner_css = &css[body_start..body_start + end];
                            if let Some(cond) = header.strip_prefix("@media") {
                                let outer = MediaQuery::parse(cond.trim());
                                for mut rule in parse_rules(inner_css) {
                                    crate::bump!(css_media_rules);
                                    // aninhamento @media-em-@media: AND das queries.
                                    rule.media = Some(match rule.media {
                                        Some(inner) => inner.and(outer),
                                        None => outer,
                                    });
                                    rules.push(rule);
                                }
                            } else if let Some(cond) =
                                header.trim_start().strip_prefix("@supports")
                            {
                                // AVALIADO (não transparente): era o único sítio
                                // do motor onde aplicávamos regras que o Chrome
                                // não aplica de todo — e uma folha real escreve
                                // os DOIS ramos de um par exclusivo, contando
                                // que o motor escolha um. Ver `supports.rs`,
                                // incluindo o que "suportamos" quer dizer aqui.
                                if super::supports::avalia(cond) {
                                    for rule in parse_rules(inner_css) {
                                        rules.push(rule);
                                    }
                                } else {
                                    crate::bump!(css_supports_rejeitado);
                                }
                            } else if header.trim_start().starts_with("@layer") {
                                // `@layer <nome> { ... }` / `@layer { ... }` (anônimo) e
                                // `@supports (...) { ... }`: TRANSPARENTES para o matching.
                                // As regras internas entram no nível atual (a precedência
                                // fina entre camadas / a avaliação real do @supports são
                                // refinamento posterior — o essencial é APLICAR as regras,
                                // senão o Tailwind v4, que embrulha TUDO em @layer, some).
                                // As camadas do Tailwind vêm em ordem correta no arquivo, e
                                // o `order` da cascade (posição) já as desempata bem.
                                for rule in parse_rules(inner_css) {
                                    rules.push(rule);
                                }
                            }
                            i = body_start + end + 1;
                        }
                        None => break, // bloco não fecha: nada mais a parsear.
                    }
                }
                (None, None) => break,
            }
            continue;
        }
        // Acha o `{` que abre o bloco de declarações.
        let Some(brace) = css[i..].find('{').map(|r| i + r) else {
            break;
        };
        let selectors_raw = css[i..brace].trim();
        // Acha o `}` que fecha; sem fechar, vai até o fim (tolerante).
        let close = css[brace + 1..].find('}').map(|r| brace + 1 + r);
        let (body, next) = match close {
            Some(end) => (&css[brace + 1..end], end + 1),
            None => (&css[brace + 1..], css.len()),
        };
        let decls = parse_inline_block(body); // reusa o parser de declarações (normal+important).
        // `a, b, .c { }` → uma regra por seletor (lista separada por vírgula).
        let _emit = crate::metrics::phases::scope("css-rule-emit");
        let decls = std::rc::Rc::new(RuleDecls::from_block(decls));
        // `content` é lido do corpo CRU, e não do bloco já parseado, porque não
        // é uma propriedade do `ComputedStyle` — o parser de declarações
        // descarta-a. Só se procura quando algum seletor da regra tem
        // pseudo-elemento: numa folha real são umas dezenas de regras em
        // milhares, e varrer o corpo de todas custaria em cada página.
        let content = std::cell::OnceCell::new();
        // Os contadores são lidos de TODA regra e não só das de pseudo-elemento
        // (o `counter-increment` que numera as referências está num `<li>`), mas
        // o `parse_ops` sai num `contains("counter-")` antes de dividir o corpo,
        // que é o mesmo corte barato que o `content` obtém pela guarda do
        // pseudo. `Rc` porque `a, b { counter-increment:x }` é uma regra por
        // seletor e as duas partilham o valor.
        let counters = crate::counters::parse_ops(body).map(std::rc::Rc::new);
        // A vírgula que separa a LISTA é a de topo. `split(',')` cru cortava
        // dentro de `:is(.a, .b)` e de `[data-x="a,b"]`, produzindo dois pedaços
        // que não parseiam — a regra desaparecia e a contagem de recusadas
        // culpava o seletor, não o corte.
        for sel_str in super::selector::split_top_level_commas(selectors_raw) {
            if let Some(selector) = ComplexSelector::parse(sel_str) {
                let content = selector.pseudo_element.and_then(|_| {
                    content
                        .get_or_init(|| content_do_corpo(body).map(std::rc::Rc::new))
                        .clone()
                });
                rules.push(Rule {
                    selector,
                    decls: std::rc::Rc::clone(&decls),
                    order: 0,
                    media: None,
                    content,
                    counters: counters.clone(),
                });
            } else if !sel_str.trim().is_empty() {
                // Um seletor que o parser recusa é uma regra que a página tem e o
                // motor NÃO aplica — a diferença mais comum entre "parece o
                // Chrome" e "não parece", e invisível sem contá-la.
                crate::bump!(css_rules_dropped);
                crate::bump!(selector_parse_failures);
                crate::note!("seletor-recusado", sel_str.trim().to_string());
            }
        }
        i = next;
    }
    rules
}

/// Acha o índice do `}` que fecha o bloco iniciado APÓS o `{` já consumido, contando
/// o aninhamento (`@keyframes` tem `{}` por stop). `None` se não fecha.
pub(in crate::style::stylesheet) fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parseia o corpo de um `@keyframes`: `0% { ... } 50% { ... } to { ... }` → stops
/// ordenados por offset. `from`=0%, `to`=100%. Cada stop reusa o parser de declarações.
/// Acha a declaração `content` no corpo CRU de uma regra e parseia-a.
///
/// Percorre por `;` de topo em vez de usar uma expressão sobre o texto todo
/// porque o valor pode conter `;` dentro de aspas (`content: ";"`), e porque
/// `font-content`/`--content` não são a propriedade procurada — o nome tem de
/// casar inteiro.
fn content_do_corpo(body: &str) -> Option<crate::pseudo::Content> {
    let mut achado = None;
    for decl in split_top_level_semicolons(body) {
        let Some((nome, valor)) = decl.split_once(':') else {
            continue;
        };
        if !nome.trim().eq_ignore_ascii_case("content") {
            continue;
        }
        // A ÚLTIMA declaração do bloco vence, como em qualquer bloco CSS; uma
        // que não saibamos parsear não apaga a anterior que sabíamos.
        if let Some(c) = crate::pseudo::parse_content(valor.trim_end_matches("!important").trim()) {
            achado = Some(c);
        }
    }
    achado
}

/// Divide um corpo de declarações nos `;` de topo (fora de aspas e parênteses).
pub(crate) fn split_top_level_semicolons(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut par, mut inicio) = (0i32, 0usize);
    let mut aspa: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if aspa.is_none() => aspa = Some(c),
            q if Some(q) == aspa => aspa = None,
            '(' if aspa.is_none() => par += 1,
            ')' if aspa.is_none() => par -= 1,
            ';' if aspa.is_none() && par == 0 => {
                out.push(&s[inicio..i]);
                inicio = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[inicio..]);
    out
}

pub(in crate::style::stylesheet) fn parse_keyframe_body(body: &str) -> crate::anim::Keyframes {
    let mut stops = Vec::new();
    let mut rest = body;
    loop {
        let Some(brace) = rest.find('{') else { break };
        let selector = rest[..brace].trim();
        let Some(close_rel) = find_matching_brace(&rest[brace + 1..]) else {
            break;
        };
        let decl_body = &rest[brace + 1..brace + 1 + close_rel];
        let decls = parse_inline_block(decl_body);
        // o seletor de stop pode ser uma lista: `0%, 50%`.
        for tok in selector.split(',') {
            if let Some(offset) = parse_keyframe_offset(tok.trim()) {
                let mut style = decls.normal.clone();
                style.merge_over(&decls.important);
                stops.push(crate::anim::Keyframe { offset, style });
            }
        }
        rest = &rest[brace + 1 + close_rel + 1..];
    }
    stops.sort_by(|a, b| {
        a.offset
            .partial_cmp(&b.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    crate::anim::Keyframes { stops }
}

/// `0%`/`from`/`50%`/`100%`/`to` → offset ∈ [0,1]. `None` se inválido.
fn parse_keyframe_offset(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("from") {
        return Some(0.0);
    }
    if s.eq_ignore_ascii_case("to") {
        return Some(1.0);
    }
    s.strip_suffix('%')?
        .trim()
        .parse::<f32>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// Resolve uma declaração PENDENTE (`prop: …var()…`) contra as custom props do
/// elemento e aplica no estilo — re-parseando a declaração única pelo parser
/// normal (mantém TODO o vocabulário: cores, dimensões, shorthands…).
pub(crate) fn apply_resolved_decl(
    css: &mut ComputedStyle,
    prop: &str,
    raw: &str,
    vars: &std::collections::HashMap<String, String>,
) {
    let resolved = super::vars::substitute(raw, vars);
    if resolved.trim().is_empty() {
        return; // var() sem valor nem fallback: declaração inválida (spec: unset)
    }
    let mini = parse_inline_block(&format!("{prop}: {resolved}"));
    css.merge_over(&mini.normal);
}

/// Remove blocos de comentário `/* ... */` do CSS (um passe, tolerante a não-fechado).
pub(in crate::style::stylesheet) fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out, // comentário não fechado: descarta o resto.
        }
    }
    out.push_str(rest);
    out
}

// Testes da herança e da resolução de `var()` por elemento. Num ficheiro à
// parte (via `#[path]`) e não num `mod` de `style/mod.rs`: este ficheiro já
// está no seu teto de linhas e `mod.rs` está a ser editado noutra frente.
#[cfg(test)]
#[path = "vars_cascade_tests.rs"]
mod vars_cascade_tests;
