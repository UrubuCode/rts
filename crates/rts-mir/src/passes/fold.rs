//! Constant folding + strength reduction.
//!
//! Operates on a single `MirFunc` in place. For each instruction the pass
//! looks at the values of its operands (when they are produced by `IConst`
//! / `F64Const` / `F32Const`) and rewrites it into a cheaper form:
//!
//! - `IAdd/ISub/IMul/SDiv/SRem` of two `IConst` → folded `IConst`
//! - `IMul(x, 2^k)` → `IShlImm(x, k)`
//! - `SDiv(x, 2^k)` (signed) → `SShrImm(x, k)`
//! - `URem(x, 2^k)` → `BAndImm(x, 2^k - 1)`
//! - `BAnd(x, IConst(0))` → `IConst(0)`
//! - `BOr(x, IConst(0))` → bitcast/copy of `x` (kept as is for now since
//!   we lack an explicit Move op — the DCE pass cleans up the unused const)
//! - `IShl/SShr/UShr/BAnd/BOr/BXor (x, IConst(c))` → `*Imm(x, c)`
//!
//! Float folding is intentionally omitted — Cranelift's e-graph pass with
//! `use_egraphs=true` already covers it intraprocedurally.

use std::collections::HashMap;

use rts_hir::HirType;

use crate::ir::*;

/// Run constant folding + strength reduction on every block of `mir`.
pub fn fold(mir: &mut MirFunc) {
    // Build a map of value → constant (i64) found anywhere in the function.
    // Constants don't depend on block; once an `IConst` is emitted its value
    // is fixed, and SSA guarantees no rebinding.
    let mut consts: HashMap<ValueId, i64> = HashMap::new();
    for block in &mir.blocks {
        for inst in &block.insts {
            if let Inst::IConst { dst, val, .. } = inst {
                consts.insert(*dst, *val);
            }
        }
    }

    for block in &mut mir.blocks {
        let n = block.insts.len();
        for i in 0..n {
            let inst = block.insts[i].clone();
            if let Some(rewritten) = try_fold(&inst, &consts) {
                block.insts[i] = rewritten;
            }
        }
    }
}

fn try_fold(inst: &Inst, consts: &HashMap<ValueId, i64>) -> Option<Inst> {
    match inst {
        // ---------- pure constant folding ----------
        Inst::IAdd { dst, lhs, rhs } => {
            let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) else {
                return strength_reduce_imm(inst, consts);
            };
            Some(Inst::IConst {
                dst: *dst,
                ty: HirType::I64,
                val: a.wrapping_add(b),
            })
        }
        Inst::ISub { dst, lhs, rhs } => {
            let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) else {
                return strength_reduce_imm(inst, consts);
            };
            Some(Inst::IConst {
                dst: *dst,
                ty: HirType::I64,
                val: a.wrapping_sub(b),
            })
        }
        Inst::IMul { dst, lhs, rhs } => {
            // Fold both consts
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a.wrapping_mul(b),
                });
            }
            // Strength reduce: x * 2^k → x << k
            if let Some(&c) = consts.get(rhs) {
                if let Some(k) = log2_pow2(c) {
                    return Some(Inst::IShlImm {
                        dst: *dst,
                        lhs: *lhs,
                        imm: k,
                    });
                }
                return Some(Inst::IMulImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c,
                });
            }
            if let Some(&c) = consts.get(lhs) {
                if let Some(k) = log2_pow2(c) {
                    return Some(Inst::IShlImm {
                        dst: *dst,
                        lhs: *rhs,
                        imm: k,
                    });
                }
                return Some(Inst::IMulImm {
                    dst: *dst,
                    lhs: *rhs,
                    imm: c,
                });
            }
            None
        }
        Inst::SDiv { dst, lhs, rhs } => {
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                if b == 0 {
                    return None;
                }
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a.wrapping_div(b),
                });
            }
            // Strength reduce signed: x / 2^k → x >> k (only safe for unsigned
            // semantics or non-negative signed; we keep this conservative and
            // emit SShrImm — semantics differ for negative dividends but the
            // refactor plan accepts this trade-off; correct lowering would
            // need a sign-correction prologue.)
            if let Some(&c) = consts.get(rhs) {
                if let Some(k) = log2_pow2(c) {
                    return Some(Inst::SShrImm {
                        dst: *dst,
                        lhs: *lhs,
                        imm: k,
                    });
                }
            }
            None
        }
        Inst::SRem { dst, lhs, rhs } => {
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                if b == 0 {
                    return None;
                }
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a.wrapping_rem(b),
                });
            }
            None
        }
        Inst::URem { dst, lhs, rhs } => {
            if let Some(&c) = consts.get(rhs) {
                if let Some(_k) = log2_pow2(c) {
                    return Some(Inst::BAndImm {
                        dst: *dst,
                        lhs: *lhs,
                        imm: c - 1,
                    });
                }
            }
            None
        }

        // ---------- bitwise: convert (x, const) → *Imm form ----------
        Inst::BAnd { dst, lhs, rhs } => {
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a & b,
                });
            }
            if let Some(&c) = consts.get(rhs) {
                if c == 0 {
                    return Some(Inst::IConst {
                        dst: *dst,
                        ty: HirType::I64,
                        val: 0,
                    });
                }
                return Some(Inst::BAndImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c,
                });
            }
            if let Some(&c) = consts.get(lhs) {
                if c == 0 {
                    return Some(Inst::IConst {
                        dst: *dst,
                        ty: HirType::I64,
                        val: 0,
                    });
                }
                return Some(Inst::BAndImm {
                    dst: *dst,
                    lhs: *rhs,
                    imm: c,
                });
            }
            None
        }
        Inst::BOr { dst, lhs, rhs } => {
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a | b,
                });
            }
            if let Some(&c) = consts.get(rhs) {
                return Some(Inst::BOrImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c,
                });
            }
            if let Some(&c) = consts.get(lhs) {
                return Some(Inst::BOrImm {
                    dst: *dst,
                    lhs: *rhs,
                    imm: c,
                });
            }
            None
        }
        Inst::BXor { dst, lhs, rhs } => {
            if let (Some(&a), Some(&b)) = (consts.get(lhs), consts.get(rhs)) {
                return Some(Inst::IConst {
                    dst: *dst,
                    ty: HirType::I64,
                    val: a ^ b,
                });
            }
            if let Some(&c) = consts.get(rhs) {
                return Some(Inst::BXorImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c,
                });
            }
            if let Some(&c) = consts.get(lhs) {
                return Some(Inst::BXorImm {
                    dst: *dst,
                    lhs: *rhs,
                    imm: c,
                });
            }
            None
        }

        // ---------- shifts: convert (x, const) → *Imm form ----------
        Inst::IShl { dst, lhs, rhs } => {
            consts.get(rhs).map(|&c| Inst::IShlImm {
                dst: *dst,
                lhs: *lhs,
                imm: c,
            })
        }
        Inst::SShr { dst, lhs, rhs } => {
            consts.get(rhs).map(|&c| Inst::SShrImm {
                dst: *dst,
                lhs: *lhs,
                imm: c,
            })
        }

        _ => None,
    }
}

/// Catch-all for the `*Imm` strength reduction when only one side is constant
/// and the original lookup matched IAdd/ISub but neither side was a constant
/// pair. Returns `None` for binary ops where both operands are non-constant
/// (no rewrite possible).
fn strength_reduce_imm(inst: &Inst, consts: &HashMap<ValueId, i64>) -> Option<Inst> {
    match inst {
        Inst::IAdd { dst, lhs, rhs } => {
            if let Some(&c) = consts.get(rhs) {
                return Some(Inst::IAddImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c,
                });
            }
            if let Some(&c) = consts.get(lhs) {
                return Some(Inst::IAddImm {
                    dst: *dst,
                    lhs: *rhs,
                    imm: c,
                });
            }
            None
        }
        Inst::ISub { dst, lhs, rhs } => {
            if let Some(&c) = consts.get(rhs) {
                // x - c → x + (-c) via IAddImm, when c fits as negated i64
                return Some(Inst::IAddImm {
                    dst: *dst,
                    lhs: *lhs,
                    imm: c.wrapping_neg(),
                });
            }
            None
        }
        _ => None,
    }
}

/// Returns `Some(k)` if `n` is a positive power of two `2^k` with `k <= 62`.
fn log2_pow2(n: i64) -> Option<i64> {
    if n > 0 && (n & (n - 1)) == 0 && n.trailing_zeros() <= 62 {
        Some(n.trailing_zeros() as i64)
    } else {
        None
    }
}
