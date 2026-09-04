//! Índice de regras por CHAVE do compound-alvo — o que transforma a cascade de
//! O(nós × TODAS as regras) em O(nós × regras-candidatas). Sem ele, uma página
//! Tailwind (~4500 regras) × ~600 nós = ~2.8M testes de seletor por layout (medido
//! ~4s); com ele, cada nó só testa as regras cuja chave-alvo o nó pode satisfazer
//! (id/classe/tag do nó), tipicamente dezenas.
//!
//! CORRETUDE: a chave é uma condição NECESSÁRIA do último compound (o alvo). Uma
//! regra `… .card` só casa um nó que tem a classe `card`; então indexá-la no bucket
//! `card` e só consultá-la para nós com essa classe NÃO perde nenhum casamento — o
//! `matches` completo (que navega a árvore para os combinadores anteriores) ainda
//! roda sobre as candidatas, decidindo de fato. Seletores cujo alvo é só universal/
//! pseudo/atributo (sem tag/classe/id âncora) caem no bucket `universal`, testado
//! para todo nó (conservador: nunca menos candidatas do que o correto).

use super::selector::{ComplexSelector, SimpleSelector};
use super::stylesheet::Rule;
use std::collections::HashMap;

/// A CHAVE de indexação de uma regra: a âncora mais restritiva do compound-alvo.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Key {
    Id(String),
    Class(String),
    Tag(String),
    /// Alvo sem âncora tag/classe/id (universal, ou só pseudo/attr) → testado sempre.
    Universal,
}

/// Índice invertido: cada chave → os ÍNDICES (em `Stylesheet::rules`) das regras cujo
/// alvo tem aquela chave. `universal` é a lista das regras testadas para todo nó.
#[derive(Clone, Default, Debug)]
pub struct RuleIndex {
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
    /// Especificidade pré-computada por índice de regra; evita percorrer todos os
    /// compounds novamente em cada cascade de nó.
    specificity: Vec<u32>,
    /// Nº de regras que o índice cobre — se `rules.len()` diverge, o índice está
    /// stale e é reconstruído.
    covered: usize,
    has_custom_rules: bool,
    /// Alguma regra tem `::before`/`::after`. Sem nenhuma, o layout não precisa
    /// sequer de perguntar por conteúdo gerado.
    has_pseudo_elements: bool,
    /// Há seletores cuja correspondência depende de atributos ou pseudo-classes
    /// representadas por atributos (`:checked`, `:disabled`, etc.).
    has_attribute_selectors: bool,
    /// TODAS as classes citadas em QUALQUER compound de qualquer regra — não só
    /// as que servem de chave-alvo. Uma classe fora deste conjunto não pode
    /// mudar o estilo de nó nenhum, e trocá-la não precisa invalidar nada. É a
    /// versão mínima do "invalidation set" que os browsers mantêm.
    ///
    /// TODAS e não só as chaves: em `.a.b { }` a chave é `.a`, mas trocar `b`
    /// muda o resultado do mesmo jeito.
    mentioned_classes: std::collections::HashSet<String>,
    /// Os NOMES de atributo que algum seletor observa — a versão fina de
    /// [`has_attribute_selectors`](Self::has_attribute_selectors), por
    /// NOME em vez de um booleano só. Existe porque a folha de UA (lote I)
    /// tem `[hidden]`/`input:disabled`, e sem isto QUALQUER `setAttribute`
    /// em QUALQUER nó passou a invalidar o documento inteiro assim que essa
    /// folha entrou — o booleano não distingue "mudou `hidden`" de "mudou
    /// `data-um-nome-qualquer`".
    mentioned_attr_names: std::collections::HashSet<String>,
}

/// A âncora do compound-alvo (último compound) de um seletor. Id > Class > Tag; se
/// nenhum, Universal. (A PRIMEIRA classe/id que aparecer basta como âncora: o nó
/// precisa TÊ-LA para casar, então é uma condição necessária suficiente p/ o bucket.)
fn key_of(sel: &ComplexSelector) -> Key {
    let Some(target) = sel.compounds.last() else {
        return Key::Universal;
    };
    let mut tag: Option<&str> = None;
    for part in &target.parts {
        match part {
            SimpleSelector::Id(i) => return Key::Id(i.clone()),
            SimpleSelector::Class(c) => return Key::Class(c.clone()),
            SimpleSelector::Tag(t) => {
                if tag.is_none() {
                    tag = Some(t.as_str());
                }
            }
            _ => {}
        }
    }
    match tag {
        Some(t) => Key::Tag(t.to_string()),
        None => Key::Universal,
    }
}

impl RuleIndex {
    /// (Re)constrói o índice a partir das regras. Idempotente; chame quando as regras
    /// mudam (o `covered` detecta stale).
    pub fn build(rules: &[Rule]) -> RuleIndex {
        let mut idx = RuleIndex::default();
        for (i, rule) in rules.iter().enumerate() {
            match key_of(&rule.selector) {
                Key::Id(id) => idx.by_id.entry(id).or_default().push(i),
                Key::Class(c) => idx.by_class.entry(c).or_default().push(i),
                Key::Tag(t) => idx.by_tag.entry(t).or_default().push(i),
                Key::Universal => idx.universal.push(i),
            }
        }
        // A varredura entra nas pseudo funcionais: a classe de `:is(.b)` é tão
        // observada pela regra como a de `.b` solto, e deixá-la de fora fazia a
        // troca de `b` passar por inerte.
        for rule in rules {
            super::selector::visit_simples(&rule.selector, &mut |part| {
                if let SimpleSelector::Class(c) = part {
                    if !idx.mentioned_classes.contains(c) {
                        idx.mentioned_classes.insert(c.clone());
                    }
                }
            });
        }
        idx.specificity = rules.iter().map(|r| r.selector.specificity()).collect();
        idx.covered = rules.len();
        idx.has_custom_rules = rules
            .iter()
            .any(|r| !r.decls.custom.is_empty() || !r.decls.custom_important.is_empty());
        idx.has_pseudo_elements = rules.iter().any(|r| r.selector.pseudo_element.is_some());
        for rule in rules {
            super::selector::visit_simples(&rule.selector, &mut |part| {
                use super::selector::PseudoClass as P;
                // As pseudo que LEEM um atributo sem o nomear: `:checked` lê
                // `checked`, `:disabled`/`:enabled` leem `disabled`,
                // `:required` lê `required`, `:link` lê `href`,
                // `:read-only`/`:read-write` leem `readonly`/`contenteditable`,
                // `:lang` lê `lang`. Sem elas, um `setAttribute` desses não
                // invalidava o estilo.
                let mut nomes: &[&str] = &[];
                match part {
                    SimpleSelector::Attr { name, .. } => {
                        idx.mentioned_attr_names.insert(name.clone());
                    }
                    SimpleSelector::Pseudo(P::Checked) => nomes = &["checked"],
                    SimpleSelector::Pseudo(P::Disabled | P::Enabled) => nomes = &["disabled"],
                    SimpleSelector::Pseudo(P::Required) => nomes = &["required"],
                    SimpleSelector::Pseudo(P::Link) => nomes = &["href"],
                    SimpleSelector::Pseudo(P::ReadOnly | P::ReadWrite) => {
                        nomes = &["readonly", "contenteditable"]
                    }
                    SimpleSelector::Pseudo(P::Lang(_)) => nomes = &["lang"],
                    _ => {}
                }
                for n in nomes {
                    idx.mentioned_attr_names.insert((*n).to_string());
                }
            });
        }
        idx.has_attribute_selectors = !idx.mentioned_attr_names.is_empty();
        idx
    }

    /// `true` se o índice está sincronizado com um stylesheet de `n` regras.
    pub fn is_current(&self, n: usize) -> bool {
        self.covered == n
    }

    pub fn has_custom_rules(&self) -> bool {
        self.has_custom_rules
    }

    pub fn has_pseudo_elements(&self) -> bool {
        self.has_pseudo_elements
    }

    /// `true` se alguma regra CITA esta classe (em qualquer posição).
    pub fn mentions_class(&self, class: &str) -> bool {
        self.mentioned_classes.contains(class)
    }

    pub fn has_attribute_selectors(&self) -> bool {
        self.has_attribute_selectors
    }

    /// `true` se algum seletor observa este NOME de atributo especificamente
    /// (`[hidden]` observa `hidden`; `:disabled` observa `disabled`) — a
    /// versão fina de [`has_attribute_selectors`](Self::has_attribute_selectors).
    pub fn mentions_attribute_name(&self, name: &str) -> bool {
        self.mentioned_attr_names.contains(name)
    }

    pub fn specificity(&self, rule: usize) -> u32 {
        self.specificity.get(rule).copied().unwrap_or(0)
    }

    /// Os índices das regras CANDIDATAS a casar um nó `(tag, id, classes)`: a união
    /// dos buckets do id, de cada classe, da tag e do universal. Cada regra foi
    /// indexada por uma única âncora (`key_of`), portanto não há duplicatas entre
    /// buckets; o consumidor reordena os matches pela especificidade e pela ordem.
    pub fn candidates(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> Vec<usize> {
        let id_len = id.and_then(|key| self.by_id.get(key)).map_or(0, Vec::len);
        let class_len: usize = classes
            .iter()
            .filter_map(|class| self.by_class.get(*class))
            .map(Vec::len)
            .sum();
        let tag_len = self.by_tag.get(tag).map_or(0, Vec::len);
        let mut out = Vec::with_capacity(self.universal.len() + id_len + class_len + tag_len);
        self.candidates_into(tag, id, classes, &mut out);
        out
    }

    pub fn candidates_into(
        &self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
        out: &mut Vec<usize>,
    ) {
        out.clear();
        out.extend_from_slice(&self.universal);
        if let Some(id) = id {
            if let Some(v) = self.by_id.get(id) {
                out.extend_from_slice(v);
            }
        }
        for c in classes {
            if let Some(v) = self.by_class.get(*c) {
                out.extend_from_slice(v);
            }
        }
        if let Some(v) = self.by_tag.get(tag) {
            out.extend_from_slice(v);
        }
    }
}
