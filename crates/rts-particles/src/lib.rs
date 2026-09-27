//! `rts:particles` — o kernel nativo de simulação de partículas
//! (`particlesStep`), a resposta a `docs`/`build/particulas-kernel/kernel-report.md`
//! (`rts-game`): a `rts-game/src/scripts/particlesystem.ts` medida (10 000
//! partículas, interpretada) custava `update()` 8,8 ms + preenchimento do
//! buffer de instância 15,5 ms + bucket sort 9,6 ms — muito acima do
//! orçamento de 1 ms/quadro. Este crate move as quatro passadas por-partícula
//! (envelhecer/reciclar, vento+arrasto, integração de posição, gradiente/
//! curva + bucket sort) para um laço Rust compilado, do mesmo espírito de
//! `rts-audio::mix::mix_add` — o script continua dono de POLÍTICA (emissão,
//! bursts, ciclo de loop, seleção de editor, frustum por emissor); o kernel
//! só faz o trabalho por-partícula que o motor precisa fazer rápido.
//!
//! Sem dependência de plataforma (ao contrário de `rts-audio`/`rts-physics`):
//! `install` roda incondicionalmente em todo host, sem feature própria — ver
//! o comentário do `Cargo.toml`.

// Avisos como erro só neste crate (um RUSTFLAGS global no CI pegaria avisos
// antigos de outros crates do workspace, como rts-core).
#![deny(warnings)]
#![deny(missing_docs)]
#![deny(dead_code)]

pub mod kernel;
mod surface;

use rts_core::entry::{self, Context};

/// Registra `rts:particles`. Chamado pelo host (JIT: `rts-host/src/run.rs`;
/// AOT: `rts-runtime-boot`), não por um construtor daqui — mesma razão de
/// `rts_audio::install`/`rts_dom_bridge::install`.
pub fn install(context: &mut Context) {
    let ns = surface::namespace(context);
    entry::declare_module(context, "rts:particles", ns);
}
