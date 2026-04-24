//! Expression lowering to Cranelift IR.
//!
//! Entry point: `lower_expr` — recursively compiles a SWC expression into a
//! `TypedVal`. Handles literals, identifiers, binary ops, unary ops, and
//! namespace calls. String concatenation (`+` with a string operand) is
//! lowered to `__RTS_FN_NS_GC_STRING_CONCAT`.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{
    BinExpr, BinaryOp, CallExpr, Callee, Expr, Lit, MemberProp, Tpl, UnaryOp, UpdateOp,
};

use cranelift_module::Module;

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;

use super::ctx::{FnCtx, TypedVal, ValTy};

/// Compiles a SWC expression and returns a typed Cranelift value.
pub fn lower_expr(ctx: &mut FnCtx, expr: &Expr) -> Result<TypedVal> {
    match expr {
        // ── Literals ──────────────────────────────────────────────────────
        Expr::Lit(lit) => lower_lit(ctx, lit),

        // ── Identifiers ───────────────────────────────────────────────────
        Expr::Ident(id) => {
            let name = id.sym.as_str();
            ctx.read_local(name)
                .ok_or_else(|| anyhow!("undefined variable `{name}`"))
        }

        // ── Parenthesised ─────────────────────────────────────────────────
        Expr::Paren(p) => lower_expr(ctx, &p.expr),

        // ── Unary ─────────────────────────────────────────────────────────
        Expr::Unary(u) => lower_unary(ctx, u),

        // ── Update (++, --) ───────────────────────────────────────────────
        Expr::Update(u) => {
            let name = ident_name(&u.arg)
                .ok_or_else(|| anyhow!("update target must be a simple identifier"))?;
            let cur = ctx
                .read_local(name)
                .ok_or_else(|| anyhow!("undefined variable `{name}`"))?;
            let one = match cur.ty {
                ValTy::I32 => TypedVal::new(ctx.builder.ins().iconst(cl::I32, 1), ValTy::I32),
                _ => TypedVal::new(ctx.builder.ins().iconst(cl::I64, 1), ValTy::I64),
            };
            let new_val = match u.op {
                UpdateOp::PlusPlus => lower_add(ctx, cur, one)?,
                UpdateOp::MinusMinus => lower_sub(ctx, cur, one)?,
            };
            ctx.write_local(name, new_val.val)?;
            // Prefix: return new value; postfix: return old value.
            if u.prefix { Ok(new_val) } else { Ok(cur) }
        }

        // ── Binary ────────────────────────────────────────────────────────
        Expr::Bin(bin) => lower_bin(ctx, bin),

        // ── Assignment ────────────────────────────────────────────────────
        Expr::Assign(a) => {
            use swc_ecma_ast::AssignTarget;
            let name = match &a.left {
                AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) => {
                    id.sym.as_str().to_string()
                }
                _ => return Err(anyhow!("only simple identifier assignment is supported")),
            };
            let rhs = lower_expr(ctx, &a.right)?;
            // Coerce rhs to match the declared type of the local.
            let coerced = match ctx.var_ty(&name) {
                Some(ValTy::I32) => ctx.coerce_to_i32(rhs),
                Some(ValTy::I64) => ctx.coerce_to_i64(rhs),
                Some(ValTy::Handle) => ctx.coerce_to_handle(rhs)?,
                _ => rhs,
            };
            ctx.write_local(&name, coerced.val)?;
            Ok(coerced)
        }

        // ── Call ──────────────────────────────────────────────────────────
        Expr::Call(call) => lower_call(ctx, call),

        // ── Template literal ──────────────────────────────────────────────
        Expr::Tpl(tpl) => lower_tpl(ctx, tpl),

        // ── Ternary (a ? b : c) ───────────────────────────────────────────
        Expr::Cond(cond) => lower_cond(ctx, cond),

        // ── Member ────────────────────────────────────────────────────────
        // Resolves `ns.CONST` into a direct call to the constant's accessor
        // symbol. Regular function references like `io.print` (without a
        // call) are rejected.
        Expr::Member(_) => {
            let qualified = qualified_member_name(expr)
                .ok_or_else(|| anyhow!("bare member expression not supported as value"))?;
            let (_spec, member) = lookup(&qualified)
                .ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;
            if !matches!(member.kind, crate::abi::MemberKind::Constant) {
                return Err(anyhow!(
                    "`{qualified}` is a function, not a constant — use `{qualified}(...)`"
                ));
            }
            emit_constant_load(ctx, member)
        }

        other => Err(anyhow!(
            "unsupported expression kind: {}",
            expr_kind_name(other)
        )),
    }
}

// ── Literals ──────────────────────────────────────────────────────────────

fn lower_lit(ctx: &mut FnCtx, lit: &Lit) -> Result<TypedVal> {
    match lit {
        Lit::Num(n) => {
            let v = n.value;
            // If the source written form carries a decimal point or exponent,
            // treat the literal as f64 even when the value happens to be
            // integral. Without this, `1.0` would silently become i32 and
            // poison divisions like `1.0 / 5.0`.
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);

            if wrote_as_float || !v.is_finite() || v.fract() != 0.0 {
                Ok(TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64))
            } else if v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                // Default to I32 for integer literals that fit; codegen
                // coerces when the context demands I64.
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I32, v as i64),
                    ValTy::I32,
                ))
            } else {
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I64, v as i64),
                    ValTy::I64,
                ))
            }
        }
        Lit::Bool(b) => Ok(TypedVal::new(
            ctx.builder
                .ins()
                .iconst(cl::I64, if b.value { 1 } else { 0 }),
            ValTy::Bool,
        )),
        Lit::Str(s) => {
            let tv = ctx.emit_str_handle(s.value.as_bytes())?;
            Ok(tv)
        }
        Lit::Null(_) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        )),
        other => Err(anyhow!("unsupported literal: {other:?}")),
    }
}

// ── Unary ─────────────────────────────────────────────────────────────────

fn lower_unary(ctx: &mut FnCtx, u: &swc_ecma_ast::UnaryExpr) -> Result<TypedVal> {
    let operand = lower_expr(ctx, &u.arg)?;
    match u.op {
        UnaryOp::Minus => match operand.ty {
            ValTy::F64 => Ok(TypedVal::new(
                ctx.builder.ins().fneg(operand.val),
                ValTy::F64,
            )),
            ValTy::I32 => Ok(TypedVal::new(
                ctx.builder.ins().ineg(operand.val),
                ValTy::I32,
            )),
            _ => {
                let as_i64 = ctx.coerce_to_i64(operand);
                Ok(TypedVal::new(
                    ctx.builder.ins().ineg(as_i64.val),
                    ValTy::I64,
                ))
            }
        },
        UnaryOp::Bang => {
            let as_i64 = ctx.coerce_to_i64(operand);
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let cmp = ctx.builder.ins().icmp(IntCC::Equal, as_i64.val, zero);
            let ext = ctx.builder.ins().uextend(cl::I64, cmp);
            Ok(TypedVal::new(ext, ValTy::Bool))
        }
        UnaryOp::Plus => {
            // Numeric identity — coerce to i64 if needed
            Ok(ctx.coerce_to_i64(operand))
        }
        UnaryOp::Tilde => {
            let as_i64 = ctx.coerce_to_i64(operand);
            Ok(TypedVal::new(
                ctx.builder.ins().bnot(as_i64.val),
                ValTy::I64,
            ))
        }
        op => Err(anyhow!("unsupported unary op: {op:?}")),
    }
}

// ── Template literals ─────────────────────────────────────────────────────

/// Desugars a template literal into a chain of `gc::string_concat` calls.
///
/// `` `a${x}b${y}c` `` becomes `concat(concat(concat(concat("a", x), "b"), y), "c")`.
/// Each quasi cooked value is uploaded as a static string handle; each
/// interpolated expression is coerced to a handle via `coerce_to_handle`.
fn lower_tpl(ctx: &mut FnCtx, tpl: &Tpl) -> Result<TypedVal> {
    let cook = |e: &swc_ecma_ast::TplElement| -> Vec<u8> {
        if let Some(c) = &e.cooked {
            if let Some(s) = c.as_str() {
                return s.as_bytes().to_vec();
            }
        }
        e.raw.as_bytes().to_vec()
    };

    // Start from the first quasi (there is always at least one).
    let first = tpl
        .quasis
        .first()
        .ok_or_else(|| anyhow!("template literal has no quasis"))?;
    let mut acc = ctx.emit_str_handle(&cook(first))?;

    let fref = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_CONCAT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    for (i, expr) in tpl.exprs.iter().enumerate() {
        // Interpolated expression → handle
        let val = lower_expr(ctx, expr)?;
        let h = ctx.coerce_to_handle(val)?;
        let inst = ctx.builder.ins().call(fref, &[acc.val, h.val]);
        let v = ctx.builder.inst_results(inst)[0];
        acc = TypedVal::new(v, ValTy::Handle);

        // Trailing quasi after this expression
        let q = tpl
            .quasis
            .get(i + 1)
            .ok_or_else(|| anyhow!("malformed template: missing quasi after expression"))?;
        let bytes = cook(q);
        if !bytes.is_empty() {
            let qh = ctx.emit_str_handle(&bytes)?;
            let inst = ctx.builder.ins().call(fref, &[acc.val, qh.val]);
            let v = ctx.builder.inst_results(inst)[0];
            acc = TypedVal::new(v, ValTy::Handle);
        }
    }

    Ok(acc)
}

// ── Binary ────────────────────────────────────────────────────────────────

fn lower_bin(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    // Short-circuit logical ops
    if matches!(bin.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
        return lower_logical(ctx, bin);
    }

    let lhs = lower_expr(ctx, &bin.left)?;
    let rhs = lower_expr(ctx, &bin.right)?;

    // String concat: if either side is a Handle, use string concat
    if matches!(bin.op, BinaryOp::Add) && (lhs.ty == ValTy::Handle || rhs.ty == ValTy::Handle) {
        let lh = ctx.coerce_to_handle(lhs)?;
        let rh = ctx.coerce_to_handle(rhs)?;
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lh.val, rh.val]);
        let val = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(val, ValTy::Handle));
    }

    // Numeric: promote to common type
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs);

    match bin.op {
        BinaryOp::Add => lower_add(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Sub => lower_sub(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mul => lower_mul(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Div => lower_div(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mod => lower_mod(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),

        BinaryOp::EqEq | BinaryOp::EqEqEq => Ok(lower_icmp(
            ctx,
            IntCC::Equal,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Ok(lower_icmp(
            ctx,
            IntCC::NotEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::Lt => Ok(lower_icmp(
            ctx,
            if ty == ValTy::F64 {
                IntCC::SignedLessThan
            } else {
                IntCC::SignedLessThan
            },
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::LtEq => Ok(lower_icmp(
            ctx,
            IntCC::SignedLessThanOrEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::Gt => Ok(lower_icmp(
            ctx,
            IntCC::SignedGreaterThan,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::GtEq => Ok(lower_icmp(
            ctx,
            IntCC::SignedGreaterThanOrEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),

        // Bitwise — always operate on i64. JS spec truncates to i32 but the
        // rest of the codebase works in i64; matching existing conventions.
        BinaryOp::BitAnd => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().band(li, ri), ValTy::I64))
        }
        BinaryOp::BitOr => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().bor(li, ri), ValTy::I64))
        }
        BinaryOp::BitXor => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().bxor(li, ri), ValTy::I64))
        }
        BinaryOp::LShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().ishl(li, ri), ValTy::I64))
        }
        BinaryOp::RShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().sshr(li, ri), ValTy::I64))
        }
        BinaryOp::ZeroFillRShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().ushr(li, ri), ValTy::I64))
        }

        op => Err(anyhow!("unsupported binary op: {op:?}")),
    }
}

fn lower_cond(ctx: &mut FnCtx, cond: &swc_ecma_ast::CondExpr) -> Result<TypedVal> {
    // Evaluate test, branch into cons/alt, merge in i64 slot.
    let test = lower_expr(ctx, &cond.test)?;
    let test_i64 = ctx.coerce_to_i64(test);
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_truthy = ctx.builder.ins().icmp(IntCC::NotEqual, test_i64.val, zero);

    let cons_block = ctx.builder.create_block();
    let alt_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();
    let result_var = ctx.builder.declare_var(cl::I64);

    ctx.builder
        .ins()
        .brif(is_truthy, cons_block, &[], alt_block, &[]);

    ctx.builder.switch_to_block(cons_block);
    ctx.builder.seal_block(cons_block);
    let cons = lower_expr(ctx, &cond.cons)?;
    let cons_ty = cons.ty;
    let cons_i64 = ctx.coerce_to_i64(cons);
    ctx.builder.def_var(result_var, cons_i64.val);
    ctx.builder.ins().jump(merge_block, &[]);

    ctx.builder.switch_to_block(alt_block);
    ctx.builder.seal_block(alt_block);
    let alt = lower_expr(ctx, &cond.alt)?;
    let alt_ty = alt.ty;
    let alt_i64 = ctx.coerce_to_i64(alt);
    ctx.builder.def_var(result_var, alt_i64.val);
    ctx.builder.ins().jump(merge_block, &[]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    let result = ctx.builder.use_var(result_var);
    // Slot is i64; if both branches report Handle/Bool (also i64-backed in
    // Cranelift) we keep the tag so downstream code skips redundant work.
    let ty = match (cons_ty, alt_ty) {
        (ValTy::Handle, ValTy::Handle) => ValTy::Handle,
        (ValTy::Bool, ValTy::Bool) => ValTy::Bool,
        _ => ValTy::I64,
    };
    Ok(TypedVal::new(result, ty))
}

fn lower_logical(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    // &&: evaluate lhs; if falsy, result = lhs (0); else result = rhs
    // ||: evaluate lhs; if truthy, result = lhs; else result = rhs
    let result_var = ctx.builder.declare_var(cl::I64);

    let lhs = lower_expr(ctx, &bin.left)?;
    let lhs_i64 = ctx.coerce_to_i64(lhs);

    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_truthy = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64.val, zero);

    let true_block = ctx.builder.create_block();
    let false_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    ctx.builder
        .ins()
        .brif(is_truthy, true_block, &[], false_block, &[]);

    match bin.op {
        BinaryOp::LogicalAnd => {
            // true branch: evaluate rhs
            ctx.builder.switch_to_block(true_block);
            ctx.builder.seal_block(true_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs);
            ctx.builder.def_var(result_var, rhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);

            // false branch: short-circuit with 0
            ctx.builder.switch_to_block(false_block);
            ctx.builder.seal_block(false_block);
            ctx.builder.def_var(result_var, zero);
            ctx.builder.ins().jump(merge_block, &[]);
        }
        BinaryOp::LogicalOr => {
            // true branch: short-circuit with lhs value
            ctx.builder.switch_to_block(true_block);
            ctx.builder.seal_block(true_block);
            ctx.builder.def_var(result_var, lhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);

            // false branch: evaluate rhs
            ctx.builder.switch_to_block(false_block);
            ctx.builder.seal_block(false_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs);
            ctx.builder.def_var(result_var, rhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);
        }
        _ => unreachable!(),
    }

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    let val = ctx.builder.use_var(result_var);
    Ok(TypedVal::new(val, ValTy::Bool))
}

fn promote_numeric(
    ctx: &mut FnCtx,
    lhs: TypedVal,
    rhs: TypedVal,
) -> (
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    ValTy,
) {
    // If either side is f64, promote both
    if lhs.ty == ValTy::F64 || rhs.ty == ValTy::F64 {
        let lv = to_f64(ctx, lhs);
        let rv = to_f64(ctx, rhs);
        return (lv, rv, ValTy::F64);
    }

    // If either side is I64/Handle/Bool, widen both
    if lhs.ty == ValTy::I64
        || lhs.ty == ValTy::Handle
        || lhs.ty == ValTy::Bool
        || rhs.ty == ValTy::I64
        || rhs.ty == ValTy::Handle
        || rhs.ty == ValTy::Bool
    {
        let lv = ctx.coerce_to_i64(lhs).val;
        let rv = ctx.coerce_to_i64(rhs).val;
        return (lv, rv, ValTy::I64);
    }

    // Both I32: evaluate in I64 to avoid premature overflow in mixed
    // arithmetic chains like `(a * b + c) % m`. We truncate only when the
    // value is assigned/stored into an I32-typed slot.
    let lv = ctx.coerce_to_i64(lhs).val;
    let rv = ctx.coerce_to_i64(rhs).val;
    (lv, rv, ValTy::I64)
}

/// Coerces a raw Cranelift value (of type `ty` as seen by promote_numeric)
/// into an i64 suitable for bitwise ops. F64 is reinterpreted by converting
/// to signed integer (JS spec would ToInt32; we use i64 for consistency).
fn coerce_bits_i64(
    ctx: &mut FnCtx,
    val: cranelift_codegen::ir::Value,
    ty: ValTy,
) -> cranelift_codegen::ir::Value {
    if ty == ValTy::F64 {
        ctx.builder.ins().fcvt_to_sint_sat(cl::I64, val)
    } else {
        val
    }
}

fn to_f64(ctx: &mut FnCtx, tv: TypedVal) -> cranelift_codegen::ir::Value {
    match tv.ty {
        ValTy::F64 => tv.val,
        ValTy::I32 => ctx.builder.ins().fcvt_from_sint(cl::F64, tv.val),
        _ => {
            let as_i64 = ctx.coerce_to_i64(tv);
            ctx.builder.ins().fcvt_from_sint(cl::F64, as_i64.val)
        }
    }
}

fn lower_add(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fadd(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().iadd(lhs.val, rhs.val),
        _ => ctx.builder.ins().iadd(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_sub(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fsub(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().isub(lhs.val, rhs.val),
        _ => ctx.builder.ins().isub(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_mul(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fmul(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().imul(lhs.val, rhs.val),
        _ => ctx.builder.ins().imul(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_div(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fdiv(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().sdiv(lhs.val, rhs.val),
        _ => ctx.builder.ins().sdiv(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_mod(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => {
            // f64 remainder not directly in Cranelift; use libcall or manual
            // For now emit integer truncation path
            let li = ctx.builder.ins().fcvt_to_sint_sat(cl::I64, lhs.val);
            let ri = ctx.builder.ins().fcvt_to_sint_sat(cl::I64, rhs.val);
            let rem = ctx.builder.ins().srem(li, ri);
            return Ok(TypedVal::new(
                ctx.builder.ins().fcvt_from_sint(cl::F64, rem),
                ValTy::F64,
            ));
        }
        ValTy::I32 => ctx.builder.ins().srem(lhs.val, rhs.val),
        _ => ctx.builder.ins().srem(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_icmp(ctx: &mut FnCtx, cc: IntCC, lhs: TypedVal, rhs: TypedVal) -> TypedVal {
    let cmp = if lhs.ty == ValTy::F64 {
        use cranelift_codegen::ir::condcodes::FloatCC;
        let fcc = match cc {
            IntCC::Equal => FloatCC::Equal,
            IntCC::NotEqual => FloatCC::NotEqual,
            IntCC::SignedLessThan => FloatCC::LessThan,
            IntCC::SignedLessThanOrEqual => FloatCC::LessThanOrEqual,
            IntCC::SignedGreaterThan => FloatCC::GreaterThan,
            IntCC::SignedGreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
            _ => FloatCC::Equal,
        };
        ctx.builder.ins().fcmp(fcc, lhs.val, rhs.val)
    } else {
        ctx.builder.ins().icmp(cc, lhs.val, rhs.val)
    };
    let ext = ctx.builder.ins().uextend(cl::I64, cmp);
    TypedVal::new(ext, ValTy::Bool)
}

// ── Calls ─────────────────────────────────────────────────────────────────

fn lower_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    // Namespace call: `ns.fn(...)`
    if let Callee::Expr(callee) = &call.callee {
        if let Some(qualified) = qualified_member_name(callee) {
            return lower_ns_call(ctx, &qualified, call);
        }
        // User-defined function call: `fn_name(...)`
        if let Expr::Ident(id) = callee.as_ref() {
            return lower_user_call(ctx, id.sym.as_str(), call);
        }
    }
    Err(anyhow!("unsupported call expression form"))
}

/// Emits a zero-arg call to a constant's accessor symbol (e.g. `math.PI`).
///
/// Constants are backed by thin `extern "C"` functions declared via the ABI
/// so callers can read `math.PI` as an expression; LLVM/Cranelift is free
/// to inline the returned literal through normal import rules.
fn emit_constant_load(
    ctx: &mut FnCtx,
    member: &crate::abi::NamespaceMember,
) -> Result<TypedVal> {
    use cranelift_codegen::ir::{AbiParam, Signature};
    use cranelift_module::Linkage;

    let lowered = lower_member(member);
    let ret_cl = lowered
        .ret
        .ok_or_else(|| anyhow!("constant `{}` has no return type", member.name))?;

    // Declare import (idempotent via cache).
    let func_id = if let Some(id) = ctx.extern_cache.get(member.symbol).copied() {
        id
    } else {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        sig.returns.push(AbiParam::new(ret_cl));
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol, id);
        id
    };
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
    let inst = ctx.builder.ins().call(fref, &[]);
    let val = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(val, ValTy::from_abi(member.returns)))
}

/// Emits Cranelift IR inline for an intrinsic. Returns `Ok(None)` when the
/// intrinsic is not handled here (e.g. still pending implementation) so the
/// caller falls back to a regular extern call.
fn lower_intrinsic(
    ctx: &mut FnCtx,
    kind: crate::abi::Intrinsic,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::abi::Intrinsic;
    use cranelift_codegen::ir::condcodes::IntCC;

    // Helper: evaluate each argument and coerce to the requested scalar.
    fn arg_f64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(to_f64(ctx, tv))
    }
    fn arg_i64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    match kind {
        Intrinsic::Sqrt => {
            let x = arg_f64(ctx, call, 0)?;
            let v = ctx.builder.ins().sqrt(x);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::AbsF64 => {
            let x = arg_f64(ctx, call, 0)?;
            let v = ctx.builder.ins().fabs(x);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::MinF64 => {
            let a = arg_f64(ctx, call, 0)?;
            let b = arg_f64(ctx, call, 1)?;
            let v = ctx.builder.ins().fmin(a, b);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::MaxF64 => {
            let a = arg_f64(ctx, call, 0)?;
            let b = arg_f64(ctx, call, 1)?;
            let v = ctx.builder.ins().fmax(a, b);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::AbsI64 => {
            // `abs(x) = x >= 0 ? x : -x` via select; matches wrapping_abs for i64::MIN.
            let x = arg_i64(ctx, call, 0)?;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_neg = ctx.builder.ins().icmp(IntCC::SignedLessThan, x, zero);
            let neg = ctx.builder.ins().ineg(x);
            let v = ctx.builder.ins().select(is_neg, neg, x);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::MinI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let less = ctx.builder.ins().icmp(IntCC::SignedLessThan, a, b);
            let v = ctx.builder.ins().select(less, a, b);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::MaxI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let greater = ctx.builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            let v = ctx.builder.ins().select(greater, a, b);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::RandomF64 => {
            // Inline xorshift64:
            //   x = *state
            //   x ^= x << 13; x ^= x >> 7; x ^= x << 17
            //   *state = x
            //   f = ((x >> 11) as f64) / 2^53
            use cranelift_codegen::ir::MemFlags;
            use cranelift_module::{DataDescription, Linkage};

            const STATE_SYMBOL: &str = "__RTS_DATA_NS_MATH_RNG_STATE";
            // Declare the data as an import (idempotent). We never define
            // it here — the runtime staticlib provides the actual storage.
            let data_id = ctx
                .module
                .declare_data(STATE_SYMBOL, Linkage::Import, true, false)
                .map_err(|e| anyhow!("failed to declare {STATE_SYMBOL}: {e}"))?;
            // declare_data for an import is fine even if called multiple
            // times; Cranelift dedupes.
            let _ = DataDescription::new(); // keep type in scope, no-op

            let gv = ctx.module.declare_data_in_func(data_id, ctx.builder.func);
            let ptr_ty = ctx.module.isa().pointer_type();
            let ptr = ctx.builder.ins().global_value(ptr_ty, gv);

            let x0 = ctx.builder.ins().load(cl::I64, MemFlags::new(), ptr, 0);
            let s13 = ctx.builder.ins().ishl_imm(x0, 13);
            let x1 = ctx.builder.ins().bxor(x0, s13);
            let s7 = ctx.builder.ins().ushr_imm(x1, 7);
            let x2 = ctx.builder.ins().bxor(x1, s7);
            let s17 = ctx.builder.ins().ishl_imm(x2, 17);
            let x3 = ctx.builder.ins().bxor(x2, s17);
            ctx.builder.ins().store(MemFlags::new(), x3, ptr, 0);

            // Take top 53 bits and divide by 2^53 as f64.
            let bits = ctx.builder.ins().ushr_imm(x3, 11);
            let as_f = ctx.builder.ins().fcvt_from_uint(cl::F64, bits);
            let scale = ctx
                .builder
                .ins()
                .f64const(1.0f64 / ((1u64 << 53) as f64));
            let result = ctx.builder.ins().fmul(as_f, scale);
            Ok(Some(TypedVal::new(result, ValTy::F64)))
        }
    }
}

fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

    // If the member has an intrinsic, emit IR inline. Falls through to the
    // extern call only when the intrinsic is not recognised (keeps the
    // symbol alive so reflection/FFI consumers see the exported impl).
    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }

    let lowered = lower_member(member);

    // Declare the extern (idempotent via cache)
    let func_id = {
        if !ctx.extern_cache.contains_key(member.symbol) {
            use cranelift_codegen::ir::{AbiParam, Signature};
            use cranelift_module::Linkage;
            let mut sig = Signature::new(ctx.module.isa().default_call_conv());
            for &p in &lowered.params {
                sig.params.push(AbiParam::new(p));
            }
            if let Some(r) = lowered.ret {
                sig.returns.push(AbiParam::new(r));
            }
            let id = ctx
                .module
                .declare_function(member.symbol, Linkage::Import, &sig)
                .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
            ctx.extern_cache.insert(member.symbol, id);
        }
        *ctx.extern_cache.get(member.symbol).unwrap()
    };
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);

    // Build argument values
    let mut values = Vec::new();
    let mut arg_iter = call.args.iter();
    for &abi_ty in member.args {
        let arg = arg_iter
            .next()
            .ok_or_else(|| anyhow!("too few arguments for `{qualified}`"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in namespace calls"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                let tv = lower_expr(ctx, &arg.expr)?;
                match tv.ty {
                    ValTy::Handle => {
                        // Extract ptr+len from the handle
                        let ptr_fref =
                            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                        let len_fref =
                            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                        let pi = ctx.builder.ins().call(ptr_fref, &[tv.val]);
                        let ptr = ctx.builder.inst_results(pi)[0];
                        let li = ctx.builder.ins().call(len_fref, &[tv.val]);
                        let len = ctx.builder.inst_results(li)[0];
                        values.push(ptr);
                        values.push(len);
                    }
                    _ => {
                        // Literal string: get (ptr, len) from rodata
                        return Err(anyhow!("StrPtr argument must be a string value"));
                    }
                }
            }
            AbiType::I32 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val);
            }
            AbiType::I64 | AbiType::U64 | AbiType::Handle => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                let fv = to_f64(ctx, tv);
                values.push(fv);
            }
            AbiType::Bool => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
            AbiType::Void => {}
        }
    }

    let inst = ctx.builder.ins().call(fref, &values);
    let ret_val = if let Some(_ret_cl) = lowered.ret {
        let v = ctx.builder.inst_results(inst)[0];
        let ret_ty = ValTy::from_abi(member.returns);
        TypedVal::new(v, ret_ty)
    } else {
        TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64)
    };
    Ok(ret_val)
}

fn lower_user_call(ctx: &mut FnCtx, name: &str, call: &CallExpr) -> Result<TypedVal> {
    let abi = ctx
        .user_fns
        .get(name)
        .ok_or_else(|| anyhow!("call to undeclared user function `{name}`"))?
        .clone();

    let mangled: &'static str = Box::leak(format!("__user_{name}").into_boxed_str());
    if !ctx.extern_cache.contains_key(mangled) {
        return Err(anyhow!("call to undeclared user function `{name}`"));
    }
    let func_id = *ctx.extern_cache.get(mangled).unwrap();
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);

    if call.args.len() != abi.params.len() {
        return Err(anyhow!(
            "function `{name}` expects {} argument(s), got {}",
            abi.params.len(),
            call.args.len()
        ));
    }

    let mut values = Vec::new();
    for (arg, expected_ty) in call.args.iter().zip(abi.params.iter().copied()) {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::I64 | ValTy::Bool | ValTy::Handle => ctx.coerce_to_i64(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
        };
        values.push(value);
    }

    let inst = ctx.builder.ins().call(fref, &values);
    let results = ctx.builder.inst_results(inst);
    if let Some(ret_ty) = abi.ret {
        if let Some(&value) = results.first() {
            Ok(TypedVal::new(value, ret_ty))
        } else {
            Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ret_ty))
        }
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn qualified_member_name(expr: &Expr) -> Option<String> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(ns) = m.obj.as_ref() else {
        return None;
    };
    let fn_name = match &m.prop {
        MemberProp::Ident(id) => id.sym.as_str().to_string(),
        _ => return None,
    };
    Some(format!("{}.{}", ns.sym.as_str(), fn_name))
}

fn ident_name(expr: &Expr) -> Option<&str> {
    if let Expr::Ident(id) = expr {
        Some(id.sym.as_str())
    } else {
        None
    }
}

fn expr_kind_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Array(_) => "array",
        Expr::Arrow(_) => "arrow",
        Expr::Await(_) => "await",
        Expr::Bin(_) => "binary",
        Expr::Call(_) => "call",
        Expr::Class(_) => "class",
        Expr::Cond(_) => "ternary",
        Expr::Fn(_) => "function-expr",
        Expr::Ident(_) => "ident",
        Expr::Lit(_) => "literal",
        Expr::Member(_) => "member",
        Expr::MetaProp(_) => "meta-prop",
        Expr::New(_) => "new",
        Expr::Object(_) => "object",
        Expr::Paren(_) => "paren",
        Expr::Seq(_) => "sequence",
        Expr::TaggedTpl(_) => "tagged-template",
        Expr::This(_) => "this",
        Expr::Tpl(_) => "template",
        Expr::Unary(_) => "unary",
        Expr::Update(_) => "update",
        Expr::Yield(_) => "yield",
        _ => "unknown",
    }
}
