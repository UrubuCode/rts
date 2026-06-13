//! Pipeline orchestration — TS -> native, JIT and AOT sharing one lowering.
//!
//! Mirrors the one genuinely-good piece of the old pipeline (shared
//! `compile_program` for JIT/AOT, SHA256 object cache with transitive-dep +
//! compiler-fingerprint invalidation) while dropping the dual-codegen and the
//! hand-written symbol table. Filled in once `lower` + `dispatch` + `abi_gen`
//! land.

/// Compile + JIT-run a module (the `rts run` path). To be built out.
pub fn run_jit() {
    todo!("phase: pipeline")
}

/// Compile a module to a native object/binary (the `rts compile` path).
pub fn compile_aot() {
    todo!("phase: pipeline")
}
