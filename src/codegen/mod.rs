//! Cranelift codegen for the RTS compiler.
//!
//! Compiles a full [`crate::parser::ast::Program`] — user functions,
//! control flow, variables, arithmetic, and namespace calls — into a
//! native object file via the `lower` module.

pub mod emit;
pub mod lower;
pub mod object;

pub use emit::compile_program_to_object;
pub use object::ObjectArtifact;
