//! The ABI contract: the stable vocabulary shared between codegen and runtime.
//!
//! This crate owns the types both sides agree on — `AbiType`, `SymbolDesc`,
//! `NamespaceMember`/`NamespaceSpec`, the `__RTS_*` symbol naming convention,
//! signature lowering, handle-word layout, the guard decision table, the
//! JS error-kind enum, and the `Str` ABI helper. It is pure data and pure
//! functions: no heap, no GC, no Registry, no Cranelift. Anything that only
//! wants to NAME a symbol or describe a signature depends on this crate alone,
//! instead of pulling in the whole `rts-engine` (heap/GC/Registry)
//! implementation.
//!
//! `rts-engine` re-exports this crate's contents as `rts_engine::abi` so the
//! existing `rts_engine::abi::X` import paths keep compiling unchanged.

pub mod guards;
pub mod handles;
pub mod js_error;
pub mod member;
pub mod signature;
pub mod str_abi;
pub mod symbols;
pub mod ty;
pub mod types;

pub use js_error::JsErrorKind;
pub use member::{
    DefaultArg, MemberFlags, MemberKind, NamespaceMember, NamespaceSpec, concat_members,
};
pub use types::{AbiType, SymbolDesc};
