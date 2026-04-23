//! RTS compiler/runtime core.
//!
//! The legacy `RuntimeValue`/dispatch path has been removed. What remains is
//! the parser, type system, module resolver, diagnostics, linker and the new
//! ABI surface exposed by `crate::abi` + `crate::namespaces`. Codegen and
//! the AOT pipeline are being rebuilt on top of this foundation.

pub mod abi;
pub mod cli;
pub mod codegen;
pub mod compile_options;
pub mod diagnostics;
pub mod linker;
pub mod module;
pub mod namespaces;
pub mod parser;
pub mod pipeline;
pub mod runtime;
pub mod type_system;
