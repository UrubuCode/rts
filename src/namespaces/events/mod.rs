//! `events` namespace — EventEmitter primitivo handle-based.
//!
//! Listeners sao function pointers raw (`func_addr` em i64). Caller
//! (codegen) materializa o endereco da fn via `Expr::Ident` → `func_addr`
//! e passa pra `events.on`. `events.emit*` invoca o ponteiro via
//! `unsafe transmute` para a signature apropriada.
//!
//! `node:events` (#290) é wrapper TS sobre este namespace.

pub mod abi;
pub mod ops;
