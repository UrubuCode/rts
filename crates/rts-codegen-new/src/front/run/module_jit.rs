//! Whole-module JIT: compile every user function plus the synthesized
//! `__rtsn_main` into ONE `JITModule`, resolve inter-function calls, install the
//! runtime symbols, finalize, and return the `__rtsn_main` entry pointer.
//!
//! This is the increment-4 generalization of the single-function harnesses
//! ([`crate::lower::jit`], [`crate::front::jit`]): the lowering emits Cranelift
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

use crate::front::error::{FrontResult, Unsupported};
use crate::runtime;

use super::lower::Lowerer;
use super::sig::FnSig;

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
/// installed (so `console.log` / `__rtsn_add` / … resolve at link time).
fn make_module() -> JITModule {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish host isa");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for sym in runtime::symbols() {
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
pub fn compile_program(funcs: &[HirFunc], main: &HirFunc) -> FrontResult<Program> {
    let mut module = make_module();

    // 1. Freeze every signature (user funcs by their HIR types; main is void).
    let mut sigs: HashMap<String, FnSig> = HashMap::new();
    for f in funcs {
        sigs.insert(f.name.clone(), FnSig::of_func(f));
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

    // 3. Define each user function body.
    for f in funcs {
        let sig = sigs[&f.name].clone();
        define_one(&mut module, ids[&f.name], f, &sig, &sigs)?;
    }
    // 4. Define main (the top-level body).
    define_one(&mut module, main_id, main, &main_sig, &sigs)?;

    module.finalize_definitions().map_err(|e| {
        Unsupported::new(format!("finalize module: {e}"))
    })?;

    let main = module.get_finalized_function(main_id);
    Ok(Program { _module: module, main })
}

/// Lower + define one function into the module. On an `Unsupported` bail the
/// half-built `ctx` is discarded WITHOUT finalizing (an incomplete Cranelift
/// function must never be defined), and the error propagates.
fn define_one(
    module: &mut JITModule,
    id: cranelift_module::FuncId,
    func: &HirFunc,
    sig: &FnSig,
    sigs: &HashMap<String, FnSig>,
) -> FrontResult<()> {
    let mut ctx = module.make_context();
    ctx.func.signature = sig.to_cranelift(module);

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let res = Lowerer::lower_function(module, &mut fb, func, sig, sigs);
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
