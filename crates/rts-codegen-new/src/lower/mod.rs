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

/// Lower one typed function to Cranelift IR (entry point — to be built out).
pub fn lower_function() {
    todo!("phase: pipeline — HIR -> Cranelift, single path")
}
