//! `front/run` — run a whole real `.ts` program and capture its stdout.
//!
//! Increment 4: the new engine executes a REAL program for the first time —
//! top-level code + function defs + cross-function calls + the Tagged/string/
//! polymorphic path + `console.log` — through
//!
//! ```text
//! TS --swc--> rts-ast --rts-hir--> {HirFunc...} + __rtsn_main
//!         --run::lower--> Cranelift module --JIT--> execute --> captured stdout
//! ```
//!
//! [`run_source`] is the entry: it parses the whole `Program`, lowers every
//! top-level function to HIR and synthesizes a `__rtsn_main` from the top-level
//! statements, JITs the module with the runtime symbols installed, resets the
//! console capture buffer, calls `__rtsn_main`, and returns what it printed.
//!
//! If ANY function or the main body hits an unsupported construct the whole run
//! returns `Err(Unsupported)` — the program never runs partially, and never
//! emits a wrong value (the soundness floor this redesign exists to keep).
//!
//! Submodules (each < 500 lines): [`sig`] (per-fn ABI signatures), [`lower`]
//! (driver + locals + coercions), [`expr`] (expressions, incl. the Tagged path),
//! [`stmt`] (statements + control flow), [`module_jit`] (N-function JIT).

mod call;
mod expr;
pub mod module_jit;
mod sig;
mod stmt;

pub mod lower;

#[cfg(test)]
mod fixture_check;
#[cfg(test)]
mod tests;

use rts_hir::ir::{HirFunc, HirType};

use super::error::{FrontResult, Unsupported};

/// Parse, lower, JIT, and run `src`, returning everything `console.log` printed
/// (each line terminated by `"\n"`, matching Node/Bun).
///
/// Errors:
/// - a parse error (returned as an `Unsupported` wrapping the message), or
/// - any construct outside the implemented subset (an explicit `Unsupported`).
pub fn run_source(src: &str) -> FrontResult<String> {
    let (funcs, main) = build_program(src)?;
    let program = module_jit::compile_program(&funcs, &main)?;

    // Hold the run-lock so reset → run → take is atomic: the console capture
    // buffer is process-global, so concurrent runs (parallel tests) would
    // otherwise interleave. Compilation above touches no shared state, so only
    // the execution window is serialized.
    let _guard = crate::runtime::console::run_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    crate::runtime::console::reset_output();
    program.run_main();
    Ok(crate::runtime::console::take_output())
}

/// Parse `src` and lower it to (user functions, synthesized `__rtsn_main`).
///
/// All top-level functions are lowered into a single shared HIR scope first (so
/// each function's signature is registered for the others — and for the main
/// body — to resolve cross-calls and return types), then the top-level
/// statements are lowered against that same scope and wrapped as the body of a
/// synthetic void function named `__rtsn_main`.
fn build_program(src: &str) -> FrontResult<(Vec<HirFunc>, HirFunc)> {
    let program = rts_parser::parse_source(src)
        .map_err(|e| Unsupported::new(format!("parse error: {e}")))?;

    let mut scope = rts_hir::scope::Scope::new();

    // 1. Lower all top-level functions (registers their signatures in `scope`).
    let mut funcs: Vec<HirFunc> = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            funcs.push(rts_hir::lower::lower_func(fdecl, &mut scope));
        }
    }

    // 2. Lower the top-level statements (everything that is `Item::Statement`)
    //    against the same scope, in source order.
    let mut top_stmts: Vec<rts_ast::ast::Statement> = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Statement(stmt) = item {
            top_stmts.push(stmt.clone());
        }
    }
    let body = rts_hir::lower::lower_stmts(&top_stmts, &mut scope);

    let main = HirFunc {
        name: "__rtsn_main".to_string(),
        params: Vec::new(),
        ret: HirType::Void,
        body,
        is_async: false,
        is_arrow: false,
    };

    Ok((funcs, main))
}
