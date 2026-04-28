use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{
    InstBuilder,
    condcodes::{FloatCC, IntCC},
    types as cl,
};
use swc_ecma_ast::{BinExpr, BinaryOp, CallExpr, Expr, Lit, UpdateOp};

use super::calls::lower_class_method_call_with_recv;
use super::lower_expr;
use super::members::lhs_static_class;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};

pub(super) fn lower_update_expr(ctx: &mut FnCtx, u: &swc_ecma_ast::UpdateExpr) -> Result<TypedVal> {
    let name =
        ident_name(&u.arg).ok_or_else(|| anyhow!("update target must be a simple identifier"))?;
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
    if u.prefix { Ok(new_val) } else { Ok(cur) }
}

fn as_int_literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(Lit::Num(n)) if n.value.fract() == 0.0 && n.value.is_finite() => {
            Some(n.value as i64)
        }
        Expr::Paren(p) => as_int_literal(&p.expr),
        _ => None,
    }
}

fn try_bin_imm(ctx: &mut FnCtx, bin: &BinExpr) -> Result<Option<TypedVal>> {
    let (var_side, imm) = match (as_int_literal(&bin.left), as_int_literal(&bin.right)) {
        (Some(imm), None) => (&bin.right, imm),
        (None, Some(imm)) => (&bin.left, imm),
        _ => return Ok(None),
    };

    let lhs = lower_expr(ctx, var_side)?;
    let imm_tv = if matches!(lhs.ty, ValTy::I32) {
        TypedVal::new(ctx.builder.ins().iconst(cl::I32, imm), ValTy::I32)
    } else {
        TypedVal::new(ctx.builder.ins().iconst(cl::I64, imm), ValTy::I64)
    };

    let result = match bin.op {
        BinaryOp::Add => lower_add(ctx, lhs, imm_tv)?,
        BinaryOp::Sub => lower_sub(ctx, lhs, imm_tv)?,
        BinaryOp::Mul => lower_mul(ctx, lhs, imm_tv)?,
        BinaryOp::Div => lower_div(ctx, lhs, imm_tv)?,
        BinaryOp::Mod => lower_mod(ctx, lhs, imm_tv)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn operator_method_name(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("add"),
        BinaryOp::Sub => Some("sub"),
        BinaryOp::Mul => Some("mul"),
        BinaryOp::Div => Some("div"),
        BinaryOp::Mod => Some("mod"),
        BinaryOp::EqEq | BinaryOp::EqEqEq => Some("eq"),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Some("ne"),
        BinaryOp::Lt => Some("lt"),
        BinaryOp::LtEq => Some("le"),
        BinaryOp::Gt => Some("gt"),
        BinaryOp::GtEq => Some("ge"),
        _ => None,
    }
}

fn try_operator_overload(ctx: &mut FnCtx, bin: &BinExpr) -> Result<Option<TypedVal>> {
    let method = match operator_method_name(bin.op) {
        Some(method) => method,
        None => return Ok(None),
    };
    let lhs_tv = lower_expr(ctx, &bin.left)?;
    let Some(class_name) = lhs_static_class(ctx, &bin.left) else {
        return Ok(None);
    };
    let recv_i64 = ctx.coerce_to_i64(lhs_tv).val;
    let synthetic_call = CallExpr {
        span: bin.span,
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: bin.span,
            ctxt: Default::default(),
            sym: method.into(),
            optional: false,
        }))),
        args: vec![swc_ecma_ast::ExprOrSpread {
            spread: None,
            expr: bin.right.clone(),
        }],
        type_args: None,
    };
    let result =
        lower_class_method_call_with_recv(ctx, &class_name, method, recv_i64, &synthetic_call)?;
    Ok(Some(result))
}

pub(super) fn lower_bin(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    if matches!(
        bin.op,
        BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing
    ) {
        return lower_logical(ctx, bin);
    }
    if let Some(tv) = try_operator_overload(ctx, bin)? {
        return Ok(tv);
    }
    if let Some(tv) = try_bin_imm(ctx, bin)? {
        return Ok(tv);
    }

    let lhs = lower_expr(ctx, &bin.left)?;
    let rhs = lower_expr(ctx, &bin.right)?;

    // Add precisa do tipo original (string concat detecta Handle).
    // Demais ops aritmeticos promovem internamente.
    if matches!(bin.op, BinaryOp::Add) {
        return lower_add(ctx, lhs, rhs);
    }

    // String equality (#130): quando ambos sao Handle, comparar por
    // conteudo via __RTS_FN_NS_GC_STRING_EQ. Sem isso `==` compararia
    // handles u64 (sempre distintos para interneds diferentes).
    if matches!(
        bin.op,
        BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
    ) && lhs.ty == ValTy::Handle
        && rhs.ty == ValTy::Handle
    {
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_EQ",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lhs.val, rhs.val]);
        let eq = ctx.builder.inst_results(inst)[0];
        let result = if matches!(bin.op, BinaryOp::NotEq | BinaryOp::NotEqEq) {
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            ctx.builder.ins().bxor(eq, one)
        } else {
            eq
        };
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;

    match bin.op {
        BinaryOp::Add => unreachable!(),
        BinaryOp::Sub => lower_sub(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mul => lower_mul(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Div => lower_div(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mod => lower_mod(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::EqEq | BinaryOp::EqEqEq => Ok(lower_icmp(ctx, IntCC::Equal, lhs, rhs)),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Ok(lower_icmp(ctx, IntCC::NotEqual, lhs, rhs)),
        BinaryOp::Lt => Ok(lower_icmp(ctx, IntCC::SignedLessThan, lhs, rhs)),
        BinaryOp::LtEq => Ok(lower_icmp(ctx, IntCC::SignedLessThanOrEqual, lhs, rhs)),
        BinaryOp::Gt => Ok(lower_icmp(ctx, IntCC::SignedGreaterThan, lhs, rhs)),
        BinaryOp::GtEq => Ok(lower_icmp(ctx, IntCC::SignedGreaterThanOrEqual, lhs, rhs)),
        BinaryOp::BitOr => Ok(TypedVal::new(ctx.builder.ins().bor(lv, rv), ty)),
        BinaryOp::BitXor => Ok(TypedVal::new(ctx.builder.ins().bxor(lv, rv), ty)),
        BinaryOp::BitAnd => Ok(TypedVal::new(ctx.builder.ins().band(lv, rv), ty)),
        BinaryOp::LShift => {
            Ok(TypedVal::new(ctx.builder.ins().ishl(lv, rv), ty))
        }
        BinaryOp::RShift => {
            Ok(TypedVal::new(ctx.builder.ins().sshr(lv, rv), ty))
        }
        BinaryOp::ZeroFillRShift => {
            Ok(TypedVal::new(ctx.builder.ins().ushr(lv, rv), ty))
        }
        BinaryOp::Exp => {
            let lf = to_f64(ctx, TypedVal::new(lv, ty));
            let rf = to_f64(ctx, TypedVal::new(rv, ty));
            let fref = ctx.get_extern("pow", &[cl::F64, cl::F64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(fref, &[lf, rf]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::F64))
        }
        other => Err(anyhow!("unsupported binary op: {other:?}")),
    }
}

pub(super) fn lower_opt_chain(
    ctx: &mut FnCtx,
    opt: &swc_ecma_ast::OptChainExpr,
) -> Result<TypedVal> {
    match opt.base.as_ref() {
        swc_ecma_ast::OptChainBase::Member(member) => {
            super::members::lower_member_expr(ctx, member)
        }
        swc_ecma_ast::OptChainBase::Call(call) => {
            // `callee?.(args)`: se callee for 0 (null), retorna 0 sem chamar.
            // Caso contrario, faz call_indirect via i64 funcptr.
            let callee_tv = lower_expr(ctx, &call.callee)?;
            let callee_i64 = ctx.coerce_to_i64(callee_tv).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, callee_i64, zero);

            let null_block = ctx.builder.create_block();
            let call_block = ctx.builder.create_block();
            let merge = ctx.builder.create_block();
            let result = ctx.builder.append_block_param(merge, cl::I64);

            ctx.builder
                .ins()
                .brif(is_null, null_block, &[], call_block, &[]);

            ctx.builder.switch_to_block(null_block);
            ctx.builder.seal_block(null_block);
            let z = ctx.builder.ins().iconst(cl::I64, 0);
            ctx.builder.ins().jump(merge, &[z.into()]);

            ctx.builder.switch_to_block(call_block);
            ctx.builder.seal_block(call_block);
            let synthetic = CallExpr {
                span: call.span,
                ctxt: call.ctxt,
                callee: swc_ecma_ast::Callee::Expr(call.callee.clone()),
                args: call.args.clone(),
                type_args: call.type_args.clone(),
            };
            let call_tv = super::calls::lower_call(ctx, &synthetic)?;
            let call_i64 = ctx.coerce_to_i64(call_tv).val;
            ctx.builder.ins().jump(merge, &[call_i64.into()]);

            ctx.builder.switch_to_block(merge);
            ctx.builder.seal_block(merge);
            Ok(TypedVal::new(result, ValTy::I64))
        }
    }
}

pub(super) fn lower_cond(ctx: &mut FnCtx, cond: &swc_ecma_ast::CondExpr) -> Result<TypedVal> {
    let test = lower_expr(ctx, &cond.test)?;
    let test_i64 = ctx.coerce_to_i64(test).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, test_i64, zero);

    let then_block = ctx.builder.create_block();
    let else_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    let result_ty = promote_result_ty(ctx, &cond.cons, &cond.alt)?;
    let result_param = ctx
        .builder
        .append_block_param(merge_block, result_ty.cl_type());

    ctx.builder
        .ins()
        .brif(is_true, then_block, &[], else_block, &[]);

    ctx.builder.switch_to_block(then_block);
    ctx.builder.seal_block(then_block);
    let cons = lower_expr(ctx, &cond.cons)?;
    let cons_val = coerce_result(ctx, cons, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[cons_val.into()]);

    ctx.builder.switch_to_block(else_block);
    ctx.builder.seal_block(else_block);
    let alt = lower_expr(ctx, &cond.alt)?;
    let alt_val = coerce_result(ctx, alt, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[alt_val.into()]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    Ok(TypedVal::new(result_param, result_ty))
}

fn lower_logical(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    let lhs = lower_expr(ctx, &bin.left)?;
    let lhs_i64 = ctx.coerce_to_i64(lhs).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let merge = ctx.builder.create_block();
    let result = ctx.builder.append_block_param(merge, cl::I64);

    match bin.op {
        BinaryOp::LogicalAnd => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_true, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::LogicalOr => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_true, merge, &[lhs_i64.into()], rhs_block, &[]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::NullishCoalescing => {
            let rhs_block = ctx.builder.create_block();
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_null, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        _ => unreachable!(),
    }

    ctx.builder.switch_to_block(merge);
    ctx.builder.seal_block(merge);
    Ok(TypedVal::new(result, ValTy::I64))
}

fn promote_numeric(
    ctx: &mut FnCtx,
    lhs: TypedVal,
    rhs: TypedVal,
) -> Result<(
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    ValTy,
)> {
    if matches!(lhs.ty, ValTy::F64) || matches!(rhs.ty, ValTy::F64) {
        return Ok((to_f64(ctx, lhs), to_f64(ctx, rhs), ValTy::F64));
    }
    if matches!(lhs.ty, ValTy::I32) && matches!(rhs.ty, ValTy::I32) {
        return Ok((lhs.val, rhs.val, ValTy::I32));
    }
    let result_ty = if matches!(lhs.ty, ValTy::U64) || matches!(rhs.ty, ValTy::U64) {
        ValTy::U64
    } else {
        ValTy::I64
    };
    Ok((
        ctx.coerce_to_i64(lhs).val,
        ctx.coerce_to_i64(rhs).val,
        result_ty,
    ))
}

fn promote_result_ty(ctx: &FnCtx, cons: &Expr, alt: &Expr) -> Result<ValTy> {
    let guess = |expr: &Expr| match expr {
        Expr::Lit(Lit::Num(n))
            if n.value.fract() == 0.0
                && n.value >= i32::MIN as f64
                && n.value <= i32::MAX as f64 =>
        {
            Some(ValTy::I32)
        }
        Expr::Lit(Lit::Num(_)) => Some(ValTy::F64),
        Expr::Lit(Lit::Str(_)) => Some(ValTy::Handle),
        Expr::Lit(Lit::Bool(_)) => Some(ValTy::Bool),
        Expr::Ident(id) => ctx.var_ty(id.sym.as_str()),
        _ => None,
    };
    Ok(match (guess(cons), guess(alt)) {
        (Some(ValTy::F64), _) | (_, Some(ValTy::F64)) => ValTy::F64,
        (Some(ValTy::Handle), _) | (_, Some(ValTy::Handle)) => ValTy::Handle,
        (Some(ValTy::I32), Some(ValTy::I32)) => ValTy::I32,
        _ => ValTy::I64,
    })
}

fn coerce_result(
    ctx: &mut FnCtx,
    value: TypedVal,
    target: ValTy,
) -> Result<cranelift_codegen::ir::Value> {
    Ok(match target {
        ValTy::I32 => ctx.coerce_to_i32(value).val,
        ValTy::F64 => to_f64(ctx, value),
        ValTy::Handle => ctx.coerce_to_handle(value)?.val,
        _ => ctx.coerce_to_i64(value).val,
    })
}


pub(super) fn to_f64(ctx: &mut FnCtx, tv: TypedVal) -> cranelift_codegen::ir::Value {
    match tv.ty {
        ValTy::F64 => tv.val,
        ValTy::I32 => ctx.builder.ins().fcvt_from_sint(cl::F64, tv.val),
        _ => {
            let value = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().fcvt_from_sint(cl::F64, value)
        }
    }
}

pub(super) fn lower_add(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    if matches!(lhs.ty, ValTy::Handle) || matches!(rhs.ty, ValTy::Handle) {
        let concat = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let lhs_h = ctx.coerce_to_handle(lhs)?.val;
        let rhs_h = ctx.coerce_to_handle(rhs)?.val;
        let inst = ctx.builder.ins().call(concat, &[lhs_h, rhs_h]);
        return Ok(TypedVal::new(
            ctx.builder.inst_results(inst)[0],
            ValTy::Handle,
        ));
    }
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fadd(lv, rv),
        ValTy::I32 => ctx.builder.ins().iadd(lv, rv),
        _ => ctx.builder.ins().iadd(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

pub(super) fn lower_sub(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fsub(lv, rv),
        _ => ctx.builder.ins().isub(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_mul(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fmul(lv, rv),
        _ => ctx.builder.ins().imul(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_div(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fdiv(lv, rv),
        _ => ctx.builder.ins().sdiv(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_mod(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => {
            let div = ctx.builder.ins().fdiv(lv, rv);
            let trunc = ctx.builder.ins().trunc(div);
            let mul = ctx.builder.ins().fmul(trunc, rv);
            ctx.builder.ins().fsub(lv, mul)
        }
        _ => ctx.builder.ins().srem(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_icmp(ctx: &mut FnCtx, cc: IntCC, lhs: TypedVal, rhs: TypedVal) -> TypedVal {
    let cmp = if matches!(lhs.ty, ValTy::F64) || matches!(rhs.ty, ValTy::F64) {
        let lhs = to_f64(ctx, lhs);
        let rhs = to_f64(ctx, rhs);
        let fcc = match cc {
            IntCC::Equal => FloatCC::Equal,
            IntCC::NotEqual => FloatCC::NotEqual,
            IntCC::SignedLessThan => FloatCC::LessThan,
            IntCC::SignedLessThanOrEqual => FloatCC::LessThanOrEqual,
            IntCC::SignedGreaterThan => FloatCC::GreaterThan,
            IntCC::SignedGreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
            _ => FloatCC::Equal,
        };
        ctx.builder.ins().fcmp(fcc, lhs, rhs)
    } else {
        let lhs = ctx.coerce_to_i64(lhs).val;
        let rhs = ctx.coerce_to_i64(rhs).val;
        ctx.builder.ins().icmp(cc, lhs, rhs)
    };
    // Mantem cmp como i8 (Bool nativo Cranelift). Quando precisar i64
    // (ex: \`const flag = a < b\`), coerce_to_i64(Bool) faz uextend
    // explicito. Em brif (loop/if), to_branch_cond passa direto sem
    // re-extender — elimina \`uextend + iconst 0 + icmp ne\` que era
    // emitido em todos os hot loops.
    TypedVal::new(cmp, ValTy::Bool)
}

fn ident_name(expr: &Expr) -> Option<&str> {
    if let Expr::Ident(id) = expr {
        Some(id.sym.as_str())
    } else {
        None
    }
}
