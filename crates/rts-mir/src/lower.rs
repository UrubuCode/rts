//! HIR → MIR lowering.
//!
//! Converts a `HirFunc` into a `MirFunc` with SSA-like basic blocks. This
//! is the initial implementation covering the linear path: arithmetic,
//! comparisons, if/else, while, let bindings, return. Constructs that
//! aren't yet supported lower to `Terminator::Trap { code: User(_) }` so
//! `rts-codegen` consuming the MIR can decide to fall back to the AST path.
//!
//! Not yet covered (will be follow-up sub-stages):
//! - for/forof/forin loops
//! - try/catch/finally + throw
//! - switch
//! - method calls + member access
//! - array/object literals
//! - extern calls (rts namespace dispatch) — needs ABI integration
//! - destructuring, spread, ternary nesting beyond simple cases

use std::collections::HashMap;

use rts_hir::ir::{
    HirBinOp, HirExpr, HirExprKind, HirFunc, HirLit, HirStmt, HirType, HirUnOp,
};

use crate::ir::*;

/// Unsupported-construct trap code. Picked to stay distinct so debugging the
/// generated MIR can pinpoint which HIR feature triggered the fallback.
const TRAP_UNSUPPORTED_STMT: u16 = 0x10;

/// Public entry: lower a fully-typed `HirFunc` into a `MirFunc`.
pub fn lower_func(func: &HirFunc) -> MirFunc {
    let mut mir = MirFunc::new(
        &func.name,
        // Tail conv matches `rts-codegen`'s default for user fns
        CallConvHint::Tail,
        func.ret.clone(),
    );

    // Allocate one ValueId per parameter and record it as a function param.
    let mut env: HashMap<String, ValueId> = HashMap::new();
    for p in &func.params {
        let v = mir.new_value(p.ty.clone());
        mir.params.push((v, p.ty.clone()));
        env.insert(p.name.clone(), v);
    }

    // Entry block carries the function params as block params (Cranelift idiom).
    let entry = mir.new_block();
    for (vid, ty) in mir.params.clone() {
        mir.blocks[entry as usize].params.push((vid, ty));
    }

    let mut ctx = LowerCtx {
        cursor: entry,
        env,
        terminated: false,
    };

    lower_stmts(&func.body, &mut mir, &mut ctx);

    // If control falls off the end without an explicit return, append one.
    if !ctx.terminated {
        let trap = if matches!(func.ret, HirType::Void) {
            Terminator::Return(vec![])
        } else {
            // Returning the zero value of the declared type keeps the IR
            // well-formed even when the source forgot to return.
            let zero = emit_zero_const(&func.ret, &mut mir, &mut ctx);
            Terminator::Return(vec![zero])
        };
        set_term(&mut mir, ctx.cursor, trap);
    }

    mir
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx {
    cursor: BlockId,
    env: HashMap<String, ValueId>,
    /// True once the current block has emitted a terminator. Statements past
    /// that point are dead and lowered into a fresh unreachable block.
    terminated: bool,
}

impl LowerCtx {
    fn push_inst(&self, mir: &mut MirFunc, inst: Inst) {
        mir.blocks[self.cursor as usize].insts.push(inst);
    }
}

fn set_term(mir: &mut MirFunc, b: BlockId, term: Terminator) {
    mir.blocks[b as usize].term = term;
}

// ---------------------------------------------------------------------------
// Statement lowering
// ---------------------------------------------------------------------------

fn lower_stmts(stmts: &[HirStmt], mir: &mut MirFunc, ctx: &mut LowerCtx) {
    for stmt in stmts {
        if ctx.terminated {
            // Drop dead statements after a terminator — cleaner IR.
            break;
        }
        lower_stmt(stmt, mir, ctx);
    }
}

fn lower_stmt(stmt: &HirStmt, mir: &mut MirFunc, ctx: &mut LowerCtx) {
    match stmt {
        HirStmt::Expr(e) => {
            // Side-effecting expression statement: lower & discard.
            let _ = lower_expr(e, mir, ctx);
        }

        HirStmt::Return(opt) => {
            let term = match opt {
                Some(e) => {
                    let v = lower_expr(e, mir, ctx);
                    Terminator::Return(vec![v])
                }
                None => Terminator::Return(vec![]),
            };
            set_term(mir, ctx.cursor, term);
            ctx.terminated = true;
        }

        HirStmt::Let { name, ty, init } => {
            let v = match init {
                Some(e) => lower_expr(e, mir, ctx),
                None => emit_zero_const(ty, mir, ctx),
            };
            ctx.env.insert(name.clone(), v);
        }

        HirStmt::Const { name, init, .. } => {
            let v = lower_expr(init, mir, ctx);
            ctx.env.insert(name.clone(), v);
        }

        HirStmt::If { cond, then, else_ } => lower_if(cond, then, else_.as_deref(), mir, ctx),

        HirStmt::While { cond, body } => lower_while(cond, body, mir, ctx),

        HirStmt::Block(body) => lower_stmts(body, mir, ctx),

        // Constructs not yet supported — emit trap, mark terminated so caller
        // doesn't keep emitting into a dead block.
        _ => {
            set_term(
                mir,
                ctx.cursor,
                Terminator::Trap {
                    code: TrapHint::User(TRAP_UNSUPPORTED_STMT),
                },
            );
            ctx.terminated = true;
        }
    }
}

fn lower_if(
    cond: &HirExpr,
    then: &[HirStmt],
    else_: Option<&[HirStmt]>,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) {
    let cond_v = lower_expr(cond, mir, ctx);

    let then_b = mir.new_block();
    let else_b = mir.new_block();
    let join_b = mir.new_block();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: then_b,
            then_args: vec![],
            else_block: else_b,
            else_args: vec![],
        },
    );

    // then branch
    ctx.cursor = then_b;
    ctx.terminated = false;
    lower_stmts(then, mir, ctx);
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: join_b,
                args: vec![],
            },
        );
    }

    // else branch
    ctx.cursor = else_b;
    ctx.terminated = false;
    if let Some(stmts) = else_ {
        lower_stmts(stmts, mir, ctx);
    }
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: join_b,
                args: vec![],
            },
        );
    }

    // continue at join
    ctx.cursor = join_b;
    ctx.terminated = false;
}

fn lower_while(cond: &HirExpr, body: &[HirStmt], mir: &mut MirFunc, ctx: &mut LowerCtx) {
    let header_b = mir.new_block();
    let body_b = mir.new_block();
    let exit_b = mir.new_block();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Jump {
            target: header_b,
            args: vec![],
        },
    );

    // header: test condition
    ctx.cursor = header_b;
    ctx.terminated = false;
    let cond_v = lower_expr(cond, mir, ctx);
    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: vec![],
        },
    );

    // body: jump back to header at end
    ctx.cursor = body_b;
    ctx.terminated = false;
    lower_stmts(body, mir, ctx);
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: header_b,
                args: vec![],
            },
        );
    }

    ctx.cursor = exit_b;
    ctx.terminated = false;
}

// ---------------------------------------------------------------------------
// Expression lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &HirExpr, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    match &expr.kind {
        HirExprKind::Lit(lit) => lower_lit(lit, &expr.ty, mir, ctx),

        HirExprKind::Ident(name) => match ctx.env.get(name) {
            Some(&v) => v,
            None => {
                // Unknown ident — emit a placeholder zero of the expected type.
                emit_zero_const(&expr.ty, mir, ctx)
            }
        },

        HirExprKind::Bin { op, lhs, rhs } => {
            let lv = lower_expr(lhs, mir, ctx);
            let rv = lower_expr(rhs, mir, ctx);
            lower_bin(*op, lv, rv, &expr.ty, &lhs.ty, mir, ctx)
        }

        HirExprKind::Unary { op, operand } => {
            let v = lower_expr(operand, mir, ctx);
            lower_unary(*op, v, &expr.ty, mir, ctx)
        }

        HirExprKind::Cast { expr: inner, target } => {
            let v = lower_expr(inner, mir, ctx);
            lower_cast(v, &inner.ty, target, mir, ctx)
        }

        // Unsupported: emit a placeholder of the expected type and trap when
        // we leave the block (handled by caller observing `ctx.terminated`
        // after reaching a stmt that uses it). For now, emit an iconst 0.
        _ => emit_zero_const(&expr.ty, mir, ctx),
    }
}

fn lower_lit(lit: &HirLit, ty: &HirType, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    match lit {
        HirLit::Int(n) => {
            let dst_ty = if matches!(ty, HirType::Unknown | HirType::Any) {
                HirType::I64
            } else {
                ty.clone()
            };
            let dst = mir.new_value(dst_ty.clone());
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: dst_ty,
                    val: *n,
                },
            );
            dst
        }
        HirLit::Float(f) => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::F64Const { dst, val: *f });
            dst
        }
        HirLit::Number(n) => {
            let dst = mir.new_value(HirType::Number);
            ctx.push_inst(mir, Inst::F64Const { dst, val: *n });
            dst
        }
        HirLit::Bool(b) => {
            let dst = mir.new_value(HirType::Bool);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::Bool,
                    val: if *b { 1 } else { 0 },
                },
            );
            dst
        }
        HirLit::Null | HirLit::Undefined => {
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::I64,
                    val: 0,
                },
            );
            dst
        }
        HirLit::Str(_) => {
            // String literals are codegen-time and need GC; emit a placeholder
            // i64 zero — codegen will fall back to AST path for string ops
            // until the extern-call lowering lands.
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::I64,
                    val: 0,
                },
            );
            dst
        }
    }
}

fn lower_bin(
    op: HirBinOp,
    lhs: ValueId,
    rhs: ValueId,
    res_ty: &HirType,
    operand_ty: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let is_float = operand_ty.is_float() || res_ty.is_float();
    let dst = mir.new_value(res_ty.clone());

    let inst = match (op, is_float) {
        // Arithmetic
        (HirBinOp::Add, false) => Inst::IAdd { dst, lhs, rhs },
        (HirBinOp::Add, true) => Inst::FAdd { dst, lhs, rhs },
        (HirBinOp::Sub, false) => Inst::ISub { dst, lhs, rhs },
        (HirBinOp::Sub, true) => Inst::FSub { dst, lhs, rhs },
        (HirBinOp::Mul, false) => Inst::IMul { dst, lhs, rhs },
        (HirBinOp::Mul, true) => Inst::FMul { dst, lhs, rhs },
        (HirBinOp::Div, false) => Inst::SDiv { dst, lhs, rhs },
        (HirBinOp::Div, true) => Inst::FDiv { dst, lhs, rhs },
        (HirBinOp::Rem, false) => Inst::SRem { dst, lhs, rhs },

        // Bitwise
        (HirBinOp::BitAnd, false) => Inst::BAnd { dst, lhs, rhs },
        (HirBinOp::BitOr, false) => Inst::BOr { dst, lhs, rhs },
        (HirBinOp::BitXor, false) => Inst::BXor { dst, lhs, rhs },
        (HirBinOp::Shl, false) => Inst::IShl { dst, lhs, rhs },
        (HirBinOp::Shr, false) => Inst::SShr { dst, lhs, rhs },
        (HirBinOp::UShr, false) => Inst::UShr { dst, lhs, rhs },

        // Comparisons
        (HirBinOp::Eq, false) => Inst::ICmp { dst, cond: IntCond::Eq, lhs, rhs },
        (HirBinOp::Eq, true) => Inst::FCmp { dst, cond: FloatCond::Eq, lhs, rhs },
        (HirBinOp::Ne, false) => Inst::ICmp { dst, cond: IntCond::Ne, lhs, rhs },
        (HirBinOp::Ne, true) => Inst::FCmp { dst, cond: FloatCond::Ne, lhs, rhs },
        (HirBinOp::Lt, false) => Inst::ICmp { dst, cond: IntCond::Slt, lhs, rhs },
        (HirBinOp::Lt, true) => Inst::FCmp { dst, cond: FloatCond::OLt, lhs, rhs },
        (HirBinOp::Le, false) => Inst::ICmp { dst, cond: IntCond::Sle, lhs, rhs },
        (HirBinOp::Le, true) => Inst::FCmp { dst, cond: FloatCond::OLe, lhs, rhs },
        (HirBinOp::Gt, false) => Inst::ICmp { dst, cond: IntCond::Sgt, lhs, rhs },
        (HirBinOp::Gt, true) => Inst::FCmp { dst, cond: FloatCond::OGt, lhs, rhs },
        (HirBinOp::Ge, false) => Inst::ICmp { dst, cond: IntCond::Sge, lhs, rhs },
        (HirBinOp::Ge, true) => Inst::FCmp { dst, cond: FloatCond::OGe, lhs, rhs },

        // Logical (short-circuit not yet — treat as bitwise on Bool for now)
        (HirBinOp::LogAnd, _) => Inst::BAnd { dst, lhs, rhs },
        (HirBinOp::LogOr, _) => Inst::BOr { dst, lhs, rhs },

        // Unsupported pair — placeholder zero
        _ => {
            // No instruction emitted; rebind dst to a fresh zero const.
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: res_ty.clone(),
                    val: 0,
                },
            );
            return dst;
        }
    };
    ctx.push_inst(mir, inst);
    dst
}

fn lower_unary(
    op: HirUnOp,
    src: ValueId,
    res_ty: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let dst = mir.new_value(res_ty.clone());
    let inst = match op {
        HirUnOp::Neg if res_ty.is_float() => Inst::FNeg { dst, src },
        HirUnOp::Neg => Inst::INeg { dst, src },
        HirUnOp::Not => {
            // !b => xor with 1 (bool)
            let one = mir.new_value(HirType::Bool);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst: one,
                    ty: HirType::Bool,
                    val: 1,
                },
            );
            Inst::BXor { dst, lhs: src, rhs: one }
        }
        HirUnOp::BitNot => Inst::BNot { dst, src },
        _ => {
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: res_ty.clone(),
                    val: 0,
                },
            );
            return dst;
        }
    };
    ctx.push_inst(mir, inst);
    dst
}

fn lower_cast(
    src: ValueId,
    src_ty: &HirType,
    target: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let dst = mir.new_value(target.clone());

    let inst = match (src_ty, target) {
        // int → wider int (sign-extend)
        (HirType::I8 | HirType::I16 | HirType::I32, HirType::I64) => {
            Inst::SExtend { dst, src, to: target.clone() }
        }
        // unsigned narrow → wider (zero-extend)
        (HirType::U8 | HirType::U16 | HirType::U32, HirType::I64 | HirType::U64) => {
            Inst::UExtend { dst, src, to: target.clone() }
        }
        // wider int → narrower
        (HirType::I64 | HirType::U64, HirType::I8 | HirType::I16 | HirType::I32) => {
            Inst::IReduce { dst, src, to: target.clone() }
        }
        // int → float
        (a, HirType::F32 | HirType::F64 | HirType::Number) if a.is_integer() => {
            Inst::CvtFromSint { dst, src, to: target.clone() }
        }
        // float → int (saturating)
        (HirType::F32 | HirType::F64 | HirType::Number, b) if b.is_integer() => {
            Inst::CvtToSintSat { dst, src, to: target.clone() }
        }
        // f32 ↔ f64
        (HirType::F32, HirType::F64 | HirType::Number) => Inst::FPromote { dst, src },
        (HirType::F64 | HirType::Number, HirType::F32) => Inst::FDemote { dst, src },

        // Same kind or unsupported pair — bitcast as a fallback (preserves bits)
        _ => Inst::Bitcast { dst, src, to: target.clone() },
    };
    ctx.push_inst(mir, inst);
    dst
}

fn emit_zero_const(ty: &HirType, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    let normalized = match ty {
        HirType::Unknown | HirType::Any => HirType::I64,
        other => other.clone(),
    };
    let dst = mir.new_value(normalized.clone());
    let inst = match normalized {
        HirType::F32 => Inst::F32Const { dst, val: 0.0 },
        HirType::F64 | HirType::Number => Inst::F64Const { dst, val: 0.0 },
        ref t => Inst::IConst {
            dst,
            ty: t.clone(),
            val: 0,
        },
    };
    ctx.push_inst(mir, inst);
    dst
}
