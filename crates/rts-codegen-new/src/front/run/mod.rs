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

mod assign;
mod binop;
mod call;
mod call_shape;
mod call_spread;
mod class;
mod desugar;
mod expr;
mod funcval;
mod globalclass;
mod globals;
mod loops;
mod mathobj;
mod method;
mod method_array;
mod method_dyn;
pub mod module_jit;
mod newexpr;
mod obj;
mod optchain_lower;
mod objstatic;
mod sig;
mod stmt;
mod thunk;

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
    let prog = build_program(src)?;
    let program = module_jit::compile_program(&prog)?;
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
    let prog = build_program(src)?;
    let program = module_jit::compile_program(&prog)?;
    let ((), out) = crate::value::abi_adapter::with_capture(|| program.run_main());
    Ok(out)
}

/// A lowered program ready to JIT: the user functions (incl. synthesized class
/// constructors/methods + extracted arrows), the synthesized `__rtsn_main`, the
/// class table, and the synthesized-fn → owning-class map (for binding `this`).
pub(crate) struct LoweredProgram {
    pub funcs: Vec<HirFunc>,
    pub main: HirFunc,
    pub classes: class::ClassTable,
    /// synthesized constructor/method fn name → its class name (so the lowerer
    /// binds `this` to that class). Absent for ordinary user functions.
    pub fn_this_class: std::collections::HashMap<String, String>,
    /// synthesized CLOSURE fn name → ordered captured outer-local names (P5.7).
    /// Drives env construction at reify and the env-read split in the thunk.
    pub captures: std::collections::HashMap<String, Vec<String>>,
}

/// Parse `src` and lower it to (user functions, synthesized `__rtsn_main`).
///
/// All top-level functions are lowered into a single shared HIR scope first (so
/// each function's signature is registered for the others — and for the main
/// body — to resolve cross-calls and return types), then the top-level
/// statements are lowered against that same scope and wrapped as the body of a
/// synthetic void function named `__rtsn_main`.
fn build_program(src: &str) -> FrontResult<LoweredProgram> {
    let program = rts_parser::parse_source(src)
        .map_err(|e| Unsupported::new(format!("parse error: {e}")))?;

    let mut scope = rts_hir::scope::Scope::new();

    // 0. Collect every `class` declaration into descriptors + synthesized
    //    constructor/method HirFuncs (each with `this` as the implicit first
    //    param). Register each class name in `scope` so `new C(..)` resolves its
    //    type. A class outside the no-inheritance subset bails the whole program.
    let class_decls: Vec<&rts_ast::ast::ClassDecl> = program
        .items
        .iter()
        .filter_map(|it| match it {
            rts_ast::ast::Item::Class(c) => Some(c),
            _ => None,
        })
        .collect();
    for c in &class_decls {
        scope.register_class(c.name.clone());
    }
    let (classes, class_funcs) = class::collect_classes(&class_decls)?;

    // Map each synthesized constructor/method fn name → its class (for `this`).
    let mut fn_this_class: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // Iterate every collected descriptor — user-declared AND synthesized virtual
    // builtin-error parents (P5.3), whose ctor/toString also bind `this`.
    for desc in classes.iter() {
        fn_this_class.insert(desc.ctor.clone(), desc.name.clone());
        // Bind `this` for every instance method + accessor to the class on
        // which it is SYNTHESIZED (the function names encode that class). A
        // method/accessor inherited unchanged keeps its declaring class's
        // `this` (its body references its OWN class's fields — which sit at the
        // same flattened slots in the child, so the binding is sound either way).
        for fn_name in desc.methods.values() {
            fn_this_class.entry(fn_name.clone()).or_insert(desc.name.clone());
        }
        for acc in desc.accessors.values() {
            for fn_name in [acc.getter.as_ref(), acc.setter.as_ref()].into_iter().flatten() {
                fn_this_class.entry(fn_name.clone()).or_insert(desc.name.clone());
            }
        }
    }

    // 1. Lower all top-level functions (registers their signatures in `scope`).
    let mut funcs: Vec<HirFunc> = class_funcs;
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

    let mut main = HirFunc {
        name: "__rtsn_main".to_string(),
        params: Vec::new(),
        ret: HirType::Void,
        body,
        is_async: false,
        is_arrow: false,
    };

    // P5.8: recover template literals + optional chaining. rts-hir lowered both to
    // structureless `Raw` placeholders (it cannot model them and we must not modify
    // that crate); this pass re-walks the parsed swc AST PAIRED with the HIR and
    // rewrites each placeholder into ordinary HIR (a string `+` chain / a guarded
    // ternary). Run BEFORE arrow extraction so a template/chain inside a top-level
    // arrow is rewritten while still in the `main` body it was parsed from.
    desugar::desugar(&program, &mut main.body, &mut funcs);

    // P5.11: destructuring — array `[a, b, ...rest]` / object `{x, y: z, w = 5}`
    // patterns in let/const, for-of bindings, and function parameters. rts-hir
    // flattened every pattern to a single `"_"` name (keeping the initializer);
    // this pass re-reads the swc AST (incl. a fresh swc re-parse for the param
    // patterns rts-ast does not carry) and expands each into element/property reads
    // the existing lowerer runs. Run AFTER the template/optchain desugar (so a
    // template inside a destructured initializer is already real HIR) and BEFORE
    // arrow extraction (so a destructuring let inside a top-level arrow is expanded
    // while still in the main body).
    desugar::desugar_destructure(src, &program, &mut main.body, &mut funcs);

    // P4.6/P5.7: extract every inline arrow used as a value (an arg, a returned
    // arrow) into a fresh top-level function, rewriting the `Arrow` node to an
    // `Ident` of the synthesized name. A non-capturing arrow becomes a plain
    // function; a capturing arrow becomes a CLOSURE (captures prepended as leading
    // params + recorded in `captures`). Unsound captures are left to bail.
    let extracted = funcval::extract_arrows(&mut funcs, &mut main);
    funcs.extend(extracted.funcs);

    Ok(LoweredProgram { funcs, main, classes, fn_this_class, captures: extracted.captures })
}
