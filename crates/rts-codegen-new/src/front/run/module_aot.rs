//! Whole-module AOT: lower the SAME program ([`super::module_jit::populate_module`])
//! into a Cranelift [`ObjectModule`] instead of a JIT module, synthesize a C
//! `main` entry that drives it, and emit a relocatable object (`.o`/`.obj`).
//!
//! The single lowering path is shared with the JIT (design doc pilar 5): only the
//! `Module` backend differs. Every `__RTS_*` runtime symbol and `__rtsadp_*`
//! adapter trampoline is left as an `Import` here — the linker resolves them at
//! final link against the `rts-runtime` + `rts-adapters` staticlibs.
//!
//! The emitted `main` (the CRT entry) calls `__rtsn_main` (the lowered top-level)
//! and then `__RTS_FN_RT_RUN_EVENT_LOOP` (drains pending microtasks/timers so an
//! AOT binary matches `rts run`), and returns `0`.

use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::front::error::{FrontResult, Unsupported};

/// Lower `prog` into a fresh `ObjectModule`, append the `main` entry shim, and
/// return the emitted object-file bytes (COFF/ELF/Mach-O per host triple).
pub(crate) fn compile_program_aot(prog: &super::LoweredProgram) -> FrontResult<Vec<u8>> {
    let mut module = make_object_module()?;

    // Declare + define every user fn + __rtsn_main + thunks (the SAME path the JIT
    // uses). `__rtsn_main` is Local; the `main` shim below calls it in-object.
    let main_id = super::module_jit::populate_module(&mut module, prog)?;

    // The CRT entry `int main(void)`: run the program, drain the event loop, exit 0.
    emit_main_entry(&mut module, main_id)?;

    let product = module.finish();
    product
        .emit()
        .map_err(|e| Unsupported::new(format!("emit object: {e}")))
}

/// Host-target `ObjectModule` with the egraph optimizer on (matching the JIT
/// flags). PIC is enabled so the object relocates cleanly on ELF/Mach-O; on
/// Windows COFF it is harmless.
fn make_object_module() -> FrontResult<ObjectModule> {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    // Position-independent so the linker can place the code anywhere (PIE on
    // modern toolchains). No effect on Windows/COFF.
    flags.set("is_pic", "true").unwrap();
    let isa = cranelift_native::builder()
        .map_err(|e| Unsupported::new(format!("host isa builder: {e}")))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| Unsupported::new(format!("finish host isa: {e}")))?;
    let builder = ObjectBuilder::new(isa, "rts_program", cranelift_module::default_libcall_names())
        .map_err(|e| Unsupported::new(format!("object builder: {e}")))?;
    Ok(ObjectModule::new(builder))
}

/// Emit `extern "C" int main(void)` that calls the lowered top-level
/// (`__rtsn_main`, `rtsn_main_id`), then `__RTS_FN_RT_RUN_EVENT_LOOP`, returning 0.
fn emit_main_entry(
    module: &mut ObjectModule,
    rtsn_main_id: cranelift_module::FuncId,
) -> FrontResult<()> {
    // The runtime's event-loop drain, resolved at link time.
    let evloop_sig = module.make_signature(); // () -> ()
    let evloop_id = module
        .declare_function("__RTS_FN_RT_RUN_EVENT_LOOP", Linkage::Import, &evloop_sig)
        .map_err(|e| Unsupported::new(format!("declare event-loop drain: {e}")))?;

    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));
    let main_id = module
        .declare_function("main", Linkage::Export, &main_sig)
        .map_err(|e| Unsupported::new(format!("declare main: {e}")))?;

    let mut ctx = module.make_context();
    ctx.func.signature = main_sig;
    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let blk = fb.create_block();
        fb.append_block_params_for_function_params(blk);
        fb.switch_to_block(blk);
        fb.seal_block(blk);

        let rtsn_main_ref = module.declare_func_in_func(rtsn_main_id, fb.func);
        fb.ins().call(rtsn_main_ref, &[]);
        let evloop_ref = module.declare_func_in_func(evloop_id, fb.func);
        fb.ins().call(evloop_ref, &[]);

        let zero = fb.ins().iconst(types::I32, 0);
        fb.ins().return_(&[zero]);
        fb.finalize();
    }
    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| Unsupported::new(format!("define main: {e}")))?;
    module.clear_context(&mut ctx);
    Ok(())
}
