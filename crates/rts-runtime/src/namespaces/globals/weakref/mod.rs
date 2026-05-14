//! `WeakRef` global class (#685 v0) — semantica strong temporaria.
//!
//! v0: armazena referencia forte ao target. `deref()` retorna o target
//! enquanto vivo. Coleta automatica fica para PR futura.

pub mod abi;
pub mod rt;

pub use abi::WEAKREF_CLASS_SPEC;
