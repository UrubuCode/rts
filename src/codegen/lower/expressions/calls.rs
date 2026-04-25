use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{CallExpr, Callee, Expr, MemberProp};

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;

use super::lower_expr;
use super::members::{
    field_type_in_hierarchy, lhs_static_class, map_get_static_typed, qualified_member_name,
    validate_visibility,
};
use super::operators::to_f64;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::func::{class_getter_name, class_setter_name, class_static_method_name};

pub(super) fn lower_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    if matches!(&call.callee, Callee::Super(_)) {
        return lower_super_call(ctx, call);
    }
    if let Callee::Expr(callee) = &call.callee {
        if let Expr::SuperProp(sp) = callee.as_ref() {
            return lower_super_method_call(ctx, sp, call);
        }
        if let Expr::Member(m) = callee.as_ref() {
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let cn = obj_id.sym.as_str();
                if let Some(meta) = ctx.classes.get(cn) {
                    if let MemberProp::Ident(method_id) = &m.prop {
                        let mn = method_id.sym.as_str();
                        if meta.static_methods.iter().any(|m| m == mn) {
                            let fn_name = class_static_method_name(cn, mn);
                            return lower_user_call(ctx, &fn_name, call);
                        }
                    }
                }
            }
            if let Some(qualified) = qualified_member_name(callee) {
                if lookup(&qualified).is_some() {
                    return lower_ns_call(ctx, &qualified, call);
                }
            }
            if let MemberProp::Ident(method_id) = &m.prop {
                if let Some(class_name) = lhs_static_class(ctx, &m.obj) {
                    let method_name = method_id.sym.as_str();
                    if resolve_method_owner(ctx, &class_name, method_name).is_some() {
                        let recv_tv = lower_expr(ctx, &m.obj)?;
                        let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
                        return lower_class_method_call_with_recv(
                            ctx,
                            &class_name,
                            method_name,
                            recv_i64,
                            call,
                        );
                    }
                }
            }
        }
        if let Some(qualified) = qualified_member_name(callee) {
            return lower_ns_call(ctx, &qualified, call);
        }
        if let Expr::Ident(id) = callee.as_ref() {
            let name = id.sym.as_str();
            if ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none() {
                return lower_user_call(ctx, name, call);
            }
            if ctx.var_ty(name).is_some() {
                return lower_indirect_call(ctx, callee, call);
            }
            return lower_user_call(ctx, name, call);
        }
    }
    Err(anyhow!("unsupported call expression form"))
}

fn resolve_method_owner(ctx: &FnCtx, class: &str, method: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.methods.iter().any(|m| m == method) {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

fn resolve_init_owner(ctx: &FnCtx, class: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.has_constructor {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

fn is_subclass_of(ctx: &FnCtx, child: &str, ancestor: &str) -> bool {
    let mut cur = child.to_string();
    loop {
        if cur == ancestor {
            return true;
        }
        let Some(meta) = ctx.classes.get(&cur) else {
            return false;
        };
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return false,
        }
    }
}

fn resolve_getter_owner(ctx: &FnCtx, class: &str, prop: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.getters.iter().any(|g| g == prop) {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

pub(super) fn resolve_setter_owner(ctx: &FnCtx, class: &str, prop: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.setters.iter().any(|s| s == prop) {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum AccessorKind {
    Getter,
    Setter,
}

fn accessor_mangled(kind: AccessorKind, owner: &str, prop: &str) -> String {
    match kind {
        AccessorKind::Getter => class_getter_name(owner, prop),
        AccessorKind::Setter => class_setter_name(owner, prop),
    }
}

fn class_has_accessor(
    meta: &crate::codegen::lower::ctx::ClassMeta,
    kind: AccessorKind,
    prop: &str,
) -> bool {
    match kind {
        AccessorKind::Getter => meta.getters.iter().any(|g| g == prop),
        AccessorKind::Setter => meta.setters.iter().any(|s| s == prop),
    }
}

fn resolve_accessor_owner(
    ctx: &FnCtx,
    kind: AccessorKind,
    class: &str,
    prop: &str,
) -> Option<String> {
    match kind {
        AccessorKind::Getter => resolve_getter_owner(ctx, class, prop),
        AccessorKind::Setter => resolve_setter_owner(ctx, class, prop),
    }
}

pub(super) fn emit_virtual_accessor_dispatch(
    ctx: &mut FnCtx,
    static_class: &str,
    static_owner: &str,
    kind: AccessorKind,
    prop: &str,
    recv_i64: cranelift_codegen::ir::Value,
    arg_values: &[cranelift_codegen::ir::Value],
) -> Result<TypedVal> {
    let mut overrides: Vec<(String, String)> = Vec::new();
    for (cname, _meta) in ctx.classes.iter() {
        if !is_subclass_of(ctx, cname, static_class) {
            continue;
        }
        if let Some(owner) = resolve_accessor_owner(ctx, kind, cname, prop) {
            overrides.push((cname.clone(), owner));
        }
    }
    let mut distinct: Vec<String> = Vec::new();
    for (_c, o) in &overrides {
        if !distinct.contains(o) {
            distinct.push(o.clone());
        }
    }
    if !distinct.contains(&static_owner.to_string()) {
        distinct.insert(0, static_owner.to_string());
    }
    if distinct.len() == 1 {
        return emit_named_method_call(
            ctx,
            &accessor_mangled(kind, static_owner, prop),
            recv_i64,
            arg_values,
        );
    }

    let static_fn_name = accessor_mangled(kind, static_owner, prop);
    let ret_ty = ctx
        .user_fns
        .get(&static_fn_name)
        .and_then(|abi| abi.ret)
        .unwrap_or(ValTy::I64);

    let (key_ptr, key_len) = ctx.emit_str_literal(b"__rts_class")?;
    let map_get = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx
        .builder
        .ins()
        .call(map_get, &[recv_i64, key_ptr, key_len]);
    let class_handle = ctx.builder.inst_results(inst)[0];

    let mut ordered: Vec<(String, String)> = overrides
        .iter()
        .filter(|(c, _)| {
            ctx.classes
                .get(c)
                .map(|m| class_has_accessor(m, kind, prop))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    ordered.sort_by_key(|(c, _)| {
        let mut depth = 0;
        let mut cur = c.clone();
        while let Some(meta) = ctx.classes.get(&cur) {
            match &meta.super_class {
                Some(p) => {
                    depth += 1;
                    cur = p.clone();
                }
                None => break,
            }
        }
        std::cmp::Reverse(depth)
    });

    let merge_block = ctx.builder.create_block();
    let result_param = ctx
        .builder
        .append_block_param(merge_block, ret_ty.cl_type());
    let str_eq = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_EQ",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    for (cname, owner) in &ordered {
        let (cn_ptr, cn_len) = ctx.emit_str_literal(cname.as_bytes())?;
        let from_static = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(from_static, &[cn_ptr, cn_len]);
        let target_handle = ctx.builder.inst_results(inst)[0];
        let inst = ctx
            .builder
            .ins()
            .call(str_eq, &[class_handle, target_handle]);
        let cmp = ctx.builder.inst_results(inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let is_eq =
            ctx.builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, cmp, zero);

        let then_block = ctx.builder.create_block();
        let else_block = ctx.builder.create_block();
        ctx.builder
            .ins()
            .brif(is_eq, then_block, &[], else_block, &[]);
        ctx.builder.switch_to_block(then_block);
        ctx.builder.seal_block(then_block);
        let result = emit_named_method_call(
            ctx,
            &accessor_mangled(kind, owner, prop),
            recv_i64,
            arg_values,
        )?;
        let coerced = match ret_ty {
            ValTy::I32 => ctx.coerce_to_i32(result).val,
            ValTy::F64 => to_f64(ctx, result),
            _ => ctx.coerce_to_i64(result).val,
        };
        ctx.builder.ins().jump(merge_block, &[coerced.into()]);
        ctx.builder.switch_to_block(else_block);
        ctx.builder.seal_block(else_block);
    }

    let result = emit_named_method_call(
        ctx,
        &accessor_mangled(kind, static_owner, prop),
        recv_i64,
        arg_values,
    )?;
    let coerced = match ret_ty {
        ValTy::I32 => ctx.coerce_to_i32(result).val,
        ValTy::F64 => to_f64(ctx, result),
        _ => ctx.coerce_to_i64(result).val,
    };
    ctx.builder.ins().jump(merge_block, &[coerced.into()]);
    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    Ok(TypedVal::new(result_param, ret_ty))
}

fn emit_named_method_call(
    ctx: &mut FnCtx,
    fn_name: &str,
    recv_i64: cranelift_codegen::ir::Value,
    arg_values: &[cranelift_codegen::ir::Value],
) -> Result<TypedVal> {
    let abi = ctx
        .user_fns
        .get(fn_name)
        .ok_or_else(|| anyhow!("user fn `{fn_name}` nao registrada"))?
        .clone();
    let mangled: &'static str = Box::leak(format!("__user_{fn_name}").into_boxed_str());
    let fn_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("mangled `{mangled}` nao registrado"))?;
    let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

    let mut args = Vec::with_capacity(arg_values.len() + 1);
    args.push(recv_i64);
    args.extend_from_slice(arg_values);
    let inst = ctx.builder.ins().call(fref, &args);
    let results = ctx.builder.inst_results(inst);
    if let Some(&v) = results.first() {
        Ok(TypedVal::new(v, abi.ret.unwrap_or(ValTy::I64)))
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
}

fn collect_method_overrides(ctx: &FnCtx, base: &str, method: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (cname, _meta) in ctx.classes.iter() {
        if !is_subclass_of(ctx, cname, base) {
            continue;
        }
        if let Some(owner) = resolve_method_owner(ctx, cname, method) {
            out.push((cname.clone(), owner));
        }
    }
    out
}

pub(super) fn lower_new(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
    let class_name = match new_expr.callee.as_ref() {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => {
            return Err(anyhow!(
                "`new` so suporta callee identifier (sem `new (expr)()`)"
            ));
        }
    };
    let meta = ctx
        .classes
        .get(&class_name)
        .ok_or_else(|| anyhow!("classe `{class_name}` nao declarada"))?
        .clone();
    if meta.is_abstract {
        return Err(anyhow!(
            "classe abstract `{class_name}` nao pode ser instanciada via `new`"
        ));
    }

    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    let (class_ptr, class_len) = ctx.emit_str_literal(class_name.as_bytes())?;
    let from_static = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(from_static, &[class_ptr, class_len]);
    let class_str_handle = ctx.builder.inst_results(inst)[0];
    let (key_ptr, key_len) = ctx.emit_str_literal(b"__rts_class")?;
    let map_set = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_SET",
        &[cl::I64, cl::I64, cl::I64, cl::I64],
        None,
    )?;
    ctx.builder
        .ins()
        .call(map_set, &[handle, key_ptr, key_len, class_str_handle]);

    if let Some(init_owner) = resolve_init_owner(ctx, &class_name) {
        let init_fn_name = format!("__class_{init_owner}__init");
        let abi = ctx
            .user_fns
            .get(&init_fn_name)
            .ok_or_else(|| anyhow!("init de classe `{init_owner}` nao registrado"))?
            .clone();
        let mangled: &'static str = Box::leak(format!("__user_{init_fn_name}").into_boxed_str());
        let fn_id = *ctx
            .extern_cache
            .get(mangled)
            .ok_or_else(|| anyhow!("init mangled `{mangled}` faltando"))?;
        let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

        let user_args: &[swc_ecma_ast::ExprOrSpread] =
            new_expr.args.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
        let expected = abi.params.len().saturating_sub(1);
        if user_args.len() != expected {
            return Err(anyhow!(
                "constructor de `{class_name}` espera {} argumento(s), recebeu {}",
                expected,
                user_args.len()
            ));
        }
        let mut args = vec![handle];
        for (a, expected_ty) in user_args.iter().zip(abi.params.iter().skip(1).copied()) {
            if a.spread.is_some() {
                return Err(anyhow!("spread em `new` nao suportado"));
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
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

fn lower_super_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super(...)` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super(...)` em classe sem extends"))?;

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
    let mangled: &'static str = Box::leak(format!("__user_{init_fn_name}").into_boxed_str());
    let fn_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("super init mangled `{mangled}` faltando"))?;
    let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

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

pub(super) fn lower_super_prop_read(
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

pub(super) fn lower_super_prop_assign(
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

fn lower_super_method_call(
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

pub(super) fn lower_class_method_call_with_recv(
    ctx: &mut FnCtx,
    class_name: &str,
    method_name: &str,
    recv_i64: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<TypedVal> {
    validate_visibility(ctx, class_name, method_name)?;

    let static_owner = resolve_method_owner(ctx, class_name, method_name).ok_or_else(|| {
        anyhow!("metodo `{method_name}` nao encontrado em `{class_name}` ou ancestrais")
    })?;

    let overrides = collect_method_overrides(ctx, class_name, method_name);
    let mut distinct_owners = Vec::new();
    for (_c, o) in &overrides {
        if !distinct_owners.contains(o) {
            distinct_owners.push(o.clone());
        }
    }
    if !distinct_owners.contains(&static_owner) {
        distinct_owners.insert(0, static_owner.clone());
    }

    let abi_static = ctx
        .user_fns
        .get(&format!("__class_{static_owner}_{method_name}"))
        .ok_or_else(|| anyhow!("metodo estatico `{static_owner}.{method_name}` nao registrado"))?
        .clone();
    let expected = abi_static.params.len().saturating_sub(1);
    if call.args.len() != expected {
        return Err(anyhow!(
            "metodo `{static_owner}.{method_name}` espera {} argumento(s), recebeu {}",
            expected,
            call.args.len()
        ));
    }
    let mut arg_values = Vec::with_capacity(expected);
    for (a, expected_ty) in call
        .args
        .iter()
        .zip(abi_static.params.iter().skip(1).copied())
    {
        if a.spread.is_some() {
            return Err(anyhow!("spread em chamada de metodo nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
            _ => ctx.coerce_to_i64(tv).val,
        };
        arg_values.push(value);
    }

    if distinct_owners.len() == 1 {
        return emit_method_call(ctx, &static_owner, method_name, recv_i64, &arg_values);
    }

    emit_virtual_dispatch(
        ctx,
        class_name,
        method_name,
        &static_owner,
        recv_i64,
        &arg_values,
        &overrides,
    )
}

fn emit_method_call(
    ctx: &mut FnCtx,
    owner: &str,
    method_name: &str,
    recv_i64: cranelift_codegen::ir::Value,
    arg_values: &[cranelift_codegen::ir::Value],
) -> Result<TypedVal> {
    let fn_name = format!("__class_{owner}_{method_name}");
    let abi = ctx
        .user_fns
        .get(&fn_name)
        .ok_or_else(|| anyhow!("metodo `{owner}.{method_name}` nao registrado"))?
        .clone();
    let mangled: &'static str = Box::leak(format!("__user_{fn_name}").into_boxed_str());
    let fn_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("metodo mangled `{mangled}` faltando"))?;
    let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

    let mut args = Vec::with_capacity(arg_values.len() + 1);
    args.push(recv_i64);
    args.extend_from_slice(arg_values);
    let inst = ctx.builder.ins().call(fref, &args);
    let results = ctx.builder.inst_results(inst);
    if let Some(&v) = results.first() {
        Ok(TypedVal::new(v, abi.ret.unwrap_or(ValTy::I64)))
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
}

fn emit_virtual_dispatch(
    ctx: &mut FnCtx,
    class_name: &str,
    method_name: &str,
    static_owner: &str,
    recv_i64: cranelift_codegen::ir::Value,
    arg_values: &[cranelift_codegen::ir::Value],
    overrides: &[(String, String)],
) -> Result<TypedVal> {
    let (key_ptr, key_len) = ctx.emit_str_literal(b"__rts_class")?;
    let map_get = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx
        .builder
        .ins()
        .call(map_get, &[recv_i64, key_ptr, key_len]);
    let class_handle = ctx.builder.inst_results(inst)[0];

    let ret_ty = ctx
        .user_fns
        .get(&format!("__class_{static_owner}_{method_name}"))
        .and_then(|abi| abi.ret)
        .unwrap_or(ValTy::I64);

    let mut ordered = overrides.to_vec();
    ordered.sort_by_key(|(c, _)| {
        let mut depth = 0;
        let mut cur = c.clone();
        while let Some(meta) = ctx.classes.get(&cur) {
            match &meta.super_class {
                Some(p) => {
                    depth += 1;
                    cur = p.clone();
                }
                None => break,
            }
        }
        std::cmp::Reverse(depth)
    });

    let merge_block = ctx.builder.create_block();
    let result_param = ctx
        .builder
        .append_block_param(merge_block, ret_ty.cl_type());
    let str_eq = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_EQ",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    let _ = class_name;
    for (cname, owner) in &ordered {
        let (cn_ptr, cn_len) = ctx.emit_str_literal(cname.as_bytes())?;
        let from_static = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(from_static, &[cn_ptr, cn_len]);
        let target_handle = ctx.builder.inst_results(inst)[0];
        let inst = ctx
            .builder
            .ins()
            .call(str_eq, &[class_handle, target_handle]);
        let cmp = ctx.builder.inst_results(inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let is_eq =
            ctx.builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, cmp, zero);

        let then_block = ctx.builder.create_block();
        let else_block = ctx.builder.create_block();
        ctx.builder
            .ins()
            .brif(is_eq, then_block, &[], else_block, &[]);

        ctx.builder.switch_to_block(then_block);
        ctx.builder.seal_block(then_block);
        let result = emit_method_call(ctx, owner, method_name, recv_i64, arg_values)?;
        let coerced = match ret_ty {
            ValTy::I32 => ctx.coerce_to_i32(result).val,
            ValTy::F64 => to_f64(ctx, result),
            _ => ctx.coerce_to_i64(result).val,
        };
        ctx.builder.ins().jump(merge_block, &[coerced.into()]);

        ctx.builder.switch_to_block(else_block);
        ctx.builder.seal_block(else_block);
    }

    let result = emit_method_call(ctx, static_owner, method_name, recv_i64, arg_values)?;
    let coerced = match ret_ty {
        ValTy::I32 => ctx.coerce_to_i32(result).val,
        ValTy::F64 => to_f64(ctx, result),
        _ => ctx.coerce_to_i64(result).val,
    };
    ctx.builder.ins().jump(merge_block, &[coerced.into()]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    Ok(TypedVal::new(result_param, ret_ty))
}

pub(super) fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    let mangled: &'static str = Box::leak(format!("__user_{name}").into_boxed_str());
    let func_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("user function `{name}` has no cached id"))?;
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
    let ptr_ty = ctx.module.isa().pointer_type();
    let addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    Ok(TypedVal::new(addr, ValTy::I64))
}

fn lower_indirect_call(ctx: &mut FnCtx, callee_expr: &Expr, call: &CallExpr) -> Result<TypedVal> {
    use cranelift_codegen::isa::CallConv;

    let callee = lower_expr(ctx, callee_expr)?;
    let callee_val = ctx.coerce_to_i64(callee).val;

    let mut sig = Signature::new(CallConv::Tail);
    for _ in &call.args {
        sig.params.push(AbiParam::new(cl::I64));
    }
    sig.returns.push(AbiParam::new(cl::I64));
    let sig_ref = ctx.builder.import_signature(sig);

    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in indirect call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        args.push(ctx.coerce_to_i64(tv).val);
    }

    let inst = ctx.builder.ins().call_indirect(sig_ref, callee_val, &args);
    let results = ctx.builder.inst_results(inst);
    let v = results
        .first()
        .copied()
        .unwrap_or_else(|| ctx.builder.ins().iconst(cl::I64, 0));
    Ok(TypedVal::new(v, ValTy::I64))
}

fn emit_constant_load(ctx: &mut FnCtx, member: &crate::abi::NamespaceMember) -> Result<TypedVal> {
    let lowered = lower_member(member);
    let ret_cl = lowered
        .ret
        .ok_or_else(|| anyhow!("constant `{}` has no return type", member.name))?;

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

fn lower_intrinsic(
    ctx: &mut FnCtx,
    kind: crate::abi::Intrinsic,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::abi::Intrinsic;
    use cranelift_codegen::ir::condcodes::IntCC;
    use cranelift_module::DataDescription;

    fn arg_f64(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<cranelift_codegen::ir::Value> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(to_f64(ctx, tv))
    }

    fn arg_i64(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<cranelift_codegen::ir::Value> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    match kind {
        Intrinsic::Sqrt => {
            let x = arg_f64(ctx, call, 0)?;
            Ok(Some(TypedVal::new(ctx.builder.ins().sqrt(x), ValTy::F64)))
        }
        Intrinsic::AbsF64 => {
            let x = arg_f64(ctx, call, 0)?;
            Ok(Some(TypedVal::new(ctx.builder.ins().fabs(x), ValTy::F64)))
        }
        Intrinsic::MinF64 => Ok(Some(TypedVal::new(
            {
                let a = arg_f64(ctx, call, 0)?;
                let b = arg_f64(ctx, call, 1)?;
                ctx.builder.ins().fmin(a, b)
            },
            ValTy::F64,
        ))),
        Intrinsic::MaxF64 => Ok(Some(TypedVal::new(
            {
                let a = arg_f64(ctx, call, 0)?;
                let b = arg_f64(ctx, call, 1)?;
                ctx.builder.ins().fmax(a, b)
            },
            ValTy::F64,
        ))),
        Intrinsic::AbsI64 => {
            let x = arg_i64(ctx, call, 0)?;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_neg = ctx.builder.ins().icmp(IntCC::SignedLessThan, x, zero);
            let neg = ctx.builder.ins().ineg(x);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(is_neg, neg, x),
                ValTy::I64,
            )))
        }
        Intrinsic::MinI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let less = ctx.builder.ins().icmp(IntCC::SignedLessThan, a, b);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(less, a, b),
                ValTy::I64,
            )))
        }
        Intrinsic::MaxI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let greater = ctx.builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(greater, a, b),
                ValTy::I64,
            )))
        }
        Intrinsic::RandomF64 => {
            use cranelift_codegen::ir::MemFlags;

            const STATE_SYMBOL: &str = "__RTS_DATA_NS_MATH_RNG_STATE";
            let data_id = ctx
                .module
                .declare_data(STATE_SYMBOL, Linkage::Import, true, false)
                .map_err(|e| anyhow!("failed to declare {STATE_SYMBOL}: {e}"))?;
            let _ = DataDescription::new();
            let gv = ctx.module.declare_data_in_func(data_id, ctx.builder.func);
            let ptr_ty = ctx.module.isa().pointer_type();
            let ptr = ctx.builder.ins().global_value(ptr_ty, gv);

            let x0 = ctx.builder.ins().load(cl::I64, MemFlags::trusted(), ptr, 0);
            let s13 = ctx.builder.ins().ishl_imm(x0, 13);
            let x1 = ctx.builder.ins().bxor(x0, s13);
            let s7 = ctx.builder.ins().ushr_imm(x1, 7);
            let x2 = ctx.builder.ins().bxor(x1, s7);
            let s17 = ctx.builder.ins().ishl_imm(x2, 17);
            let x3 = ctx.builder.ins().bxor(x2, s17);
            ctx.builder.ins().store(MemFlags::trusted(), x3, ptr, 0);

            let bits = ctx.builder.ins().ushr_imm(x3, 11);
            let as_f = ctx.builder.ins().fcvt_from_uint(cl::F64, bits);
            let scale = ctx.builder.ins().f64const(1.0f64 / ((1u64 << 53) as f64));
            Ok(Some(TypedVal::new(
                ctx.builder.ins().fmul(as_f, scale),
                ValTy::F64,
            )))
        }
    }
}

fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }

    let lowered = lower_member(member);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
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
        id
    } else {
        *ctx.extern_cache.get(member.symbol).unwrap()
    };
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);

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
                    _ => return Err(anyhow!("StrPtr argument must be a string value")),
                }
            }
            AbiType::I32 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val)
            }
            AbiType::I64 | AbiType::U64 | AbiType::Handle | AbiType::Bool => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val)
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(to_f64(ctx, tv))
            }
            AbiType::Void => {}
        }
    }

    let inst = ctx.builder.ins().call(fref, &values);
    if lowered.ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        Ok(TypedVal::new(v, ValTy::from_abi(member.returns)))
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
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

    if ctx.is_tail_conv && ctx.in_tail_position {
        ctx.builder.ins().return_call(fref, &values);
        let cont = ctx.builder.create_block();
        ctx.builder.switch_to_block(cont);
        ctx.builder.seal_block(cont);
        let ty = abi.ret.unwrap_or(ValTy::I64);
        let placeholder = match ty {
            ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
            ValTy::F64 => ctx.builder.ins().f64const(0.0),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        return Ok(TypedVal::new(placeholder, ty));
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

pub(super) fn emit_namespace_constant(
    ctx: &mut FnCtx,
    qualified: &str,
) -> Result<Option<TypedVal>> {
    let Some((_spec, member)) = lookup(qualified) else {
        return Ok(None);
    };
    if !matches!(member.kind, crate::abi::MemberKind::Constant) {
        return Err(anyhow!(
            "`{qualified}` is a function, not a constant — use `{qualified}(...)`"
        ));
    }
    Ok(Some(emit_constant_load(ctx, member)?))
}
