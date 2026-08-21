//! **A pergunta "este nó casa com este seletor?" mora AQUI, e só aqui.**
//!
//! Antes da modularização a pergunta estava em SETE sítios do `dom.rs`, e três
//! deles não a respondiam — aproximavam-na. Uma aproximação isolada é barata e
//! correta; sete espalhadas por 5 974 linhas é como uma delas fica para trás
//! quando a pergunta muda. No `layout.rs`, a pergunta gémea ("é de bloco?",
//! escrita como "não é inline?") apareceu em CINCO sítios e cada correção
//! chegou a um deles de cada vez — quatro lotes, quatro medições, e a quinta
//! cópia estava escrita à mão dentro de um laço.
//!
//! As sete, e o que cada uma responde:
//!
//! | função | responde | exata ou aproximada |
//! |---|---|---|
//! | `matches` | uma LISTA `a, b` casa? | exata — delega no `matches_complex` |
//! | `matches_complex` | um seletor complexo casa, com combinadores? | exata |
//! | `match_combinators` | os compounds à ESQUERDA casam o contexto? | exata |
//! | `compound_matches_idx` | um compound casa este elemento? | exata — é a base das outras |
//! | `could_match_hover` | casaria SE estivesse sob o cursor? | **aproxima**: responde `:hover` como verdadeiro e testa o resto a sério |
//! | `class_change_is_inert` | trocar `class` pode mudar algum estilo? | **aproxima**: pergunta ao stylesheet se ALGUMA classe que entra ou sai é citada, sem casar seletor nenhum |
//! | `TargetKey::can_match` | pode este nó casar, olhando só a chave-alvo? | **aproxima**: filtro barato por `#id`/`.classe`/tag antes do matcher |
//!
//! **As três aproximações partilham uma regra e é a razão de estarem juntas:
//! podem responder "sim" a mais, NUNCA "não" a mais.** Um falso positivo custa
//! uma cascade a mais; um falso negativo perde um resultado ou deixa um nó com
//! estilo velho. Quem acrescentar a oitava aproximação tem de a escrever com o
//! erro para o mesmo lado — e é para não ter de descobrir isso sozinho que esta
//! tabela existe.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// `true` se o nó `idx` casa o seletor `sel` (string). Aceita uma LISTA separada
    /// por vírgula (`p, a` casa se QUALQUER um casar). Cada item é um seletor
    /// COMPLEXO (compostos + combinadores + atributo + pseudo). Item inválido é
    /// ignorado; lista toda inválida → false. (#1752)
    pub(in crate::dom) fn matches(&self, idx: NodeIdx, sel: &str) -> bool {
        crate::style::parse_selector_list(sel)
            .iter()
            .any(|complex| self.matches_complex(idx, complex))
    }

    /// Casa um [`ComplexSelector`] contra o nó `idx`, navegando a árvore para os
    /// combinadores. O ÚLTIMO compound casa `idx`; os anteriores casam ancestrais/
    /// irmãos conforme o combinador (matching da direita p/ a esquerda).
    pub(in crate::dom) fn matches_complex(&self, idx: NodeIdx, sel: &crate::style::ComplexSelector) -> bool {
        crate::bump!(selector_matches);
        let n = sel.compounds.len();
        if !self.compound_matches_idx(idx, &sel.compounds[n - 1]) {
            return false;
        }
        if n == 1 {
            return true;
        }
        self.match_combinators(idx, sel, n - 1)
    }

    /// Tenta casar os compounds [0..=i-1] contra o contexto (ancestrais/irmãos) de
    /// `idx`, dado que `compounds[i]` já casou `idx`. Backtracking p/ descendente e
    /// irmão-geral (que têm múltiplos candidatos).
    fn match_combinators(
        &self,
        idx: NodeIdx,
        sel: &crate::style::ComplexSelector,
        i: usize,
    ) -> bool {
        if i == 0 {
            return true;
        }
        let combinator = sel.combinators[i - 1];
        let prev = &sel.compounds[i - 1];
        use crate::style::Combinator;
        match combinator {
            Combinator::Child => match self.parent_element_idx(idx) {
                Some(p) if self.compound_matches_idx(p, prev) => {
                    self.match_combinators(p, sel, i - 1)
                }
                _ => false,
            },
            Combinator::Descendant => {
                let mut cur = self.parent_element_idx(idx);
                while let Some(a) = cur {
                    if self.compound_matches_idx(a, prev) && self.match_combinators(a, sel, i - 1) {
                        return true;
                    }
                    cur = self.parent_element_idx(a);
                }
                false
            }
            Combinator::NextSibling => match self.prev_element_sibling_idx(idx) {
                Some(s) if self.compound_matches_idx(s, prev) => {
                    self.match_combinators(s, sel, i - 1)
                }
                _ => false,
            },
            Combinator::SubsequentSibling => {
                let mut cur = self.prev_element_sibling_idx(idx);
                while let Some(s) = cur {
                    if self.compound_matches_idx(s, prev) && self.match_combinators(s, sel, i - 1) {
                        return true;
                    }
                    cur = self.prev_element_sibling_idx(s);
                }
                false
            }
        }
    }

    /// `true` se o COMPOUND casa o elemento `idx` (tag/id/classe/atributo/pseudo).
    fn compound_matches_idx(
        &self,
        idx: NodeIdx,
        compound: &crate::style::CompoundSelector,
    ) -> bool {
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.as_str(),
            _ => return false,
        };
        let id = self.nodes[idx].attr("id");
        let class_attr = self.nodes[idx].attr("class");
        let attr = |name: &str| self.nodes[idx].attr(name);
        let pseudo = |pc: &crate::style::PseudoClass| self.pseudo_matches(idx, pc);
        crate::style::compound_matches_borrowed(compound, tag, id, class_attr, &attr, &pseudo)
    }

    /// `true` se o nó casaria algum compound com `:hover` SE estivesse sob o
    /// cursor — o `:hover` é respondido como verdadeiro e o resto do compound
    /// (tag, classe, id, atributo, outras pseudo) é testado de verdade. É a
    /// pergunta do invalidation set: "o hover pode mudar o estilo DESTE nó?".
    pub(in crate::dom) fn could_match_hover(
        &self,
        idx: NodeIdx,
        compounds: &[&crate::style::CompoundSelector],
    ) -> bool {
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.as_str(),
            _ => return false,
        };
        let id = self.nodes[idx].attr("id");
        let class_attr = self.nodes[idx].attr("class");
        let attr = |name: &str| self.nodes[idx].attr(name);
        let pseudo = |pc: &crate::style::PseudoClass| {
            matches!(pc, crate::style::PseudoClass::Hover) || self.pseudo_matches(idx, pc)
        };
        compounds.iter().any(|c| {
            crate::style::compound_matches_borrowed(c, tag, id, class_attr, &attr, &pseudo)
        })
    }

    /// `true` se trocar o `class` deste nó para `novo` não pode mudar estilo
    /// nenhum: toda classe que ENTRA ou SAI está fora do conjunto de classes
    /// citadas pelo stylesheet.
    ///
    /// Só as que MUDAM: as que ficam não afetam nada por definição, e uma delas
    /// citada não torna a troca relevante.
    pub(in crate::dom) fn class_change_is_inert(&self, idx: NodeIdx, novo: &str) -> bool {
        let antigo = self.nodes[idx].attr("class").unwrap_or_default();
        let mudou = antigo
            .split_whitespace()
            .filter(|c| !novo.split_whitespace().any(|n| n == *c))
            .chain(
                novo.split_whitespace()
                    .filter(|c| !antigo.split_whitespace().any(|a| a == *c)),
            );
        let mut alguma = false;
        for c in mudou {
            alguma = true;
            if self.stylesheet.mentions_class(c) {
                return false;
            }
        }
        alguma
    }

    /// Resolve uma pseudo-classe contra o nó (posição entre irmãos / atributo de estado).
    fn pseudo_matches(&self, idx: NodeIdx, pc: &crate::style::PseudoClass) -> bool {
        use crate::style::PseudoClass as P;
        match pc {
            // `:root` = o elemento raiz do documento (o `<html>`). Num DOM headless de
            // FRAGMENTO (sem <html>), casa só se for o ÚNICO elemento top-level — senão
            // 0 (fiel ao browser, que tem exatamente 1 root).
            //
            // A CONTAGEM SOZINHA NÃO CHEGA, e o caso não é raro: um `<style>` ou um
            // `<link>` antes do `<html>` fica IRMÃO dele — `open_implicit_body`
            // recusa-lhe estrutura implícita de propósito, para não enterrar o
            // `<html>` real dentro de um implícito. O documento passa a ter dois
            // elementos de topo e o `== 1` recusava tudo, incluindo o `<html>`. Com
            // a folha do Google — que declara as suas 83 variáveis em `:root` — isso
            // esvaziava o mapa de custom properties do documento inteiro: 329 dos 368
            // elementos ficavam com o `font-size` errado e 297 com a cor errada.
            // Havendo um `<html>` de topo, ele É a raiz; a contagem fica para o
            // fragmento que não tem nenhum.
            P::Root => {
                self.nodes[idx].parent == Some(self.root) && {
                    let topo = || {
                        self.nodes[self.root]
                            .children
                            .iter()
                            .copied()
                            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
                    };
                    match topo().find(
                        |&c| matches!(&self.nodes[c].kind, NodeKind::Element { tag } if tag == "html"),
                    ) {
                        Some(html) => html == idx,
                        None => topo().count() == 1,
                    }
                }
            }
            P::Empty => !self.nodes[idx].children.iter().any(|&c| {
                matches!(self.nodes[c].kind, NodeKind::Element { .. })
                    || matches!(&self.nodes[c].kind, NodeKind::Text(t) if !t.trim().is_empty())
            }),
            P::FirstChild => self.element_index_among_siblings(idx) == Some(0),
            P::LastChild => self.element_siblings(idx).last() == Some(&idx),
            P::OnlyChild => self.element_siblings(idx).len() == 1,
            P::NthChild(a, b) => match self.element_index_among_siblings(idx) {
                Some(zero_based) => nth_casa(*a, *b, zero_based as i32 + 1),
                None => false,
            },
            // estado → presença de atributo (DOM headless, sem UI viva).
            P::Checked => {
                self.nodes[idx].attr("checked").is_some()
                    || self.nodes[idx].attr("selected").is_some()
            }
            P::Disabled => self.nodes[idx].attr("disabled").is_some(),
            P::Required => self.nodes[idx].attr("required").is_some(),
            P::Enabled => {
                let is_form = matches!(&self.nodes[idx].kind,
                    NodeKind::Element { tag } if matches!(tag.as_str(),
                        "input" | "button" | "select" | "textarea" | "option" | "fieldset"));
                is_form && self.nodes[idx].attr("disabled").is_none()
            }
            // `:hover` VIVO: casa se o nó É o hovered ou um ANCESTRAL dele (o hover
            // propaga — passar o mouse no <a> deixa o <li> pai também :hover, como
            // no browser). `hovered` vem do backend (hit-test); headless = nunca.
            P::Hover => match self.hovered.get() {
                Some(hovered) => self.is_ancestor(idx, hovered),
                None => false,
            },
            // `:focus` NÃO propaga aos ancestrais (isso é `:focus-within`), por
            // isso a comparação é de igualdade e não `is_ancestor` como no hover.
            // `:focus-visible` casa o mesmo que `:focus` — ver a variante.
            P::Focus | P::FocusVisible => self.focused_input == Some(idx),
            // `:focus-within` propaga para os ancestrais, como o `:hover`.
            P::FocusWithin => match self.focused_input {
                Some(f) => self.is_ancestor(idx, f),
                None => false,
            },
            // A família `-of-type` conta só os irmãos da MESMA tag.
            P::FirstOfType => self.type_siblings(idx).first() == Some(&idx),
            P::LastOfType => self.type_siblings(idx).last() == Some(&idx),
            P::OnlyOfType => self.type_siblings(idx).len() == 1,
            P::NthOfType(a, b) => {
                let irmaos = self.type_siblings(idx);
                match irmaos.iter().position(|&s| s == idx) {
                    Some(zero_based) => nth_casa(*a, *b, zero_based as i32 + 1),
                    None => false,
                }
            }
            // Sem estado de botão premido nem histórico no DOM — ver os
            // comentários das variantes em `style::selector`.
            P::Active | P::Visited => false,
            P::Link => {
                let is_anchor = matches!(&self.nodes[idx].kind,
                    NodeKind::Element { tag } if matches!(tag.as_str(), "a" | "area"));
                is_anchor && self.nodes[idx].attr("href").is_some()
            }
            P::ReadWrite => self.is_read_write(idx),
            P::ReadOnly => !self.is_read_write(idx),
            // O idioma herda-se: o `lang` do ancestral mais próximo é o do nó.
            P::Lang(want) => match self.nearest_lang(idx) {
                // `en` casa `en-US` mas não `english` — é a mesma regra do
                // operador `[lang|=en]`, e é por isso que é escrita igual.
                Some(have) => {
                    let have = have.to_ascii_lowercase();
                    have == *want || have.starts_with(&format!("{want}-"))
                }
                None => false,
            },
            // `:not(a, b)` casa quando NENHUM casa; `:is`/`:where` quando ALGUM.
            // O argumento é um seletor COMPLEXO, logo volta a `matches_complex` —
            // o `:not(div > p)` precisa de navegar a árvore como qualquer outro.
            P::Not(list) => !list.iter().any(|s| self.matches_complex(idx, s)),
            P::Is(list) | P::Where(list) => list.iter().any(|s| self.matches_complex(idx, s)),
        }
    }

    /// `true` se o utilizador pode editar o conteúdo deste nó — a definição de
    /// `:read-write` (e o complemento da de `:read-only`).
    fn is_read_write(&self, idx: NodeIdx) -> bool {
        if let Some(ce) = self.nodes[idx].attr("contenteditable") {
            // `contenteditable=""` vale `true` (atributo booleano do HTML).
            return !ce.eq_ignore_ascii_case("false");
        }
        let editavel_por_tag = matches!(&self.nodes[idx].kind,
            NodeKind::Element { tag } if matches!(tag.as_str(), "input" | "textarea"));
        editavel_por_tag
            && self.nodes[idx].attr("readonly").is_none()
            && self.nodes[idx].attr("disabled").is_none()
    }

    /// Os irmãos-elemento com a MESMA tag de `idx` (incluindo ele), em ordem —
    /// o universo que a família `-of-type` conta.
    fn type_siblings(&self, idx: NodeIdx) -> Vec<NodeIdx> {
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return Vec::new();
        };
        let alvo = tag.as_str();
        let Some(parent) = self.nodes[idx].parent else {
            return vec![idx];
        };
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| matches!(&self.nodes[c].kind, NodeKind::Element { tag } if tag == alvo))
            .collect()
    }

    /// O valor de `lang` do nó ou do ancestral mais próximo que o tenha.
    fn nearest_lang(&self, idx: NodeIdx) -> Option<&str> {
        let mut cur = Some(idx);
        while let Some(n) = cur {
            if let Some(lang) = self.nodes[n].attr("lang") {
                return Some(lang);
            }
            cur = self.parent_element_idx(n);
        }
        None
    }

    /// Os irmãos-ELEMENTO de `idx` (incluindo ele), em ordem.
    fn element_siblings(&self, idx: NodeIdx) -> Vec<NodeIdx> {
        let Some(parent) = self.nodes[idx].parent else {
            return vec![idx];
        };
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .collect()
    }

    /// Índice (0-based) de `idx` entre seus irmãos-elemento, ou `None`.
    fn element_index_among_siblings(&self, idx: NodeIdx) -> Option<usize> {
        self.element_siblings(idx).iter().position(|&c| c == idx)
    }

    /// O pai de `idx` SE for elemento (não o #document), em índice cru.
    fn parent_element_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let p = self.nodes[idx].parent?;
        matches!(self.nodes[p].kind, NodeKind::Element { .. }).then_some(p)
    }

    /// O irmão-elemento imediatamente anterior a `idx`, em índice cru.
    fn prev_element_sibling_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let sibs = self.element_siblings(idx);
        let pos = sibs.iter().position(|&c| c == idx)?;
        (pos > 0).then(|| sibs[pos - 1])
    }
}

/// A CHAVE-ALVO de um seletor: o que o último compound exige do nó que ele casa.
/// Um filtro barato antes do matcher completo (que navega a árvore) — a mesma
/// ideia do `RuleIndex` da cascade, aplicada às consultas.
///
/// Só uma chave por seletor, e a mais seletiva disponível: `#id` descarta quase
/// tudo, `.classe` descarta muito, a tag descarta o resto. `Any` (universal,
/// `[attr]`, pseudo) não descarta nada e cai direto no matcher — é o caso em que
/// o filtro não ajuda, e ele não pode ATRAPALHAR respondendo "não" por engano.
pub(in crate::dom) enum TargetKey {
    Id(String),
    Class(String),
    Tag(String),
    Any,
}

impl TargetKey {
    pub(in crate::dom) fn of(sel: &crate::style::ComplexSelector) -> TargetKey {
        use crate::style::SimpleSelector as S;
        let Some(last) = sel.compounds.last() else {
            return TargetKey::Any;
        };
        for p in &last.parts {
            if let S::Id(v) = p {
                return TargetKey::Id(v.clone());
            }
        }
        for p in &last.parts {
            if let S::Class(v) = p {
                return TargetKey::Class(v.clone());
            }
        }
        for p in &last.parts {
            if let S::Tag(v) = p {
                return TargetKey::Tag(v.clone());
            }
        }
        TargetKey::Any
    }

    /// `false` só quando o nó NÃO pode casar — um falso negativo aqui perderia
    /// um resultado, então cada braço espelha exatamente o que o matcher exige.
    pub(in crate::dom) fn can_match(&self, tag: &str, id: Option<&str>, class_attr: Option<&str>) -> bool {
        match self {
            TargetKey::Any => true,
            TargetKey::Tag(t) => t == tag,
            TargetKey::Id(want) => id == Some(want.as_str()),
            TargetKey::Class(want) => class_attr
                .map(|c| c.split_whitespace().any(|x| x == want))
                .unwrap_or(false),
        }
    }
}
