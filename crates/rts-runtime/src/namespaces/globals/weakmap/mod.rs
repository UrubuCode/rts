//! `WeakMap` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica de entries quando key e' freed
//! (faltando FinalizationRegistry / weak refs reais). Comporta como Map
//! forte com chaves u64 (handles).

pub mod abi;
pub mod rt;

pub use abi::WEAKMAP_CLASS_SPEC;
