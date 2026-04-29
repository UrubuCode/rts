//! Expression lowering to Cranelift IR.

mod basics;
mod calls;
mod members;
mod operators;

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::InstBuilder;
use swc_ecma_ast::{BinExpr, BinaryOp, Expr, Lit, MemberProp};

use super::ctx::{FnCtx, TypedVal, ValTy};
use super::func::class_setter_name;

use self::calls::{
    AccessorKind, emit_user_fn_addr, emit_virtual_accessor_dispatch, lower_call, lower_new,
    lower_super_prop_assign, lower_super_prop_read, resolve_setter_owner,
};
use self::members::{
    class_field_uses_flat, emit_flat_field_write, field_is_readonly_in_hierarchy, lhs_static_class,
    lower_array_lit, lower_member_expr, lower_object_lit, validate_private_scope,
    validate_visibility,
};
use self::operators::{lower_bin, lower_cond, lower_opt_chain, lower_update_expr, to_f64};

/// Compiles a SWC expression and returns a typed Cranelift value.
pub fn lower_expr(ctx: &mut FnCtx, expr: &Expr) -> Result<TypedVal> {
    match expr {
        Expr::Lit(lit) => basics::lower_lit(ctx, lit),
        Expr::Ident(id) => lower_ident_expr(ctx, id.sym.as_str()),
        Expr::Paren(p) => lower_expr(ctx, &p.expr),
        Expr::Unary(u) => basics::lower_unary(ctx, u),
        Expr::Update(u) => lower_update_expr(ctx, u),
        Expr::Bin(bin) => lower_bin(ctx, bin),
        Expr::Assign(assign) => lower_assign_expr(ctx, assign),
        Expr::Call(call) => lower_call(ctx, call),
        Expr::Tpl(tpl) => basics::lower_tpl(ctx, tpl),
        Expr::Cond(cond) => lower_cond(ctx, cond),
        Expr::Array(arr) => lower_array_lit(ctx, arr),
        Expr::Object(obj) => lower_object_lit(ctx, obj),
        Expr::Member(member) => lower_member_expr(ctx, member),
        Expr::OptChain(opt) => lower_opt_chain(ctx, opt),
        Expr::SuperProp(sp) => lower_super_prop_read(ctx, sp),
        Expr::New(new_expr) => lower_new(ctx, new_expr),
        Expr::This(_) => ctx
            .read_local("this")
            .ok_or_else(|| anyhow!("`this` unavailable in current context")),
        Expr::TsAs(a) => lower_expr(ctx, &a.expr),
        Expr::TsTypeAssertion(a) => lower_expr(ctx, &a.expr),
        Expr::TsConstAssertion(a) => lower_expr(ctx, &a.expr),
        Expr::TsSatisfies(a) => lower_expr(ctx, &a.expr),
        Expr::TsNonNull(n) => lower_expr(ctx, &n.expr),
        Expr::Await(a) => lower_expr(ctx, &a.arg),
        Expr::Seq(s) => {
            // Comma operator: avalia tudo pelo side-effect, retorna o ultimo.
            let mut last: Option<TypedVal> = None;
            for e in &s.exprs {
                last = Some(lower_expr(ctx, e)?);
            }
            last.ok_or_else(|| anyhow!("empty sequence expression"))
        }
        other => Err(anyhow!("unsupported expression: {}", expr_kind_name(other))),
    }
}

fn lower_ident_expr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    if let Some(tv) = ctx.read_local(name) {
        return Ok(tv);
    }
    if ctx.user_fns.contains_key(name) {
        return emit_user_fn_addr(ctx, name);
    }
    // (#298) Globais JS NaN/Infinity/undefined. NaN e Infinity sao
    // f64 IEEE; \`undefined\` em RTS nao tem representacao distinta de
    // 0/null entao mapeamos para 0 (caller que comparar com === detecta
    // tipo via context). Cobre uso comum em template/aritmetica.
    use cranelift_codegen::ir::{InstBuilder, types as cl};
    use crate::codegen::lower::ctx::ValTy;
    match name {
        "NaN" => {
            let v = ctx.builder.ins().f64const(f64::NAN);
            return Ok(TypedVal::new(v, ValTy::F64));
        }
        "Infinity" => {
            let v = ctx.builder.ins().f64const(f64::INFINITY);
            return Ok(TypedVal::new(v, ValTy::F64));
        }
        "undefined" => {
            // Usado em \`x === undefined\`, \`x ?? def\`, \`${undefined}\`.
            // Sentinel 0 cobre as 3 — strict check distingue tipo via
            // operador, e template literal converte i64 0 para "0".
            // Pra alinhar com JS \`${undefined}\` -> "undefined", emitimos
            // string handle "undefined" direto.
            return ctx.emit_str_handle(b"undefined");
        }
        _ => {}
    }
    Err(anyhow!("undefined variable `{name}`"))
}

fn lower_assign_expr(ctx: &mut FnCtx, a: &swc_ecma_ast::AssignExpr) -> Result<TypedVal> {
    use swc_ecma_ast::{AssignOp, AssignTarget};

    if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::SuperProp(sp)) = &a.left {
        return lower_super_prop_assign(ctx, sp, a);
    }

    if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) = &a.left {
        let final_rhs_expr: Box<Expr> = if matches!(a.op, AssignOp::Assign) {
            a.right.clone()
        } else {
            let binop = match a.op {
                AssignOp::AddAssign => BinaryOp::Add,
                AssignOp::SubAssign => BinaryOp::Sub,
                AssignOp::MulAssign => BinaryOp::Mul,
                AssignOp::DivAssign => BinaryOp::Div,
                AssignOp::ModAssign => BinaryOp::Mod,
                AssignOp::LShiftAssign => BinaryOp::LShift,
                AssignOp::RShiftAssign => BinaryOp::RShift,
                AssignOp::ZeroFillRShiftAssign => BinaryOp::ZeroFillRShift,
                AssignOp::BitOrAssign => BinaryOp::BitOr,
                AssignOp::BitXorAssign => BinaryOp::BitXor,
                AssignOp::BitAndAssign => BinaryOp::BitAnd,
                AssignOp::ExpAssign => BinaryOp::Exp,
                AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                    // `obj.x ||= y` → recursa como `obj.x = obj.x || y`.
                    // O lower_logical em lower_bin emite curto-circuito;
                    // depois cai no caminho normal de Member assign abaixo.
                    let logical_op = match a.op {
                        AssignOp::AndAssign => BinaryOp::LogicalAnd,
                        AssignOp::OrAssign => BinaryOp::LogicalOr,
                        AssignOp::NullishAssign => BinaryOp::NullishCoalescing,
                        _ => unreachable!(),
                    };
                    let read_lhs = Expr::Member(swc_ecma_ast::MemberExpr {
                        span: a.span,
                        obj: m.obj.clone(),
                        prop: m.prop.clone(),
                    });
                    let synthetic_right = Box::new(Expr::Bin(BinExpr {
                        span: a.span,
                        op: logical_op,
                        left: Box::new(read_lhs),
                        right: a.right.clone(),
                    }));
                    let synthetic_assign = swc_ecma_ast::AssignExpr {
                        span: a.span,
                        op: AssignOp::Assign,
                        left: a.left.clone(),
                        right: synthetic_right,
                    };
                    return lower_assign_expr(ctx, &synthetic_assign);
                }
                AssignOp::Assign => unreachable!(),
            };
            let read_lhs = Expr::Member(swc_ecma_ast::MemberExpr {
                span: a.span,
                obj: m.obj.clone(),
                prop: m.prop.clone(),
            });
            Box::new(Expr::Bin(BinExpr {
                span: a.span,
                op: binop,
                left: Box::new(read_lhs),
                right: a.right.clone(),
            }))
        };

        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                validate_visibility(ctx, &cls, prop_name)?;
                if field_is_readonly_in_hierarchy(ctx, &cls, prop_name)
                    && (!ctx.current_is_ctor || ctx.current_class.as_deref() != Some(&cls))
                {
                    return Err(anyhow!(
                        "readonly `{cls}.{prop_name}` so pode ser atribuido dentro do constructor de `{cls}`"
                    ));
                }
            }
        }

        let rhs = lower_expr(ctx, &final_rhs_expr)?;

        // Dual-path #147 passo 7: escrita tipada em campo flat. Preserva
        // o tipo do RHS para coercao no slot exato (i32/f64/i64/handle).
        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                if class_field_uses_flat(ctx, &cls, prop_name) {
                    // Setters dinamicos ja descartam o flat path em
                    // `class_field_uses_flat`, entao chegando aqui e seguro
                    // emitir store direto.
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    emit_flat_field_write(ctx, obj_h, &cls, prop_name, rhs)?;
                    return Ok(rhs);
                }
            }
        }

        let rhs_i64 = ctx.coerce_to_i64(rhs).val;

        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                if let Some(setter_owner) = resolve_setter_owner(ctx, &cls, prop_name) {
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    let setter_fn_name = class_setter_name(&setter_owner, prop_name);
                    let setter_abi = ctx
                        .user_fns
                        .get(&setter_fn_name)
                        .ok_or_else(|| anyhow!("setter `{setter_fn_name}` nao registrada"))?
                        .clone();
                    let param_ty = setter_abi.params.get(1).copied().unwrap_or(ValTy::I64);
                    let rhs_tv = TypedVal::new(rhs_i64, ValTy::I64);
                    let coerced = match param_ty {
                        ValTy::I32 => ctx.coerce_to_i32(rhs_tv).val,
                        ValTy::F64 => to_f64(ctx, rhs_tv),
                        _ => rhs_i64,
                    };
                    let cls_owned = cls.clone();
                    let prop_owned = prop_name.to_string();
                    emit_virtual_accessor_dispatch(
                        ctx,
                        &cls_owned,
                        &setter_owner,
                        AccessorKind::Setter,
                        &prop_owned,
                        obj_h,
                        &[coerced],
                    )?;
                    return Ok(TypedVal::new(rhs_i64, ValTy::I64));
                }
            }
        }

        let obj_tv = lower_expr(ctx, &m.obj)?;
        let obj_h = ctx.coerce_to_i64(obj_tv).val;
        let set_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            &[
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
            ],
            None,
        )?;
        match &m.prop {
            MemberProp::Ident(id) => {
                let (kp, kl) = ctx.emit_str_literal(id.sym.as_bytes())?;
                ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
            }
            MemberProp::Computed(c) => {
                if let Expr::Lit(Lit::Str(s)) = c.expr.as_ref() {
                    let (kp, kl) = ctx.emit_str_literal(s.value.as_bytes())?;
                    ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
                } else {
                    let idx_tv = lower_expr(ctx, &c.expr)?;
                    let idx = ctx.coerce_to_i64(idx_tv).val;
                    let vec_set = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_SET",
                        &[
                            cranelift_codegen::ir::types::I64,
                            cranelift_codegen::ir::types::I64,
                            cranelift_codegen::ir::types::I64,
                        ],
                        None,
                    )?;
                    ctx.builder.ins().call(vec_set, &[obj_h, idx, rhs_i64]);
                }
            }
            MemberProp::PrivateName(pn) => {
                let key = format!("#{}", pn.name.as_ref());
                validate_private_scope(ctx, &key)?;
                let (kp, kl) = ctx.emit_str_literal(key.as_bytes())?;
                ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
            }
        }
        return Ok(TypedVal::new(rhs_i64, ValTy::I64));
    }

    let name = match &a.left {
        AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) => {
            id.sym.as_str().to_string()
        }
        _ => return Err(anyhow!("only simple identifier assignment is supported")),
    };

    // Logical compound assignment: `x ||= y`, `x &&= y`, `x ??= y` —
    // semantica curto-circuito. Translado para `x = x op y` via Bin
    // logical, que ja avalia y so quando necessario.
    if matches!(a.op, AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign) {
        let logical_op = match a.op {
            AssignOp::AndAssign => BinaryOp::LogicalAnd,
            AssignOp::OrAssign => BinaryOp::LogicalOr,
            AssignOp::NullishAssign => BinaryOp::NullishCoalescing,
            _ => unreachable!(),
        };
        let synthetic_left = Expr::Ident(swc_ecma_ast::Ident {
            span: a.span,
            ctxt: Default::default(),
            sym: name.as_str().into(),
            optional: false,
        });
        let bin = BinExpr {
            span: a.span,
            op: logical_op,
            left: Box::new(synthetic_left),
            right: a.right.clone(),
        };
        let rhs_val = lower_bin(ctx, &bin)?;
        let coerced = match ctx.var_ty(&name) {
            Some(ValTy::I32) => ctx.coerce_to_i32(rhs_val),
            Some(ValTy::I64) => ctx.coerce_to_i64(rhs_val),
            Some(ValTy::Handle) => ctx.coerce_to_handle(rhs_val)?,
            _ => rhs_val,
        };
        ctx.write_local(&name, coerced.val)?;
        return Ok(coerced);
    }

    let rhs_val = if matches!(a.op, AssignOp::Assign) {
        lower_expr(ctx, &a.right)?
    } else {
        let binop = match a.op {
            AssignOp::AddAssign => BinaryOp::Add,
            AssignOp::SubAssign => BinaryOp::Sub,
            AssignOp::MulAssign => BinaryOp::Mul,
            AssignOp::DivAssign => BinaryOp::Div,
            AssignOp::ModAssign => BinaryOp::Mod,
            AssignOp::LShiftAssign => BinaryOp::LShift,
            AssignOp::RShiftAssign => BinaryOp::RShift,
            AssignOp::ZeroFillRShiftAssign => BinaryOp::ZeroFillRShift,
            AssignOp::BitOrAssign => BinaryOp::BitOr,
            AssignOp::BitXorAssign => BinaryOp::BitXor,
            AssignOp::BitAndAssign => BinaryOp::BitAnd,
            AssignOp::ExpAssign => BinaryOp::Exp,
            AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                unreachable!("logical compound handled above")
            }
            AssignOp::Assign => unreachable!(),
        };
        let synthetic_left = Expr::Ident(swc_ecma_ast::Ident {
            span: a.span,
            ctxt: Default::default(),
            sym: name.as_str().into(),
            optional: false,
        });
        let bin = BinExpr {
            span: a.span,
            op: binop,
            left: Box::new(synthetic_left),
            right: a.right.clone(),
        };
        lower_bin(ctx, &bin)?
    };

    let coerced = match ctx.var_ty(&name) {
        Some(ValTy::I32) => ctx.coerce_to_i32(rhs_val),
        Some(ValTy::I64) => ctx.coerce_to_i64(rhs_val),
        Some(ValTy::Handle) => ctx.coerce_to_handle(rhs_val)?,
        _ => rhs_val,
    };
    ctx.write_local(&name, coerced.val)?;
    Ok(coerced)
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
