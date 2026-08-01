//! `new Function(params…, body)` — RUNTIME compilation of one dynamic function
//! through the SAME pipeline as any source (swc parse → HIR → lowering → JIT).
//!
//! rts-primitives owns the `Function` global's `__RTS_FN_GL_FUNCTION_NEW`, which
//! needs an engine to compile the body but must not depend on one (dependency
//! direction). It exposes a `COMPILE_FN_HOOK` the engine installs at program
//! bootstrap ([`register_hook`], called from `module_jit::compile_program`) —
//! the same OnceLock-hook pattern as the async errslot bridge.
//!
//! The compiled snippet is a NESTED program in its own `JITModule`, kept mapped
//! for the function's whole lifetime via the `CompiledFn::keep_alive` slot the
//! `Entry::Function` carries. Nested compilation is safe by design: the global
//! codegen state (shapes, ctor table) is ADDITIVE and never reset mid-run (see
//! `crate::state` — `reset_codegen_state` is a quiescent-boundary-only
//! API precisely so `new Function`/eval can compile while the outer program is
//! live).
//!
//! Invoke contract: the produced `fn_ptr` is the function's UNIFORM-ABI THUNK
//! (`(env, a0..a3, rest) -> word`, PolyValue words both ways — the same shape
//! every engine fn-value uses), flagged `uniform: true` so the Function invoke
//! paths route it like any first-class fn. Params are synthesized UNTYPED
//! (Tagged/`any` — real JS semantics: method calls/getters/setters on a param
//! dispatch dynamically via the proto tables), and the return is a word (a
//! string return survives the boundary). Fallback: if the thunk was not
//! definable for this body, the raw pointer ships with `uniform: false`
//! (legacy all-i64 — the pre-P5 contract).

use std::sync::{Arc, Mutex};

use cranelift_module::{FuncOrDataId, Module};

use rts_runtime::namespaces::globals::function::ops::{CompiledFn, register_compile_fn};

/// The synthesized name of every dynamic function inside its own module. Never
/// collides with user code: each snippet compiles into a FRESH `JITModule`.
const DYN_FN_NAME: &str = "__rtsdyn_anonymous";

/// Diagnostics: with `RTS_DYNFN_DUMP=<dir>` set, a `new Function` body that
/// FAILS to compile is written to `<dir>/dynfn_fail_<N>.js` alongside its error.
/// A dynamic body only exists in memory (its span does not index any on-disk
/// source), so without this there is no way to extract a failing function from
/// a multi-megabyte page bundle for a minimal repro.
fn dump_failing_body(src: &str, err: &impl std::fmt::Display) {
    let Ok(dir) = std::env::var("RTS_DYNFN_DUMP") else {
        return;
    };
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let path = std::path::Path::new(&dir).join(format!("dynfn_fail_{n}.js"));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, format!("// error: {err}\n{src}"));
}

/// Install [`compile_dynamic_fn`] as the rts-primitives `COMPILE_FN_HOOK`.
/// Idempotent (OnceLock behind `register_compile_fn`).
pub(super) fn register_hook() {
    register_compile_fn(compile_dynamic_fn);
}

/// The `COMPILE_FN_HOOK` impl: build `function __rtsdyn_anonymous(p: i64, …):
/// i64 { body }`, run it through the whole-program pipeline into a fresh
/// `JITModule`, and hand back the finalized code pointer + the module as the
/// keep-alive anchor.
fn compile_dynamic_fn(params: &[&str], body: &str) -> anyhow::Result<CompiledFn> {
    // A multi-megabyte minified page bundle recurses DEEP through the swc
    // parse + normalize + lowering pipeline — deeper than the caller thread's
    // default stack (the browser host's main thread overflowed the moment the
    // WhatsApp bundles started compiling past their first error). Compile on a
    // dedicated big-stack thread, synchronously. Safe off-thread: the codegen
    // state that must be shared (shapes / ctor tables / gcells) is process-
    // global by design (see `crate::state`), and the per-thread items are pure
    // caches; `parcompile` already compiles on worker threads the same way.
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("rts-dynfn-compile".into())
            .stack_size(256 * 1024 * 1024)
            .spawn_scoped(s, || compile_dynamic_fn_inner(params, body))
            .map_err(|e| anyhow::anyhow!("new Function: compile thread spawn failed: {e}"))?
            .join()
            .unwrap_or_else(|_| anyhow::bail!("new Function: compile thread panicked"))
    })
}

fn compile_dynamic_fn_inner(params: &[&str], body: &str) -> anyhow::Result<CompiledFn> {
    // Params UNTYPED (Tagged/`any`): real-JS semantics — a page script's
    // `function f(el) { el.textContent = x }` dispatches dynamically. A caller
    // MAY still pass an explicit annotation in the param string ("h: i64").
    let plist = params.to_vec().join(", ");
    let src = format!("function {DYN_FN_NAME}({plist}) {{\n{body}\n}}\n");
    let prog = super::build_with_includes(&src).map_err(|e| {
        dump_failing_body(&src, &e);
        anyhow::anyhow!("new Function body: {e}")
    })?;
    // THUNK-ON-DEMAND: nothing in this one-function module takes the body's
    // address, but the caller below asks for its thunk pointer BY NAME after
    // finalize — so the thunk must be emitted anyway.
    super::thunk::force_used(DYN_FN_NAME);
    let mut module = super::module_jit::make_module();
    super::module_jit::populate_module(&mut module, &prog).map_err(|e| {
        dump_failing_body(&src, &e);
        anyhow::anyhow!("new Function body: {e}")
    })?;
    module
        .finalize_definitions()
        .map_err(|e| anyhow::anyhow!("new Function finalize: {e}"))?;
    // Prefer the fn's UNIFORM THUNK (words in/out — see the module doc): the
    // invoke paths then treat it like any engine fn-value. The raw body pointer
    // is the legacy fallback when the thunk wasn't definable for this body.
    if let Some(FuncOrDataId::Func(tid)) = module.get_name(&super::thunk::thunk_name(DYN_FN_NAME)) {
        let fn_ptr = module.get_finalized_function(tid) as u64;
        return Ok(CompiledFn {
            fn_ptr,
            arity: params.len() as u8,
            uniform: true,
            keep_alive: Arc::new(Mutex::new(module)),
        });
    }
    let Some(FuncOrDataId::Func(id)) = module.get_name(DYN_FN_NAME) else {
        anyhow::bail!("new Function: compiled module lost `{DYN_FN_NAME}`");
    };
    let fn_ptr = module.get_finalized_function(id) as u64;
    Ok(CompiledFn {
        fn_ptr,
        arity: params.len() as u8,
        uniform: false,
        keep_alive: Arc::new(Mutex::new(module)),
    })
}
