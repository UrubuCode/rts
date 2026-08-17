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
    /// Nº de regras que o índice cobre — se `rules.len()` diverge, o índice está
    /// stale e é reconstruído.
    covered: usize,
    has_custom_rules: bool,
}

/// A âncora do compound-alvo (último compound) de um seletor. Id > Class > Tag; se
/// nenhum, Universal. (A PRIMEIRA classe/id que aparecer basta como âncora: o nó
/// precisa TÊ-LA para casar, então é uma condição necessária suficiente p/ o bucket.)
fn key_of(sel: &ComplexSelector) -> Key {
    let Some(target) = sel.compounds.last() else { return Key::Universal };
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
        idx.covered = rules.len();
        idx.has_custom_rules = rules.iter().any(|r| !r.decls.custom.is_empty());
        idx
    }

    /// `true` se o índice está sincronizado com um stylesheet de `n` regras.
    pub fn is_current(&self, n: usize) -> bool {
        self.covered == n
    }

    pub fn has_custom_rules(&self) -> bool {
        self.has_custom_rules
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
        out
    }
}
