//! The single lowering path: HIR -> Cranelift IR. No second optimizer tier.
//!
//! The old engine had TWO full codegens (an "AST authoritative" path and an
//! HIR->MIR->Cranelift path that re-did Cranelift's egraph and silently fell
//! back to AST for ~99% of real JS). This crate has ONE path: typed HIR lowers
//! directly to Cranelift, and Cranelift's egraph (`use_egraphs=true`) is the sole
//! optimizer. The front-end's only IR job is what Cranelift genuinely cannot do:
//! JS-semantic lowering (ToNumber/ToString/ToBoolean), polymorphic-`+`
//! resolution, box/unbox insertion ([`crate::value`]), shape/IC site emission,
//! narrow-int wrap semantics, and exception edges. box/unbox are pure Cranelift
//! ops so the egraph folds redundant box-then-unbox away.
//!
//! ## P1 slice
//!
//! [`ir`] is a minimal typed IR (`Node`/`Func`); [`lower`] lowers it to a
//! Cranelift `Function` over `&mut dyn Module` (the same `Module`-trait surface
//! AOT and JIT share); [`jit`] is the JIT harness that finalizes a `Func` to
//! executable memory with the `runtime::__rtsn_*` symbols installed. Together
//! they prove the [`crate::value::PolyValue`] model end-to-end through real
//! Cranelift execution (see `crate::proof_tests`).

pub mod ir;
pub mod jit;
pub mod lower;

/// Lower one typed function to Cranelift IR (full-HIR entry point — built out in
/// later phases; P1 uses [`lower::lower_func`] over the minimal [`ir::Func`]).
pub fn lower_function() {
    todo!("phase: pipeline — full HIR -> Cranelift, single path")
}
