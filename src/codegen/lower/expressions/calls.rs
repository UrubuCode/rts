use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{CallExpr, Callee, Expr, MemberProp};

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;

use super::lower_expr;
use super::members::{
    emit_class_tag_read, field_type_in_hierarchy, lhs_static_class, map_get_static_typed,
    qualified_member_name, validate_visibility,
};
use super::operators::to_f64;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::func::{class_getter_name, class_setter_name, class_static_method_name};

pub(super) fn lower_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    if matches!(&call.callee, Callee::Super(_)) {
        return lower_super_call(ctx, call);
    }
    // Dynamic import(expr) — lowers to runtime.eval_file(path).
    if matches!(&call.callee, Callee::Import(_)) {
        return lower_dynamic_import(ctx, call);
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
            if lookup(&qualified).is_some() {
                return lower_ns_call(ctx, &qualified, call);
            }
            // Console builtin (#221): console.log/info/debug → io.print,
            // console.error/warn → io.eprint. Args concatenados separados
            // por espaco. Implementado em codegen direto pra evitar
            // dependencia em pacote builtin/console que precisaria ser
            // auto-importado.
            if let Some(tv) = lower_console_call(ctx, &qualified, call)? {
                return Ok(tv);
            }
            // JSON global (#215): JSON.parse / JSON.stringify mapeiam
            // direto pra namespace json. Permite usar JSON.X(...) sem
            // import explicito, paridade com a semantica JS.
            if let Some(redirected) = qualified.strip_prefix("JSON.") {
                let target = format!("json.{redirected}");
                if lookup(&target).is_some() {
                    return lower_ns_call(ctx, &target, call);
                }
            }
            // Date global (#220): Date.now() → date.now_ms,
            // Date.parse(s) → date.from_iso. v0 expoe primitivas
            // sobre i64 (ts_ms); construtor `new Date(...)` e
            // getters de instancia ficam follow-up.
            if let Some(method) = qualified.strip_prefix("Date.") {
                let target = match method {
                    "now" => "date.now_ms",
                    "parse" => "date.from_iso",
                    _ => "",
                };
                if !target.is_empty() && lookup(target).is_some() {
                    return lower_ns_call(ctx, target, call);
                }
            }
            // Fallback: ident.fn(...) onde ident e var (ex: namespace TS
            // desugared para const Foo = { ... }). Faz map_get pela key
            // e despacha via call_indirect.
            if let Expr::Member(m) = callee.as_ref() {
                if let Expr::Ident(obj_id) = m.obj.as_ref() {
                    if ctx.var_ty(obj_id.sym.as_str()).is_some() {
                        if let MemberProp::Ident(prop) = &m.prop {
                            return lower_var_member_call(
                                ctx,
                                obj_id.sym.as_str(),
                                prop.sym.as_str(),
                                call,
                            );
                        }
                    }
                }
            }
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

/// Lowers `import(expr)` to `runtime.eval_file(path)`.
///
/// The path expression is evaluated and passed as a string handle to
/// `__RTS_FN_NS_RUNTIME_EVAL_FILE`. The return value is an i64 exit code for
/// now — full module-namespace handles are a follow-up (dynamic exports require
/// a map of heterogeneous values).
fn lower_dynamic_import(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    use crate::codegen::lower::ctx::ValTy;

    let path_arg = call
        .args
        .first()
        .ok_or_else(|| anyhow!("import() requires exactly one argument"))?;

    lower_ns_call(ctx, "runtime.eval_file", &CallExpr {
        span: call.span,
        callee: call.callee.clone(),
        args: vec![path_arg.clone()],
        type_args: None,
        ctxt: Default::default(),
    })
    .map(|tv| crate::codegen::lower::ctx::TypedVal { val: tv.val, ty: ValTy::I64 })
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

pub(super) fn lower_new(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
    let class_name = match new_expr.callee.as_ref() {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => {
            return Err(anyhow!(
                "`new` so suporta callee identifier (sem `new (expr)()`)"
            ));
        }
    };

    // #222 Map/Set v0 — `new Map()` e `new Set()` mapeiam para
    // collections.map_new (mesmo backing store HashMap<string, i64>).
    // Set usa value=1 sentinel; metodos respectivos sao lower em
    // lower_var_member_call. v0 nao suporta entries iniciais
    // (`new Map([["a",1]])`) nem iteradores.
    if class_name == "Map" || class_name == "Set" {
        if !ctx.classes.contains_key(&class_name) {
            let new_fn =
                ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(new_fn, &[]);
            let h = ctx.builder.inst_results(inst)[0];
            return Ok(TypedVal::new(h, ValTy::Handle));
        }
    }

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

    // Dual-path #147 passos 5-7: classes opt-in alocam via `gc.instance_*`
    // com layout nativo computado em compile-time. Caminho default
    // (HashMap-based) preservado intacto para todas as outras classes.
    let use_flat = meta.layout.is_some()
        && crate::codegen::lower::ctx::is_class_flat_enabled(&class_name);

    let (class_ptr, class_len) = ctx.emit_str_literal(class_name.as_bytes())?;
    let from_static = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(from_static, &[class_ptr, class_len]);
    let class_str_handle = ctx.builder.inst_results(inst)[0];

    let handle = if use_flat {
        let layout = meta.layout.as_ref().expect("layout checado acima");
        let size_val = ctx
            .builder
            .ins()
            .iconst(cl::I32, layout.size_bytes as i64);
        let new_fn = ctx.get_extern(
            "__RTS_FN_NS_GC_INSTANCE_NEW",
            &[cl::I32, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(new_fn, &[size_val, class_str_handle]);
        ctx.builder.inst_results(inst)[0]
    } else {
        let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(new_fn, &[]);
        let handle = ctx.builder.inst_results(inst)[0];

        let (key_ptr, key_len) = ctx.emit_str_literal(b"__rts_class")?;
        let map_set = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            &[cl::I64, cl::I64, cl::I64, cl::I64],
            None,
        )?;
        ctx.builder
            .ins()
            .call(map_set, &[handle, key_ptr, key_len, class_str_handle]);
        handle
    };

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
        let fref = ctx.fref_for_id(fn_id);

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

pub(super) fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    // User fns cujo endereço é tomado são declaradas com C callconv
    // (ver `address_taken_fns` em compile_program / #206) — seguro para
    // `thread.spawn` e FFI.
    let mangled: &'static str = Box::leak(format!("__user_{name}").into_boxed_str());
    let func_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("user function `{name}` has no cached id"))?;
    let fref = ctx.fref_for_id(func_id);
    let ptr_ty = ctx.module.isa().pointer_type();
    let addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    Ok(TypedVal::new(addr, ValTy::I64))
}

/// `obj.fn(...)` onde `obj` e uma var local (HashMap-like, ex: namespace
/// TS desugared). Faz map_get(obj, "fn") -> i64 (funcptr) e
/// call_indirect com signature i64-only.
fn lower_var_member_call(
    ctx: &mut FnCtx,
    obj_name: &str,
    prop: &str,
    call: &CallExpr,
) -> Result<TypedVal> {
    let obj_tv = ctx
        .read_local(obj_name)
        .ok_or_else(|| anyhow!("var `{obj_name}` nao encontrada"))?;
    let obj_h = ctx.coerce_to_i64(obj_tv).val;

    // Builtins de string em receiver Handle: s.indexOf(...), s.startsWith(...), etc.
    // Tem que vir antes do map_get porque uma string handle nao e um map —
    // map_get retornaria lixo, e o call_indirect subsequente saltaria pra
    // endereco invalido. (#235: indexOf travava/SIGSEGV em string com \0)
    if matches!(obj_tv.ty, ValTy::Handle) {
        if let Some(tv) = lower_string_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
        // #222 — Map/Set methods em receiver Handle. Heuristica conservadora:
        // so age quando o nome do metodo eh tipico de Map/Set e nao colide
        // com classes do usuario. Ergonomia v0 — usuario que tem classe
        // chamada `set()` em var Handle precisa anotar tipo da var pra
        // resolver dispatch antes do builtin.
        if let Some(tv) = lower_map_set_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
    }

    // Builtins de array/map: arr.push(x), arr.length() etc.
    if let Some(tv) = lower_array_builtin(ctx, prop, obj_h, call)? {
        return Ok(tv);
    }

    let (kp, kl) = ctx.emit_str_literal(prop.as_bytes())?;
    let map_get = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(map_get, &[obj_h, kp, kl]);
    let callee_val = ctx.builder.inst_results(inst)[0];

    // namespace fns address-taken sao SystemV/Win64 (ver
    // user_call_conv). call_indirect tem que casar a callconv da
    // target ou args/return chegam corrompidos.
    let cc = ctx.module.isa().default_call_conv();
    let mut sig = Signature::new(cc);
    for _ in &call.args {
        sig.params.push(AbiParam::new(cl::I64));
    }
    sig.returns.push(AbiParam::new(cl::I64));
    let sig_ref = ctx.builder.import_signature(sig);

    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in var.member call"));
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

/// Builtins de String.prototype em receiver Handle (string pool).
/// Mapeia os metodos JS-classicos para chamadas no namespace `string`/`gc`.
/// Retorna `Some` quando reconheceu o metodo. Necessario porque um
/// string handle nao e um map; tentar `map_get` num handle e depois
/// `call_indirect` no resultado salta pra lixo (#235).
fn lower_string_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // Helper: extrai (ptr, len) de um string handle via gc.string_ptr/len.
    fn handle_to_strptr(
        ctx: &mut FnCtx,
        h: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
        let ptr_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fref, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fref, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l))
    }

    fn arg_strptr(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in string builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let h = ctx.coerce_to_handle(tv)?.val;
        let ptr_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fref, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fref, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l))
    }

    match method {
        "length" => {
            // s.length() — comprimento em bytes UTF-8 (paridade com string.byte_len).
            let len_fref =
                ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(len_fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "indexOf" => {
            // s.indexOf(needle) — string.find(s, needle), retorna i64.
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let (np, nl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_FIND",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, np, nl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "includes" | "contains" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let (np, nl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_CONTAINS",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, np, nl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "startsWith" | "starts_with" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let (np, nl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_STARTS_WITH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, np, nl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "endsWith" | "ends_with" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let (np, nl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_ENDS_WITH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, np, nl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "toLowerCase" | "toLocaleLowerCase" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_TO_LOWER",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toUpperCase" | "toLocaleUpperCase" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_TO_UPPER",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trim" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_TRIM",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimStart" | "trim_start" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_TRIM_START",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimEnd" | "trim_end" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_TRIM_END",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "charCodeAt" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let arg = call
                .args
                .first()
                .ok_or_else(|| anyhow!("charCodeAt requires index"))?;
            let tv = lower_expr(ctx, &arg.expr)?;
            let idx = ctx.coerce_to_i64(tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_CHAR_CODE_AT",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, idx]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "charAt" => {
            let (sp, sl) = handle_to_strptr(ctx, recv_h)?;
            let arg = call
                .args
                .first()
                .ok_or_else(|| anyhow!("charAt requires index"))?;
            let tv = lower_expr(ctx, &arg.expr)?;
            let idx = ctx.coerce_to_i64(tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_STRING_CHAR_AT",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[sp, sl, idx]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}

/// Map/Set methods (#222) em receiver Handle. v0 mapeia direto pra
/// collections.map_* (mesmo backing store). Set usa Map<key, 1> com
/// key sempre string — limitacao aceita de v0.
///
/// Reconhecidos: set/get/has/delete/clear/add/size. Para `m.size`
/// (sem parens) ainda nao tem caminho — usuario chama `m.size()` em v0.
fn lower_map_set_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    fn arg_strptr(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        // Coerce qualquer valor a string handle (string_from_i64 / passthrough).
        let h = ctx.coerce_to_handle(tv)?.val;
        let ptr_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fref, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fref, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l))
    }

    match method {
        "set" => {
            // Map.set(key, value) — value pode ser handle ou number.
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let val_arg = call
                .args
                .get(1)
                .ok_or_else(|| anyhow!("Map.set requires value"))?;
            let val_tv = lower_expr(ctx, &val_arg.expr)?;
            let val_i64 = ctx.coerce_to_i64(val_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h, kp, kl, val_i64]);
            // Map.set retorna o proprio map (chainable em JS).
            Ok(Some(TypedVal::new(recv_h, ValTy::Handle)))
        }
        "add" => {
            // Set.add(value) → map_set(h, value, 1).
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h, kp, kl, one]);
            Ok(Some(TypedVal::new(recv_h, ValTy::Handle)))
        }
        "get" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "has" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_HAS",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "delete" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_DELETE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "clear" => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR",
                &[cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h]);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            )))
        }
        "size" => {
            // Em JS `m.size` eh property; v0 aceita `m.size()` como method
            // call ate ter property access em handles.
            if !call.args.is_empty() {
                return Ok(None);
            }
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        _ => Ok(None),
    }
}

/// Console object (#221) — mapeia console.log/info/debug → io.print
/// e console.error/warn → io.eprint. Args sao concatenados como string
/// separados por espaco (semantica JS). Implementado em codegen direto
/// pra que `console.X(...)` funcione sem import explicito.
fn lower_console_call(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let Some(method) = qualified.strip_prefix("console.") else {
        return Ok(None);
    };

    let target_symbol: &str = match method {
        "log" | "info" | "debug" => "__RTS_FN_NS_IO_PRINT",
        "error" | "warn" => "__RTS_FN_NS_IO_EPRINT",
        _ => return Ok(None),
    };

    // Concatena todos os args como string. JS: separador eh " " entre args.
    // Caso 0 args: imprime linha vazia (io.print/eprint ja adicionam \n).
    let space = ctx.emit_str_handle(b" ")?.val;
    let mut acc: Option<cranelift_codegen::ir::Value> = None;
    let concat = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_CONCAT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in console.* args"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let h = ctx.coerce_to_handle(tv)?.val;
        acc = Some(match acc {
            None => h,
            Some(prev) => {
                let with_space = ctx.builder.ins().call(concat, &[prev, space]);
                let prev_sp = ctx.builder.inst_results(with_space)[0];
                let combined = ctx.builder.ins().call(concat, &[prev_sp, h]);
                ctx.builder.inst_results(combined)[0]
            }
        });
    }

    let msg_handle = match acc {
        Some(v) => v,
        None => ctx.emit_str_handle(b"")?.val,
    };

    // Extrai (ptr, len) do handle e chama io.print/eprint (assinatura StrPtr).
    let ptr_fref =
        ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let len_fref =
        ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let pi = ctx.builder.ins().call(ptr_fref, &[msg_handle]);
    let p = ctx.builder.inst_results(pi)[0];
    let li = ctx.builder.ins().call(len_fref, &[msg_handle]);
    let l = ctx.builder.inst_results(li)[0];

    let print_fref = ctx.get_extern(target_symbol, &[cl::I64, cl::I64], None)?;
    ctx.builder.ins().call(print_fref, &[p, l]);

    Ok(Some(TypedVal::new(
        ctx.builder.ins().iconst(cl::I64, 0),
        ValTy::I64,
    )))
}

/// Builtins universais para arrays/maps via handle. Retorna `Some` se
/// a chamada foi tratada como builtin.
fn lower_array_builtin(
    ctx: &mut FnCtx,
    method: &str,
    obj_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    match method {
        "push" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg = &call.args[0];
            if arg.spread.is_some() {
                return Ok(None);
            }
            let push_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let tv = lower_expr(ctx, &arg.expr)?;
            let v = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(push_fn, &[obj_h, v]);
            // JS: push retorna novo length.
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(len_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "pop" => {
            // JS: retorna o ultimo elemento (ou undefined). v0 retorna 0 quando vazio.
            let pop_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_POP",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(pop_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "length" | "size" => {
            // length/size sao property em JS, mas v0 aceita como method call
            // (`arr.length()`) ate ter property access em handles.
            if !call.args.is_empty() {
                return Ok(None);
            }
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(len_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "at" => {
            // Negative indexing seria mais complexo; v0 so aceita non-negative.
            let idx_arg = call.args.first().ok_or_else(|| anyhow!("at requires index"))?;
            let tv = lower_expr(ctx, &idx_arg.expr)?;
            let idx = ctx.coerce_to_i64(tv).val;
            let get_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(get_fn, &[obj_h, idx]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "clear" => {
            let fref =
                ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_CLEAR", &[cl::I64], None)?;
            ctx.builder.ins().call(fref, &[obj_h]);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            )))
        }
        _ => Ok(None),
    }
}

fn lower_indirect_call(ctx: &mut FnCtx, callee_expr: &Expr, call: &CallExpr) -> Result<TypedVal> {
    let callee = lower_expr(ctx, callee_expr)?;
    let callee_val = ctx.coerce_to_i64(callee).val;

    // User fns address-taken (apply(double, ...), thread.spawn) sao
    // declaradas com platform default callconv (SystemV/Win64) — ver
    // user_call_conv. call_indirect precisa casar isso ou o argumento
    // chega no registrador errado (#206 era stack corruption; o caso
    // first_class_functions e arg_in_wrong_register).
    let cc = ctx.module.isa().default_call_conv();
    let mut sig = Signature::new(cc);
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
    let fref = ctx.fref_for_id(func_id);
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
            // Cache pra evitar redeclaracao em cada call de random_f64.
            // Cada declare_data nova em ObjectModule produz gv distinto
            // que Cranelift nao deduplica — em hot loops com 2+ chamadas
            // por iter, isso gerava 2 global_value/load/store
            // independentes no IR.
            let gv = if let Some(g) = ctx.gv_cache.get(STATE_SYMBOL).copied() {
                g
            } else {
                let data_id = if let Some(id) = ctx.data_cache.get(STATE_SYMBOL).copied() {
                    id
                } else {
                    let id = ctx
                        .module
                        .declare_data(STATE_SYMBOL, Linkage::Import, true, false)
                        .map_err(|e| anyhow!("failed to declare {STATE_SYMBOL}: {e}"))?;
                    ctx.data_cache.insert(STATE_SYMBOL, id);
                    id
                };
                let g = ctx.module.declare_data_in_func(data_id, ctx.builder.func);
                ctx.gv_cache.insert(STATE_SYMBOL, g);
                g
            };
            let ptr_ty = ctx.module.isa().pointer_type();
            let cur_block = ctx.builder.current_block();
            // Reusa x3 anterior se ainda no mesmo block: salta load
            // E pula o store da call anterior (dead store — overwrite
            // imediato). Em hot loops com 2+ random_f64() consecutivos,
            // isso elimina N-1 stores (so' o ultimo do block fica).
            let (ptr, x0) = if let (Some(blk), Some((cached_blk, cached_ptr, cached_x3))) =
                (cur_block, ctx.rng_state_cached)
            {
                if blk == cached_blk {
                    (cached_ptr, cached_x3)
                } else {
                    let ptr = ctx.builder.ins().global_value(ptr_ty, gv);
                    let x0 = ctx.builder.ins().load(cl::I64, MemFlags::trusted(), ptr, 0);
                    (ptr, x0)
                }
            } else {
                let ptr = ctx.builder.ins().global_value(ptr_ty, gv);
                let x0 = ctx.builder.ins().load(cl::I64, MemFlags::trusted(), ptr, 0);
                (ptr, x0)
            };
            let s13 = ctx.builder.ins().ishl_imm(x0, 13);
            let x1 = ctx.builder.ins().bxor(x0, s13);
            let s7 = ctx.builder.ins().ushr_imm(x1, 7);
            let x2 = ctx.builder.ins().bxor(x1, s7);
            let s17 = ctx.builder.ins().ishl_imm(x2, 17);
            let x3 = ctx.builder.ins().bxor(x2, s17);
            ctx.builder.ins().store(MemFlags::trusted(), x3, ptr, 0);

            // Cache pra prox call no mesmo block reusar x3 sem reload.
            if let Some(blk) = cur_block {
                ctx.rng_state_cached = Some((blk, ptr, x3));
            }

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
    let fref = ctx.fref_for_id(func_id);

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
                // Fast path: arg eh literal string. Emite (ptr, len)
                // estaticos direto, sem string_from_static + string_ptr
                // + string_len. Em \`io.print(\"hello\")\` reduz 4 calls
                // pra 1.
                fn unwrap_paren(e: &Expr) -> &Expr {
                    match e {
                        Expr::Paren(p) => unwrap_paren(&p.expr),
                        _ => e,
                    }
                }
                let lit_bytes: Option<Vec<u8>> = match unwrap_paren(&arg.expr) {
                    Expr::Lit(swc_ecma_ast::Lit::Str(s)) => {
                        Some(s.value.as_bytes().to_vec())
                    }
                    _ => None,
                };
                if let Some(bytes) = lit_bytes {
                    let (ptr, len) = ctx.emit_str_literal(&bytes)?;
                    values.push(ptr);
                    values.push(len);
                    continue;
                }
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
            AbiType::U64 => {
                // U64 e tipo opaco (handle/ptr/bits brutos). Quando o
                // input e f64, preservamos o bit-pattern via bitcast
                // em vez de truncar (fcvt_to_sint_sat) — ex:
                // `thread.spawn(fp, 3.14)` precisa entregar 3.14 ao
                // worker, nao 3.
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = match tv.ty {
                    crate::codegen::lower::ctx::ValTy::F64 => {
                        use cranelift_codegen::ir::MemFlags;
                        ctx.builder.ins().bitcast(cl::I64, MemFlags::new(), tv.val)
                    }
                    _ => ctx.coerce_to_i64(tv).val,
                };
                values.push(v)
            }
            AbiType::I64 | AbiType::Handle | AbiType::Bool => {
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
    let fref = ctx.fref_for_id(func_id);

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
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => ctx.coerce_to_i64(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
        };
        values.push(value);
    }

    // Tail calls: still check depth (self-recursion runs as loop via return_call).
    if ctx.is_tail_conv && ctx.in_tail_position {
        let ty = abi.ret.unwrap_or(ValTy::I64);
        let push_fref = ctx.get_extern("__RTS_FN_RT_STACK_PUSH", &[], Some(cl::I32))?;
        let push_inst = ctx.builder.ins().call(push_fref, &[]);
        let ok_flag = ctx.builder.inst_results(push_inst)[0];

        let tail_block = ctx.builder.create_block();
        let overflow_block = ctx.builder.create_block();
        ctx.builder.ins().brif(ok_flag, tail_block, &[], overflow_block, &[]);

        // overflow: return sentinel without doing the tail call
        ctx.builder.switch_to_block(overflow_block);
        ctx.builder.seal_block(overflow_block);
        let sentinel: cranelift_codegen::ir::Value = match ty {
            ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
            ValTy::F64 => ctx.builder.ins().f64const(0.0),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        ctx.builder.ins().return_(&[sentinel]);

        // ok: do the actual tail call — no pop, depth accumulates across tail iterations.
        ctx.builder.switch_to_block(tail_block);
        ctx.builder.seal_block(tail_block);
        ctx.builder.ins().return_call(fref, &values);
        let cont = ctx.builder.create_block();
        ctx.builder.switch_to_block(cont);
        ctx.builder.seal_block(cont);
        let placeholder = match ty {
            ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
            ValTy::F64 => ctx.builder.ins().f64const(0.0),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        return Ok(TypedVal::new(placeholder, ty));
    }

    let ret_ty = abi.ret.unwrap_or(ValTy::I64);

    // Stack depth guard: push → brif → call → pop.
    let push_fref = ctx.get_extern("__RTS_FN_RT_STACK_PUSH", &[], Some(cl::I32))?;
    let push_inst = ctx.builder.ins().call(push_fref, &[]);
    let ok_flag = ctx.builder.inst_results(push_inst)[0];

    let call_block = ctx.builder.create_block();
    let overflow_block = ctx.builder.create_block();
    let after_block = ctx.builder.create_block();
    let cl_ty = match ret_ty {
        ValTy::I32 => cl::I32,
        ValTy::F64 => cl::F64,
        _ => cl::I64,
    };
    ctx.builder.append_block_param(after_block, cl_ty);

    ctx.builder.ins().brif(ok_flag, call_block, &[], overflow_block, &[]);

    // overflow path — error slot set by STACK_PUSH, return sentinel
    ctx.builder.switch_to_block(overflow_block);
    ctx.builder.seal_block(overflow_block);
    let sentinel: cranelift_codegen::ir::Value = match ret_ty {
        ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
        ValTy::F64 => ctx.builder.ins().f64const(0.0),
        _ => ctx.builder.ins().iconst(cl::I64, 0),
    };
    ctx.builder.ins().jump(after_block, &[sentinel.into()]);

    // normal call path
    ctx.builder.switch_to_block(call_block);
    ctx.builder.seal_block(call_block);
    let inst = ctx.builder.ins().call(fref, &values);
    let pop_fref = ctx.get_extern("__RTS_FN_RT_STACK_POP", &[], None)?;
    ctx.builder.ins().call(pop_fref, &[]);
    let ret_val: cranelift_codegen::ir::Value = {
        let results = ctx.builder.inst_results(inst);
        if let Some(&v) = results.first() {
            v
        } else {
            match ret_ty {
                ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
                ValTy::F64 => ctx.builder.ins().f64const(0.0),
                _ => ctx.builder.ins().iconst(cl::I64, 0),
            }
        }
    };
    ctx.builder.ins().jump(after_block, &[ret_val.into()]);

    ctx.builder.switch_to_block(after_block);
    ctx.builder.seal_block(after_block);
    let result = ctx.builder.block_params(after_block)[0];
    Ok(TypedVal::new(result, ret_ty))
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
