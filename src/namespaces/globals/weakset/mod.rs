//! `WeakSet` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica quando elementos sao freed.

pub mod abi;
pub mod rt;

pub use abi::WEAKSET_CLASS_SPEC;
