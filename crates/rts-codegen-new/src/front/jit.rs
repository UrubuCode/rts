//! JIT harness for numeric-subset HIR functions: compile one [`HirFunc`] to
//! executable memory and hand back a callable.
//!
//! Mirrors the P1 [`crate::lower::jit`] harness but drives the real
//! [`super::hir_lower`] (HIR → Cranelift) instead of the hand-built `Func` IR.
//! No runtime externs are installed — the numeric subset is self-contained
//! native code (no boxing, no container/string ops). The egraph (`opt_level =
//! speed`) is the sole optimizer (design pilar 5).

use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use rts_hir::HirFunc;

use super::error::FrontResult;
use super::hir_lower::{lower_func, signature_for};

/// A finalized JIT function: keeps the owning `JITModule` mapped and carries the
/// raw code pointer.
pub struct JitFunc {
    _module: JITModule,
    code: *const u8,
}

impl JitFunc {
    /// The raw finalized code pointer. `transmute` it to the `extern "C" fn(...)`
    /// shape matching the lowered signature.
    pub fn ptr(&self) -> *const u8 {
        self.code
    }
}

/// Build a host `JITModule` with the egraph optimizer on. No runtime symbols are
/// needed for the numeric subset (it emits no `call` to externs).
fn make_module() -> JITModule {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish host isa");
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    JITModule::new(builder)
}

/// Compile a numeric-subset [`HirFunc`] to executable memory.
///
/// Returns `Err(Unsupported)` if the function steps outside the numeric subset
/// (the explicit bail — never a silent miscompile).
pub fn compile(func: &HirFunc) -> FrontResult<JitFunc> {
    let mut module = make_module();

    let sig = signature_for(func, &module)?;
    let func_id = module
        .declare_function(&func.name, Linkage::Local, &sig)
        .expect("declare entry");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let result = lower_func(&mut module, &mut fb, func);
        // `finalize` consumes the builder and validates block structure; only
        // call it when lowering succeeded (otherwise the function is incomplete).
        match result {
            Ok(()) => fb.finalize(),
            Err(e) => return Err(e),
        }
    }

    module
        .define_function(func_id, &mut ctx)
        .expect("define entry");
    module.clear_context(&mut ctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(func_id);
    Ok(JitFunc {
        _module: module,
        code,
    })
}

// ---------------------------------------------------------------------------
// Typed run wrappers — compile + transmute to the matching ABI shape. Each holds
// the JIT alive for the closure's lifetime.
// ---------------------------------------------------------------------------

/// `fn(f64) -> f64`.
pub fn run_f64_f64(func: &HirFunc) -> FrontResult<impl Fn(f64) -> f64> {
    let jf = compile(func)?;
    let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(jf.ptr()) };
    Ok(move |x| {
        let _keep = &jf;
        f(x)
    })
}

/// `fn(f64, f64) -> f64`.
pub fn run_ff_f64(func: &HirFunc) -> FrontResult<impl Fn(f64, f64) -> f64> {
    let jf = compile(func)?;
    let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(jf.ptr()) };
    Ok(move |a, b| {
        let _keep = &jf;
        f(a, b)
    })
}

/// `fn(i32, i32) -> i32`. The i32s are passed in i64 registers (the numeric
/// subset carries Int32 sign-extended), matching the lowered ABI.
pub fn run_ii_i32(func: &HirFunc) -> FrontResult<impl Fn(i64, i64) -> i64> {
    let jf = compile(func)?;
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(jf.ptr()) };
    Ok(move |a, b| {
        let _keep = &jf;
        f(a, b)
    })
}

/// `fn(f64) -> i64`. Used when the function takes a `number` (f64 register) but
/// the HIR narrows its return to a 64-bit integer (e.g. an integer Fibonacci
/// whose `0`/`1` literals type as `I64`).
pub fn run_f64_i64(func: &HirFunc) -> FrontResult<impl Fn(f64) -> i64> {
    let jf = compile(func)?;
    let f: extern "C" fn(f64) -> i64 = unsafe { std::mem::transmute(jf.ptr()) };
    Ok(move |x| {
        let _keep = &jf;
        f(x)
    })
}

/// `fn() -> f64`.
pub fn run_unit_f64(func: &HirFunc) -> FrontResult<impl Fn() -> f64> {
    let jf = compile(func)?;
    let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(jf.ptr()) };
    Ok(move || {
        let _keep = &jf;
        f()
    })
}
