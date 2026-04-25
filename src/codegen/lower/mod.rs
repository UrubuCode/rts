//! Full-program Cranelift lowering.
//!
//! Replaces the bootstrap `program.rs` extractor with a compiler that
//! handles variables, arithmetic, control flow, user functions, and
//! string operations.

pub mod ctx;
pub mod expressions;
pub mod func;
pub mod statements;

pub use func::compile_program;
