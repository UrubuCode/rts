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

mod argsobj;
mod assign;
mod asyncspawn;
mod binop;
mod breakcont;
mod dynfn;
mod fnhoist;
mod floatscan;
mod binop_eq;
mod ctorfn;
mod call;
mod call_shape;
mod call_spread;
mod aot_str;
mod cell;
mod class;
mod desugar;
mod engineobj;
mod expr;
mod funcval;
mod gcell;
mod globalclass;
mod globalclass_receiver;
mod globals;
mod loops;
mod mathobj;
mod methodtable;
mod method;
mod method_array;
mod method_array_coerce;
mod method_dyn;
mod module_aot;
mod module_entry;
pub mod module_jit;
mod newexpr;
mod obj;
mod objstatic;
mod optchain_lower;
mod regex;
pub(crate) mod registry;
mod registry_build;
mod registry_call;
mod registryclass;
mod sig;
mod stmt;
mod stmt_assign;
mod stmt_let;
mod switch;
mod tco;
mod thunk;
mod varhoist;
mod toprimitive;
mod trycatch;

pub mod lower;

pub use module_entry::{compile_path_to_object, dump_ir_path, render_path, run_path};

#[cfg(test)]
mod fixture_check;
#[cfg(test)]
mod tests;

use rts_hir::HirStmt;
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
    // An UNCAUGHT top-level throw (e.g. a runtime `TypeError: not a function`)
    // left the pending-error slot set — surface it as an error, exactly like
    // [`module_entry::run_path`] does for the disk path. Without this, a unit
    // test over a throwing program would silently pass with partial output.
    if crate::value::errslot::__rtsadp_err_pending() != 0 {
        let word = crate::value::errslot::__rtsadp_err_take();
        let s_word = crate::value::genops::__rtsadp_to_string(word);
        let text =
            crate::value::abi_adapter::resolve_poly(crate::value::PolyValue::from_raw(s_word));
        return Err(crate::front::error::Unsupported::new(format!(
            "uncaught exception: {text}"
        )));
    }
    Ok(())
}

/// `rts ir -e` — build + JIT-compile `src` (a single source string, no disk
/// imports) with the per-function Cranelift IR dump enabled, WITHOUT running
/// `__rtsn_main`. The string-source twin of [`module_entry::dump_ir_path`].
pub fn dump_ir_source(src: &str) -> FrontResult<()> {
    let prog = build_with_includes(src)?;
    module_jit::set_dump_ir(true);
    let res = module_jit::compile_program(&prog);
    module_jit::set_dump_ir(false);
    res.map(|_| ())
}

/// Build `src` with the embedded stdlib `include`s (if any) prepended as a
/// declarations-only prelude via [`merge_programs`]. With NO includes registered
/// this is exactly [`build_program`] — zero behavior change.
///
/// The prelude is built FIRST; its [`class::ClassTable`] is passed as AMBIENT
/// classes to the user build so a user `class X extends Error` (or `extends Map`)
/// resolves its parent against the real prelude class (the engine no longer
/// synthesizes a virtual builtin-error parent in codegen).
fn build_with_includes(src: &str) -> FrontResult<LoweredProgram> {
    let inc = registry::includes_prelude();
    if inc.is_empty() {
        build_program(src, "")
    } else {
        let prelude = build_program(&inc, PRELUDE_ARROW_NS)?;
        let ambient_fns = prelude_fn_names(&prelude);
        let user = build_program_with_ambient(src, &prelude.classes, &ambient_fns)?;
        merge_programs(prelude, user)
    }
}

/// Arrow-name namespace for the PRELUDE build (`__rtsn_arrow_p{N}`). The prelude
/// and the user program are lowered separately (each arrow counter starts at 0)
/// and merged last-wins — without distinct namespaces a prelude arrow and a user
/// arrow collided on `__rtsn_arrow_0`.
const PRELUDE_ARROW_NS: &str = "p";

/// The set of AMBIENT names a `prelude` exposes to a user program — its top-level
/// FUNCTIONS (`rts:test` describe/test/expect, primordial `.ts` helpers) AND its
/// top-level SINGLETON INSTANCES (`const console = new Console()`). Seeded into the
/// user build's arrow extraction so a callback arrow naming one (`x =>
/// console.log(x)`) is NOT misjudged a capture (the singleton resolves as a shared
/// gcell after the merge, not a captured outer local). See [`build_from_program`]
/// → [`funcval::extract_arrows`].
pub(super) fn prelude_fn_names(prelude: &LoweredProgram) -> std::collections::HashSet<String> {
    let mut names: std::collections::HashSet<String> =
        prelude.funcs.iter().map(|f| f.name.clone()).collect();
    // Top-level `const x = new C()` singletons in the prelude main (`console`) are
    // ambient non-captures for the user side too — matched by SHAPE, no name list.
    for s in &prelude.main.body {
        if let rts_hir::ir::HirStmt::Const { name, init, .. } = s {
            if matches!(&init.kind, rts_hir::ir::HirExprKind::New { .. }) {
                names.insert(name.clone());
            }
        }
    }
    names
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
    build_program(src, PRELUDE_ARROW_NS)
}

/// [`build_program`] with AMBIENT (prelude) classes available for `extends`
/// resolution: a user `class X extends Error`/`Map` resolves its parent against
/// `ambient`. Used by the user side of every prelude+user build.
fn build_program_with_ambient(
    src: &str,
    ambient: &class::ClassTable,
    ambient_fns: &std::collections::HashSet<String>,
) -> FrontResult<LoweredProgram> {
    let program =
        rts_parser::parse_source(src).map_err(|e| Unsupported::new(format!("parse error: {e}")))?;
    build_from_program(program, src, ambient, ambient_fns, "")
}

/// Compile `prelude_src` (a declarations-only TS stdlib) ahead of `user_src`,
/// merged into one module so the prelude's classes/functions are ambient in the
/// user program (a prelude `class Map` shadows the native Map). Returns captured
/// stdout. The prelude must contain only declarations (no top-level statements).
pub fn render_source_with_prelude(prelude_src: &str, user_src: &str) -> FrontResult<String> {
    // The engine's embedded includes (the `.ts` `class String`/`Number`/`Error`/…)
    // are ALWAYS present in a real compile (`build_with_includes`), so the test
    // prelude is composed AHEAD-OF the user prelude — both are declarations-only.
    // This keeps the primitive→prelude method dispatch (e.g. `s.charCodeAt(i)` on a
    // native-string field) available to the user prelude's classes, exactly as in
    // a real program.
    let engine_inc = registry::includes_prelude();
    let merged_prelude_src = if engine_inc.is_empty() {
        prelude_src.to_string()
    } else {
        format!("{engine_inc}\n{prelude_src}")
    };
    let prelude = build_program(&merged_prelude_src, PRELUDE_ARROW_NS)?;
    let ambient_fns = prelude_fn_names(&prelude);
    let user = build_program_with_ambient(user_src, &prelude.classes, &ambient_fns)?;
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
/// The prelude MAY carry top-level statements (module-level `let`/`const`
/// initializers, e.g. the `rts:test` bundle's hook-slot globals `let
/// _before_all_fn = 0`): they are PREPENDED to the user's `__rtsn_main` body so
/// they run FIRST (initialization order: prelude then user). The combined main is
/// the single `__rtsn_main`; module-level mutable globals are recomputed over it so
/// a prelude top-level `let` written from a prelude function (the hook setters)
/// becomes a shared cell exactly like a user one.
fn merge_programs(prelude: LoweredProgram, user: LoweredProgram) -> FrontResult<LoweredProgram> {
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
    // FUNCTION-LOCAL CELLS (#195): union (prelude then user). Keyed by function name;
    // the two sides' synthesized-fn names never collide, so a plain extend is exact.
    let mut cells = prelude.cells;
    cells.extend(user.cells);

    // builtins: prelude then user (the prelude does not import `rts:<ns>` today, but
    // merging keeps the type total and user wins on a name clash).
    let mut builtins = prelude.builtins;
    builtins.extend(user.builtins);
    // CLASS-TYPED PARAMS: union (prelude then user). Keyed by function name; the two
    // sides' top-level function names do not collide in practice, and a user win is
    // the right shadow behavior.
    let mut param_classes = prelude.param_classes;
    param_classes.extend(user.param_classes);

    // MAIN: prepend the prelude's top-level statements (its initializers) ahead of
    // the user's, in ONE `__rtsn_main`. Run order is prelude-first, so a prelude
    // module-global is initialized before any user code observes it.
    let mut main = user.main;
    if !prelude.main.body.is_empty() {
        let mut body = prelude.main.body;
        body.extend(std::mem::take(&mut main.body));
        main.body = body;
    }

    // Recompute the module-globals over the MERGED funcs + the COMBINED main so cell
    // ids are unique across prelude+user and a prelude top-level `let` written from a
    // prelude function (the `rts:test` hook setters) is promoted to a shared cell.
    // The force-promotion set (captured-and-mutated top lets) is the UNION of both
    // sides — arrows are already lifted here, so the names were computed pre-extract
    // and carried on each program; re-promoting keeps the lifted arrows' free reads
    // resolving to the cell rather than an unbound ident.
    let mut forced_globals = prelude.forced_globals;
    forced_globals.extend(user.forced_globals);
    // Top-level singleton-instance globals (`const X = new Y()`, and a `const X =
    // <builtin import>(…)` whose spec ret names a registered class) over the
    // COMBINED main — data-driven, no name allow-list. Force-promote each to a cell
    // (so it reaches inside functions) and carry `name → class` so the receiver-class
    // lookups recover what the gcell erases. This is the site that sees the merged
    // `builtins` map, so it is where an imported-ctor singleton is classified.
    let gcell_classes = funcval::singleton_instance_globals(&funcs, &main, &builtins);
    forced_globals.extend(gcell_classes.keys().cloned());
    let gcells = funcval::module_globals(&funcs, &main, &forced_globals);

    Ok(LoweredProgram {
        funcs,
        main,
        classes,
        fn_this_class,
        captures,
        cells,
        gcells,
        gcell_classes,
        forced_globals,
        prelude_fns,
        builtins,
        param_classes,
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
    /// FUNCTION-LOCAL CELLS (#195): function name → its LOCAL names promoted to
    /// per-invocation cells (a closure captures + mutates them). Keyed under both the
    /// declaring function and the synthesized closure. Union of prelude + user on
    /// merge. See [`funcval::extract_arrows`] / [`cell`].
    pub cells: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// MODULE-LEVEL MUTABLE GLOBALS (epic #195): top-level `let` name → cell id.
    /// A top-level var written from inside a function is a runtime CELL; every
    /// access goes through `__RTS_FN_NS_GC_GCELL_GET/SET` by this id. Computed by
    /// [`funcval::module_globals`] over the final funcs+main.
    pub gcells: std::collections::HashMap<String, u32>,
    /// Top-level SINGLETON-INSTANCE globals (`const X = new Y()`) → their class `Y`.
    /// Data-driven (matched by SHAPE, no name allow-list). Carried so the lowerer's
    /// `static_instance_class` recovers the receiver class for a gcell-promoted
    /// singleton (`console.log` dispatches on `Console` inside any function) — the
    /// gcell promotion erases the `local_classes` entry a plain `new Y()` would set.
    pub gcell_classes: std::collections::HashMap<String, String>,
    /// CELL-promotion set (#195): top-level `let`s force-promoted to runtime cells
    /// because a closure captures them AND they are mutated (a by-value snapshot
    /// would be stale). Carried so the multi-file merge re-promotes the same names
    /// over the combined main (arrows are already lifted by merge time). Union of
    /// prelude + user on merge.
    pub forced_globals: std::collections::HashSet<String>,
    /// PRIVACY GATE (engine namespace): the set of function names that came from
    /// the engine's PRELUDE (embedded TS includes). Only these functions may name
    /// the PRIVATE `engine.*` global; a user function (incl. `__rtsn_main`) that
    /// references it bails explicitly. Empty for a user-only program (no prelude),
    /// so the gate denies `engine.*` everywhere unless a prelude is present.
    pub prelude_fns: std::collections::HashSet<String>,
    /// BUILTIN-IMPORT bindings (M1b): a LOCAL name imported from `rts:<ns>` →
    /// `(ns, member)`. A call to such a name lowers to the real `__RTS_FN_NS_*`
    /// symbol via the generic Registry marshal ([`module_entry`] populates this
    /// from the flattened module's binding map; the single-source path leaves it
    /// empty — no imports there). Bare `"rts"` imports a namespace OBJECT, not a
    /// member, so they are NOT recorded here (the call lowering bails on them).
    pub builtins: std::collections::HashMap<String, (String, String)>,
    /// CLASS-TYPED FUNCTION PARAMETERS: top-level function name → (param name →
    /// the user CLASS its `: C` annotation names). Recovered from the AST (rts-hir
    /// drops a bare class-name annotation to `Unknown`), so the lowerer records the
    /// param in `local_classes` and a `param.method(..)` dispatches — STATICALLY
    /// when monomorphic, or VIRTUALLY (by the instance's runtime shape) when the
    /// method is overridden in `C`'s subtree. Keyed by fn name; unioned on merge.
    pub param_classes: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

/// Parse `src` and lower it to (user functions, synthesized `__rtsn_main`).
///
/// All top-level functions are lowered into a single shared HIR scope first (so
/// each function's signature is registered for the others — and for the main
/// body — to resolve cross-calls and return types), then the top-level
/// statements are lowered against that same scope and wrapped as the body of a
/// synthetic void function named `__rtsn_main`.
fn build_program(src: &str, arrow_ns: &str) -> FrontResult<LoweredProgram> {
    let program =
        rts_parser::parse_source(src).map_err(|e| Unsupported::new(format!("parse error: {e}")))?;
    build_from_program(
        program,
        src,
        &class::ClassTable::default(),
        &std::collections::HashSet::new(),
        arrow_ns,
    )
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
    ambient: &class::ClassTable,
    ambient_fns: &std::collections::HashSet<String>,
    arrow_ns: &str,
) -> FrontResult<LoweredProgram> {
    let src = destructure_src;
    let mut scope = rts_hir::scope::Scope::new();

    // -1. Lift ES5 constructor-functions (`function F(){ this.x = … }`, used as
    //     `new F()`) into synthetic classes BEFORE class collection, so they flow
    //     through the whole class pipeline. Non-regressing: such functions bail at
    //     lowering today (no `this` binding), so the program already fails.
    let program = ctorfn::lift_constructor_functions(program);

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
    let (mut classes, class_funcs) = class::collect_classes(&class_decls, ambient)?;

    // CLASS-TYPED PARAMS: recover each top-level function's `param: C` annotations
    // from the AST (rts-hir lowers a bare class-name annotation to `Unknown`, so the
    // type is otherwise lost). Only a param whose annotation EXACTLY names a user
    // class declared in this program is recorded — `C[]`, `Map<…>`, primitives, etc.
    // are ignored. The lowerer turns this into a `local_classes` entry so the param
    // is a method-dispatch receiver (see [`Lowerer::lower_function`]).
    let class_name_set: std::collections::HashSet<&str> =
        class_decls.iter().map(|c| c.name.as_str()).collect();
    let mut param_classes: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let mut pm: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for p in &fdecl.parameters {
                if let Some(ann) = &p.type_annotation {
                    if class_name_set.contains(ann.as_str()) {
                        pm.insert(p.name.clone(), ann.clone());
                    }
                }
            }
            if !pm.is_empty() {
                param_classes.insert(fdecl.name.clone(), pm);
            }
        }
    }

    // 1. Lower all top-level functions (registers their signatures in `scope`).
    let mut funcs: Vec<HirFunc> = class_funcs;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            // `var` HOISTING (#301): predeclare every `var`-kind name at the
            // top of the fn body (JS function scope); the in-body `var x = init`
            // decls are rewritten to plain ASSIGNMENTS of the hoisted binding
            // (so one inside an explicit `{ }` block escapes the lexical
            // save/restore — the `var` semantic). See `varhoist`.
            let hoists = varhoist::hoisted_var_lets(&fdecl.body);
            let mut f = rts_hir::lower::lower_func(fdecl, &mut scope);
            if !hoists.is_empty() {
                let names = varhoist::hoisted_var_names(&fdecl.body);
                varhoist::rewrite_var_decls_to_assigns(&mut f.body, &names);
                f.body.splice(0..0, hoists);
            }
            // Function-DECLARATION hoisting: a `function inner(){…}` declared in
            // the body is visible (name + value) from the top, so a call textually
            // before it resolves. Runs AFTER the var-hoist splice so the fn decls
            // lead the whole body. See `fnhoist`.
            fnhoist::hoist_fn_decls(&mut f.body);
            funcs.push(f);
        }
    }

    // 1b. Materialize the `arguments` object (#450/#299): a zero-arity fn
    //     referencing `arguments` gains a synthetic variadic rest param (sees
    //     every call arg); a declared-params fn gains a `let arguments = [p…]`
    //     prologue. Runs on the lowered HirFuncs (incl. class methods), before
    //     signatures freeze in `populate_module`.
    argsobj::expand_arguments_object(&mut funcs);

    // 2. Lower the top-level statements (everything that is `Item::Statement`)
    //    against the same scope, in source order.
    let mut top_stmts: Vec<rts_ast::ast::Statement> = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Statement(stmt) = item {
            top_stmts.push(stmt.clone());
        }
    }
    // `var` HOISTING (#301) at MODULE scope: a top-level `var z` predeclares
    // at the head of main so a read before its line resolves; in-body decls
    // become assignments (see the fn-body hoist above).
    let top_var_hoists = varhoist::hoisted_var_lets(&top_stmts);
    let mut body = rts_hir::lower::lower_stmts(&top_stmts, &mut scope);
    if !top_var_hoists.is_empty() {
        let names = varhoist::hoisted_var_names(&top_stmts);
        varhoist::rewrite_var_decls_to_assigns(&mut body, &names);
        body.splice(0..0, top_var_hoists);
    }
    // Function-DECLARATION hoisting at MODULE scope: a top-level `function f(){…}`
    // used before its line resolves (name + value hoisted to the head of main).
    fnhoist::hoist_fn_decls(&mut body);

    // STATIC fields + `static {}` init blocks: every static FIELD of a user class
    // lives in a WRITABLE module-global cell named by the synth convention
    // (`__rtsn_sfield_C_f`), declared as a top-level `let` at the HEAD of main
    // (its name is force-promoted to a gcell below) and initialized in
    // declaration order; each class's `static {}` blocks run right after its
    // field inits — before ANY user top-level statement, matching class
    // evaluation order for the hoisted-class common case. Reads (`C.f`) prefer
    // the cell; writes (`C.f = v`, incl. inside static blocks) store to it.
    let mut static_prelude: Vec<HirStmt> = Vec::new();
    let mut static_field_cells: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for c in &class_decls {
        for m in &c.members {
            if let rts_ast::ast::ClassMember::Property(pd) = m {
                if pd.modifiers.is_static {
                    let cell = class::synth::static_field_getter_name(&c.name, &pd.name);
                    let init = match &pd.initializer {
                        Some(e) => rts_hir::lower::lower_swc_expr(e, &scope),
                        None => rts_hir::ir::HirExpr::new(
                            rts_hir::ir::HirExprKind::Lit(rts_hir::HirLit::Undefined),
                            HirType::Unknown,
                        ),
                    };
                    static_prelude.push(HirStmt::Let {
                        name: cell.clone(),
                        ty: HirType::Unknown,
                        init: Some(init),
                    });
                    static_field_cells.insert(cell);
                }
            }
        }
        // `static { … }` blocks run LEXICALLY inside the class body (JS): a
        // `C.#priv` access inside one must pass the private-name re-check. They
        // are therefore synthesized as STATIC fns (`__rtsn_static_<C>_init<i>` —
        // the `enclosing_class` prefix parse recovers `<C>`), called from the
        // static prelude in declaration order — not inlined into `__rtsn_main`
        // (whose enclosing class is None, which refused the access).
        for (i, (_pos, stmts)) in c.static_init_blocks.iter().enumerate() {
            let fn_name = format!("__rtsn_static_{}_init{}", c.name, i);
            let block_body = rts_hir::lower::lower_stmts(stmts, &mut scope);
            funcs.push(rts_hir::ir::HirFunc {
                name: fn_name.clone(),
                params: Vec::new(),
                ret: HirType::Void,
                body: block_body,
                is_async: false,
                is_arrow: false,
            });
            static_prelude.push(HirStmt::Expr(rts_hir::ir::HirExpr::new(
                rts_hir::ir::HirExprKind::Call {
                    callee: Box::new(rts_hir::ir::HirExpr::new(
                        rts_hir::ir::HirExprKind::Ident(fn_name),
                        HirType::Unknown,
                    )),
                    args: Vec::new(),
                },
                HirType::Void,
            )));
        }
    }
    let body = if static_prelude.is_empty() {
        body
    } else {
        static_prelude.extend(body);
        static_prelude
    };

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
    let (lit_fn_units, lit_fn_bodies) = {
        let main_name = main.name.clone();
        desugar::desugar_obj_methods(&program, &main_name, &mut main.body, &mut funcs, &mut classes)
    };

    // Map each synthesized constructor/method fn name → its class (for `this`). Built
    // AFTER object-literal recovery so the synthesized literal-class methods are
    // included. Iterates every collected descriptor — user classes, virtual
    // builtin-error parents (P5.3), AND literal classes (P5.15).
    let mut fn_this_class: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // A class's `methods`/`accessors` maps are FLATTENED over the parent's (an
    // inherited method keeps the PARENT's synthesized fn name). The `classes`
    // collection iterates in HashMap (non-deterministic) order, so a plain
    // first-seen `or_insert` could attribute an inherited+shared method (e.g.
    // `Derived` inheriting `Base.greet`) to the SUBCLASS — then `this.<m>()` inside
    // it dispatches on the wrong static class and a virtual override resolves wrong
    // (e.g. a `Base` instance running `Derived`'s override). Phase 1 attributes each
    // synthesized fn to the class that DEFINES it (its own `method_name`/accessor
    // name produces that fn) — order-independent and authoritative. Phase 2 is a
    // defensive fallback so any fn not matched by a known naming pattern still gets a
    // binding (never lose a `this`), without overwriting a Phase-1 attribution.
    for desc in classes.iter() {
        fn_this_class.insert(desc.ctor.clone(), desc.name.clone());
        for (key, fn_name) in desc.methods.iter() {
            if *fn_name == class::synth::method_name(&desc.name, key)
                || *fn_name == class::synth::method_name_lit(&desc.name, key)
            {
                fn_this_class.insert(fn_name.clone(), desc.name.clone());
            }
        }
        for (key, acc) in desc.accessors.iter() {
            for fn_name in [acc.getter.as_ref(), acc.setter.as_ref()].into_iter().flatten() {
                if *fn_name == class::synth::getter_name(&desc.name, key)
                    || *fn_name == class::synth::setter_name(&desc.name, key)
                {
                    fn_this_class.insert(fn_name.clone(), desc.name.clone());
                }
            }
        }
    }
    for desc in classes.iter() {
        for fn_name in desc.methods.values() {
            fn_this_class.entry(fn_name.clone()).or_insert(desc.name.clone());
        }
        for acc in desc.accessors.values() {
            for fn_name in [acc.getter.as_ref(), acc.setter.as_ref()].into_iter().flatten() {
                fn_this_class.entry(fn_name.clone()).or_insert(desc.name.clone());
            }
        }
    }

    // P5.8: recover template literals + optional chaining. rts-hir lowered both to
    // structureless `Raw` placeholders (it cannot model them and we must not modify
    // that crate); this pass re-walks the parsed swc AST PAIRED with the HIR and
    // rewrites each placeholder into ordinary HIR (a string `+` chain / a guarded
    // ternary). Run BEFORE arrow extraction so a template/chain inside a top-level
    // arrow is rewritten while still in the `main` body it was parsed from.
    desugar::desugar(&program, &mut main.body, &mut funcs, &classes, &lit_fn_bodies);

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
    // CELL-promotion set (#195): top-level `let`s that must be runtime CELLs, not
    // capturable locals. Two sources, BOTH computed on the PRE-extraction HIR (arrows
    // still present): (a) `captured_mutated_top_lets` — a top let mutated at top level
    // AND captured by a closure (its by-value snapshot would be stale); (b) the
    // function-written lets `module_globals` finds. Computed BEFORE arrow extraction
    // so the capture classifier treats such a name as a non-capture (the closure reads
    // the live cell) instead of bailing. Stored on the program so the multi-file merge
    // re-promotes the same names over the combined main (arrows are gone by then).
    let mut forced_globals = funcval::captured_mutated_top_lets(&funcs, &main);
    // Top-level singleton-instance globals (`const X = new Y()`) — data-driven,
    // force-promoted to cells so they reach inside user functions (a plain
    // top-level `const` does not); `name → class` carried for `static_instance_class`.
    // No `builtins` here: this path has no import bindings (see the struct below), so
    // only the `new` shape can be classified — `merge_programs` recomputes this over
    // the merged main with the real map.
    let gcell_classes =
        funcval::singleton_instance_globals(&funcs, &main, &std::collections::HashMap::new());
    forced_globals.extend(gcell_classes.keys().cloned());
    // Static-field cells (`__rtsn_sfield_C_f` lets prepended to main): ALWAYS
    // promoted — static methods read/write them from inside functions.
    forced_globals.extend(static_field_cells.iter().cloned());
    let pre_globals: std::collections::HashSet<String> =
        funcval::module_globals(&funcs, &main, &forced_globals)
            .into_keys()
            .collect();
    // Class names (ambient `.ts` collections like Set/Map + user classes +
    // synthesized parents) — a free reference to one in an arrow is a known class,
    // not a captured local. `classes` holds the USER decls; `ambient` holds the
    // prelude classes (Set/Map/…), which are NOT merged into `classes`, so both
    // are gathered.
    let mut class_names: std::collections::HashSet<String> = classes
        .iter()
        .map(|d| d.name.clone())
        .chain(ambient.iter().map(|d| d.name.clone()))
        .collect();
    // BUILTIN import bindings (`import { net, tls } from "rts"` / `rts:<ns>` /
    // `node:<ns>`): a free reference inside an arrow (`test(() =>
    // net.tcp_connect(..))`) is a namespace/member binding resolved at lowering,
    // NEVER a captured outer local — without this seed the arrow was misjudged
    // an unsound capture and bailed ("expression arrow").
    for item in &program.items {
        if let rts_ast::ast::Item::Import(decl) = item {
            let spec = decl.from.as_str();
            if spec == "rts" || spec.starts_with("rts:") || spec.starts_with("node:") {
                for n in &decl.names {
                    class_names.insert(n.local.clone());
                }
                if let Some(d) = &decl.default_name {
                    class_names.insert(d.clone());
                }
            }
        }
    }
    let extracted = funcval::extract_arrows(
        &mut funcs,
        &mut main,
        ambient_fns,
        &class_names,
        &pre_globals,
        arrow_ns,
        &lit_fn_units,
    );
    funcs.extend(extracted.funcs);

    // Module-level mutable globals (#195): function-written top lets + the
    // captured-and-mutated top lets force-promoted above.
    let gcells = funcval::module_globals(&funcs, &main, &forced_globals);

    Ok(LoweredProgram {
        funcs,
        main,
        classes,
        fn_this_class,
        captures: extracted.captures,
        cells: extracted.cells,
        gcells,
        gcell_classes,
        forced_globals,
        // A single `build_program` has no prelude/user split — every fn is "user"
        // here. When `merge_programs` combines a prelude + user program it computes
        // the real prelude-origin set; this empty default denies `engine.*` for a
        // user-only program (the gate's safe default).
        prelude_fns: std::collections::HashSet::new(),
        // No imports on the single-source / parse-from-string path. The disk
        // multi-file path ([`module_entry`]) fills this from the binding map.
        builtins: std::collections::HashMap::new(),
        param_classes,
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
