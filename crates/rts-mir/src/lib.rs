//! MIR — Mid-level IR between HIR and Cranelift codegen.
//!
//! SSA-like with basic blocks + block parameters (= phis), modeled isomorphic
//! to Cranelift IR so the lowering MIR → Cranelift in `rts-codegen` is a
//! mechanical 1:1 translation.
//!
//! See `RTS_REFACTOR.md` §rts-mir for the full specification.
//!
//! Status: Fase 3 inicial — esqueleto de tipos. Lowering HIR → MIR e passes
//! (fold/dce/narrow/inline/escape) virão em sub-etapas seguintes.

pub mod ir;

pub use ir::{
    BasicBlock, BlockId, CallConvHint, FloatCond, IntCond, Inst, MemHint, MemOrder, MirFunc,
    RmwOp, Terminator, TrapHint, ValueId,
};

#[cfg(test)]
mod tests;
