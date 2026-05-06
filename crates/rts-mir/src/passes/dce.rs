//! Dead code elimination over MIR.
//!
//! Removes instructions whose `dst` is never read by:
//! - any other instruction (operand fields)
//! - any terminator (return/jump args, brif cond/args, switch index)
//! - any block param entry (phi sources arrive via terminator args)
//!
//! Side-effecting instructions are kept: `Store`, `IStore8/16/32`,
//! `CallExtern`, `AtomicStore`, `AtomicRmw` (without dst), `AtomicCas`,
//! `Fence`, `DeclareGcValue`. These have either no `dst` or are observable.
//!
//! The pass runs to a single fixed point: it iterates until no instruction
//! is removed in a full sweep.

use std::collections::HashSet;

use crate::ir::*;

/// Strip dead instructions from `mir` in place.
pub fn dce(mir: &mut MirFunc) {
    loop {
        let used = collect_used(mir);
        let mut changed = false;
        for block in &mut mir.blocks {
            let before = block.insts.len();
            block.insts.retain(|inst| keep_inst(inst, &used));
            if block.insts.len() != before {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Set of all `ValueId`s that are read somewhere — by another instruction's
/// operand or by a terminator.
fn collect_used(mir: &MirFunc) -> HashSet<ValueId> {
    let mut used = HashSet::new();
    for block in &mir.blocks {
        for inst in &block.insts {
            collect_used_in_inst(inst, &mut used);
        }
        collect_used_in_term(&block.term, &mut used);
    }
    used
}

fn collect_used_in_inst(inst: &Inst, used: &mut HashSet<ValueId>) {
    use Inst::*;
    match inst {
        IAdd { lhs, rhs, .. }
        | ISub { lhs, rhs, .. }
        | IMul { lhs, rhs, .. }
        | SDiv { lhs, rhs, .. }
        | UDiv { lhs, rhs, .. }
        | SRem { lhs, rhs, .. }
        | URem { lhs, rhs, .. }
        | BAnd { lhs, rhs, .. }
        | BOr { lhs, rhs, .. }
        | BXor { lhs, rhs, .. }
        | IShl { lhs, rhs, .. }
        | SShr { lhs, rhs, .. }
        | UShr { lhs, rhs, .. }
        | Rotl { lhs, rhs, .. }
        | Rotr { lhs, rhs, .. }
        | FAdd { lhs, rhs, .. }
        | FSub { lhs, rhs, .. }
        | FMul { lhs, rhs, .. }
        | FDiv { lhs, rhs, .. }
        | FMin { lhs, rhs, .. }
        | FMax { lhs, rhs, .. }
        | ICmp { lhs, rhs, .. }
        | FCmp { lhs, rhs, .. } => {
            used.insert(*lhs);
            used.insert(*rhs);
        }

        IAddImm { lhs, .. }
        | IMulImm { lhs, .. }
        | BAndImm { lhs, .. }
        | BOrImm { lhs, .. }
        | BXorImm { lhs, .. }
        | IShlImm { lhs, .. }
        | SShrImm { lhs, .. } => {
            used.insert(*lhs);
        }

        INeg { src, .. }
        | BNot { src, .. }
        | Clz { src, .. }
        | Ctz { src, .. }
        | Popcnt { src, .. }
        | Bswap { src, .. }
        | FNeg { src, .. }
        | FAbs { src, .. }
        | Sqrt { src, .. }
        | Ceil { src, .. }
        | Floor { src, .. }
        | Trunc { src, .. }
        | IReduce { src, .. }
        | UExtend { src, .. }
        | SExtend { src, .. }
        | FPromote { src, .. }
        | FDemote { src, .. }
        | CvtFromSint { src, .. }
        | CvtFromUint { src, .. }
        | CvtToSintSat { src, .. }
        | CvtToUintSat { src, .. }
        | Bitcast { src, .. } => {
            used.insert(*src);
        }

        Fma { a, b, c, .. } => {
            used.insert(*a);
            used.insert(*b);
            used.insert(*c);
        }

        Load { ptr, .. } | ULoad8 { ptr, .. } | ULoad16 { ptr, .. } | ULoad32 { ptr, .. }
        | SLoad8 { ptr, .. } | SLoad16 { ptr, .. } | SLoad32 { ptr, .. } => {
            used.insert(*ptr);
        }

        Store { val, ptr, .. }
        | IStore8 { val, ptr, .. }
        | IStore16 { val, ptr, .. }
        | IStore32 { val, ptr, .. } => {
            used.insert(*val);
            used.insert(*ptr);
        }

        Select { cond, on_true, on_false, .. } => {
            used.insert(*cond);
            used.insert(*on_true);
            used.insert(*on_false);
        }

        CallExtern { args, .. } => {
            for a in args {
                used.insert(*a);
            }
        }

        CallUser { args, .. } => {
            for a in args {
                used.insert(*a);
            }
        }

        AtomicLoad { ptr, .. } => {
            used.insert(*ptr);
        }
        AtomicStore { val, ptr, .. } => {
            used.insert(*val);
            used.insert(*ptr);
        }
        AtomicRmw { ptr, val, .. } => {
            used.insert(*ptr);
            used.insert(*val);
        }
        AtomicCas { ptr, expected, replacement, .. } => {
            used.insert(*ptr);
            used.insert(*expected);
            used.insert(*replacement);
        }

        DeclareGcValue { val } => {
            used.insert(*val);
        }

        // Constants and stack alloc and Fence — no operand reads
        IConst { .. }
        | F32Const { .. }
        | F64Const { .. }
        | StackAlloc { .. }
        | Fence { .. } => {}
    }
}

fn collect_used_in_term(term: &Terminator, used: &mut HashSet<ValueId>) {
    match term {
        Terminator::Return(vs) => {
            for v in vs {
                used.insert(*v);
            }
        }
        Terminator::Jump { args, .. } => {
            for v in args {
                used.insert(*v);
            }
        }
        Terminator::Brif { cond, then_args, else_args, .. } => {
            used.insert(*cond);
            for v in then_args {
                used.insert(*v);
            }
            for v in else_args {
                used.insert(*v);
            }
        }
        Terminator::Switch { index, .. } => {
            used.insert(*index);
        }
        Terminator::TailCall { args, .. } => {
            for v in args {
                used.insert(*v);
            }
        }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            used.insert(*fn_ptr);
            for v in args {
                used.insert(*v);
            }
        }
        Terminator::Trap { .. } => {}
    }
}

/// Decide whether to keep an instruction.
fn keep_inst(inst: &Inst, used: &HashSet<ValueId>) -> bool {
    use Inst::*;
    // Side-effecting / observable instructions are always kept.
    match inst {
        Store { .. }
        | IStore8 { .. }
        | IStore16 { .. }
        | IStore32 { .. }
        | CallExtern { dst: None, .. }
        | CallUser { .. } // user calls always kept — may have side effects
        | AtomicStore { .. }
        | AtomicRmw { .. } // even if dst is unused, the rmw side effect is observable
        | AtomicCas { .. }
        | Fence { .. }
        | DeclareGcValue { .. } => return true,
        _ => {}
    }
    // For instructions with a `dst`, keep iff some other reader exists.
    match inst {
        IAdd { dst, .. }
        | ISub { dst, .. }
        | IMul { dst, .. }
        | IMulImm { dst, .. }
        | IAddImm { dst, .. }
        | SDiv { dst, .. }
        | UDiv { dst, .. }
        | SRem { dst, .. }
        | URem { dst, .. }
        | INeg { dst, .. }
        | BAnd { dst, .. }
        | BAndImm { dst, .. }
        | BOr { dst, .. }
        | BOrImm { dst, .. }
        | BXor { dst, .. }
        | BXorImm { dst, .. }
        | BNot { dst, .. }
        | IShl { dst, .. }
        | IShlImm { dst, .. }
        | UShr { dst, .. }
        | SShr { dst, .. }
        | SShrImm { dst, .. }
        | Rotl { dst, .. }
        | Rotr { dst, .. }
        | Clz { dst, .. }
        | Ctz { dst, .. }
        | Popcnt { dst, .. }
        | Bswap { dst, .. }
        | FAdd { dst, .. }
        | FSub { dst, .. }
        | FMul { dst, .. }
        | FDiv { dst, .. }
        | FNeg { dst, .. }
        | FAbs { dst, .. }
        | Sqrt { dst, .. }
        | Fma { dst, .. }
        | FMin { dst, .. }
        | FMax { dst, .. }
        | Ceil { dst, .. }
        | Floor { dst, .. }
        | Trunc { dst, .. }
        | IReduce { dst, .. }
        | UExtend { dst, .. }
        | SExtend { dst, .. }
        | FPromote { dst, .. }
        | FDemote { dst, .. }
        | CvtFromSint { dst, .. }
        | CvtFromUint { dst, .. }
        | CvtToSintSat { dst, .. }
        | CvtToUintSat { dst, .. }
        | Bitcast { dst, .. }
        | ICmp { dst, .. }
        | FCmp { dst, .. }
        | IConst { dst, .. }
        | F32Const { dst, .. }
        | F64Const { dst, .. }
        | Load { dst, .. }
        | ULoad8 { dst, .. }
        | ULoad16 { dst, .. }
        | ULoad32 { dst, .. }
        | SLoad8 { dst, .. }
        | SLoad16 { dst, .. }
        | SLoad32 { dst, .. }
        | StackAlloc { dst, .. }
        | Select { dst, .. }
        | AtomicLoad { dst, .. } => used.contains(dst),

        CallExtern { dst: Some(dst), .. } => used.contains(dst),

        // Already handled above as side-effecting.
        Store { .. }
        | IStore8 { .. }
        | IStore16 { .. }
        | IStore32 { .. }
        | CallExtern { dst: None, .. }
        | CallUser { .. }
        | AtomicStore { .. }
        | AtomicRmw { .. }
        | AtomicCas { .. }
        | Fence { .. }
        | DeclareGcValue { .. } => true,
    }
}

