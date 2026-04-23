//! Full-program Cranelift lowering.
//!
//! Replaces the bootstrap `program.rs` extractor with a compiler that
//! handles variables, arithmetic, control flow, user functions, and
//! string operations.

pub mod ctx;
pub mod expr;
pub mod func;
pub mod stmt;

pub use func::compile_program;
