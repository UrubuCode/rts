//! MIR → Cranelift IR lowering — the final layer of the rts-mir pipeline.
//!
//! Converts a `MirFunc` into a Cranelift `Function` via `FunctionBuilder`.
//! Each `Inst` variant maps 1:1 to `builder.ins().*` calls; each `Terminator`
//! maps to a Cranelift terminator. This module is intentionally isolated
//! from `crate::codegen::lower::*` (the AST-driven path) so the two can
//! coexist while the MIR path matures.
//!
//! Status: Fase 3 etapa 3.9 — esqueleto inicial. Cobre o subset que o
//! `rts_mir::lower` já produz hoje (literais, aritmética, casts,
//! comparações, branches, return). Não plugado no pipeline ainda.

pub mod hint_bridge;
pub mod lower;

pub use lower::lower_mir_func;

#[cfg(test)]
mod tests;
