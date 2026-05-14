//! `FinalizationRegistry` global class (#685 v0) — stub sem GC weak real.
//!
//! v0: register/unregister sao noops. Callback nunca dispara — RTS nao tem
//! weak ref real ainda. Cobre a shape API exigida por libs que checam
//! disponibilidade.

pub mod abi;
pub mod rt;

pub use abi::FINALIZATION_REGISTRY_CLASS_SPEC;
