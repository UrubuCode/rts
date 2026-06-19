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
        sigs.insert(f.name.clone(), sig);
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
            module, &mut fb, func, sig, sigs, thunks, classes, captures, gcells, this_class,
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
