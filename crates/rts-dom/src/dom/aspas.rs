//! A ponte entre o `Dom` e [`crate::quotes`] — a herança de `quotes` por
//! ancestral e a passagem documental de profundidade, chamadas por
//! `dom::cascade::pseudo_box`.
//!
//! Ficheiro à parte e não um acréscimo a `cascade.rs`: esse já passa das 500
//! linhas do tecto do `CLAUDE.md` (não está na lista dos que "já estão acima
//! e não crescem" do `PLAN.md`, mas a regra para qualquer ficheiro no limite
//! é a mesma — lógica nova é módulo novo pequeno).

use super::*;

impl Dom {
    /// A lista de aspas (`quotes`) HERDADA e efetiva no elemento `idx` — o
    /// par mais interno declarado por ele ou por um ancestral, ou o par
    /// tipográfico por omissão quando nenhum o declara.
    ///
    /// `quotes` fica fora do `ComputedStyle` (ver `Rule::quotes`), então a
    /// herança normal da cascade não a resolve; subir a árvore É o mecanismo
    /// de herança aqui, pela mesma razão que `counters::Pilha` é o mecanismo
    /// de escopo dos contadores. Custa O(profundidade) por caixa gerada e só
    /// corre quando `has_quote_content()` já filtrou que a página usa
    /// `open-quote`/`close-quote`.
    pub(in crate::dom) fn effective_quotes(&self, idx: NodeIdx) -> crate::quotes::Pares {
        let media_ctx = self.media_context();
        let mut atual = Some(idx);
        while let Some(id) = atual {
            if let NodeKind::Element { tag } = &self.nodes[id].kind {
                let classes: Vec<&str> = self.nodes[id]
                    .attr("class")
                    .map(|c| c.split_whitespace().collect())
                    .unwrap_or_default();
                let matched = self.stylesheet.matched_for_node(
                    &media_ctx,
                    tag,
                    self.nodes[id].attr("id"),
                    &classes,
                    |sel| self.matches_complex(id, sel),
                );
                if let Some(q) = self.stylesheet.quotes_from(&matched) {
                    return (*q).clone();
                }
            }
            atual = self.nodes[id].parent;
        }
        crate::quotes::default_pares()
    }

    /// O `content` VENCEDOR de um pseudo-elemento — a mesma pergunta que
    /// `pseudo_box` já faz, isolada para [`document_quote_depths`] não
    /// precisar montar o `PseudoBox` inteiro só para ver o `content`.
    fn pseudo_content(
        &self,
        idx: NodeIdx,
        pe: crate::style::PseudoElement,
    ) -> Option<std::rc::Rc<crate::pseudo::Content>> {
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        self.stylesheet
            .matched_for_pseudo(
                &self.media_context(),
                tag,
                self.nodes[idx].attr("id"),
                &classes,
                pe,
                |sel| self.matches_complex(idx, sel),
            )
            .1
    }

    /// A tabela de PROFUNDIDADES de aspas do documento, calculada uma vez por
    /// revisão — a imagem em ponto pequeno de `document_counters` em
    /// `cascade.rs`, e pela mesma razão: a profundidade de um pseudo depende
    /// de tudo o que veio antes dele em ordem documental.
    pub(in crate::dom) fn document_quote_depths(&self) -> std::rc::Rc<crate::quotes::Profundidades> {
        let chave = (self.revision, crate::style::props::style_epoch());
        if self.quote_memo_revision.get() == chave {
            if let Some(t) = self.quote_memo.borrow().as_ref() {
                return std::rc::Rc::clone(t);
            }
        }
        let tabela = if self.stylesheet.has_quote_content() {
            crate::quotes::calcula(self, &|idx, pe| self.pseudo_content(idx, pe))
        } else {
            crate::quotes::Profundidades::default()
        };
        let tabela = std::rc::Rc::new(tabela);
        *self.quote_memo.borrow_mut() = Some(std::rc::Rc::clone(&tabela));
        self.quote_memo_revision.set(chave);
        tabela
    }
}
