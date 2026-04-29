//! RTS compiler/runtime core.
//!
//! The legacy `RuntimeValue`/dispatch path has been removed. What remains is
//! the parser, type system, module resolver, diagnostics, linker and the new
//! ABI surface exposed by `crate::abi` + `crate::namespaces`. Codegen and
//! the AOT pipeline are being rebuilt on top of this foundation.

pub mod abi;
pub mod cache;
pub mod cli;
pub mod codegen;
pub mod compile_options;
pub mod diagnostics;
pub mod dotenv;
pub mod linker;
pub mod module;
pub mod namespaces;
pub mod nodespace;
pub mod parser;
pub mod pipeline;
pub mod registers;
pub mod runtime;
pub mod type_system;

pub(crate) mod runtime_objects;
