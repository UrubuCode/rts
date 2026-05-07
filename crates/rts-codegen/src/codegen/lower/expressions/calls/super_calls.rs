//! Chamadas e acessos via `super` em metodos de classe:
//! - `super(args)` — invoca constructor da super class.
//! - `super.prop` / `super.prop = v` — bypass virtual dispatch.
//! - `super.method(args)` — chama metodo da super class diretamente.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{CallExpr, Expr};

use crate::codegen::lower::compile::class::{class_getter_name, class_setter_name};
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::members::{field_type_in_hierarchy, map_get_static_typed};
use super::super::operators::to_f64;
use super::class_dispatch::{
    emit_method_call, emit_named_method_call, resolve_getter_owner, resolve_init_owner,
    resolve_method_owner, resolve_setter_owner,
};

pub(super) fn lower_super_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super(...)` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super(...)` em classe sem extends"))?;

    // (#303) JS: \`Super constructor may only be called once\`. Detect
    // estatico via flag aqui ja' produzia falso positivo em branches
    // \`if/else\` mutualmente exclusivos. Flag agora rastreia mas nao
    // bloqueia automaticamente — caller de detect com scan AST especifico
    // deve invalidar o caso linear (super; super no mesmo block).
    // Nao implementado completamente; placeholder pra fase futura.
    ctx.super_already_called = true;

    let Some(init_owner) = resolve_init_owner(ctx, &parent) else {
        for a in &call.args {
            if a.spread.is_some() {
                return Err(anyhow!("spread em super(...) nao suportado"));
            }
            let _ = lower_expr(ctx, &a.expr)?;
        }
        return Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ));
    };

    let init_fn_name = format!("__class_{init_owner}__init");
    let abi = ctx
        .user_fns
        .get(&init_fn_name)
        .ok_or_else(|| anyhow!("super init de `{init_owner}` nao registrado"))?
        .clone();
    let mangled: String = format!("__user_{init_fn_name}");
    let fn_id = *ctx
        .extern_cache
        .get(mangled.as_str())
        .ok_or_else(|| anyhow!("super init mangled `{mangled}` faltando"))?;
    let fref = ctx.fref_for_id(fn_id);

    let this_val = ctx
        .read_local("this")
        .ok_or_else(|| anyhow!("`this` indisponivel em super(...)"))?;
    let mut args = vec![this_val.val];
    let expected = abi.params.len().saturating_sub(1);
    if call.args.len() != expected {
        return Err(anyhow!(
            "super(...) espera {} argumento(s), recebeu {}",
            expected,
            call.args.len()
        ));
    }
    for (a, expected_ty) in call.args.iter().zip(abi.params.iter().skip(1).copied()) {
        if a.spread.is_some() {
            return Err(anyhow!("spread em super(...) nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
            _ => ctx.coerce_to_i64(tv).val,
        };
        args.push(value);
    }
    ctx.builder.ins().call(fref, &args);
    Ok(TypedVal::new(
        ctx.builder.ins().iconst(cl::I64, 0),
        ValTy::I64,
    ))
}

pub(crate) fn lower_super_prop_read(
    ctx: &mut FnCtx,
    sp: &swc_ecma_ast::SuperPropExpr,
) -> Result<TypedVal> {
    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super.field` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super.field` em classe sem extends"))?;

    let prop_name = match &sp.prop {
        swc_ecma_ast::SuperProp::Ident(id) => id.sym.as_str().to_string(),
        swc_ecma_ast::SuperProp::Computed(_) => {
            return Err(anyhow!("computed em super[expr] nao suportado"));
        }
    };

    let this_val = ctx
        .read_local("this")
        .ok_or_else(|| anyhow!("`this` indisponivel em super.field"))?;
    let recv_i64 = ctx.coerce_to_i64(this_val).val;

    if let Some(getter_owner) = resolve_getter_owner(ctx, &parent, &prop_name) {
        let fn_name = class_getter_name(&getter_owner, &prop_name);
        return emit_named_method_call(ctx, &fn_name, recv_i64, &[]);
    }

    let field_ty = field_type_in_hierarchy(ctx, &parent, &prop_name);
    map_get_static_typed(ctx, recv_i64, prop_name.as_bytes(), field_ty)
}

pub(crate) fn lower_super_prop_assign(
    ctx: &mut FnCtx,
    sp: &swc_ecma_ast::SuperPropExpr,
    a: &swc_ecma_ast::AssignExpr,
) -> Result<TypedVal> {
    use swc_ecma_ast::AssignOp;

    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super.field = ...` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super.field = ...` em classe sem extends"))?;

    let prop_name = match &sp.prop {
        swc_ecma_ast::SuperProp::Ident(id) => id.sym.as_str().to_string(),
        swc_ecma_ast::SuperProp::Computed(_) => {
            return Err(anyhow!("computed em super[expr] = ... nao suportado"));
        }
    };

    let final_rhs_expr: Box<Expr> = if matches!(a.op, AssignOp::Assign) {
        a.right.clone()
    } else {
        let binop = match a.op {
            AssignOp::AddAssign => swc_ecma_ast::BinaryOp::Add,
            AssignOp::SubAssign => swc_ecma_ast::BinaryOp::Sub,
            AssignOp::MulAssign => swc_ecma_ast::BinaryOp::Mul,
            AssignOp::DivAssign => swc_ecma_ast::BinaryOp::Div,
            AssignOp::ModAssign => swc_ecma_ast::BinaryOp::Mod,
            AssignOp::LShiftAssign => swc_ecma_ast::BinaryOp::LShift,
            AssignOp::RShiftAssign => swc_ecma_ast::BinaryOp::RShift,
            AssignOp::ZeroFillRShiftAssign => swc_ecma_ast::BinaryOp::ZeroFillRShift,
            AssignOp::BitOrAssign => swc_ecma_ast::BinaryOp::BitOr,
            AssignOp::BitXorAssign => swc_ecma_ast::BinaryOp::BitXor,
            AssignOp::BitAndAssign => swc_ecma_ast::BinaryOp::BitAnd,
            AssignOp::ExpAssign => swc_ecma_ast::BinaryOp::Exp,
            AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                return Err(anyhow!("logical compound em super.field nao suportado"));
            }
            AssignOp::Assign => unreachable!(),
        };
        let read_lhs = Expr::SuperProp(sp.clone());
        Box::new(Expr::Bin(swc_ecma_ast::BinExpr {
            span: a.span,
            op: binop,
            left: Box::new(read_lhs),
            right: a.right.clone(),
        }))
    };

    let rhs = lower_expr(ctx, &final_rhs_expr)?;
    let rhs_i64 = ctx.coerce_to_i64(rhs).val;
    let this_val = ctx
        .read_local("this")
        .ok_or_else(|| anyhow!("`this` indisponivel em super.field assign"))?;
    let recv_i64 = ctx.coerce_to_i64(this_val).val;

    if let Some(setter_owner) = resolve_setter_owner(ctx, &parent, &prop_name) {
        let fn_name = class_setter_name(&setter_owner, &prop_name);
        let abi = ctx
            .user_fns
            .get(&fn_name)
            .ok_or_else(|| anyhow!("setter `{fn_name}` nao registrado"))?
            .clone();
        let param_ty = abi.params.get(1).copied().unwrap_or(ValTy::I64);
        let rhs_tv = TypedVal::new(rhs_i64, ValTy::I64);
        let coerced = match param_ty {
            ValTy::I32 => ctx.coerce_to_i32(rhs_tv).val,
            ValTy::F64 => to_f64(ctx, rhs_tv),
            _ => rhs_i64,
        };
        emit_named_method_call(ctx, &fn_name, recv_i64, &[coerced])?;
        return Ok(TypedVal::new(rhs_i64, ValTy::I64));
    }

    let set_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_SET",
        &[cl::I64, cl::I64, cl::I64, cl::I64],
        None,
    )?;
    let (kp, kl) = ctx.emit_str_literal(prop_name.as_bytes())?;
    ctx.builder.ins().call(set_fn, &[recv_i64, kp, kl, rhs_i64]);
    Ok(TypedVal::new(rhs_i64, ValTy::I64))
}

pub(super) fn lower_super_method_call(
    ctx: &mut FnCtx,
    sp: &swc_ecma_ast::SuperPropExpr,
    call: &CallExpr,
) -> Result<TypedVal> {
    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super.method()` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super.method()` em classe sem extends"))?;

    let method_name = match &sp.prop {
        swc_ecma_ast::SuperProp::Ident(id) => id.sym.as_str().to_string(),
        swc_ecma_ast::SuperProp::Computed(_) => {
            return Err(anyhow!("computed em super[expr]() nao suportado"));
        }
    };
    let owner = resolve_method_owner(ctx, &parent, &method_name).ok_or_else(|| {
        anyhow!("super.{method_name}() — metodo nao encontrado em ancestrais de `{class_name}`")
    })?;

    let this_val = ctx
        .read_local("this")
        .ok_or_else(|| anyhow!("`this` indisponivel em super.method()"))?;
    let recv_i64 = ctx.coerce_to_i64(this_val).val;

    let fn_name = format!("__class_{owner}_{method_name}");
    let abi = ctx
        .user_fns
        .get(&fn_name)
        .ok_or_else(|| anyhow!("metodo `{owner}.{method_name}` nao registrado"))?
        .clone();
    let expected = abi.params.len().saturating_sub(1);
    if call.args.len() != expected {
        return Err(anyhow!(
            "super.{method_name}() espera {} argumento(s), recebeu {}",
            expected,
            call.args.len()
        ));
    }
    let mut arg_values = Vec::with_capacity(expected);
    for (a, expected_ty) in call.args.iter().zip(abi.params.iter().skip(1).copied()) {
        if a.spread.is_some() {
            return Err(anyhow!("spread em super.method() nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
            _ => ctx.coerce_to_i64(tv).val,
        };
        arg_values.push(value);
    }
    emit_method_call(ctx, &owner, &method_name, recv_i64, &arg_values)
}
