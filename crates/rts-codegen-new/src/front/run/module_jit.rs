//! Whole-module JIT: compile every user function plus the synthesized
//! `__rtsn_main` into ONE `JITModule`, resolve inter-function calls, install the
//! runtime symbols, finalize, and return the `__rtsn_main` entry pointer.
//!
//! This is the increment-4 generalization of the single-function harness
//! ([`crate::front::jit`]): the lowering emits Cranelift
//! `call`s between user functions (and to the `console.log` / generic-op
//! externs), so all definitions must live in the same module with a shared
//! symbol space. The host calls `__rtsn_main` (a `fn()` — it returns nothing and
//! prints into the capture buffer).

use std::collections::HashMap;

use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use rts_hir::HirFunc;

use crate::abi_gen;
use crate::front::error::{FrontResult, Unsupported};

use super::lower::Lowerer;
use super::sig::FnSig;
use super::thunk;

use cranelift_module::FuncId;

/// A finalized module: keeps the `JITModule` mapped and carries the
/// `__rtsn_main` entry pointer.
pub struct Program {
    _module: JITModule,
    main: *const u8,
}

impl Program {
    /// Run `__rtsn_main` (it prints into the console capture buffer). Unsafe-free
    /// at the call site: the entry is an `extern "C" fn()`.
    pub fn run_main(&self) {
        let f: extern "C" fn() = unsafe { std::mem::transmute(self.main) };
        f();
        // keep the module mapped until after the call returns.
        let _keep = &self._module;
    }
}

/// Build a host `JITModule` with the egraph optimizer and every runtime symbol
/// installed (so `__RTS_FN_NS_IO_PRINT` / `__RTS_FN_NS_GC_STRING_*` /
/// `__rtsadp_*` / … resolve at link time). The symbol set is the REAL runtime
/// surface the lowering calls plus the codegen-owned adapter trampolines, sourced
/// from [`crate::runtime_link`] via [`abi_gen::jit_symbols`].
fn make_module() -> JITModule {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish host isa");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for sym in abi_gen::jit_symbols() {
        builder.symbol(sym.name, sym.ptr);
    }
    JITModule::new(builder)
}

/// Compile a whole program: the user functions `funcs` plus a `main` HirFunc
/// (synthesized from the top-level statements). Returns the live module + the
/// `__rtsn_main` entry pointer.
///
/// On ANY function (or main) hitting an unsupported construct, returns the
/// `Unsupported` — the module is dropped and nothing runs (no partial program).
pub(crate) fn compile_program(prog: &super::LoweredProgram) -> FrontResult<Program> {
    let funcs = &prog.funcs;
    let main = &prog.main;
    let classes = &prog.classes;
    let fn_this_class = &prog.fn_this_class;
    let captures = &prog.captures;
    let gcells = &prog.gcells;
    let prelude_fns = &prog.prelude_fns;
    let builtins = &prog.builtins;
    let mut module = make_module();

    // 1. Freeze every signature (user funcs by their HIR types; main is void).
    let mut sigs: HashMap<String, FnSig> = HashMap::new();
    for f in funcs {
        let mut sig = FnSig::of_func(f);
        // `has_this` (set by `of_func` for any `this`-first fn) drives the PLAIN-call
        // `F(args)` undefined-receiver prepend — which is a FREE-function Phase 1
        // behavior. A CLASS ctor/method (in `fn_this_class`) ALSO has a `this`-first
        // param but binds/receives `this` through the class machinery (`new`, method
        // dispatch, and the explicit `this` arg of a forwarding `super(...)` call),
        // so it must NOT also get an undefined receiver. Clear the flag for class fns.
        if fn_this_class.contains_key(&f.name) {
            sig.has_this = false;
        }
        // Infer the provable RETURN CLASS so a chained call on the result dispatches
        // statically (`expect(x).toBe(y)`; `c.inc().add(5)`; `const c2 = c.inc()`).
        // FREE function: every `return new C(..)` of the same known class C. METHOD
        // (in `fn_this_class`): the same, PLUS `return this` → the method's OWNING
        // class (the fluent-builder `inc(): C { …; return this }`).
        // PASS 1 (no cross-fn return classes yet): a `return new C(..)` of a known
        // class, or a method whose every return is `this` (the fluent-builder).
        sig.ret_class = infer_ret_class(f, classes, &|_| None).or_else(|| {
            fn_this_class
                .get(&f.name)
                .filter(|_| method_returns_this(f))
                .cloned()
        });
        sigs.insert(f.name.clone(), sig);
    }
    // PASS 2 (fixpoint): resolve the return class of functions whose return is a
    // CHAIN on a known base — `return new C().m()…` or `return f()` — now that
    // every base return class from pass 1 is known. Monotonic (only ADDS a proven
    // class, never overwrites/clears), so it terminates in ≤ `funcs.len()` rounds.
    // Lets `function mk(): C { return new C()…; }` then `mk().method()` dispatch
    // statically — the static class of a value survives a call/method result, not
    // just a `new C()` or a `let`-bound local.
    loop {
        let snapshot: HashMap<String, Option<String>> =
            sigs.iter().map(|(k, v)| (k.clone(), v.ret_class.clone())).collect();
        let ret_of = |name: &str| snapshot.get(name).and_then(|o| o.clone());
        let mut changed = false;
        for f in funcs {
            if ret_of(&f.name).is_some() {
                continue;
            }
            if let Some(class) = infer_ret_class(f, classes, &ret_of) {
                sigs.get_mut(&f.name).expect("sig built in pass 1").ret_class = Some(class);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let main_sig = FnSig::main_sig();

    // 2. Declare every function up front so cross-calls resolve to a FuncId.
    let mut ids = HashMap::new();
    for f in funcs {
        let sig = &sigs[&f.name];
        let cl_sig = sig.to_cranelift(&module);
        let id = module
            .declare_function(&f.name, Linkage::Local, &cl_sig)
            .map_err(|e| Unsupported::new(format!("declare `{}`: {e}", f.name)))?;
        ids.insert(f.name.clone(), id);
    }
    let main_cl_sig = main_sig.to_cranelift(&module);
    let main_id = module
        .declare_function(&main_sig.name, Linkage::Local, &main_cl_sig)
        .map_err(|e| Unsupported::new(format!("declare main: {e}")))?;

    // 2b. Declare a uniform-ABI THUNK for every user function (P4.6). A function
    //     referenced as a VALUE reifies via `func_addr` of its thunk; declaring
    //     one per function keeps reify a pure address lookup (no second pass to
    //     decide which are values). `main` is never a value, so it gets none.
    let mut thunks: HashMap<String, FuncId> = HashMap::new();
    for f in funcs {
        let id = thunk::declare_thunk(&mut module, &f.name)?;
        thunks.insert(f.name.clone(), id);
    }
    // 2c. Declare a per-class NEW-THUNK (`<class>__rtsn_newthunk`) for every user
    //     class with a REAL synthesized constructor (a literal-shape class has a
    //     `__rtsl_noctor_*` placeholder not in `ids` — skip it; it is never `new`ed
    //     through a value). A class used as a VALUE (`const C = Box`) reifies to
    //     this thunk's address, so `new C(args)` allocates + constructs uniformly.
    //     Keyed in the same `thunks` map under the distinct `__rtsn_newthunk` name.
    for desc in classes.iter() {
        if ids.contains_key(&desc.ctor) {
            let id = thunk::declare_new_thunk(&mut module, &desc.name)?;
            thunks.insert(thunk::new_thunk_name(&desc.name), id);
        }
    }

    // 3. Define each user function body.
    for f in funcs {
        let sig = sigs[&f.name].clone();
        let this_class = fn_this_class.get(&f.name).map(String::as_str);
        // PRIVACY GATE: a prelude-origin function may name the PRIVATE `engine.*`
        // global; a user function may not.
        let is_prelude = prelude_fns.contains(&f.name);
        define_one(
            &mut module,
            ids[&f.name],
            f,
            &sig,
            &sigs,
            &thunks,
            &ids,
            classes,
            captures,
            gcells,
            this_class,
            is_prelude,
            builtins,
        )?;
    }
    // 4. Define main (the top-level body). `__rtsn_main` is USER code — never
    //    prelude — so it cannot name the PRIVATE `engine.*` global.
    define_one(
        &mut module,
        main_id,
        main,
        &main_sig,
        &sigs,
        &thunks,
        &ids,
        classes,
        captures,
        gcells,
        None,
        false,
        builtins,
    )?;

    // 4b. Define every thunk body (bridges the uniform ABI to the real signature).
    //     A CLOSURE thunk reads its leading `capture_count` real params from the
    //     env array; a non-capturing thunk has `capture_count = 0`.
    for f in funcs {
        let capture_count = captures.get(&f.name).map(Vec::len).unwrap_or(0);
        thunk::define_thunk(
            &mut module,
            thunks[&f.name],
            ids[&f.name],
            &sigs[&f.name],
            capture_count,
        )?;
    }
    // 4c. Define every class NEW-THUNK: allocate the instance + run the ctor +
    //     return the instance word (the constructor's `this` is synthesized in the
    //     allocation, sidestepping the `reify_function` `has_this` bail).
    for desc in classes.iter() {
        if let Some(&ctor_id) = ids.get(&desc.ctor) {
            thunk::define_new_thunk(
                &mut module,
                thunks[&thunk::new_thunk_name(&desc.name)],
                ctor_id,
                &sigs[&desc.ctor],
                desc.global_shape,
                desc.fields.len(),
            )?;
        }
    }

    module
        .finalize_definitions()
        .map_err(|e| Unsupported::new(format!("finalize module: {e}")))?;

    let main = module.get_finalized_function(main_id);
    Ok(Program {
        _module: module,
        main,
    })
}

/// Lower + define one function into the module. On an `Unsupported` bail the
/// half-built `ctx` is discarded WITHOUT finalizing (an incomplete Cranelift
/// function must never be defined), and the error propagates.
#[allow(clippy::too_many_arguments)]
fn define_one(
    module: &mut JITModule,
    id: cranelift_module::FuncId,
    func: &HirFunc,
    sig: &FnSig,
    sigs: &HashMap<String, FnSig>,
    thunks: &HashMap<String, FuncId>,
    ids: &HashMap<String, FuncId>,
    classes: &super::class::ClassTable,
    captures: &HashMap<String, Vec<String>>,
    gcells: &HashMap<String, u32>,
    this_class: Option<&str>,
    is_prelude: bool,
    builtins: &HashMap<String, (String, String)>,
) -> FrontResult<()> {
    let mut ctx = module.make_context();
    ctx.func.signature = sig.to_cranelift(module);

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let res = Lowerer::lower_function(
            module, &mut fb, func, sig, sigs, thunks, ids, classes, captures, gcells, this_class,
            is_prelude, builtins,
        );
        match res {
            Ok(()) => fb.finalize(),
            Err(e) => {
                // drop the builder/ctx without defining; clear and bail.
                module.clear_context(&mut ctx);
                return Err(e);
            }
        }
    }

    module
        .define_function(id, &mut ctx)
        .map_err(|e| Unsupported::new(format!("define `{}`: {e}", func.name)))?;
    module.clear_context(&mut ctx);
    Ok(())
}

/// Infer the user-CLASS a function provably RETURNS: `Some(C)` iff EVERY value
/// `return e` in the body has `e == new C(..)` for the SAME class `C` that the
/// program's class table knows, and there is at least one such return. A `return;`
/// (void) is ignored; any value return that is NOT `new <known-class>(..)` (or a
/// DIFFERENT class) makes the result `None` (we never guess). This powers static
/// dispatch on a call result (`expect(x).toBe(y)`).
/// Whether EVERY value `return` in `func`'s body returns `this` (`return this`),
/// ignoring `return;` (void). For a method this means it returns the receiver —
/// i.e. the OWNING class (the fluent-builder `return this`). A body with a value
/// return that is NOT `this` → `false` (we never guess).
fn method_returns_this(func: &HirFunc) -> bool {
    use rts_hir::ir::HirExprKind;
    use rts_hir::HirStmt;

    fn walk(stmts: &[HirStmt], any: &mut bool, all_this: &mut bool) {
        for s in stmts {
            match s {
                HirStmt::Return(Some(e)) => {
                    *any = true;
                    if !matches!(&e.kind, HirExprKind::Ident(n) if n == "this") {
                        *all_this = false;
                    }
                }
                HirStmt::If { then, else_, .. } => {
                    walk(then, any, all_this);
                    if let Some(e) = else_ {
                        walk(e, any, all_this);
                    }
                }
                HirStmt::While { body, .. }
                | HirStmt::DoWhile { body, .. }
                | HirStmt::For { body, .. }
                | HirStmt::ForOf { body, .. }
                | HirStmt::ForIn { body, .. }
                | HirStmt::Block(body) => walk(body, any, all_this),
                HirStmt::Try { body, catch, finally } => {
                    walk(body, any, all_this);
                    if let Some(c) = catch {
                        walk(&c.body, any, all_this);
                    }
                    if let Some(f) = finally {
                        walk(f, any, all_this);
                    }
                }
                HirStmt::Switch { cases, .. } => {
                    for c in cases {
                        walk(&c.body, any, all_this);
                    }
                }
                _ => {}
            }
        }
    }
    let mut any = false;
    let mut all_this = true;
    walk(&func.body, &mut any, &mut all_this);
    any && all_this
}

/// The statically-PROVABLE class of an expression, from the class table plus a
/// `ret_of` lookup of each function's already-known return class. The free-fn
/// mirror of [`Lowerer::static_instance_class`], used at sig-build time:
/// - `new C(..)` of a known class C → `C`;
/// - `f(..)` (bare-ident callee) → `ret_of(f)`;
/// - `recv.m(..)` → the class of `recv`, then class `recv`'s method `m`'s `ret_of`.
///
/// Never guesses: an expression whose class is not provable is `None` (a wrong
/// class would silently mis-dispatch a later chained call). With a `ret_of` that
/// always returns `None` it degrades to exactly the old "`return new C`" rule.
fn expr_static_class(
    e: &rts_hir::ir::HirExpr,
    classes: &super::class::ClassTable,
    ret_of: &dyn Fn(&str) -> Option<String>,
    locals: &HashMap<String, String>,
) -> Option<String> {
    use rts_hir::ir::HirExprKind;
    match &e.kind {
        HirExprKind::New { class, .. } => classes.get(class).map(|_| class.clone()),
        // A local PROVEN to hold a class instance (`const m = new C(); … return m`).
        HirExprKind::Ident(name) => locals.get(name).cloned(),
        HirExprKind::Call { callee, .. } => match &callee.kind {
            HirExprKind::Ident(f) => ret_of(f),
            _ => None,
        },
        HirExprKind::MethodCall { object, method, .. } => {
            let recv = expr_static_class(object, classes, ret_of, locals)?;
            let synth = classes.get(&recv)?.methods.get(method)?;
            ret_of(synth)
        }
        _ => None,
    }
}

/// Infer a function's provable RETURN CLASS: every `return <expr>` must resolve
/// (via [`expr_static_class`]) to the SAME known class. A bare `return;` (void) is
/// permitted and ignored (the historical rule). Any return whose class is not
/// provable, or a disagreement between returns, yields `None` — never a guess.
fn infer_ret_class(
    func: &HirFunc,
    classes: &super::class::ClassTable,
    ret_of: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    use rts_hir::HirStmt;

    // `locals` maps an in-scope CONST binding name → its proven class. Const is
    // immutable, so the binding cannot be reassigned to another class — sound.
    // Block/branch/loop bodies save+restore the map so a sibling-scope binding
    // never leaks (full lexical soundness). `let` is NOT tracked (a later `let`
    // reassignment could change the class — a deferred increment).
    fn walk(
        stmts: &[HirStmt],
        classes: &super::class::ClassTable,
        ret_of: &dyn Fn(&str) -> Option<String>,
        locals: &mut HashMap<String, String>,
        found: &mut Option<String>,
    ) -> bool {
        for s in stmts {
            let ok = match s {
                HirStmt::Const { name, init, .. } => {
                    if let Some(class) = expr_static_class(init, classes, ret_of, locals) {
                        locals.insert(name.clone(), class);
                    } else {
                        locals.remove(name);
                    }
                    true
                }
                HirStmt::Let { name, .. } => {
                    // Untracked: drop any shadowed const so we never use a stale class.
                    locals.remove(name);
                    true
                }
                HirStmt::Return(Some(e)) => match expr_static_class(e, classes, ret_of, locals) {
                    Some(class) => match found {
                        Some(c) if *c != class => false,
                        _ => {
                            *found = Some(class);
                            true
                        }
                    },
                    None => false,
                },
                HirStmt::Return(None) => true,
                HirStmt::If { then, else_, .. } => {
                    let saved = locals.clone();
                    let a = walk(then, classes, ret_of, locals, found);
                    *locals = saved.clone();
                    let b = else_.as_deref().map(|e| walk(e, classes, ret_of, locals, found)).unwrap_or(true);
                    *locals = saved;
                    a && b
                }
                HirStmt::While { body, .. }
                | HirStmt::DoWhile { body, .. }
                | HirStmt::For { body, .. }
                | HirStmt::ForOf { body, .. }
                | HirStmt::ForIn { body, .. }
                | HirStmt::Block(body) => {
                    let saved = locals.clone();
                    let r = walk(body, classes, ret_of, locals, found);
                    *locals = saved;
                    r
                }
                HirStmt::Try { body, catch, finally } => {
                    let saved = locals.clone();
                    let mut r = walk(body, classes, ret_of, locals, found);
                    *locals = saved.clone();
                    if let Some(c) = catch {
                        r = r && walk(&c.body, classes, ret_of, locals, found);
                        *locals = saved.clone();
                    }
                    if let Some(f) = finally {
                        r = r && walk(f, classes, ret_of, locals, found);
                    }
                    *locals = saved;
                    r
                }
                HirStmt::Switch { cases, .. } => {
                    let saved = locals.clone();
                    let r = cases.iter().all(|c| {
                        *locals = saved.clone();
                        walk(&c.body, classes, ret_of, locals, found)
                    });
                    *locals = saved;
                    r
                }
                // Other statements (Expr/Throw/Break/…) don't bind a const class.
                _ => true,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    let mut locals = HashMap::new();
    let mut found = None;
    if walk(&func.body, classes, ret_of, &mut locals, &mut found) {
        found
    } else {
        None
    }
}
