//! MÉTRICAS do motor de DOM — o que aconteceu por dentro, em três formas.
//!
//! | módulo | responde |
//! |---|---|
//! | [`counters`] | QUANTAS vezes cada operação aconteceu (parse, cascade, layout, invalidação) |
//! | [`phases`] | QUANTO TEMPO cada fase nomeada levou — inclusive fases de fora deste crate (render, loop, script) |
//! | [`samples`] | QUAIS foram os casos — os nomes por trás de um contador (quais seletores caíram, quais propriedades faltam) |
//! | [`audit`] | se a árvore está CONSISTENTE consigo mesma: índices, elos de parentesco, ids duplicados |
//! | [`footprint`] | QUANTA MEMÓRIA a árvore ocupa, e em qual área — o consumo que nem contador nem relógio vê |
//!
//! ## Por que as três e não só um cronômetro
//!
//! Um tempo diz que algo está lento; não diz o quê. As perguntas que este crate
//! não conseguia responder eram todas de CONTAGEM, e cada uma tem um cache no
//! caminho — e cache se avalia por RAZÃO (hits/chamadas), que é um par de
//! contadores, nunca um relógio. Já a auditoria responde a outra pergunta, que
//! nenhum número de desempenho responde: se o que foi medido estava CERTO. Um
//! índice `.classe` apontando para um nó removido não fica lento — fica errado, e
//! só uma varredura que compara o índice com a árvore acha isso.
//!
//! ## Custo quando desligada: zero
//!
//! Contadores e fases passam por macros que, sem a feature `metrics`, expandem
//! para nada — nem a leitura do `thread_local` sobra. A [`audit`] é diferente: é
//! uma varredura sob demanda, sempre disponível, porque não é instrumentação de
//! caminho quente e sim uma pergunta que se faz a uma árvore parada.
//!
//! Um número de RELÓGIO medido COM a instrumentação não é comparável a um sem
//! ela. O harness (`examples/dom_metrics.rs`) imprime de que lado está.

pub mod audit;
pub mod counters;
pub mod footprint;
pub mod phases;
pub mod samples;

pub use audit::{audit, AuditReport, Finding};
pub use footprint::{footprint, Footprint};
pub use counters::{snapshot, DomMetrics};
pub use phases::{phase_snapshot, PhaseStats, Phases};
pub use samples::Samples;

/// Zera contadores, fases E amostras desta thread. A auditoria não tem estado a
/// zerar: ela pergunta a uma árvore, não acumula nada.
pub fn reset() {
    counters::reset();
    phases::reset();
    samples::reset();
}

/// `true` quando o crate foi compilado com a instrumentação (feature `metrics`).
/// Um relatório que não diz isto é um relatório de zeros indistinguível de um
/// trabalho que não houve.
pub const fn enabled() -> bool {
    cfg!(feature = "metrics")
}
