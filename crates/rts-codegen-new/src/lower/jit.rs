//! The P1 JIT harness: compile one [`Func`] to executable memory, with all the
//! REAL runtime `__RTS_FN_*` (+ `__rtsadp_*`) symbols installed, and hand back a callable
//! pointer.
//!
//! This is the smallest honest slice of `pipeline::run_jit`: build a host ISA,
//! make a `JITModule` whose `JITBuilder` has the REAL runtime symbols (+ the
//! codegen-owned `__rtsadp_*` adapter trampolines) registered via
//! [`crate::abi_gen::jit_symbols`], lower the function with
//! [`super::lower::lower_func`], define + finalize, and return the finalized code
//! pointer. The caller `transmute`s it to the right `extern "C"` shape.

use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use crate::abi_gen;

use super::ir::Func;
use super::lower::{lower_func, signature_for};

/// A finalized JIT function: keeps the owning `JITModule` alive (freeing it would
/// unmap the code) and carries the raw code pointer.
pub struct JitFunc {
    // Field order matters for drop only loosely; we keep `module` last and never
    // drop it before the pointer is done being used (the test holds `JitFunc`).
    _module: JITModule,
    code: *const u8,
}

impl JitFunc {
    /// The raw finalized code pointer. `transmute` it to the appropriate
    /// `extern "C" fn(...)` shape matching the lowered `Func` signature.
    pub fn ptr(&self) -> *const u8 {
        self.code
    }
}

/// Build a host `JITModule` with the host ISA, `use_egraphs` on (the sole
/// optimizer, design pilar 5), and every runtime symbol installed.
fn make_module() -> JITModule {
    let mut flags = settings::builder();
    // The egraph is the only optimizer in the new engine. In Cranelift 0.131 it
    // runs unconditionally at `opt_level = speed` (the standalone `use_egraphs`
    // flag of older versions was removed); a redundant box-then-unbox folds away
    // here without any extra flag.
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

/// Compile `func` to executable memory and return a [`JitFunc`] holding the live
/// module + code pointer.
pub fn compile(func: &Func) -> JitFunc {
    let mut module = make_module();

    let sig = signature_for(func, &module);
    let func_id = module
        .declare_function("__rtsn_p1_entry", Linkage::Local, &sig)
        .expect("declare entry");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        lower_func(&mut module, &mut fb, func);
        // `finalize` consumes the builder; call it here where `fb` is owned.
        fb.finalize();
    }

    module
        .define_function(func_id, &mut ctx)
        .expect("define entry");
    module.clear_context(&mut ctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(func_id);
    JitFunc {
        _module: module,
        code,
    }
}

// ---------------------------------------------------------------------------
// Typed run wrappers. Each compiles + transmutes to the matching ABI shape.
// ---------------------------------------------------------------------------

/// Compile a `Func` of shape `fn(f64) -> f64` and return a Rust closure calling
/// it. Holds the JIT alive for the closure's lifetime.
pub fn jit_run_f64_f64(func: &Func) -> impl Fn(f64) -> f64 {
    let jf = compile(func);
    let raw = jf.ptr();
    let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(raw) };
    move |x| {
        // keep `jf` captured so the module stays mapped.
        let _keep = &jf;
        f(x)
    }
}

/// Compile a `Func` of shape `fn(u64) -> u64` (Tagged in/out) and return a
/// closure. The argument and result are raw PolyValue words.
pub fn jit_run_u64_u64(func: &Func) -> impl Fn(u64) -> u64 {
    let jf = compile(func);
    let raw = jf.ptr();
    let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(raw) };
    move |x| {
        let _keep = &jf;
        f(x)
    }
}

/// Compile a `Func` of shape `fn() -> u64` (Tagged out) and return a closure.
pub fn jit_run_unit_u64(func: &Func) -> impl Fn() -> u64 {
    let jf = compile(func);
    let raw = jf.ptr();
    let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(raw) };
    move || {
        let _keep = &jf;
        f()
    }
}

/// Compile a `Func` of shape `fn(i64) -> i64` and return a closure (used for the
/// unbox-of-box proof, where param + ret are unboxed Int32 carried as i64).
pub fn jit_run_i64_i64(func: &Func) -> impl Fn(i64) -> i64 {
    let jf = compile(func);
    let raw = jf.ptr();
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(raw) };
    move |x| {
        let _keep = &jf;
        f(x)
    }
}

/// Compile a `Func` of shape `fn(i64, i64) -> u64` and return a closure. Used by
/// the polymorphic-`+` proof: the two params are native `Int32` (i64 register),
/// boxed inside the lowered function, then `CallExtern("__rtsadp_add", ..)`; the
/// result is a raw Tagged PolyValue word.
pub fn jit_run_ii_u64(func: &Func) -> impl Fn(i64, i64) -> u64 {
    let jf = compile(func);
    let raw = jf.ptr();
    let f: extern "C" fn(i64, i64) -> u64 = unsafe { std::mem::transmute(raw) };
    move |a, b| {
        let _keep = &jf;
        f(a, b)
    }
}
