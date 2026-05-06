//! MIR verifier — invariants checked in debug builds and tests.
//!
//! Catches malformed MIR before it reaches the codegen layer, where the
//! diagnostics would be dramatically harder to track.
//!
//! Checked invariants:
//! 1. Every `BasicBlock.id` matches its position in `MirFunc.blocks`.
//! 2. Every `ValueId` referenced anywhere is `< mir.values.len()`.
//! 3. Every `BlockId` referenced by a Terminator is `< mir.blocks.len()`.
//! 4. Every block has its `term` set (default ctor uses `Trap` so any
//!    well-built block passes).
//! 5. Function params match the entry block's block params count.
//!
//! Does NOT check (out of scope):
//! - SSA dominance (each value used after its def)
//! - Type consistency between Inst operands and value types
//! - Reachability of all blocks
//!
//! These could be added; the current set is enough to catch the bugs the
//! lower + fold + dce passes can introduce in normal use.

use crate::ir::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    BlockIdMismatch { idx: usize, declared: BlockId },
    ValueOutOfRange { value: ValueId, max: usize, location: String },
    BlockOutOfRange { block: BlockId, max: usize, location: String },
    ParamCountMismatch { fn_params: usize, entry_block_params: usize },
}

/// Run all invariant checks. Returns `Ok(())` when the function is valid,
/// or the first error encountered.
pub fn verify(mir: &MirFunc) -> Result<(), VerifyError> {
    // (1) block ids match positions
    for (i, b) in mir.blocks.iter().enumerate() {
        if b.id as usize != i {
            return Err(VerifyError::BlockIdMismatch {
                idx: i,
                declared: b.id,
            });
        }
    }

    // (5) entry block params count matches function params
    if let Some(entry) = mir.blocks.first() {
        if entry.params.len() != mir.params.len() {
            return Err(VerifyError::ParamCountMismatch {
                fn_params: mir.params.len(),
                entry_block_params: entry.params.len(),
            });
        }
    }

    let nvals = mir.values.len();
    let nblocks = mir.blocks.len();

    let check_v = |v: ValueId, loc: &str| -> Result<(), VerifyError> {
        if (v as usize) >= nvals {
            return Err(VerifyError::ValueOutOfRange {
                value: v,
                max: nvals,
                location: loc.into(),
            });
        }
        Ok(())
    };
    let check_b = |b: BlockId, loc: &str| -> Result<(), VerifyError> {
        if (b as usize) >= nblocks {
            return Err(VerifyError::BlockOutOfRange {
                block: b,
                max: nblocks,
                location: loc.into(),
            });
        }
        Ok(())
    };

    for block in &mir.blocks {
        let bloc = format!("block{}", block.id);
        for (vid, _) in &block.params {
            check_v(*vid, &format!("{bloc}.param"))?;
        }
        for inst in &block.insts {
            verify_inst(inst, &bloc, &check_v)?;
        }
        verify_term(&block.term, &bloc, &check_v, &check_b)?;
    }
    Ok(())
}

fn verify_inst(
    inst: &Inst,
    bloc: &str,
    check_v: &impl Fn(ValueId, &str) -> Result<(), VerifyError>,
) -> Result<(), VerifyError> {
    use Inst::*;
    let loc = |suffix: &str| format!("{bloc}.{suffix}");
    match inst {
        IAdd { dst, lhs, rhs }
        | ISub { dst, lhs, rhs }
        | IMul { dst, lhs, rhs }
        | SDiv { dst, lhs, rhs }
        | UDiv { dst, lhs, rhs }
        | SRem { dst, lhs, rhs }
        | URem { dst, lhs, rhs }
        | BAnd { dst, lhs, rhs }
        | BOr { dst, lhs, rhs }
        | BXor { dst, lhs, rhs }
        | IShl { dst, lhs, rhs }
        | SShr { dst, lhs, rhs }
        | UShr { dst, lhs, rhs }
        | Rotl { dst, lhs, rhs }
        | Rotr { dst, lhs, rhs }
        | FAdd { dst, lhs, rhs }
        | FSub { dst, lhs, rhs }
        | FMul { dst, lhs, rhs }
        | FDiv { dst, lhs, rhs }
        | FMin { dst, lhs, rhs }
        | FMax { dst, lhs, rhs }
        | ICmp { dst, lhs, rhs, .. }
        | FCmp { dst, lhs, rhs, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*lhs, &loc("lhs"))?;
            check_v(*rhs, &loc("rhs"))?;
        }

        IAddImm { dst, lhs, .. }
        | IMulImm { dst, lhs, .. }
        | BAndImm { dst, lhs, .. }
        | BOrImm { dst, lhs, .. }
        | BXorImm { dst, lhs, .. }
        | IShlImm { dst, lhs, .. }
        | SShrImm { dst, lhs, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*lhs, &loc("lhs"))?;
        }

        INeg { dst, src }
        | BNot { dst, src }
        | Clz { dst, src }
        | Ctz { dst, src }
        | Popcnt { dst, src }
        | Bswap { dst, src }
        | FNeg { dst, src }
        | FAbs { dst, src }
        | Sqrt { dst, src }
        | Ceil { dst, src }
        | Floor { dst, src }
        | Trunc { dst, src }
        | IReduce { dst, src, .. }
        | UExtend { dst, src, .. }
        | SExtend { dst, src, .. }
        | FPromote { dst, src }
        | FDemote { dst, src }
        | CvtFromSint { dst, src, .. }
        | CvtFromUint { dst, src, .. }
        | CvtToSintSat { dst, src, .. }
        | CvtToUintSat { dst, src, .. }
        | Bitcast { dst, src, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*src, &loc("src"))?;
        }

        Fma { dst, a, b, c } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*a, &loc("a"))?;
            check_v(*b, &loc("b"))?;
            check_v(*c, &loc("c"))?;
        }

        Load { dst, ptr, .. }
        | ULoad8 { dst, ptr, .. }
        | ULoad16 { dst, ptr, .. }
        | ULoad32 { dst, ptr, .. }
        | SLoad8 { dst, ptr, .. }
        | SLoad16 { dst, ptr, .. }
        | SLoad32 { dst, ptr, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*ptr, &loc("ptr"))?;
        }
        Store { val, ptr, .. }
        | IStore8 { val, ptr, .. }
        | IStore16 { val, ptr, .. }
        | IStore32 { val, ptr, .. } => {
            check_v(*val, &loc("val"))?;
            check_v(*ptr, &loc("ptr"))?;
        }

        Select { dst, cond, on_true, on_false } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*cond, &loc("cond"))?;
            check_v(*on_true, &loc("on_true"))?;
            check_v(*on_false, &loc("on_false"))?;
        }

        CallExtern { dst, args, .. } => {
            if let Some(d) = dst {
                check_v(*d, &loc("call.dst"))?;
            }
            for (i, a) in args.iter().enumerate() {
                check_v(*a, &loc(&format!("call.arg{i}")))?;
            }
        }

        CallUser { dst, args, .. } => {
            if let Some(d) = dst {
                check_v(*d, &loc("call_user.dst"))?;
            }
            for (i, a) in args.iter().enumerate() {
                check_v(*a, &loc(&format!("call_user.arg{i}")))?;
            }
        }

        IConst { dst, .. } | F32Const { dst, .. } | F64Const { dst, .. } | StackAlloc { dst, .. } => {
            check_v(*dst, &loc("dst"))?;
        }

        AtomicLoad { dst, ptr, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*ptr, &loc("ptr"))?;
        }
        AtomicStore { val, ptr, .. } => {
            check_v(*val, &loc("val"))?;
            check_v(*ptr, &loc("ptr"))?;
        }
        AtomicRmw { dst, ptr, val, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*ptr, &loc("ptr"))?;
            check_v(*val, &loc("val"))?;
        }
        AtomicCas { dst, ptr, expected, replacement, .. } => {
            check_v(*dst, &loc("dst"))?;
            check_v(*ptr, &loc("ptr"))?;
            check_v(*expected, &loc("expected"))?;
            check_v(*replacement, &loc("replacement"))?;
        }
        Fence { .. } => {}

        DeclareGcValue { val } => {
            check_v(*val, &loc("gc"))?;
        }
    }
    Ok(())
}

fn verify_term(
    term: &Terminator,
    bloc: &str,
    check_v: &impl Fn(ValueId, &str) -> Result<(), VerifyError>,
    check_b: &impl Fn(BlockId, &str) -> Result<(), VerifyError>,
) -> Result<(), VerifyError> {
    let loc = |suffix: &str| format!("{bloc}.term.{suffix}");
    match term {
        Terminator::Return(vs) => {
            for (i, v) in vs.iter().enumerate() {
                check_v(*v, &loc(&format!("ret{i}")))?;
            }
        }
        Terminator::Jump { target, args } => {
            check_b(*target, &loc("jump.target"))?;
            for (i, v) in args.iter().enumerate() {
                check_v(*v, &loc(&format!("jump.arg{i}")))?;
            }
        }
        Terminator::Brif { cond, then_block, then_args, else_block, else_args } => {
            check_v(*cond, &loc("brif.cond"))?;
            check_b(*then_block, &loc("brif.then"))?;
            check_b(*else_block, &loc("brif.else"))?;
            for (i, v) in then_args.iter().enumerate() {
                check_v(*v, &loc(&format!("brif.then_arg{i}")))?;
            }
            for (i, v) in else_args.iter().enumerate() {
                check_v(*v, &loc(&format!("brif.else_arg{i}")))?;
            }
        }
        Terminator::Switch { index, default, cases } => {
            check_v(*index, &loc("switch.index"))?;
            check_b(*default, &loc("switch.default"))?;
            for (i, (_, b)) in cases.iter().enumerate() {
                check_b(*b, &loc(&format!("switch.case{i}")))?;
            }
        }
        Terminator::TailCall { args, .. } => {
            for (i, v) in args.iter().enumerate() {
                check_v(*v, &loc(&format!("tailcall.arg{i}")))?;
            }
        }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            check_v(*fn_ptr, &loc("tailcall.fn_ptr"))?;
            for (i, v) in args.iter().enumerate() {
                check_v(*v, &loc(&format!("tailcall.arg{i}")))?;
            }
        }
        Terminator::Trap { .. } => {}
    }
    Ok(())
}
