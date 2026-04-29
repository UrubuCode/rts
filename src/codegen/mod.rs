//! Cranelift codegen for the RTS compiler.
//!
//! Compiles a full [`crate::parser::ast::Program`] — user functions,
//! control flow, variables, arithmetic, and namespace calls — into a
//! native object file via the `lower` module.

pub mod emit;
pub mod jit;
pub mod lower;
pub mod object;

pub use emit::compile_program_to_object;
pub use jit::compile_program_to_jit;
pub use object::ObjectArtifact;

use std::cell::Cell;

thread_local! {
    static DUMP_IR: Cell<bool> = const { Cell::new(false) };
}

/// Enable IR dumping for the current thread (used by `rts ir`).
pub fn enable_ir_dump() {
    DUMP_IR.with(|f| f.set(true));
}

/// Returns whether IR dumping is active on the current thread.
pub fn ir_dump_enabled() -> bool {
    DUMP_IR.with(|f| f.get())
}
