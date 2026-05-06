//! `json` namespace — JSON.parse / JSON.stringify primitives.
//!
//! `parse` retorna handle de um Entry::Json (`serde_json::Value` boxed).
//! `stringify` aceita um Entry::Json handle e devolve string handle.
//! Acesso a campos do JSON (path-based) e conversao pra tipos nativos
//! sao expostos via membros adicionais.

pub mod abi;
pub mod ops;

pub use abi::SPEC;
