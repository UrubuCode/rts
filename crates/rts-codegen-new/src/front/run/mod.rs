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
mod method;
pub mod module_jit;
mod obj;
mod sig;
mod stmt;

pub mod lower;

#[cfg(test)]
mod fixture_check;
#[cfg(test)]
mod tests;

use rts_hir::ir::{HirFunc, HirType};

use super::error::{FrontResult, Unsupported};

/// Parse, lower, JIT, and RUN `src` against the REAL runtime. `console.log`
/// output goes to the process's real stdout via `__RTS_FN_NS_IO_PRINT`. Returns
/// `Ok(())` once `__rtsn_main` finishes (the bun fixture harness validates true
/// end-to-end stdout against `bun`).
///
/// Errors:
/// - a parse error (returned as an `Unsupported` wrapping the message), or
/// - any construct outside the implemented subset (an explicit `Unsupported`).
pub fn run_source(src: &str) -> FrontResult<()> {
    let (funcs, main) = build_program(src)?;
    let program = module_jit::compile_program(&funcs, &main)?;
    program.run_main();
    Ok(())
}

/// Parse, lower, JIT, and run `src` with `console.log` output CAPTURED into a
/// `String` instead of stdout — returning everything it printed (each line
/// terminated by `"\n"`, matching Node/Bun).
///
/// rts-std's io layer has no stdout-redirect hook, so the capture is done at the
/// adapter's `__rtsadp_print_line` trampoline (thread-local), which STILL runs
/// the real string pool (NEW/CONCAT/PTR/LEN) that produced the line — only the
/// final write target differs. Used by the in-process unit tests; for true
/// end-to-end stdout use [`run_source`] + the bun fixture harness.
pub fn render_source(src: &str) -> FrontResult<String> {
    let (funcs, main) = build_program(src)?;
    let program = module_jit::compile_program(&funcs, &main)?;
    let ((), out) = crate::value::abi_adapter::with_capture(|| program.run_main());
    Ok(out)
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
