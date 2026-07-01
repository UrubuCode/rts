//! O STYLESHEET de autor (`<style>`/`addStylesheet`) e a cascade: regras
//! ordenadas por (especificidade, ordem do fonte), camadas normal/`!important`
//! ([`DeclBlock`]) e a extração de `@keyframes`.
//!
//! ## Fidelidade à cascade CSS da MDN
//!
//! O modelo segue os estágios da cascade
//! (<https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade>):
//! 1. **Origem/importância:** normais UA(`defineStyle`) < `<style>` autor <
//!    `style=""` inline < override-por-nó; depois os `!important` por cima (autor <
//!    inline) — `!important` inverte a precedência de origem. Em `Dom::computed_style`.
//! 2. **Especificidade:** id(100) > classe(10) > tag(1) > universal(0) — em
//!    [`Selector::specificity`]; a regra mais específica sobrepõe.
//! 3. **Ordem do fonte:** empate de especificidade → a regra DECLARADA DEPOIS
//!    vence (campo `order`, desempate em [`computed_for`](Stylesheet::computed_for)).
//! 4. **Herança:** color/font-size descem do pai no render (`inherit_from` gerado
//!    pela tabela de propriedades); propriedade não-tocada fica `None`.
//!
//! **Seletores (#1752 — implementado):** compostos (`.a.b`, `p.card#x`),
//! combinadores (`div p`, `>`, `+`, `~`), atributo (`[a]`/`[a=v]`/`^=`/`$=`/`*=`/
//! `~=`/`|=`), pseudo estruturais (`:first-child`/`:last-child`/`:only-child`/
//! `:empty`/`:root`/`:nth-child`) e de estado-via-atributo (`:checked`/`:disabled`/
//! `:enabled`/`:required`), e lista por vírgula em querySelector/matches/closest.
//! **Cortes (não bugs):** `@layer`; pseudo de estado VIVO (`:hover`/`:focus`);
//! `:not()`/`:is()`/`:where()`/`:nth-of-type`; pseudo-elementos (`::before`); flag
//! de case `[a=v i]`; as keywords `inherit`/`initial`/`unset`/`revert`.
//! (`!important` — estágio 1 da MDN — JÁ é suportado.)

use super::parse::parse_inline_block;
use super::props::ComputedStyle;
use super::selector::{compound_matches, ComplexSelector, PseudoClass, Selector};

/// Uma regra do stylesheet: um seletor + as declarações já parseadas (separadas
/// nas camadas normal/important da cascade). A ordem de declaração no fonte
/// (`order`) desempata especificidades iguais.
#[derive(Clone, PartialEq, Debug)]
pub struct Rule {
    pub selector: Selector,
    pub decls: DeclBlock,
    /// Posição da regra no fonte (0-based) — desempate da cascade.
    pub order: u32,
}

/// Um bloco de declarações separado nas DUAS camadas de importância da cascade
/// (MDN estágio 1): `normal` e `important`. Na cascade os `normal` de todas as
/// regras são aplicados primeiro (por origem<especificidade<ordem); depois os
/// `important`, na mesma ordem — então `!important` SEMPRE vence o normal, mas
/// entre dois `important` a especificidade/ordem ainda desempata. Egui-free.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DeclBlock {
    /// Declarações normais (sem `!important`).
    pub normal: ComputedStyle,
    /// Declarações marcadas `!important` (vencem qualquer normal na cascade).
    pub important: ComputedStyle,
}

impl DeclBlock {
    /// `true` se nenhuma das camadas tem qualquer propriedade setada.
    pub fn is_empty(&self) -> bool {
        self.normal == ComputedStyle::default() && self.important == ComputedStyle::default()
    }
}

/// Um stylesheet de autor (o conteúdo de um `<style>`), já parseado em regras
/// ordenadas. Egui-free como o resto. É anexado ao `Dom` e consultado na cascade
/// de `computed_style`.
#[derive(Clone, Default, Debug)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// Os `@keyframes nome {...}` da página (#1776), por nome. Consultados pelo
    /// `advance` quando um nó tem `animation: nome ...`.
    pub keyframes: std::collections::HashMap<String, crate::anim::Keyframes>,
}

// PartialEq manual (Keyframes tem f32, não derivamos Eq; o diff de árvore só compara
// nodes+root, não o Stylesheet, então isto é só p/ testes).
impl PartialEq for Stylesheet {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}

impl Stylesheet {
    /// Stylesheet vazio (nenhuma regra).
    pub fn new() -> Stylesheet {
        Stylesheet { rules: Vec::new(), keyframes: std::collections::HashMap::new() }
    }

    /// `true` se não há nenhuma regra (atalho para o `computed_style` pular a
    /// cascade quando a página não tem `<style>`).
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Os `@keyframes` de um nome, se existir.
    pub fn keyframes(&self, name: &str) -> Option<&crate::anim::Keyframes> {
        self.keyframes.get(name)
    }

    /// Acrescenta as regras de mais um bloco `<style>` (uma página pode ter vários).
    /// EXTRAI os `@keyframes` primeiro (não são regras de seletor), depois as regras.
    pub fn append_css(&mut self, css: &str) {
        // 0) resolve custom properties + var() ANTES de tudo (versão temporária,
        //    textual e global — ver crate::cssvars e a issue #1779 de var() completo).
        let css = crate::cssvars::resolve(css);
        // 1) extrai e remove os blocos @keyframes (guarda por nome).
        let css_without_kf = self.extract_keyframes(&css);
        // 2) as regras normais do resto.
        let base = self.rules.len() as u32;
        for (i, rule) in parse_rules(&css_without_kf).into_iter().enumerate() {
            self.rules.push(Rule { order: base + i as u32, ..rule });
        }
    }

    /// Acha cada `@keyframes nome { ... }`, parseia os stops e guarda; devolve o CSS
    /// SEM os blocos de keyframes (p/ o parser de regras não tropeçar neles).
    fn extract_keyframes(&mut self, css: &str) -> String {
        let css = strip_css_comments(css);
        let mut out = String::new();
        let mut rest = css.as_str();
        while let Some(at) = rest.find("@keyframes") {
            out.push_str(&rest[..at]);
            let after = &rest[at + "@keyframes".len()..];
            // nome até o `{`.
            let Some(brace) = after.find('{') else { break };
            let name = after[..brace].trim().to_string();
            // acha o `}` que fecha o bloco (contando aninhamento, pois cada stop tem `{}`).
            let body_start = at + "@keyframes".len() + brace + 1;
            let Some(body_end) = find_matching_brace(&rest[body_start..]) else { break };
            let body = &rest[body_start..body_start + body_end];
            if !name.is_empty() {
                self.keyframes.insert(name, parse_keyframe_body(body));
            }
            rest = &rest[body_start + body_end + 1..];
        }
        out.push_str(rest);
        out
    }

    /// Computa o estilo de AUTOR para um elemento, aplicando as regras cujo seletor
    /// casa (decidido pelo `matches` fornecido — o `Dom` passa um que navega a
    /// árvore p/ os combinadores). Retorna um [`DeclBlock`] (normal + important
    /// separados). Dentro de cada camada, ordem de (especificidade, order) crescente.
    pub fn computed_for_node(&self, matches: impl Fn(&ComplexSelector) -> bool) -> DeclBlock {
        let mut matched: Vec<&Rule> = self.rules.iter().filter(|r| matches(&r.selector)).collect();
        matched.sort_by_key(|r| (r.selector.specificity(), r.order));
        let mut out = DeclBlock::default();
        for r in &matched {
            out.normal.merge_over(&r.decls.normal);
        }
        for r in &matched {
            out.important.merge_over(&r.decls.important);
        }
        out
    }

    /// Conveniência: computa o estilo para um elemento dado SÓ tag/id/classes (sem
    /// árvore). Casa apenas seletores de UM compound (sem combinadores nem pseudo/
    /// atributo dependentes de posição — esses retornam false aqui). Usado em testes
    /// e onde o contexto de árvore não importa.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let no_attr = |_: &str| None;
        let no_pseudo = |_: &PseudoClass| false;
        self.computed_for_node(|sel| {
            // só seletores de 1 compound casam sem a árvore.
            sel.compounds.len() == 1
                && compound_matches(&sel.compounds[0], tag, id, classes, &no_attr, &no_pseudo)
        })
    }
}

/// Parseia o corpo de um `<style>` numa lista de [`Rule`] (sem `order`, que o
/// `Stylesheet::append_css` atribui). Robusto: comentários `/* */` são removidos;
/// regras malformadas (sem `{`/`}`, seletor desconhecido) são puladas sem panicar;
/// `a, b { ... }` vira uma regra por seletor (mesmas declarações).
pub fn parse_rules(css: &str) -> Vec<Rule> {
    let css = strip_css_comments(css);
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Acha o `{` que abre o bloco de declarações.
        let Some(brace) = css[i..].find('{').map(|r| i + r) else { break };
        let selectors_raw = css[i..brace].trim();
        // Acha o `}` que fecha; sem fechar, vai até o fim (tolerante).
        let close = css[brace + 1..].find('}').map(|r| brace + 1 + r);
        let (body, next) = match close {
            Some(end) => (&css[brace + 1..end], end + 1),
            None => (&css[brace + 1..], css.len()),
        };
        let decls = parse_inline_block(body); // reusa o parser de declarações (normal+important).
        // `a, b, .c { }` → uma regra por seletor (lista separada por vírgula).
        for sel_str in selectors_raw.split(',') {
            if let Some(selector) = ComplexSelector::parse(sel_str) {
                rules.push(Rule { selector, decls: decls.clone(), order: 0 });
            }
        }
        i = next;
    }
    rules
}

/// Acha o índice do `}` que fecha o bloco iniciado APÓS o `{` já consumido, contando
/// o aninhamento (`@keyframes` tem `{}` por stop). `None` se não fecha.
fn find_matching_brace(s: &str) -> Option<usize> {
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
fn parse_keyframe_body(body: &str) -> crate::anim::Keyframes {
    let mut stops = Vec::new();
    let mut rest = body;
    loop {
        let Some(brace) = rest.find('{') else { break };
        let selector = rest[..brace].trim();
        let Some(close_rel) = find_matching_brace(&rest[brace + 1..]) else { break };
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
    stops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
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
    s.strip_suffix('%')?.trim().parse::<f32>().ok().map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// Remove blocos de comentário `/* ... */` do CSS (um passe, tolerante a não-fechado).
fn strip_css_comments(css: &str) -> String {
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
