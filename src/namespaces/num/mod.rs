//! `num` namespace — aritmetica com overflow explicito (checked/
//! saturating/wrapping), bit operations e limites.
//!
//! Usa primitivas Rust core/std (i64::checked_*, etc). Overflow em
//! checked_* eh sinalizado retornando i64::MIN como sentinela
//! (caller deve verificar antes de usar).

pub mod abi;
pub mod ops;
