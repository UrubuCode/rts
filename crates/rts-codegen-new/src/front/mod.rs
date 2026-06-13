//! `front/` — parse real TypeScript and lower its typed HIR to native code.
//!
//! Increment 3 wires the new engine to REAL source for the first time. Until now
//! the crate only ran hand-built IR (`lower::ir::Func`, the P1 proofs). Here the
//! flow is the genuine front-end:
//!
//! ```text
//! TS source --swc--> rts-ast --rts-hir--> HirFunc --hir_lower--> Cranelift --JIT--> run
//! ```
//!
//! Scope: the **proven-monomorphic NUMERIC subset** only — the winning fast path
//! the whole engine exists to preserve. A function whose params/return are
//! `number`/`f64`/`f32`/`i32`/`bool` (and whose body uses only arithmetic,
//! comparisons, logical/unary ops, `let`/`const` locals, `if`/`while`/`return`,
//! and calls to other numeric functions) lowers to native `fadd`/`iadd`/`fcmp`/…
//! with NO boxing. Anything else is an EXPLICIT [`error::Unsupported`] — never a
//! silent wrong value. That soundness (no AST-shape guessing, no silent
//! mis-coercion) is the exact failure of the old engine this redesign refutes.
//!
//! Objects, strings, closures, containers, `any`, and 64-bit integers are LATER
//! increments; they bail here, on purpose.
//!
//! ## Module layout
//!
//! - `repr_map`: [`repr_map::repr_of`] — the [`crate::repr::Repr`] lattice
//!   applied to a [`rts_hir::HirType`] (the representation-choosing rule).
//! - `error`: the [`error::Unsupported`] bail type + `Result` alias.
//! - `hir_lower`: HIR → Cranelift lowering for the numeric subset (driver +
//!   expr + stmt submodules, each well under 500 lines).
//! - `jit`: the JIT harness — compile a [`rts_hir::HirFunc`] to executable
//!   memory and hand back a callable, with typed run wrappers.
//! - `tests` (`#[cfg(test)]`): REAL TS source strings parsed + lowered + JIT'd +
//!   executed, asserting native results.

pub mod error;
pub mod hir_lower;
pub mod jit;
pub mod repr_map;
pub mod run;

#[cfg(test)]
mod tests;

pub use error::{Unsupported, FrontResult};

use rts_hir::HirFunc;

/// Parse `src` and return the typed HIR for the single top-level function named
/// `name`.
///
/// This is the real front-end entry: `rts_parser::parse_source` (swc parse +
/// rts-ast lowering) followed by `rts_hir::lower::lower_func` per `Item::Function`.
/// All functions in the source are lowered into a shared HIR scope first (so each
/// one's signature is registered for the others to resolve cross-calls), then the
/// one named `name` is returned.
///
/// Errors: a parse failure, or no top-level function named `name`.
pub fn parse_function(src: &str, name: &str) -> anyhow::Result<HirFunc> {
    let (target, _others) = parse_functions(src, name)?;
    Ok(target)
}

/// Parse `src`, lower every top-level function to HIR, and return the one named
/// `name` together with all the others (in source order). The "others" let a
/// caller JIT a whole multi-function program (cross-calls resolve by name).
pub fn parse_functions(src: &str, name: &str) -> anyhow::Result<(HirFunc, Vec<HirFunc>)> {
    let program = rts_parser::parse_source(src)?;
    let mut scope = rts_hir::scope::Scope::new();

    let mut funcs: Vec<HirFunc> = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            funcs.push(rts_hir::lower::lower_func(fdecl, &mut scope));
        }
    }

    let idx = funcs
        .iter()
        .position(|f| f.name == name)
        .ok_or_else(|| anyhow::anyhow!("no top-level function named `{name}` in source"))?;
    let target = funcs.remove(idx);
    Ok((target, funcs))
}
