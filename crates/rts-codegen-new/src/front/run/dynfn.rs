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
//! `rts_adapters::state` — `reset_codegen_state` is a quiescent-boundary-only
//! API precisely so `new Function`/eval can compile while the outer program is
//! live).
//!
//! Invoke contract: the produced `fn_ptr` is called through the LEGACY
//! `invoke_typed` path (all-i64 `extern "C"`), so every param and the return are
//! annotated `i64` in the synthesized source. A dynamic function is therefore
//! integer-typed — the `new Function` subset the old engine shipped. It is NOT
//! in the program's tail set (no `CallConv::Tail`), so the raw pointer is
//! extern-callable.

use std::sync::{Arc, Mutex};

use cranelift_module::{FuncOrDataId, Module};

use rts_runtime::namespaces::globals::function::ops::{
    CompiledFn, register_compile_fn,
};

/// The synthesized name of every dynamic function inside its own module. Never
/// collides with user code: each snippet compiles into a FRESH `JITModule`.
const DYN_FN_NAME: &str = "__rtsdyn_anonymous";

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
    let plist = params
        .iter()
        .map(|p| format!("{p}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!("function {DYN_FN_NAME}({plist}): i64 {{\n{body}\n}}\n");
    let prog = super::build_with_includes(&src)
        .map_err(|e| anyhow::anyhow!("new Function body: {e}"))?;
    let mut module = super::module_jit::make_module();
    super::module_jit::populate_module(&mut module, &prog)
        .map_err(|e| anyhow::anyhow!("new Function body: {e}"))?;
    module
        .finalize_definitions()
        .map_err(|e| anyhow::anyhow!("new Function finalize: {e}"))?;
    let Some(FuncOrDataId::Func(id)) = module.get_name(DYN_FN_NAME) else {
        anyhow::bail!("new Function: compiled module lost `{DYN_FN_NAME}`");
    };
    let fn_ptr = module.get_finalized_function(id) as u64;
    Ok(CompiledFn {
        fn_ptr,
        arity: params.len() as u8,
        keep_alive: Arc::new(Mutex::new(module)),
    })
}
