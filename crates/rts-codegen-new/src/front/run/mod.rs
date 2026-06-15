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
mod dateclass;
mod desugar;
mod engineobj;
mod expr;
mod funcval;
mod globalclass;
mod globals;
mod loops;
mod mathobj;
mod method;
mod method_array;
mod method_dyn;
mod module_entry;
pub mod module_jit;
mod newexpr;
mod obj;
mod objstatic;
mod optchain_lower;
mod regex;
mod registry;
mod registry_call;
mod sig;
mod stmt;
mod thunk;
mod toprimitive;
mod trycatch;

pub mod lower;

pub use module_entry::{render_path, run_path};

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
    let prog = build_with_includes(src)?;
    let program = module_jit::compile_program(&prog)?;
    program.run_main();
    Ok(())
}

/// Build `src` with the embedded stdlib `include`s (if any) prepended as a
/// declarations-only prelude via [`merge_programs`]. With NO includes registered
/// this is exactly [`build_program`] — zero behavior change.
fn build_with_includes(src: &str) -> FrontResult<LoweredProgram> {
    let inc = registry::includes_prelude();
    if inc.is_empty() {
        build_program(src)
    } else {
        let prelude = build_program(&inc)?;
        let user = build_program(src)?;
        merge_programs(prelude, user)
    }
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
    let prog = build_with_includes(src)?;
    render_source_core(&prog)
}

/// Compile an already-built [`LoweredProgram`] and run it with `console.log`
/// output CAPTURED into a `String`. Shared by [`render_source`] (string path) and
/// [`module_entry::render_path`] (disk multi-file path).
fn render_source_core(prog: &LoweredProgram) -> FrontResult<String> {
    let program = module_jit::compile_program(prog)?;
    let ((), out) = crate::value::abi_adapter::with_capture(|| program.run_main());
    Ok(out)
}

/// The prelude-side [`build_program`] used by [`module_entry`] to compile the
/// embedded stdlib includes ahead of a multi-file user program. A thin alias so
/// the private `build_program` stays the single post-parse entry.
fn build_program_for_prelude(src: &str) -> FrontResult<LoweredProgram> {
    build_program(src)
}

/// Compile `prelude_src` (a declarations-only TS stdlib) ahead of `user_src`,
/// merged into one module so the prelude's classes/functions are ambient in the
/// user program (a prelude `class Map` shadows the native Map). Returns captured
/// stdout. The prelude must contain only declarations (no top-level statements).
pub fn render_source_with_prelude(prelude_src: &str, user_src: &str) -> FrontResult<String> {
    let prelude = build_program(prelude_src)?;
    let user = build_program(user_src)?;
    let merged = merge_programs(prelude, user)?;
    let program = module_jit::compile_program(&merged)?;
    let ((), out) = crate::value::abi_adapter::with_capture(|| program.run_main());
    Ok(out)
}

/// Merge a declarations-only `prelude` program ahead of the `user` program into a
/// single `LoweredProgram`. The prelude's top-level classes/functions become
/// ambient in the user program; on name collision the user wins (last-wins),
/// which is exactly the shadow case (a user `class Map` overriding a prelude one).
///
/// The prelude must carry NO top-level statements (its `__rtsn_main` body must be
/// empty) — only class/function declarations. Top-level code in the prelude is
/// unsupported (there is one `__rtsn_main`, which belongs to the user program).
fn merge_programs(prelude: LoweredProgram, user: LoweredProgram) -> FrontResult<LoweredProgram> {
    if !prelude.main.body.is_empty() {
        return Err(Unsupported::new(
            "prelude must be declarations-only (no top-level statements)".to_string(),
        ));
    }

    // ClassTable: prelude first, then user (user can override).
    let mut classes = prelude.classes;
    for desc in user.classes.iter() {
        classes.insert(desc.clone());
    }

    // PRIVACY GATE: record which function names came from the PRELUDE (the engine's
    // embedded includes). The PRIVATE `engine` global is resolvable ONLY from these
    // functions; a user function naming `engine.*` bails explicitly (see
    // `engineobj`). The prelude's own classes/methods/arrows are all in
    // `prelude.funcs`, so their names are exactly this set (the user's `__rtsn_main`
    // and user functions are NOT included — `merge_programs` keeps `user.main`).
    let prelude_fns: std::collections::HashSet<String> =
        prelude.funcs.iter().map(|f| f.name.clone()).collect();

    // funcs: prelude first, then user (user appended; last-wins on name collision).
    let mut funcs = prelude.funcs;
    funcs.extend(user.funcs);

    // fn_this_class + captures: prelude then user.
    let mut fn_this_class = prelude.fn_this_class;
    fn_this_class.extend(user.fn_this_class);
    let mut captures = prelude.captures;
    captures.extend(user.captures);

    Ok(LoweredProgram {
        funcs,
        main: user.main,
        classes,
        fn_this_class,
        captures,
        prelude_fns,
    })
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
    /// PRIVACY GATE (engine namespace): the set of function names that came from
    /// the engine's PRELUDE (embedded TS includes). Only these functions may name
    /// the PRIVATE `engine.*` global; a user function (incl. `__rtsn_main`) that
    /// references it bails explicitly. Empty for a user-only program (no prelude),
    /// so the gate denies `engine.*` everywhere unless a prelude is present.
    pub prelude_fns: std::collections::HashSet<String>,
}

/// Parse `src` and lower it to (user functions, synthesized `__rtsn_main`).
///
/// All top-level functions are lowered into a single shared HIR scope first (so
/// each function's signature is registered for the others — and for the main
/// body — to resolve cross-calls and return types), then the top-level
/// statements are lowered against that same scope and wrapped as the body of a
/// synthetic void function named `__rtsn_main`.
fn build_program(src: &str) -> FrontResult<LoweredProgram> {
    let program =
        rts_parser::parse_source(src).map_err(|e| Unsupported::new(format!("parse error: {e}")))?;
    build_from_program(program, src)
}

/// Lower an ALREADY-PARSED `rts_ast::Program` to a [`LoweredProgram`]. This is the
/// shared post-parse body of [`build_program`]: it runs every desugar pass, the
/// `this`-transform, and arrow extraction, then assembles the funcs/main/classes.
///
/// `destructure_src` is the ORIGINAL source string used ONLY to recover function
/// PARAMETER destructuring patterns (rts-ast drops them; the pass re-parses the
/// source). For the single-string path it is the real source; for the multi-file
/// module path (where there is no single source string) it is `""`, so a
/// destructured PARAM simply stays `"_"` and bails at lowering — sound, never
/// wrong. Every other destructuring site (let/const/for-of) recovers from the swc
/// `Stmt` nodes carried in the flattened program, so it works across modules.
fn build_from_program(
    program: rts_ast::ast::Program,
    destructure_src: &str,
) -> FrontResult<LoweredProgram> {
    let src = destructure_src;
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
    let (mut classes, class_funcs) = class::collect_classes(&class_decls)?;

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

    // P5.15: recover OBJECT-LITERAL METHODS. rts-hir drops every method prop from an
    // object literal; this pass re-reads the swc AST PAIRED with the HIR, synthesizes
    // a "literal class" (fields + `this`-first method funcs) per method-bearing
    // literal, registers it, and prepends a `__rtsl_class__` marker field to the HIR
    // object so `lower_object_literal` records the local's class. Run FIRST (before
    // the template/destructure rewrites touch the object's field VALUES, and before
    // arrow extraction) so the swc/HIR positional pairing is on the clean lowering.
    desugar::desugar_obj_methods(&program, &mut main.body, &mut funcs, &mut classes);

    // Map each synthesized constructor/method fn name → its class (for `this`). Built
    // AFTER object-literal recovery so the synthesized literal-class methods are
    // included. Iterates every collected descriptor — user classes, virtual
    // builtin-error parents (P5.3), AND literal classes (P5.15).
    let mut fn_this_class: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for desc in classes.iter() {
        fn_this_class.insert(desc.ctor.clone(), desc.name.clone());
        for fn_name in desc.methods.values() {
            fn_this_class
                .entry(fn_name.clone())
                .or_insert(desc.name.clone());
        }
        for acc in desc.accessors.values() {
            for fn_name in [acc.getter.as_ref(), acc.setter.as_ref()]
                .into_iter()
                .flatten()
            {
                fn_this_class
                    .entry(fn_name.clone())
                    .or_insert(desc.name.clone());
            }
        }
    }

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

    // Phase 1 — FREE function `this`. A top-level `function F(){…}` (or a hoisted
    // `const F = function(){…}` — both reach `funcs` as `HirFunc`s) whose body
    // references `this` gets a synthesized leading `this` param (param 0) and its
    // `Raw("This")` nodes rewritten to `Ident("this")` (reusing the class
    // `this`-rewrite). A PLAIN call `F(args)` then passes `undefined` as the
    // receiver (see `lower_user_call`). Skips: class-synthesized ctors/methods
    // (already in `fn_this_class`, already `this`-first and rewritten), any fn whose
    // first param is already `this` (idempotent), and `async` fns (`this` in an
    // async free fn is out of this increment — left to bail). `new F()` passing a
    // real instance is PHASE 2.
    transform_free_this(&mut funcs, &fn_this_class);

    // P4.6/P5.7: extract every inline arrow used as a value (an arg, a returned
    // arrow) into a fresh top-level function, rewriting the `Arrow` node to an
    // `Ident` of the synthesized name. A non-capturing arrow becomes a plain
    // function; a capturing arrow becomes a CLOSURE (captures prepended as leading
    // params + recorded in `captures`). Unsound captures are left to bail.
    let extracted = funcval::extract_arrows(&mut funcs, &mut main);
    funcs.extend(extracted.funcs);

    Ok(LoweredProgram {
        funcs,
        main,
        classes,
        fn_this_class,
        captures: extracted.captures,
        // A single `build_program` has no prelude/user split — every fn is "user"
        // here. When `merge_programs` combines a prelude + user program it computes
        // the real prelude-origin set; this empty default denies `engine.*` for a
        // user-only program (the gate's safe default).
        prelude_fns: std::collections::HashSet::new(),
    })
}

/// Phase 1 free-function `this` transform: give every FREE function (a top-level
/// `function`/function-expression) whose body references `this` a synthesized
/// leading `this` parameter and rewrite its `Raw("This")` nodes to `Ident("this")`.
///
/// A function is SKIPPED when it is a class-synthesized ctor/method (its name is in
/// `fn_this_class` — it already binds `this` via the class machinery), when its
/// first param is already named `this` (idempotent / already transformed), or when
/// it is `async` (a free `this` in an async fn is out of this increment and is left
/// to bail at lowering). The rewrite REUSES the class `this`-rewrite so both paths
/// share one traversal.
///
/// (Hoisted top-level arrows reach `funcs` as `HirFunc`s indistinguishable from
/// real `function`s here; an arrow's `this` is lexically the enclosing scope's,
/// which at top level is `undefined` — exactly the receiver a plain call passes —
/// so the Phase 1 transform is observably correct for the top-level case. Nested
/// arrows capturing a non-top-level `this` are a later increment.)
fn transform_free_this(
    funcs: &mut [HirFunc],
    fn_this_class: &std::collections::HashMap<String, String>,
) {
    for f in funcs.iter_mut() {
        if fn_this_class.contains_key(&f.name) {
            continue; // class ctor/method — `this` bound via the class machinery.
        }
        if f.is_async {
            continue; // async free `this` — out of this increment (bails at lowering).
        }
        if f.params.first().is_some_and(|p| p.name == class::THIS) {
            continue; // already `this`-first (a class fn or already transformed).
        }
        if !class::body_uses_raw_this(&f.body) {
            continue; // no `this` reference — unchanged (no `this` param).
        }
        // Prepend the synthesized `this` param (Tagged/opaque) and rewrite the body.
        let mut params = Vec::with_capacity(f.params.len() + 1);
        params.push(class::this_param());
        params.extend(f.params.drain(..));
        f.params = params;
        class::rewrite_this_block(&mut f.body);
    }
}
