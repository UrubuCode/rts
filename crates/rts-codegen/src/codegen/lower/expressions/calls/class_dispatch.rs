//! Resolucao de metodos/accessors de classe + emissao de chamadas
//! virtuais e diretas.
//!
//! Inclui:
//! - resolve_method_owner / resolve_init_owner / resolve_getter_owner
//!   / resolve_setter_owner / resolve_accessor_owner — descem na cadeia
//!   de heranca (`super_class`) ate achar a classe que possui o membro.
//! - is_subclass_of, accessor_mangled, class_has_accessor — helpers.
//! - emit_virtual_accessor_dispatch — switch sobre __rts_class do receiver
//!   para despachar getter/setter overrides.
//! - emit_named_method_call / collect_method_overrides — analogos para
//!   metodos.
//! - lower_class_method_call_with_recv / emit_method_call /
//!   emit_virtual_dispatch — entry points para chamadas de metodo
//!   virtuais com receiver explicit.
//! - fn_name_has_this_param — heuristica de mangle.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::CallExpr;

use crate::codegen::lower::compile::class::{class_getter_name, class_setter_name};
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::members::{emit_class_tag_read, validate_visibility};
use super::super::operators::to_f64;

pub(crate) fn resolve_method_owner(ctx: &FnCtx, class: &str, method: &str) -> Option<String> {
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

pub(crate) fn resolve_init_owner(ctx: &FnCtx, class: &str) -> Option<String> {
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

pub(crate) fn resolve_getter_owner(ctx: &FnCtx, class: &str, prop: &str) -> Option<String> {
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

pub(crate) fn resolve_setter_owner(ctx: &FnCtx, class: &str, prop: &str) -> Option<String> {
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
pub(crate) enum AccessorKind {
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

pub(crate) fn emit_virtual_accessor_dispatch(
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

    let class_handle = emit_class_tag_read(ctx, recv_i64, static_class)?;

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

pub(crate) fn emit_named_method_call(
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
    let mangled: String = format!("__user_{fn_name}");
    let fn_id = *ctx
        .extern_cache
        .get(mangled.as_str())
        .ok_or_else(|| anyhow!("mangled `{mangled}` nao registrado"))?;
    let fref = ctx.fref_for_id(fn_id);

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

pub(crate) fn lower_class_method_call_with_recv(
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

pub(crate) fn emit_method_call(
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
    let mangled: String = format!("__user_{fn_name}");
    let fn_id = *ctx
        .extern_cache
        .get(mangled.as_str())
        .ok_or_else(|| anyhow!("metodo mangled `{mangled}` faltando"))?;
    let fref = ctx.fref_for_id(fn_id);

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
    let class_handle = emit_class_tag_read(ctx, recv_i64, class_name)?;

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

pub(crate) fn fn_name_has_this_param(name: &str) -> bool {
    name.starts_with("__class_")
        && !name.contains("_static_")
        && !name.ends_with("__init")
}
