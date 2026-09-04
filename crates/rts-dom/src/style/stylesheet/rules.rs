//! O PARSER de um `<style>`: regras, at-rules, `@keyframes` e os comentários
//!
//! Extraído de `stylesheet.rs` sem alterar uma linha.

use super::*;

/// Faz o lowering da sintaxe CSS preservada para o IR da cascade.
///
/// O tokenizer/AST em `style::syntax` é agora a única etapa estrutural: este
/// módulo só decide quais at-rules o motor entende e converte selectors e
/// declarações para os tipos semânticos já usados pela cascade.
pub fn parse_rules(css: &str) -> Vec<Rule> {
    let ast = crate::style::syntax::StylesheetAst::parse(css);
    parse_rules_ast(&ast)
}

#[derive(Default)]
struct LayerState {
    names: Vec<String>,
}

impl LayerState {
    fn id(&mut self, name: &str) -> u32 {
        let name = if name.is_empty() {
            "<anonymous>"
        } else {
            name
        };
        if let Some((index, _)) = self
            .names
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.as_str() == name)
        {
            return index as u32;
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        id
    }
}

/// Faz o lowering semântico de um AST já tokenizado, evitando uma segunda
/// tokenização quando o chamador também precisa dos at-rules originais.
pub(in crate::style::stylesheet) fn parse_rules_ast(
    ast: &crate::style::syntax::StylesheetAst,
) -> Vec<Rule> {
    let mut layers = Vec::new();
    parse_rules_ast_with_layers(ast, &mut layers)
}

pub(in crate::style::stylesheet) fn parse_rules_ast_with_layers(
    ast: &crate::style::syntax::StylesheetAst,
    layer_names: &mut Vec<String>,
) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut layers = LayerState {
        names: std::mem::take(layer_names),
    };
    lower_items(&ast.items, None, None, &mut layers, &mut rules);
    *layer_names = layers.names;
    rules
}

fn lower_items(
    items: &[crate::style::syntax::AstItem],
    inherited_media: Option<MediaQuery>,
    inherited_layer: Option<u32>,
    layers: &mut LayerState,
    output: &mut Vec<Rule>,
) {
    for item in items {
        match item {
            crate::style::syntax::AstItem::QualifiedRule { prelude, block, .. } => {
                let selectors_raw: String = prelude
                    .iter()
                    .map(crate::style::syntax::ComponentValue::to_css_semantic)
                    .collect();
                let body = block.to_css_semantic();
                let specified = std::rc::Rc::new(crate::style::syntax::SpecifiedStyle::from_block(block));
                let source_declarations = std::rc::Rc::clone(&specified.declarations);
                let decls = std::rc::Rc::new(RuleDecls::from_block(lower_declarations(block)));
                let content = std::cell::OnceCell::new();
                let counters = crate::counters::parse_ops(&body).map(std::rc::Rc::new);
                for sel_str in super::selector::split_top_level_commas(&selectors_raw) {
                    if let Some(selector) = ComplexSelector::parse(sel_str) {
                        let content = selector.pseudo_element.and_then(|_| {
                            content
                                .get_or_init(|| parse_content_from_body(&body).map(std::rc::Rc::new))
                                .clone()
                        });
                        output.push(Rule {
                            selector,
                            specified: std::rc::Rc::clone(&specified),
                            source_declarations: std::rc::Rc::clone(&source_declarations),
                            layer: inherited_layer,
                            decls: std::rc::Rc::clone(&decls),
                            order: 0,
                            media: inherited_media,
                            content,
                            counters: counters.clone(),
                            is_ua: false,
                        });
                    } else if !sel_str.trim().is_empty() {
                        crate::bump!(css_rules_dropped);
                        crate::bump!(selector_parse_failures);
                        crate::note!("seletor-recusado", sel_str.trim().to_string());
                    }
                }
            }
            crate::style::syntax::AstItem::AtRule {
                name,
                prelude,
                block: Some(block),
                ..
            } => {
                let cond: String = prelude
                    .iter()
                    .map(crate::style::syntax::ComponentValue::to_css_semantic)
                    .collect();
                match name.to_ascii_lowercase().as_str() {
                    "media" => {
                        let media = MediaQuery::parse(cond.trim());
                        let media = combine_media(inherited_media, media);
                        with_nested_items(block, |items| {
                            lower_items(items, media, inherited_layer, layers, output)
                        });
                    }
                    "supports" => {
                        if super::supports::avalia(cond.trim()) {
                            with_nested_items(block, |items| {
                                lower_items(items, inherited_media, inherited_layer, layers, output)
                            });
                        } else {
                            crate::bump!(css_supports_rejeitado);
                        }
                    }
                    "layer" => {
                        let layer = layers.id(cond.trim());
                        with_nested_items(block, |items| {
                            lower_items(items, inherited_media, Some(layer), layers, output)
                        });
                    }
                    _ => {}
                }
            }
            crate::style::syntax::AstItem::AtRule {
                name,
                prelude,
                block: None,
                ..
            } if name.eq_ignore_ascii_case("layer") => {
                let cond: String = prelude
                    .iter()
                    .map(crate::style::syntax::ComponentValue::to_css_semantic)
                    .collect();
                for name in cond.split(',') {
                    layers.id(name.trim());
                }
            }
            _ => {}
        }
    }
}

fn lower_declarations(block: &crate::style::syntax::BlockAst) -> DeclBlock {
    let mut lowered = DeclBlock::default();
    for declaration in block.declarations() {
        let value = declaration
            .value
            .iter()
            .map(crate::style::syntax::ComponentValue::to_css_semantic)
            .collect::<String>();
        crate::style::parse::apply_specified_declaration(
            &mut lowered,
            &declaration.name,
            &value,
            declaration.important,
        );
    }
    lowered
}

fn with_nested_items(
    block: &crate::style::syntax::BlockAst,
    f: impl FnOnce(&[crate::style::syntax::AstItem]),
) {
    if let Some(nested) = &block.nested {
        f(&nested.items);
    } else {
        let nested = crate::style::syntax::StylesheetAst::parse(&block.to_css());
        f(&nested.items);
    }
}

fn combine_media(outer: Option<MediaQuery>, inner: MediaQuery) -> Option<MediaQuery> {
    Some(match outer {
        Some(outer) => inner.and(outer),
        None => inner,
    })
}

/// Converte os stops de um `@keyframes` AST para o tipo consumido pela animação.
pub(in crate::style::stylesheet) fn parse_keyframe_ast(
    block: &crate::style::syntax::BlockAst,
) -> crate::anim::Keyframes {
    let mut stops = Vec::new();
    with_nested_items(block, |items| {
        for item in items {
            let crate::style::syntax::AstItem::QualifiedRule {
                prelude, block, ..
            } = item
            else {
                continue;
            };
            let selector: String = prelude
                .iter()
                .map(crate::style::syntax::ComponentValue::to_css_semantic)
                .collect();
            let decls = lower_declarations(block);
            for token in selector.split(',') {
                if let Some(offset) = parse_keyframe_offset(token.trim()) {
                    let mut style = decls.normal.clone();
                    style.merge_over(&decls.important);
                    stops.push(crate::anim::Keyframe { offset, style });
                }
            }
        }
    });
    stops.sort_by(|a, b| {
        a.offset
            .partial_cmp(&b.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    crate::anim::Keyframes { stops }
}


/// Parseia o corpo de um `@keyframes`: `0% { ... } 50% { ... } to { ... }` → stops
/// ordenados por offset. `from`=0%, `to`=100%. Cada stop reusa o parser de declarações.
/// Acha a declaração `content` no corpo CRU de uma regra e parseia-a.
///
/// Percorre por `;` de topo em vez de usar uma expressão sobre o texto todo
/// porque o valor pode conter `;` dentro de aspas (`content: ";"`), e porque
/// `font-content`/`--content` não são a propriedade procurada — o nome tem de
/// casar inteiro.
fn parse_content_from_body(body: &str) -> Option<crate::pseudo::Content> {
    let mut found = None;
    for decl in split_top_level_semicolons(body) {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("content") {
            continue;
        }
        // A ÚLTIMA declaração do bloco vence, como em qualquer bloco CSS; uma
        // que não saibamos parsear não apaga a anterior que sabíamos.
        if let Some(content) = crate::pseudo::parse_content(value.trim_end_matches("!important").trim()) {
            found = Some(content);
        }
    }
    found
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
    let mut block = DeclBlock::default();
    block.normal = css.clone();
    crate::style::parse::apply_specified_declaration(&mut block, prop, &resolved, false);
    *css = block.normal;
}


// Testes da herança e da resolução de `var()` por elemento. Num ficheiro à
// parte (via `#[path]`) e não num `mod` de `style/mod.rs`: este ficheiro já
// está no seu teto de linhas e `mod.rs` está a ser editado noutra frente.
#[cfg(test)]
#[path = "vars_cascade_tests.rs"]
mod vars_cascade_tests;
