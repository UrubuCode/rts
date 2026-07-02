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

/// A condição de um bloco `@media`, avaliada contra o VIEWPORT (fase 2 — antes
/// o bloco inteiro era pulado). V1 honesta: só `min-width`/`max-width` (px, e
/// em/rem ×16) e os keywords neutros `screen`/`all`/`only`; qualquer feature
/// desconhecida (`prefers-reduced-motion`, `orientation`, `print`, `not …`)
/// torna a query SEMPRE-FALSA (conservador — para `prefers-reduced-motion:
/// reduce` é exatamente o default desejado).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct MediaQuery {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    /// feature não suportada na condição → a query nunca casa.
    pub always_false: bool,
}

impl MediaQuery {
    /// Parseia a condição após o `@media` (`(min-width: 768px)`,
    /// `screen and (max-width: 991.98px)`, lista por vírgula = OR de queries).
    pub fn parse(cond: &str) -> MediaQuery {
        // lista por vírgula é OR — v1: se QUALQUER query da lista for só de
        // width, usamos a primeira suportada; senão always_false. (Bootstrap não
        // usa listas em @media; mantém simples.)
        let first = cond.split(',').next().unwrap_or("");
        let mut q = MediaQuery::default();
        for term in first.split(" and ") {
            let t = term.trim().to_ascii_lowercase();
            if t.is_empty() || t == "screen" || t == "all" || t == "only screen" || t == "only all" {
                continue; // keywords neutros
            }
            // `(feature: valor)`
            let inner = t.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(&t);
            let Some((feat, val)) = inner.split_once(':') else {
                q.always_false = true; // `not`, `print`, feature sem valor…
                continue;
            };
            let px = parse_media_len(val.trim());
            match (feat.trim(), px) {
                ("min-width", Some(v)) => q.min_width = Some(v),
                ("max-width", Some(v)) => q.max_width = Some(v),
                _ => q.always_false = true,
            }
        }
        q
    }

    /// `true` se a query casa o viewport dado.
    pub fn matches(&self, viewport_w: f32) -> bool {
        if self.always_false {
            return false;
        }
        if let Some(mn) = self.min_width {
            if viewport_w < mn {
                return false;
            }
        }
        if let Some(mx) = self.max_width {
            if viewport_w > mx {
                return false;
            }
        }
        true
    }

    /// Combina com uma query EXTERNA (aninhamento `@media` dentro de `@media`):
    /// AND dos limites.
    fn and(self, outer: MediaQuery) -> MediaQuery {
        MediaQuery {
            min_width: match (self.min_width, outer.min_width) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            max_width: match (self.max_width, outer.max_width) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            always_false: self.always_false || outer.always_false,
        }
    }
}

/// Um comprimento de condição de media: px (ou em/rem ×16, como o browser).
fn parse_media_len(v: &str) -> Option<f32> {
    let low = v.to_ascii_lowercase();
    if let Some(n) = low.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = low.strip_suffix("rem").or_else(|| low.strip_suffix("em")) {
        return n.trim().parse::<f32>().ok().map(|x| x * 16.0);
    }
    low.parse().ok()
}

/// Uma regra do stylesheet: um seletor + as declarações já parseadas (separadas
/// nas camadas normal/important da cascade). A ordem de declaração no fonte
/// (`order`) desempata especificidades iguais.
#[derive(Clone, PartialEq, Debug)]
pub struct Rule {
    pub selector: Selector,
    pub decls: DeclBlock,
    /// Posição da regra no fonte (0-based) — desempate da cascade.
    pub order: u32,
    /// A condição `@media` que ENVOLVE a regra (None = sempre aplica). Avaliada
    /// contra o viewport na cascade ([`Stylesheet::computed_for_node`]).
    pub media: Option<MediaQuery>,
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
    /// Declarações de CUSTOM PROPERTIES (`--nome: valor`) do bloco, na ordem do
    /// fonte, com o valor CRU (pode conter `var()` aninhado). Participam da
    /// cascade por elemento (#1779). Importância ignorada na v1 (documentado).
    pub custom: Vec<(String, String)>,
    /// Declarações PENDENTES — o valor contém `var()` e só resolve POR ELEMENTO
    /// (contra as custom props computadas dele): `(prop, valor-cru, important)`.
    /// Resolvidas na posição da regra em [`Stylesheet::computed_for_node`].
    pub pending: Vec<(String, String, bool)>,
}

impl DeclBlock {
    /// `true` se nenhuma das camadas tem qualquer propriedade setada.
    pub fn is_empty(&self) -> bool {
        self.normal == ComputedStyle::default()
            && self.important == ComputedStyle::default()
            && self.custom.is_empty()
            && self.pending.is_empty()
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
        // (var()/custom properties agora resolvem POR ELEMENTO na cascade — #1779;
        // o antigo passe textual GLOBAL daqui foi removido.)
        // 1) extrai e remove os blocos @keyframes (guarda por nome).
        let css_without_kf = self.extract_keyframes(css);
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
    pub fn computed_for_node(
        &self,
        viewport_w: f32,
        vars: Option<&std::collections::HashMap<String, String>>,
        matches: impl Fn(&ComplexSelector) -> bool,
    ) -> DeclBlock {
        let mut matched: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.media.map(|m| m.matches(viewport_w)).unwrap_or(true))
            .filter(|r| matches(&r.selector))
            .collect();
        matched.sort_by_key(|r| (r.selector.specificity(), r.order));
        let mut out = DeclBlock::default();
        for r in &matched {
            out.normal.merge_over(&r.decls.normal);
            // declarações PENDENTES (`prop: …var(--x)…`) resolvem AQUI, na posição
            // da regra na cascade, contra as custom props do ELEMENTO (#1779).
            if let Some(v) = vars {
                for (prop, raw, important) in &r.decls.pending {
                    if !important {
                        apply_resolved_decl(&mut out.normal, prop, raw, v);
                    }
                }
            }
        }
        for r in &matched {
            out.important.merge_over(&r.decls.important);
            if let Some(v) = vars {
                for (prop, raw, important) in &r.decls.pending {
                    if *important {
                        apply_resolved_decl(&mut out.important, prop, raw, v);
                    }
                }
            }
        }
        out
    }

    /// PASS A do var() por elemento (#1779): coleta as declarações de CUSTOM
    /// PROPERTIES (`--x: v`) das regras que casam, na ordem da cascade
    /// (especificidade + ordem — a última vence por nome no consumidor).
    pub fn custom_for_node(
        &self,
        viewport_w: f32,
        matches: impl Fn(&ComplexSelector) -> bool,
    ) -> Vec<(String, String)> {
        let mut matched: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| !r.decls.custom.is_empty())
            .filter(|r| r.media.map(|m| m.matches(viewport_w)).unwrap_or(true))
            .filter(|r| matches(&r.selector))
            .collect();
        matched.sort_by_key(|r| (r.selector.specificity(), r.order));
        matched.iter().flat_map(|r| r.decls.custom.iter().cloned()).collect()
    }

    /// Conveniência: computa o estilo para um elemento dado SÓ tag/id/classes (sem
    /// árvore). Casa apenas seletores de UM compound (sem combinadores nem pseudo/
    /// atributo dependentes de posição — esses retornam false aqui). Usado em testes
    /// e onde o contexto de árvore não importa.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let no_attr = |_: &str| None;
        let no_pseudo = |_: &PseudoClass| false;
        // viewport de referência 1280 (helper sem árvore/viewport — testes);
        // sem vars (pendentes com var() não resolvem aqui).
        self.computed_for_node(1280.0, None, |sel| {
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
        // AT-RULES: blocos ANINHADOS (`@media (...) { .x { … } }`) — o fechamento
        // raso no primeiro `}` CORROMPIA o parse (o `}` órfão engolia as regras
        // vizinhas do bootstrap.min.css). `@media` agora é AVALIADO (fase 2): a
        // condição vira [`MediaQuery`] e as regras INTERNAS entram com ela anexada
        // (a cascade filtra pelo viewport). Os demais at-rules com bloco
        // (`@supports`/`@font-face`/…) são pulados com chaves casadas;
        // `@import`/`@charset` (sem corpo) pulam até o `;`. `@keyframes` nunca
        // chega aqui (extraído antes em append_css).
        let ws = css[i..].find(|c: char| !c.is_whitespace()).map(|r| i + r).unwrap_or(css.len());
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
                            if let Some(cond) = header.strip_prefix("@media") {
                                let outer = MediaQuery::parse(cond.trim());
                                for mut rule in parse_rules(&css[body_start..body_start + end]) {
                                    // aninhamento @media-em-@media: AND das queries.
                                    rule.media = Some(match rule.media {
                                        Some(inner) => inner.and(outer),
                                        None => outer,
                                    });
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
                rules.push(Rule { selector, decls: decls.clone(), order: 0, media: None });
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
