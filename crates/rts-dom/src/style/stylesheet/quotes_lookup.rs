//! `quotes` sobre um [`Stylesheet`] — a imagem em ponto pequeno de
//! `counters_from`/`has_counters` em `sheet.rs`, num ficheiro à parte porque
//! `sheet.rs` já está acima do tecto de 500 linhas do `CLAUDE.md` e a regra
//! para um ficheiro nessa lista é "não cresce": lógica nova é módulo novo.

use super::{MatchedRules, Stylesheet};

impl Stylesheet {
    /// O `quotes` VENCEDOR (própria regra, não herdado) entre as regras que
    /// casaram — usado por [`crate::dom::Dom::effective_quotes`] para
    /// perguntar "este elemento declara `quotes`?" a cada ancestral, um de
    /// cada vez, até achar um que declare ou chegar à raiz.
    pub fn quotes_from(&self, matched: &MatchedRules) -> Option<std::rc::Rc<crate::quotes::Pares>> {
        matched
            .rules
            .iter()
            .rev()
            .find_map(|(_, _, _, _, i)| self.rules[*i].quotes.clone())
    }

    /// `true` se alguma regra desta folha gera `open-quote`/`close-quote`/
    /// `no-open-quote`/`no-close-quote` — a guarda que evita a passagem
    /// documental de [`crate::quotes::calcula`] numa página que não usa
    /// aspas de CSS (o caso comum: nenhuma das quatro folhas do corpus usa).
    pub fn has_quote_content(&self) -> bool {
        self.rules.iter().any(|r| {
            matches!(
                r.content.as_deref(),
                Some(crate::pseudo::Content::Pecas(pecas))
                    if pecas.iter().any(|p| matches!(
                        p,
                        crate::pseudo::Peca::AbreAspas
                            | crate::pseudo::Peca::FechaAspas
                            | crate::pseudo::Peca::NaoAbreAspas
                            | crate::pseudo::Peca::NaoFechaAspas
                    ))
            )
        })
    }
}
