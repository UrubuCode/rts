//! Next-generation ABI boundary between Cranelift codegen and Rust runtime.
//!
//! This module defines the contract used when codegen emits direct calls to
//! runtime-exported symbols. Unlike the legacy `namespaces::abi`, this layer
//! carries zero polymorphic values: every member declares primitive argument
//! and return types that map 1:1 onto the C ABI.
//!
//! Populated incrementally. While empty, the runtime continues to rely on the
//! legacy dispatch path; namespaces migrate one at a time.

pub mod guards;
pub mod member;
pub mod signature;
pub mod symbols;
pub mod types;

#[cfg(test)]
mod tests;

pub use member::{Intrinsic, MemberKind, NamespaceMember, NamespaceSpec};
pub use types::AbiType;

/// Global registry of namespaces exposed through the new ABI.
///
/// Each migrated namespace appends its `SPEC` here. Codegen consults this
/// table to resolve callees into symbol names and signatures without
/// dispatch overhead.
pub const SPECS: &[&NamespaceSpec] = &[
    &crate::namespaces::gc::abi::SPEC,
    &crate::namespaces::io::abi::SPEC,
    &crate::namespaces::fs::abi::SPEC,
    &crate::namespaces::math::abi::SPEC,
    &crate::namespaces::num::abi::SPEC,
    &crate::namespaces::mem::abi::SPEC,
    &crate::namespaces::backtrace::abi::SPEC,
    &crate::namespaces::bigfloat::abi::SPEC,
    &crate::namespaces::time::abi::SPEC,
    &crate::namespaces::env::abi::SPEC,
    &crate::namespaces::path::abi::SPEC,
    &crate::namespaces::buffer::abi::SPEC,
    &crate::namespaces::string::abi::SPEC,
    &crate::namespaces::process::abi::SPEC,
    &crate::namespaces::os::abi::SPEC,
    &crate::namespaces::collections::abi::SPEC,
    &crate::namespaces::hash::abi::SPEC,
    &crate::namespaces::hint::abi::SPEC,
    &crate::namespaces::fmt::abi::SPEC,
    &crate::namespaces::crypto::abi::SPEC,
    &crate::namespaces::regex::abi::SPEC,
    &crate::namespaces::ui::abi::SPEC,
];

/// Locates a member by its fully qualified name (e.g. `"io.print"`).
pub fn lookup(qualified: &str) -> Option<(&'static NamespaceSpec, &'static NamespaceMember)> {
    let (ns_name, fn_name) = qualified.split_once('.')?;
    let spec = SPECS.iter().copied().find(|spec| spec.name == ns_name)?;
    let member = spec.members.iter().find(|m| m.name == fn_name)?;
    Some((spec, member))
}
