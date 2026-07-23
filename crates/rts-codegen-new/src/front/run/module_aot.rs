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
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::front::error::{FrontResult, Unsupported};

/// Lower `prog` into a fresh `ObjectModule`, append the `main` entry shim, and
/// return the emitted object-file bytes (COFF/ELF/Mach-O per host triple).
pub(crate) fn compile_program_aot(prog: &super::LoweredProgram) -> FrontResult<Vec<u8>> {
    let mut module = make_object_module()?;

    // String literals must be lowered as DATA objects + a runtime
    // `string_from_static` call (not the JIT's compile-time-baked handle, which is
    // invalid in the produced binary). Gate ON for the whole AOT lowering, OFF
    // after so a later same-thread JIT build keeps the fast bake.
    super::aot_str::set_aot_mode(true);
    let result: FrontResult<()> = (|| {
        // Declare + define every user fn + __rtsn_main + thunks (the SAME path the
        // JIT uses). `__rtsn_main` is Local; the `main` shim below calls it in-object.
        let main_id = super::module_jit::populate_module(&mut module, prog)?;
        // SHAPE SEED (AOT-only): `populate_module` interned every shape id this
        // program bakes as an immediate; export the id→keys registry into a data
        // object the `main` shim seeds at startup, so dynamic shape reads resolve in
        // the separate AOT process (see `shapes::export_seed_blob`).
        let seed = emit_shape_seed_data(&mut module)?;
        // The CRT entry `int main(void)`: seed shapes, run the program, drain the
        // event loop, exit 0.
        emit_main_entry(&mut module, main_id, seed)?;
        Ok(())
    })();
    super::aot_str::set_aot_mode(false);
    result?;

    let product = module.finish();
    product
        .emit()
        .map_err(|e| Unsupported::new(format!("emit object: {e}")))
}

/// WHOLE-PROGRAM CACHE for AOT: emit the `.o` by REPLAYING a baked manifest's
/// machine code into the `ObjectModule` (`define_function_bytes`) instead of
/// re-compiling every function. Skips the per-fn `ctx.compile` (the expensive AOT
/// phase) — the `.o` is still produced + linked, but from the cached bytes.
///
/// Sound on hosts where the JIT-captured bytes match AOT codegen (same host ISA,
/// `opt_level=speed`, no `is_pic`): Windows + Linux. macOS AOT uses `is_pic`, so
/// its codegen differs from the non-pic JIT bytes — the caller must NOT use this on
/// macOS (fall back to `compile_program_aot`).
pub(crate) fn compile_replay_aot(m: &super::bake::PreludeManifest) -> FrontResult<Vec<u8>> {
    rts_engine::heap::shapes::reset_global_shapes();
    rts_engine::heap::shapes::seed_global_shapes(m.shapes.clone());
    rts_engine::heap::shapes::seed_error_classes(m.error_classes.clone());
    let mut prog = m.program.clone();
    prog.resident_import_names = prog.funcs.iter().map(|f| f.name.clone()).collect();
    prog.resident_main = true;

    let mut module = make_object_module()?;
    let result: FrontResult<()> = super::resident::with_replay_manifest(m.clone(), || {
        // populate replays EVERY fn (incl `__rtsn_main`) from the baked bytes into
        // the ObjectModule; its Import externs + data + relocs are written to the
        // object for the LINKER to resolve (the ±2 GB JIT far-call limit does not
        // apply — the linker + final layout handle reach).
        let main_id = super::module_jit::populate_module(&mut module, &prog)?;
        // Same shape seed as the non-replay path — the produced binary is a separate
        // process whose registry needs the (manifest-seeded) shapes at startup.
        let seed = emit_shape_seed_data(&mut module)?;
        emit_main_entry(&mut module, main_id, seed)?;
        Ok(())
    });
    result?;

    let product = module.finish();
    product
        .emit()
        .map_err(|e| Unsupported::new(format!("emit object (replay): {e}")).into())
}

/// Host-target `ObjectModule` with the EXACT same flags as the JIT
/// ([`super::module_jit`]): `opt_level=speed`, nothing else. We deliberately do
/// NOT set `is_pic` on Windows/COFF or Linux/ELF — on COFF it changes
/// addressing/relocation in a way that produced a startup stack overflow
/// (emitted code called through bad relocations), and the final binary is
/// statically linked so non-PIC is correct there. macOS is the EXCEPTION:
/// arm64 Mach-O executables are PIE (ASLR mandatory), and non-PIC absolute
/// relocations in text crash at startup (Bus error: 10 on the CI AOT smoke) —
/// so `is_pic` is set on macOS only.
pub(crate) fn make_object_module() -> FrontResult<ObjectModule> {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    if cfg!(target_os = "macos") {
        flags.set("is_pic", "true").unwrap();
    }
    // Required by `return_call` (TCO) on x86-64 — the JIT sets it too; without
    // it Cranelift panics ("frame pointers aren't fundamentally required for
    // tail calls, but the current implementation relies on them").
    flags.set("preserve_frame_pointers", "true").unwrap();
    let isa = cranelift_native::builder()
        .map_err(|e| Unsupported::new(format!("host isa builder: {e}")))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| Unsupported::new(format!("finish host isa: {e}")))?;
    let builder = ObjectBuilder::new(
        isa,
        "rts_program",
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| Unsupported::new(format!("object builder: {e}")))?;
    Ok(ObjectModule::new(builder))
}

/// A codegen-emitted shape-seed data object: its id + byte length, so the `main`
/// shim can pass `(ptr, len)` to `__RTS_FN_RT_SEED_SHAPES`.
struct ShapeSeed {
    data: cranelift_module::DataId,
    len: usize,
}

/// Serialize the compile-time global-shape + error-class registries (populated by
/// the just-completed `populate_module`) into a `Local` data object. Empty blob
/// (no shapes) still emits a valid 4-byte header, so the seed call is unconditional
/// and harmless.
fn emit_shape_seed_data(module: &mut ObjectModule) -> FrontResult<ShapeSeed> {
    let blob = rts_engine::heap::shapes::export_seed_blob();
    let len = blob.len();
    let data = module
        .declare_data("__RTS_AOT_SHAPE_SEED", Linkage::Local, false, false)
        .map_err(|e| Unsupported::new(format!("declare shape-seed data: {e}")))?;
    let mut desc = DataDescription::new();
    desc.define(blob.into_boxed_slice());
    module
        .define_data(data, &desc)
        .map_err(|e| Unsupported::new(format!("define shape-seed data: {e}")))?;
    Ok(ShapeSeed { data, len })
}

/// Emit `extern "C" int main(void)` that SEEDS the shape registry, calls the
/// lowered top-level (`__rtsn_main`, `rtsn_main_id`), then
/// `__RTS_FN_RT_RUN_EVENT_LOOP`, returning 0.
fn emit_main_entry(
    module: &mut ObjectModule,
    rtsn_main_id: cranelift_module::FuncId,
    seed: ShapeSeed,
) -> FrontResult<()> {
    // Runtime bootstrap, resolved at link time (ONE generic `RT`-scope symbol, no
    // non-primordial name in the engine). It wires the GC tick hook + main-thread
    // registration AND the UI render/input backend (the facade `rts-runtime` owns
    // the symbol because the backend lives in `rts-egui`). The JIT path calls the
    // equivalent host-side in `module_jit::compile_program`. Without it the AOT
    // binary's GC never runs (handle leak → 5M abort) and the UI backend
    // thread_local stays empty (blank window — the namespace `register()` that
    // installs it only runs at compile time, not in the binary).
    let rtinit_sig = module.make_signature(); // () -> ()
    let rtinit_id = module
        .declare_function("__RTS_FN_RT_INIT", Linkage::Import, &rtinit_sig)
        .map_err(|e| Unsupported::new(format!("declare runtime init: {e}")))?;

    // The engine's ADAPTER-side bootstrap: installs the shaped-object hook, the
    // thenable probe, and the async-error hook. The JIT path runs the equivalent
    // host-side in `module_jit::compile_program` (`install_async_error_hook` +
    // friends); the AOT binary is a separate process, so its `main` must install
    // them too. Missing, the shaped-object get/set went through an UNINSTALLED
    // (zero) function pointer — a non-deterministic startup crash the moment a
    // program touched a shaped object (e.g. a template literal's number→string),
    // which is exactly the flaky AOT smoke-test segfault.
    let boot_sig = module.make_signature(); // () -> ()
    let boot_id = module
        .declare_function("__rtsadp_engine_bootstrap", Linkage::Import, &boot_sig)
        .map_err(|e| Unsupported::new(format!("declare engine bootstrap: {e}")))?;

    // The runtime's event-loop drain, resolved at link time.
    let evloop_sig = module.make_signature(); // () -> ()
    let evloop_id = module
        .declare_function("__RTS_FN_RT_RUN_EVENT_LOOP", Linkage::Import, &evloop_sig)
        .map_err(|e| Unsupported::new(format!("declare event-loop drain: {e}")))?;

    // The shape-registry seeder: `(ptr, len) -> ()`. Resolved at link time against
    // rts-runtime. Called FIRST in main so the baked shape ids resolve before any
    // program (or bootstrap) code performs a dynamic shape read.
    let mut seed_sig = module.make_signature();
    seed_sig.params.push(AbiParam::new(types::I64)); // ptr
    seed_sig.params.push(AbiParam::new(types::I64)); // len
    let seed_id = module
        .declare_function("__RTS_FN_RT_SEED_SHAPES", Linkage::Import, &seed_sig)
        .map_err(|e| Unsupported::new(format!("declare shape seed: {e}")))?;

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

        // SEED the shape registry FIRST — before any code (bootstrap or program)
        // can perform a dynamic shape read on a baked id. `(ptr, len)` of the
        // embedded seed blob.
        let seed_gv = module.declare_data_in_func(seed.data, fb.func);
        let seed_ptr = fb.ins().global_value(types::I64, seed_gv);
        let seed_len = fb.ins().iconst(types::I64, seed.len as i64);
        let seed_ref = module.declare_func_in_func(seed_id, fb.func);
        fb.ins().call(seed_ref, &[seed_ptr, seed_len]);

        // Bootstrap BEFORE the program (GC hook + main thread + UI backend, all
        // behind the one generic `__RTS_FN_RT_INIT`).
        let rtinit_ref = module.declare_func_in_func(rtinit_id, fb.func);
        fb.ins().call(rtinit_ref, &[]);
        // …then the adapter-side hooks (shaped-object / thenable / async-error).
        // Both must run before the program touches any shaped object.
        let boot_ref = module.declare_func_in_func(boot_id, fb.func);
        fb.ins().call(boot_ref, &[]);

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
