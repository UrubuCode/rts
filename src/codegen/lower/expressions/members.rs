use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{Expr, Lit, MemberProp};

use crate::abi::lookup;

use super::calls::{AccessorKind, emit_namespace_constant, emit_virtual_accessor_dispatch};
use super::lower_expr;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};

pub(super) fn lower_array_lit(ctx: &mut FnCtx, arr: &swc_ecma_ast::ArrayLit) -> Result<TypedVal> {
    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    let push_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        &[cl::I64, cl::I64],
        None,
    )?;

    for elem in &arr.elems {
        let value = match elem {
            Some(e) => {
                if e.spread.is_some() {
                    return Err(anyhow!("spread em array literal nao suportado (MVP)"));
                }
                let tv = lower_expr(ctx, &e.expr)?;
                ctx.coerce_to_i64(tv).val
            }
            None => ctx.builder.ins().iconst(cl::I64, 0),
        };
        ctx.builder.ins().call(push_fn, &[handle, value]);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

pub(super) fn lower_object_lit(ctx: &mut FnCtx, obj: &swc_ecma_ast::ObjectLit) -> Result<TypedVal> {
    use swc_ecma_ast::{Prop, PropName, PropOrSpread};

    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    let set_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_SET",
        &[cl::I64, cl::I64, cl::I64, cl::I64],
        None,
    )?;

    for prop in &obj.props {
        let p = match prop {
            PropOrSpread::Prop(p) => p,
            PropOrSpread::Spread(_) => {
                return Err(anyhow!("spread em object literal nao suportado (MVP)"));
            }
        };

        let (key_str, value_expr): (String, Box<Expr>) = match p.as_ref() {
            Prop::KeyValue(kv) => {
                let k = match &kv.key {
                    PropName::Ident(id) => id.sym.as_str().to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    PropName::Num(n) => n.value.to_string(),
                    PropName::Computed(_) | PropName::BigInt(_) => {
                        return Err(anyhow!(
                            "computed/bigint key em object literal nao suportado (MVP)"
                        ));
                    }
                };
                (k, kv.value.clone())
            }
            Prop::Shorthand(id) => {
                let name = id.sym.as_str().to_string();
                let synthetic = Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: id.span,
                    ctxt: Default::default(),
                    sym: name.as_str().into(),
                    optional: false,
                }));
                (name, synthetic)
            }
            Prop::Method(_) | Prop::Getter(_) | Prop::Setter(_) | Prop::Assign(_) => {
                return Err(anyhow!(
                    "method/get/set/assign em object literal nao suportado (MVP)"
                ));
            }
        };

        let value_tv = lower_expr(ctx, &value_expr)?;
        let value_i64 = ctx.coerce_to_i64(value_tv).val;
        let (kptr, klen) = ctx.emit_str_literal(key_str.as_bytes())?;
        ctx.builder
            .ins()
            .call(set_fn, &[handle, kptr, klen, value_i64]);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

pub(super) fn lower_member_expr(ctx: &mut FnCtx, m: &swc_ecma_ast::MemberExpr) -> Result<TypedVal> {
    if let Some(qualified) = qualified_member_name(&Expr::Member(m.clone())) {
        if lookup(&qualified).is_some() {
            if let Some(tv) = emit_namespace_constant(ctx, &qualified)? {
                return Ok(tv);
            }
        }
    }

    let receiver_class = lhs_static_class(ctx, &m.obj);

    if let MemberProp::Ident(id) = &m.prop {
        if let Some(cls) = receiver_class.as_deref() {
            let prop_name = id.sym.as_str();
            if let Some(getter_owner) = resolve_getter_owner_local(ctx, cls, prop_name) {
                let recv_tv = lower_expr(ctx, &m.obj)?;
                let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
                return emit_virtual_accessor_dispatch(
                    ctx,
                    cls,
                    &getter_owner,
                    AccessorKind::Getter,
                    prop_name,
                    recv_i64,
                    &[],
                );
            }
        }
    }

    let obj_tv = lower_expr(ctx, &m.obj)?;
    let obj_handle = ctx.coerce_to_i64(obj_tv).val;

    match &m.prop {
        MemberProp::Ident(id) => {
            let key = id.sym.as_str();
            if let Some(cls) = receiver_class.as_deref() {
                validate_visibility(ctx, cls, key)?;
            }
            let mut field_ty = receiver_class
                .as_deref()
                .and_then(|c| field_type_in_hierarchy(ctx, c, key));
            // Fallback: tipo de campo registrado em var local (object literal).
            if field_ty.is_none() {
                if let Expr::Ident(obj_id) = m.obj.as_ref() {
                    if let Some(types) = ctx.local_obj_field_types.get(obj_id.sym.as_str()) {
                        field_ty = types.get(key).copied();
                    }
                }
            }
            map_get_static_typed(ctx, obj_handle, key.as_bytes(), field_ty)
        }
        MemberProp::Computed(c) => {
            if let Expr::Lit(Lit::Str(s)) = c.expr.as_ref() {
                return map_get_static(ctx, obj_handle, s.value.as_bytes());
            }
            let idx_tv = lower_expr(ctx, &c.expr)?;
            match idx_tv.ty {
                ValTy::Handle => {
                    let ptr_fref =
                        ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                    let len_fref =
                        ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                    let pi = ctx.builder.ins().call(ptr_fref, &[idx_tv.val]);
                    let kptr = ctx.builder.inst_results(pi)[0];
                    let li = ctx.builder.ins().call(len_fref, &[idx_tv.val]);
                    let klen = ctx.builder.inst_results(li)[0];
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, kptr, klen]);
                    Ok(TypedVal::new(ctx.builder.inst_results(inst)[0], ValTy::I64))
                }
                _ => {
                    let idx = ctx.coerce_to_i64(idx_tv).val;
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, idx]);
                    Ok(TypedVal::new(ctx.builder.inst_results(inst)[0], ValTy::I64))
                }
            }
        }
        MemberProp::PrivateName(pn) => {
            let key = format!("#{}", pn.name.as_ref());
            validate_private_scope(ctx, &key)?;
            let field_ty = receiver_class
                .as_deref()
                .and_then(|c| field_type_in_hierarchy(ctx, c, &key));
            map_get_static_typed(ctx, obj_handle, key.as_bytes(), field_ty)
        }
    }
}

pub(super) fn map_get_static(
    ctx: &mut FnCtx,
    obj_handle: cranelift_codegen::ir::Value,
    key: &[u8],
) -> Result<TypedVal> {
    map_get_static_typed(ctx, obj_handle, key, None)
}

pub(super) fn map_get_static_typed(
    ctx: &mut FnCtx,
    obj_handle: cranelift_codegen::ir::Value,
    key: &[u8],
    declared_ty: Option<ValTy>,
) -> Result<TypedVal> {
    let (kptr, klen) = ctx.emit_str_literal(key)?;
    let get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, kptr, klen]);
    let v = ctx.builder.inst_results(inst)[0];
    match declared_ty {
        Some(ValTy::I32) => Ok(TypedVal::new(
            ctx.builder.ins().ireduce(cl::I32, v),
            ValTy::I32,
        )),
        Some(ValTy::Handle) => Ok(TypedVal::new(v, ValTy::Handle)),
        Some(ValTy::Bool) => Ok(TypedVal::new(v, ValTy::Bool)),
        _ => Ok(TypedVal::new(v, ValTy::I64)),
    }
}

pub(super) fn validate_private_scope(ctx: &FnCtx, key: &str) -> Result<()> {
    let Some(current) = ctx.current_class.as_deref() else {
        return Err(anyhow!(
            "private `{key}` so pode ser acessado dentro do corpo da classe que o declara"
        ));
    };
    if let Some(meta) = ctx.classes.get(current) {
        if meta.field_types.contains_key(key) {
            return Ok(());
        }
    }
    Err(anyhow!(
        "private `{key}` nao e visivel em `{current}` (privates nao sao herdados de ancestrais)"
    ))
}

pub(super) fn validate_visibility(ctx: &FnCtx, receiver_class: &str, member: &str) -> Result<()> {
    use crate::parser::ast::Visibility;

    let mut cur = receiver_class.to_string();
    loop {
        let Some(meta) = ctx.classes.get(&cur) else {
            return Ok(());
        };
        if let Some(vis) = meta.member_visibility.get(member).copied() {
            match vis {
                Visibility::Public => return Ok(()),
                Visibility::Private => {
                    if ctx.current_class.as_deref() == Some(&cur) {
                        return Ok(());
                    }
                    return Err(anyhow!("membro `{member}` é private em `{cur}`"));
                }
                Visibility::Protected => {
                    let Some(current) = ctx.current_class.as_deref() else {
                        return Err(anyhow!("membro `{member}` é protected em `{cur}`"));
                    };
                    if is_descendant_of(ctx, current, &cur) {
                        return Ok(());
                    }
                    return Err(anyhow!(
                        "membro `{member}` é protected em `{cur}` — `{current}` nao estende `{cur}`"
                    ));
                }
            }
        }
        match meta.super_class.clone() {
            Some(p) => cur = p,
            None => return Ok(()),
        }
    }
}

fn is_descendant_of(ctx: &FnCtx, descendant: &str, ancestor: &str) -> bool {
    let mut cur = descendant.to_string();
    loop {
        if cur == ancestor {
            return true;
        }
        let Some(meta) = ctx.classes.get(&cur) else {
            return false;
        };
        match meta.super_class.clone() {
            Some(p) => cur = p,
            None => return false,
        }
    }
}

pub(super) fn field_is_readonly_in_hierarchy(ctx: &FnCtx, class: &str, field: &str) -> bool {
    let mut cur = class.to_string();
    loop {
        let Some(meta) = ctx.classes.get(&cur) else {
            return false;
        };
        if meta.readonly_fields.contains(field) {
            return true;
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return false,
        }
    }
}

pub(super) fn field_type_in_hierarchy(ctx: &FnCtx, class: &str, field: &str) -> Option<ValTy> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(ty) = meta.field_types.get(field).copied() {
            return Some(ty);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

fn field_class_in_hierarchy(ctx: &FnCtx, class: &str, field: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(ann) = meta.field_class_names.get(field) {
            if ctx.classes.contains_key(ann) {
                return Some(ann.clone());
            }
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

fn class_name_from_ts_type(ty: &swc_ecma_ast::TsType) -> Option<String> {
    if let swc_ecma_ast::TsType::TsTypeRef(r) = ty {
        if let swc_ecma_ast::TsEntityName::Ident(id) = &r.type_name {
            return Some(id.sym.as_str().to_string());
        }
    }
    None
}

pub(super) fn lhs_static_class(ctx: &FnCtx, expr: &Expr) -> Option<String> {
    match expr {
        Expr::This(_) => ctx.current_class.clone(),
        Expr::Ident(id) => ctx.local_class_ty.get(id.sym.as_str()).cloned(),
        Expr::Paren(p) => lhs_static_class(ctx, &p.expr),
        Expr::TsAs(a) => class_name_from_ts_type(&a.type_ann)
            .or_else(|| lhs_static_class(ctx, &a.expr)),
        Expr::TsTypeAssertion(a) => class_name_from_ts_type(&a.type_ann)
            .or_else(|| lhs_static_class(ctx, &a.expr)),
        Expr::TsConstAssertion(a) => lhs_static_class(ctx, &a.expr),
        Expr::TsSatisfies(a) => lhs_static_class(ctx, &a.expr),
        Expr::TsNonNull(n) => lhs_static_class(ctx, &n.expr),
        Expr::New(n) => {
            if let Expr::Ident(id) = n.callee.as_ref() {
                let name = id.sym.as_str();
                if ctx.classes.contains_key(name) {
                    return Some(name.to_string());
                }
            }
            None
        }
        Expr::Member(m) => {
            let owner = lhs_static_class(ctx, &m.obj)?;
            let prop = match &m.prop {
                MemberProp::Ident(id) => id.sym.as_str(),
                MemberProp::PrivateName(pn) => pn.name.as_ref(),
                MemberProp::Computed(_) => return None,
            };
            field_class_in_hierarchy(ctx, &owner, prop)
        }
        _ => None,
    }
}

pub(super) fn qualified_member_name(expr: &Expr) -> Option<String> {
    let Expr::Member(m) = expr else {
        return None;
    };
    let Expr::Ident(ns) = m.obj.as_ref() else {
        return None;
    };
    let fn_name = match &m.prop {
        MemberProp::Ident(id) => id.sym.as_str().to_string(),
        _ => return None,
    };
    Some(format!("{}.{}", ns.sym.as_str(), fn_name))
}

fn resolve_getter_owner_local(ctx: &FnCtx, class: &str, prop: &str) -> Option<String> {
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
