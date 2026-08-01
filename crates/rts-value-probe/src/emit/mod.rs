//! Shared Cranelift plumbing: one `JITModule` per kernel, built with the SAME
//! ISA flags the real engine uses (`module_jit.rs:70-84`) so a number here is
//! comparable to a number there.

pub mod kernel_a;
pub mod kernel_arr;
pub mod kernel_backend;
pub mod kernel_b;
pub mod kernel_c;
pub mod kernel_ir;
pub mod kernel_obj;
pub mod kernel_ops;
pub mod kernel_prim;
pub mod kernel_struct;
pub mod kernel_sym;
pub mod kernel_arch;
pub mod kernel_visible;
pub mod kernel_un;

use std::collections::HashMap;

use cranelift_codegen::Context;
use cranelift_codegen::ir::{AbiParam, FuncRef, InstBuilder, Signature, Value, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

/// Every kernel has the same shape: `(iters, arg_ptr, mask) -> raw word`.
/// The driver runs the result through `poly::to_number`, which is correct for
/// both a tagged accumulator and a native `f64` returned as bits.
pub type KernelFn = extern "C" fn(i64, i64, i64) -> i64;

/// Holds the module alive — `JITModule` frees its executable memory on drop, so
/// dropping it while the fn pointer is live is a use-after-free.
pub struct Compiled {
    _module: JITModule,
    pub f: KernelFn,
}

/// The imports a kernel may call, keyed by symbol name.
pub type Imports = HashMap<&'static str, FuncRef>;

/// Compile one kernel. `needed` lists `(symbol, arity)`; every import returns
/// `i64` and takes `i64` params, matching the real trampoline ABI.
pub fn compile(
    name: &str,
    needed: &[(&'static str, usize)],
    body: impl FnOnce(&mut FunctionBuilder, &Imports),
) -> Compiled {
    let mut flags = settings::builder();
    // Same three the real JIT sets (module_jit.rs:71-84).
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_verifier", "false").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish host isa");

    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (sym, ptr) in crate::rt::symbols() {
        builder.symbol(sym, ptr);
    }
    let mut module = JITModule::new(builder);
    let call_conv = module.isa().default_call_conv();

    // Declare the imports. A symbol whose name ends in `_f64` takes and returns
    // raw `f64` — that is the whole point of `__rtsadp_fmod_f64` in the real
    // engine (no PolyValue crosses the boundary), so the probe has to be able to
    // declare it that way too.
    let mut import_ids = Vec::new();
    for (sym, arity) in needed {
        let raw_f64 = sym.ends_with("_f64");
        let ty = if raw_f64 { types::F64 } else { types::I64 };
        let mut sig = Signature::new(call_conv);
        for _ in 0..*arity {
            sig.params.push(AbiParam::new(ty));
        }
        sig.returns.push(AbiParam::new(ty));
        let id = module
            .declare_function(sym, Linkage::Import, &sig)
            .expect("declare import");
        import_ids.push((*sym, id));
    }

    // The kernel itself.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(types::I64)); // iters
    sig.params.push(AbiParam::new(types::I64)); // arg ptr (payload array / arena base)
    sig.params.push(AbiParam::new(types::I64)); // mask
    sig.returns.push(AbiParam::new(types::I64));
    let func_id = module
        .declare_function(name, Linkage::Local, &sig)
        .expect("declare kernel");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let imports: Imports = import_ids
            .iter()
            .map(|(sym, id)| (*sym, module.declare_func_in_func(*id, fb.func)))
            .collect();
        body(&mut fb, &imports);
        fb.finalize();
    }
    module.define_function(func_id, &mut ctx).expect("define");
    module.clear_context(&mut ctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(func_id);
    // SAFETY: the signature above matches `KernelFn` exactly, and `_module`
    // keeps the code mapped for as long as `f` is reachable.
    let f: KernelFn = unsafe { std::mem::transmute(code) };
    Compiled { _module: module, f }
}

/// Build a function body into a bare `Context` WITHOUT compiling it, so a
/// caller can time `Context::compile` in isolation.
///
/// The returned `JITModule` must outlive the `Context` — it owns the `FuncId`s
/// the declared imports refer to.
pub fn build_context(
    needed: &[(&'static str, usize)],
    body: fn(&mut FunctionBuilder, &Imports),
    isa: &dyn cranelift_codegen::isa::TargetIsa,
) -> (Context, JITModule) {
    let mut builder = JITBuilder::with_isa(
        cranelift_native::builder()
            .expect("host isa builder")
            .finish(settings::Flags::new({
                let mut f = settings::builder();
                f.set("opt_level", "speed").unwrap();
                f.set("preserve_frame_pointers", "true").unwrap();
                f.set("enable_verifier", "false").unwrap();
                f
            }))
            .expect("finish isa"),
        cranelift_module::default_libcall_names(),
    );
    for (sym, ptr) in crate::rt::symbols() {
        builder.symbol(sym, ptr);
    }
    let mut module = JITModule::new(builder);
    let call_conv = isa.default_call_conv();

    let mut import_ids = Vec::new();
    for (sym, arity) in needed {
        let mut sig = Signature::new(call_conv);
        for _ in 0..*arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let id = module
            .declare_function(sym, Linkage::Import, &sig)
            .expect("declare import");
        import_ids.push((*sym, id));
    }

    let mut sig = Signature::new(call_conv);
    for _ in 0..3 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));

    let mut ctx = Context::new();
    ctx.func.signature = sig;
    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let imports: Imports = import_ids
            .iter()
            .map(|(sym, id)| (*sym, module.declare_func_in_func(*id, fb.func)))
            .collect();
        body(&mut fb, &imports);
        fb.finalize();
    }
    (ctx, module)
}

// ---------------------------------------------------------------------------
// PolyValue box/unbox as PURE IR — verbatim shape from
// `rts-codegen-new/src/value/emit.rs`. No calls: the point of the design is
// that the egraph can see through these.
// ---------------------------------------------------------------------------

/// `(v & BOX_BASE) != BOX_BASE` — 1 when the word is a genuine inline double.
pub fn emit_is_double(fb: &mut FunctionBuilder, v: Value) -> Value {
    let base = fb.ins().iconst(types::I64, crate::poly::BOX_BASE as i64);
    let masked = fb.ins().band(v, base);
    fb.ins()
        .icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, masked, base)
}

/// A word already proven to be a double IS the double — one `bitcast`, and the
/// egraph folds it against the matching box. This is the whole PolyValue thesis.
pub fn emit_unbox_double(fb: &mut FunctionBuilder, v: Value) -> Value {
    fb.ins()
        .bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), v)
}

pub fn emit_box_double(fb: &mut FunctionBuilder, v: Value) -> Value {
    fb.ins()
        .bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), v)
}

/// One import call returning `i64`.
pub fn call1(fb: &mut FunctionBuilder, f: FuncRef, args: &[Value]) -> Value {
    let inst = fb.ins().call(f, args);
    fb.inst_results(inst)[0]
}

// ---------------------------------------------------------------------------
// The loop skeleton every kernel shares, so no variant can win on scaffolding.
// ---------------------------------------------------------------------------

pub struct LoopVals {
    /// `objs[i & mask]` — the per-iteration handle payload (kernels that address
    /// memory directly read `arena_base` instead).
    pub payload: Value,
    /// Base address of the flat arena / packed element array.
    pub arena_base: Value,
    /// `i & mask` — the element index, and the value the elements hold.
    pub idx: Value,
    /// The accumulator coming into this iteration.
    pub s: Value,
}

/// `for i in 0..iters { s = body(objs[i & mask], i & mask, s) }`.
///
/// `hdr` (param 1) points at `[payload_array_ptr, arena_base]`. Every variant
/// pays the same dependent load of the payload word, so the comparison is not
/// biased by how the object list itself is addressed.
pub fn loop_kernel<F>(
    name: &str,
    needed: &[(&'static str, usize)],
    s_ty: cranelift_codegen::ir::Type,
    body: F,
) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, LoopVals) -> Value + Copy,
{
    use cranelift_codegen::ir::MemFlags;
    use cranelift_codegen::ir::condcodes::IntCC;

    compile(name, needed, move |fb, imports| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);

        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let mask = fb.block_params(entry)[2];

        let trusted = MemFlags::trusted();
        let payload_arr = fb.ins().load(types::I64, trusted, hdr_ptr, 0);
        let arena_base = fb.ins().load(types::I64, trusted, hdr_ptr, 8);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, s_ty);
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, s_ty);

        let zero = fb.ins().iconst(types::I64, 0);
        let s0 = if s_ty == types::F64 {
            fb.ins().f64const(0.0)
        } else {
            fb.ins().iconst(types::I64, 0)
        };
        fb.ins().jump(header, &[zero.into(), s0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 8);
        let addr = fb.ins().iadd(payload_arr, off);
        let payload = fb.ins().load(types::I64, trusted, addr, 0);

        let s_next = body(
            fb,
            imports,
            LoopVals {
                payload,
                arena_base,
                idx,
                s,
            },
        );
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = if s_ty == types::F64 {
            emit_box_double(fb, out)
        } else {
            out
        };
        fb.ins().return_(&[raw]);
    })
}
