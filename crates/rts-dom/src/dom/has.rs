//! `:has()` — a ÚNICA direção do matcher que DESCE a árvore a partir do alvo.
//!
//! Todo o resto de `matcher.rs` casa da direita para a esquerda subindo por
//! ancestrais/irmãos ANTERIORES (Selectors L4 — é a direção que um combinador
//! comum lê). `:has(<lista-relativa>)` inverte isso: o argumento descreve um
//! elemento RELACIONADO com o alvo (descendente por default, ou filho/irmão
//! com um combinador explícito líder), e o alvo é quem casa `:has()` quando
//! ALGUM elemento nessa relação existe.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {
    /// `true` se `anchor` casa `:has(rel)` para UM item da lista relativa —
    /// `comb` é o combinador líder desse item (`Descendant` por default) e
    /// `rel` é o seletor completo a partir daí.
    ///
    /// CORRETUDE: o primeiro compound de `rel` tem de estar na relação `comb`
    /// com `anchor` (não em qualquer lugar da árvore); os compounds seguintes,
    /// se houver, casam normalmente subindo a partir do candidato — mas a
    /// subida PÁRA em `anchor` (exclusive): um ancestral de `rel` que exista
    /// só ACIMA de `anchor` não conta, porque `:has(.a .b)` exige que `.a`
    /// esteja ele próprio na relação (descendente) de `anchor`, não em
    /// qualquer parte da árvore acima de `.b`. É essa fronteira que separa
    /// isto de reusar `matches_complex` sem mais: aquele sobe até a raiz do
    /// documento, o que faria `body:has(.a .b)` casar sempre que `.a .b` casa
    /// EM QUALQUER LUGAR da página, e não só dentro do `<body>` (o que é
    /// verdade só porque tudo está dentro do body — o teste real está em
    /// `section:has(.a .b)` com um `.a` FORA da `section`, que não deve casar).
    pub(in crate::dom) fn has_matches(
        &self,
        anchor: NodeIdx,
        comb: crate::style::Combinator,
        rel: &crate::style::ComplexSelector,
    ) -> bool {
        use crate::style::Combinator as C;
        let candidatos: Vec<NodeIdx> = match comb {
            C::Descendant => self.descendentes(anchor),
            C::Child => self.nodes[anchor]
                .children
                .iter()
                .copied()
                .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
                .collect(),
            C::NextSibling => self
                .next_element_sibling_idx(anchor)
                .into_iter()
                .collect(),
            C::SubsequentSibling => self.subsequent_element_siblings(anchor),
        };
        let n = rel.compounds.len();
        candidatos.into_iter().any(|x| {
            self.compound_matches_idx(x, &rel.compounds[n - 1])
                && (n == 1 || self.match_combinators_bounded(x, rel, n - 1, anchor))
        })
    }

    /// Como `match_combinators` (em `matcher.rs`), mas a subida por ancestral
    /// PÁRA em `boundary` — `boundary` em si NUNCA é testado como candidato a
    /// compound, porque ele é o ALVO do `:has()`, não parte do que `:has()`
    /// descreve. Só o combinador `Descendant`/`Child` sobe; `NextSibling`/
    /// `SubsequentSibling` já operam DENTRO da fronteira (um irmão de um
    /// candidato descendente de `anchor` continua descendente de `anchor`, se
    /// ainda tiver `anchor` como ancestral — por isso eles reusam a mesma
    /// checagem de fronteira antes de aceitar o irmão).
    fn match_combinators_bounded(
        &self,
        idx: NodeIdx,
        sel: &crate::style::ComplexSelector,
        i: usize,
        boundary: NodeIdx,
    ) -> bool {
        if i == 0 {
            return true;
        }
        use crate::style::Combinator as C;
        let combinator = sel.combinators[i - 1];
        let prev = &sel.compounds[i - 1];
        let dentro_da_fronteira = |n: NodeIdx| self.is_ancestor(boundary, n) && n != boundary;
        match combinator {
            C::Child => match self.parent_element_idx(idx) {
                Some(p) if p != boundary && dentro_da_fronteira(p) && self.compound_matches_idx(p, prev) => {
                    self.match_combinators_bounded(p, sel, i - 1, boundary)
                }
                _ => false,
            },
            C::Descendant => {
                let mut cur = self.parent_element_idx(idx);
                while let Some(a) = cur {
                    if a == boundary {
                        break;
                    }
                    if self.compound_matches_idx(a, prev)
                        && self.match_combinators_bounded(a, sel, i - 1, boundary)
                    {
                        return true;
                    }
                    cur = self.parent_element_idx(a);
                }
                false
            }
            C::NextSibling => match self.prev_element_sibling_idx(idx) {
                Some(s) if dentro_da_fronteira(s) && self.compound_matches_idx(s, prev) => {
                    self.match_combinators_bounded(s, sel, i - 1, boundary)
                }
                _ => false,
            },
            C::SubsequentSibling => {
                let mut cur = self.prev_element_sibling_idx(idx);
                while let Some(s) = cur {
                    if dentro_da_fronteira(s)
                        && self.compound_matches_idx(s, prev)
                        && self.match_combinators_bounded(s, sel, i - 1, boundary)
                    {
                        return true;
                    }
                    cur = self.prev_element_sibling_idx(s);
                }
                false
            }
        }
    }

    /// Todos os DESCENDENTES-elemento de `idx` (não inclui `idx`), em pré-ordem.
    fn descendentes(&self, idx: NodeIdx) -> Vec<NodeIdx> {
        let mut out = Vec::new();
        let mut pilha: Vec<NodeIdx> = self.nodes[idx].children.clone();
        while let Some(n) = pilha.pop() {
            if matches!(self.nodes[n].kind, NodeKind::Element { .. }) {
                out.push(n);
            }
            pilha.extend(self.nodes[n].children.iter().copied());
        }
        out
    }

    /// O irmão-elemento imediatamente SEGUINTE a `idx`, cru.
    fn next_element_sibling_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let Some(parent) = self.nodes[idx].parent else {
            return None;
        };
        let sibs: Vec<NodeIdx> = self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .collect();
        let pos = sibs.iter().position(|&c| c == idx)?;
        sibs.get(pos + 1).copied()
    }

    /// Os irmãos-elemento POSTERIORES a `idx` (o que `~` alcança).
    fn subsequent_element_siblings(&self, idx: NodeIdx) -> Vec<NodeIdx> {
        let Some(parent) = self.nodes[idx].parent else {
            return Vec::new();
        };
        let sibs: Vec<NodeIdx> = self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .collect();
        match sibs.iter().position(|&c| c == idx) {
            Some(pos) => sibs[pos + 1..].to_vec(),
            None => Vec::new(),
        }
    }
}
