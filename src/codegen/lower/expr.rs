//! Expression lowering to Cranelift IR.
//!
//! Entry point: `lower_expr` — recursively compiles a SWC expression into a
//! `TypedVal`. Handles literals, identifiers, binary ops, unary ops, and
//! namespace calls. String concatenation (`+` with a string operand) is
//! lowered to `__RTS_FN_NS_GC_STRING_CONCAT`.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, MemFlags, condcodes::IntCC, types as cl};
use swc_ecma_ast::{
    BinExpr, BinaryOp, CallExpr, Callee, Expr, Lit, MemberExpr, MemberProp, NewExpr, UnaryOp,
    UpdateOp,
};

use cranelift_module::Module;

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;

use super::ctx::{ClassField, ClassInfo, FnCtx, TypedVal, ValTy};

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
            // `obj.field++` / `--` — load via pointer, compute, store back.
            if let Expr::Member(m) = u.arg.as_ref() {
                let target = resolve_member_target(ctx, m)?;
                let cur = load_field(ctx, &target)?;
                let one = match cur.ty {
                    ValTy::F64 => TypedVal::new(ctx.builder.ins().f64const(1.0), ValTy::F64),
                    ValTy::I32 => TypedVal::new(ctx.builder.ins().iconst(cl::I32, 1), ValTy::I32),
                    _ => TypedVal::new(ctx.builder.ins().iconst(cl::I64, 1), ValTy::I64),
                };
                let new_val = match u.op {
                    UpdateOp::PlusPlus => lower_add(ctx, cur, one)?,
                    UpdateOp::MinusMinus => lower_sub(ctx, cur, one)?,
                };
                store_field(ctx, &target, new_val)?;
                return if u.prefix { Ok(new_val) } else { Ok(cur) };
            }

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
        Expr::Assign(a) => lower_assign(ctx, a),

        // ── Call ──────────────────────────────────────────────────────────
        Expr::Call(call) => lower_call(ctx, call),

        // ── New (class instantiation) ─────────────────────────────────────
        Expr::New(n) => lower_new(ctx, n),

        // ── This ──────────────────────────────────────────────────────────
        Expr::This(_) => ctx
            .read_local("this")
            .ok_or_else(|| anyhow!("`this` is not available outside a class member")),

        // ── Member (`obj.field` read) ─────────────────────────────────────
        Expr::Member(m) => {
            let target = resolve_member_target(ctx, m)?;
            load_field(ctx, &target)
        }

        other => Err(anyhow!(
            "unsupported expression kind: {}",
            expr_kind_name(other)
        )),
    }
}

// ── Assignment ───────────────────────────────────────────────────────────

fn lower_assign(ctx: &mut FnCtx, a: &swc_ecma_ast::AssignExpr) -> Result<TypedVal> {
    use swc_ecma_ast::{AssignOp, AssignTarget, SimpleAssignTarget};

    match &a.left {
        AssignTarget::Simple(SimpleAssignTarget::Ident(id)) => {
            let name = id.sym.as_str().to_string();
            let rhs = lower_expr(ctx, &a.right)?;
            if a.op == AssignOp::Assign {
                let coerced = coerce_for_local(ctx, &name, rhs)?;
                ctx.write_local(&name, coerced.val)?;
                return Ok(coerced);
            }
            let cur = ctx
                .read_local(&name)
                .ok_or_else(|| anyhow!("assignment to undeclared variable `{name}`"))?;
            let new_val = apply_compound(ctx, a.op, cur, rhs)?;
            let coerced = coerce_for_local(ctx, &name, new_val)?;
            ctx.write_local(&name, coerced.val)?;
            Ok(coerced)
        }
        AssignTarget::Simple(SimpleAssignTarget::Member(m)) => {
            let target = resolve_member_target(ctx, m)?;
            let rhs = lower_expr(ctx, &a.right)?;
            if a.op == AssignOp::Assign {
                store_field(ctx, &target, rhs)?;
                return Ok(rhs);
            }
            let cur = load_field(ctx, &target)?;
            let new_val = apply_compound(ctx, a.op, cur, rhs)?;
            store_field(ctx, &target, new_val)?;
            Ok(new_val)
        }
        _ => Err(anyhow!(
            "only identifier or member assignment is supported"
        )),
    }
}

fn coerce_for_local(ctx: &mut FnCtx, name: &str, val: TypedVal) -> Result<TypedVal> {
    Ok(match ctx.var_ty(name) {
        Some(ValTy::I32) => ctx.coerce_to_i32(val),
        Some(ValTy::I64) => ctx.coerce_to_i64(val),
        Some(ValTy::Handle) => ctx.coerce_to_handle(val)?,
        _ => val,
    })
}

fn apply_compound(
    ctx: &mut FnCtx,
    op: swc_ecma_ast::AssignOp,
    cur: TypedVal,
    rhs: TypedVal,
) -> Result<TypedVal> {
    use swc_ecma_ast::AssignOp;
    let (lv, rv, ty) = promote_numeric(ctx, cur, rhs);
    let l = TypedVal::new(lv, ty);
    let r = TypedVal::new(rv, ty);
    match op {
        AssignOp::AddAssign => lower_add(ctx, l, r),
        AssignOp::SubAssign => lower_sub(ctx, l, r),
        AssignOp::MulAssign => lower_mul(ctx, l, r),
        AssignOp::DivAssign => lower_div(ctx, l, r),
        AssignOp::ModAssign => lower_mod(ctx, l, r),
        other => Err(anyhow!("unsupported compound assignment op: {other:?}")),
    }
}

// ── Class instantiation and field access ─────────────────────────────────

/// A resolved `obj.field` reference ready for load/store.
struct MemberTarget<'a> {
    /// The handle (Cranelift value) of the class instance.
    handle: TypedVal,
    field: &'a ClassField,
}

fn resolve_member_target<'a>(
    ctx: &mut FnCtx,
    m: &MemberExpr,
) -> Result<MemberTarget<'static>> {
    let field_name = match &m.prop {
        MemberProp::Ident(id) => id.sym.as_str(),
        _ => return Err(anyhow!("computed member access not supported")),
    };

    // Look up the class of the object expression.
    let class_name = match m.obj.as_ref() {
        Expr::Ident(id) => ctx
            .local_class_name(id.sym.as_str())
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "cannot resolve `{}.{}` — `{}` is not a class instance",
                    id.sym.as_str(),
                    field_name,
                    id.sym.as_str()
                )
            })?,
        Expr::This(_) => ctx
            .current_class
            .clone()
            .ok_or_else(|| anyhow!("`this.{}` used outside a class member", field_name))?,
        _ => {
            return Err(anyhow!(
                "member access target must be an identifier or `this`"
            ));
        }
    };

    let class = ctx
        .classes
        .get(&class_name)
        .ok_or_else(|| anyhow!("unknown class `{class_name}`"))?;
    // Clone a 'static-compatible ClassField by transferring the actual field
    // behind a Box — class layouts live for the entire compilation, so we
    // leak a cloned ClassField to borrow it for the returned MemberTarget.
    let field = class
        .field(field_name)
        .ok_or_else(|| anyhow!("class `{class_name}` has no field `{field_name}`"))?;
    let field_owned: &'static ClassField = Box::leak(Box::new(field.clone()));

    let handle = lower_expr(ctx, &m.obj)?;
    let handle = ctx.coerce_to_i64(handle);
    let handle = TypedVal::new(handle.val, ValTy::Handle);
    Ok(MemberTarget {
        handle,
        field: field_owned,
    })
}

fn object_ptr(ctx: &mut FnCtx, handle: TypedVal) -> Result<cranelift_codegen::ir::Value> {
    let fref = ctx.get_extern(
        "__RTS_FN_NS_GC_OBJECT_PTR",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(fref, &[handle.val]);
    Ok(ctx.builder.inst_results(inst)[0])
}

fn load_field(ctx: &mut FnCtx, target: &MemberTarget<'_>) -> Result<TypedVal> {
    let ptr = object_ptr(ctx, target.handle)?;
    let cl_ty = target.field.ty.cl_type();
    let val = ctx
        .builder
        .ins()
        .load(cl_ty, MemFlags::new(), ptr, target.field.offset);
    Ok(TypedVal::new(val, target.field.ty))
}

fn store_field(ctx: &mut FnCtx, target: &MemberTarget<'_>, val: TypedVal) -> Result<()> {
    let coerced = match target.field.ty {
        ValTy::I32 => ctx.coerce_to_i32(val),
        ValTy::I64 => ctx.coerce_to_i64(val),
        ValTy::Bool | ValTy::Handle => ctx.coerce_to_i64(val),
        ValTy::F64 => TypedVal::new(to_f64(ctx, val), ValTy::F64),
    };
    let ptr = object_ptr(ctx, target.handle)?;
    ctx.builder
        .ins()
        .store(MemFlags::new(), coerced.val, ptr, target.field.offset);
    Ok(())
}

fn lower_new(ctx: &mut FnCtx, n: &NewExpr) -> Result<TypedVal> {
    let class_name = match n.callee.as_ref() {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => return Err(anyhow!("`new` callee must be a class identifier")),
    };
    let class_info: ClassInfo = ctx
        .classes
        .get(&class_name)
        .ok_or_else(|| anyhow!("unknown class `{class_name}` in `new`"))?
        .clone();

    // Allocate the object buffer.
    let size = ctx
        .builder
        .ins()
        .iconst(cl::I64, class_info.size_bytes.max(8));
    let new_fref = ctx.get_extern(
        "__RTS_FN_NS_GC_OBJECT_NEW",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(new_fref, &[size]);
    let handle = ctx.builder.inst_results(inst)[0];

    // Invoke the constructor if one exists, passing `this` as arg #0.
    if let Some(ctor) = &class_info.ctor {
        let args = n.args.as_deref().unwrap_or(&[]);
        if args.len() != ctor.params.len() {
            return Err(anyhow!(
                "constructor of `{class_name}` expects {} argument(s), got {}",
                ctor.params.len(),
                args.len()
            ));
        }
        let mut values: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(args.len() + 1);
        values.push(handle);
        for (arg, &expected_ty) in args.iter().zip(ctor.params.iter()) {
            if arg.spread.is_some() {
                return Err(anyhow!("spread not supported"));
            }
            let tv = lower_expr(ctx, &arg.expr)?;
            let v = match expected_ty {
                ValTy::I32 => ctx.coerce_to_i32(tv).val,
                ValTy::I64 | ValTy::Bool | ValTy::Handle => ctx.coerce_to_i64(tv).val,
                ValTy::F64 => to_f64(ctx, tv),
            };
            values.push(v);
        }
        let ctor_fref = ctx.get_extern(
            ctor.symbol,
            &std::iter::once(cl::I64)
                .chain(ctor.params.iter().map(|t| t.cl_type()))
                .collect::<Vec<_>>(),
            None,
        )?;
        ctx.builder.ins().call(ctor_fref, &values);
    } else if !n.args.as_deref().unwrap_or(&[]).is_empty() {
        return Err(anyhow!(
            "class `{class_name}` has no constructor but was called with arguments"
        ));
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

// ── Literals ──────────────────────────────────────────────────────────────

fn lower_lit(ctx: &mut FnCtx, lit: &Lit) -> Result<TypedVal> {
    match lit {
        Lit::Num(n) => {
            let v = n.value;
            if v.fract() == 0.0 && v.is_finite() && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                // Default to I32 for integer literals that fit; codegen
                // coerces when the context demands I64.
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I32, v as i64),
                    ValTy::I32,
                ))
            } else if v.fract() == 0.0 && v.is_finite() {
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I64, v as i64),
                    ValTy::I64,
                ))
            } else {
                Ok(TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64))
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
        op => Err(anyhow!("unsupported unary op: {op:?}")),
    }
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

        op => Err(anyhow!("unsupported binary op: {op:?}")),
    }
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

/// Public re-export of the internal `to_f64` helper so other lowering
/// modules (e.g. `stmt::Return`) can coerce values to f64 uniformly.
pub fn coerce_to_f64_val(ctx: &mut FnCtx, tv: TypedVal) -> cranelift_codegen::ir::Value {
    to_f64(ctx, tv)
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
    if let Callee::Expr(callee) = &call.callee {
        // Method call `obj.method(...)` where `obj` is a class instance.
        if let Expr::Member(m) = callee.as_ref() {
            if let Some(tv) = lower_method_call(ctx, m, call)? {
                return Ok(tv);
            }
            // Fall through to namespace call resolution.
            if let Some(qualified) = qualified_member_name(callee) {
                return lower_ns_call(ctx, &qualified, call);
            }
            return Err(anyhow!("unsupported call expression form"));
        }
        // Namespace call: `ns.fn(...)`
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

/// Attempts to lower `obj.method(...)` as a class method call. Returns
/// `Ok(None)` if the callee is not a method on a known class instance,
/// letting the caller fall through to namespace call resolution.
fn lower_method_call(
    ctx: &mut FnCtx,
    m: &MemberExpr,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let method_name = match &m.prop {
        MemberProp::Ident(id) => id.sym.as_str().to_string(),
        _ => return Ok(None),
    };

    let class_name = match m.obj.as_ref() {
        Expr::Ident(id) => match ctx.local_class_name(id.sym.as_str()) {
            Some(n) => n.clone(),
            None => return Ok(None),
        },
        Expr::This(_) => match ctx.current_class.clone() {
            Some(n) => n,
            None => return Ok(None),
        },
        _ => return Ok(None),
    };

    let method = {
        let class = match ctx.classes.get(&class_name) {
            Some(c) => c,
            None => return Ok(None),
        };
        match class.methods.get(&method_name) {
            Some(m) => m.clone(),
            None => return Ok(None),
        }
    };

    if call.args.len() != method.params.len() {
        return Err(anyhow!(
            "method `{class_name}.{method_name}` expects {} argument(s), got {}",
            method.params.len(),
            call.args.len()
        ));
    }

    // Evaluate the receiver as a handle.
    let recv = lower_expr(ctx, &m.obj)?;
    let recv = ctx.coerce_to_i64(recv);

    let mut values: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(call.args.len() + 1);
    values.push(recv.val);
    for (arg, &expected_ty) in call.args.iter().zip(method.params.iter()) {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let v = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::I64 | ValTy::Bool | ValTy::Handle => ctx.coerce_to_i64(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
        };
        values.push(v);
    }

    let param_types: Vec<cranelift_codegen::ir::Type> = std::iter::once(cl::I64)
        .chain(method.params.iter().map(|t| t.cl_type()))
        .collect();
    let ret_cl = method.ret.map(|t| t.cl_type());
    let fref = ctx.get_extern(method.symbol, &param_types, ret_cl)?;
    let inst = ctx.builder.ins().call(fref, &values);

    let result = if let Some(ret_ty) = method.ret {
        let v = ctx.builder.inst_results(inst)[0];
        TypedVal::new(v, ret_ty)
    } else {
        TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64)
    };
    Ok(Some(result))
}

fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

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
                // Any value type gets coerced to a GC string handle; numbers
                // are auto-stringified via __RTS_FN_NS_GC_STRING_FROM_I64/F64.
                let handle = ctx.coerce_to_handle(tv)?;
                let ptr_fref =
                    ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                let len_fref =
                    ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let pi = ctx.builder.ins().call(ptr_fref, &[handle.val]);
                let ptr = ctx.builder.inst_results(pi)[0];
                let li = ctx.builder.ins().call(len_fref, &[handle.val]);
                let len = ctx.builder.inst_results(li)[0];
                values.push(ptr);
                values.push(len);
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
