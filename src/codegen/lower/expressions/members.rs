use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{Expr, Lit, MemberProp};

use crate::abi::lookup;

use super::calls::{AccessorKind, emit_namespace_constant, emit_virtual_accessor_dispatch};
use super::lower_expr;
use crate::codegen::lower::ctx::{FieldSlot, FnCtx, TypedVal, ValTy, is_class_flat_enabled};

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
        match elem {
            Some(e) => {
                if e.spread.is_some() {
                    // #209: array spread — copia cada elemento da fonte
                    // pro array destino. Fonte deve ser handle Vec
                    // (qualquer expressao que avalie pra um). v0 nao
                    // suporta spread de Set/Map nem iteradores nativos
                    // — caller passa array.
                    let src_tv = lower_expr(ctx, &e.expr)?;
                    let src_h = ctx.coerce_to_i64(src_tv).val;
                    emit_vec_extend(ctx, handle, src_h, push_fn)?;
                    continue;
                }
                let tv = lower_expr(ctx, &e.expr)?;
                let value = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(push_fn, &[handle, value]);
            }
            None => {
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                ctx.builder.ins().call(push_fn, &[handle, zero]);
            }
        }
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

/// Quando o objeto eh uma var local registrada com object literal que
/// tem o campo `key`, retorna seu ValTy. Usado pra distinguir Map/Set
/// (sem campos) de objects literais (\`{ size: 5 }\`) — no segundo caso
/// queremos cair no map_get tipado, nao em map_len.
fn lhs_object_field_known(ctx: &FnCtx, obj: &Expr, key: &str) -> Option<ValTy> {
    if let Expr::Ident(id) = obj {
        let name = id.sym.as_str();
        if let Some(types) = ctx.local_obj_field_types.get(name) {
            return types.get(key).copied();
        }
    }
    None
}

/// Para cada key em src (ordem deterministica), faz dst[key] = src[key].
/// Usa map_key_at + map_get + map_set — todos do namespace collections.
fn emit_map_extend(
    ctx: &mut FnCtx,
    dst: cranelift_codegen::ir::Value,
    src: cranelift_codegen::ir::Value,
    set_fn: cranelift_codegen::ir::FuncRef,
) -> Result<()> {
    use cranelift_codegen::ir::condcodes::IntCC;

    let len_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let key_at_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let str_ptr_fn = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_PTR",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let str_len_fn = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_LEN",
        &[cl::I64],
        Some(cl::I64),
    )?;

    let len_inst = ctx.builder.ins().call(len_fn, &[src]);
    let len = ctx.builder.inst_results(len_inst)[0];

    let loop_block = ctx.builder.create_block();
    let body_block = ctx.builder.create_block();
    let exit_block = ctx.builder.create_block();
    ctx.builder.append_block_param(loop_block, cl::I64);

    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    ctx.builder.ins().jump(loop_block, &[zero.into()]);

    ctx.builder.switch_to_block(loop_block);
    let i = ctx.builder.block_params(loop_block)[0];
    let cond = ctx.builder.ins().icmp(IntCC::SignedLessThan, i, len);
    ctx.builder
        .ins()
        .brif(cond, body_block, &[], exit_block, &[]);

    ctx.builder.switch_to_block(body_block);
    ctx.builder.seal_block(body_block);

    // key_handle = map_key_at(src, i)
    let key_inst = ctx.builder.ins().call(key_at_fn, &[src, i]);
    let key_handle = ctx.builder.inst_results(key_inst)[0];
    // (kp, kl) = (string_ptr(key_handle), string_len(key_handle))
    let kp_inst = ctx.builder.ins().call(str_ptr_fn, &[key_handle]);
    let kp = ctx.builder.inst_results(kp_inst)[0];
    let kl_inst = ctx.builder.ins().call(str_len_fn, &[key_handle]);
    let kl = ctx.builder.inst_results(kl_inst)[0];
    // value = map_get(src, kp, kl)
    let val_inst = ctx.builder.ins().call(get_fn, &[src, kp, kl]);
    let value = ctx.builder.inst_results(val_inst)[0];
    // map_set(dst, kp, kl, value)
    ctx.builder.ins().call(set_fn, &[dst, kp, kl, value]);

    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let next_i = ctx.builder.ins().iadd(i, one);
    ctx.builder.ins().jump(loop_block, &[next_i.into()]);

    ctx.builder.seal_block(loop_block);
    ctx.builder.switch_to_block(exit_block);
    ctx.builder.seal_block(exit_block);
    Ok(())
}

/// Para cada elemento `i` em [0, len(src)), faz dst.push(src[i]).
/// Emite um loop em IR usando block params.
fn emit_vec_extend(
    ctx: &mut FnCtx,
    dst: cranelift_codegen::ir::Value,
    src: cranelift_codegen::ir::Value,
    push_fn: cranelift_codegen::ir::FuncRef,
) -> Result<()> {
    use cranelift_codegen::ir::condcodes::IntCC;

    let len_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    let len_inst = ctx.builder.ins().call(len_fn, &[src]);
    let len = ctx.builder.inst_results(len_inst)[0];

    // Loop classico: i = 0; while (i < len) { push(get(src, i)); i++; }
    let loop_block = ctx.builder.create_block();
    let body_block = ctx.builder.create_block();
    let exit_block = ctx.builder.create_block();
    ctx.builder.append_block_param(loop_block, cl::I64);

    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    ctx.builder.ins().jump(loop_block, &[zero.into()]);

    ctx.builder.switch_to_block(loop_block);
    let i = ctx.builder.block_params(loop_block)[0];
    let cond = ctx.builder.ins().icmp(IntCC::SignedLessThan, i, len);
    ctx.builder
        .ins()
        .brif(cond, body_block, &[], exit_block, &[]);

    ctx.builder.switch_to_block(body_block);
    ctx.builder.seal_block(body_block);
    let elem_inst = ctx.builder.ins().call(get_fn, &[src, i]);
    let elem = ctx.builder.inst_results(elem_inst)[0];
    ctx.builder.ins().call(push_fn, &[dst, elem]);
    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let next_i = ctx.builder.ins().iadd(i, one);
    ctx.builder.ins().jump(loop_block, &[next_i.into()]);

    ctx.builder.seal_block(loop_block);
    ctx.builder.switch_to_block(exit_block);
    ctx.builder.seal_block(exit_block);
    Ok(())
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
            PropOrSpread::Spread(spread) => {
                // #209: object spread — copia todas as keys da fonte
                // pro destino. Fonte deve ser map handle. Keys posteriores
                // sobrescrevem (semantica JS: \`{ ...a, x: 1, ...b }\`
                // resolve em ordem).
                let src_tv = lower_expr(ctx, &spread.expr)?;
                let src_h = ctx.coerce_to_i64(src_tv).val;
                emit_map_extend(ctx, handle, src_h, set_fn)?;
                continue;
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

    // #222: \`.size\` e \`.length\` em receiver Handle redirecionam pra
    // collections.map_len/vec_len. Antes do caminho map_get porque
    // \`size\`/\`length\` em Map/Set/Array sao properties (nao keys).
    // Para object literals (\`{ size: 5 }\`), o receiver_class eh None
    // e o local_obj_field_types tem registro — esse caso ainda cai
    // no map_get abaixo via field_ty.
    // .size/.length em receiver Handle redireciona pra map_len/vec_len
    // (#222 Map/Set, #208 Array). Limita a Handle pra nao confundir
    // com object literal { size: 5 } cujo storage de campo eh I64.
    // Em top-level com globais I64 fixo, propriedade nao funciona
    // sem two-pass scan — usuario chama m.size() como method em v0.
    if matches!(obj_tv.ty, ValTy::Handle) && receiver_class.is_none() {
        if let MemberProp::Ident(id) = &m.prop {
            let key = id.sym.as_str();
            if (key == "size" || key == "length")
                && lhs_object_field_known(ctx, &m.obj, key).is_none()
            {
                // gc.handle_len despacha pelo tipo do Entry — funciona
                // para String/Map/Vec/Buffer/Env. Caller nao precisa
                // saber se receiver eh Map vs Set vs Array.
                let len_fn = ctx.get_extern(
                    "__RTS_FN_NS_GC_HANDLE_LEN",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(len_fn, &[obj_handle]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(v, ValTy::I64));
            }
        }
    }

    match &m.prop {
        MemberProp::Ident(id) => {
            let key = id.sym.as_str();
            if let Some(cls) = receiver_class.as_deref() {
                validate_visibility(ctx, cls, key)?;
            }
            // Dual-path #147 passo 6: leitura tipada via gc.instance_*
            // quando classe e flat e field tem slot conhecido. Caso
            // contrario cai no caminho HashMap atual.
            if let Some(cls) = receiver_class.as_deref() {
                if class_field_uses_flat(ctx, cls, key) {
                    return emit_flat_field_read(ctx, obj_handle, cls, key);
                }
            }
            // Error class property reads: .message / .name / .toString via
            // Entry::ErrorObj. Route to __RTS_FN_GL_ERROR_* instead of map_get.
            if let Some(cls) = receiver_class.as_deref() {
                let is_error_cls = matches!(
                    cls,
                    "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
                );
                if is_error_cls {
                    let sym = match key {
                        "message" => Some("__RTS_FN_GL_ERROR_MESSAGE"),
                        "name" => Some("__RTS_FN_GL_ERROR_NAME"),
                        _ => None,
                    };
                    if let Some(sym) = sym {
                        let f = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
                        let inst = ctx.builder.ins().call(f, &[obj_handle]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Handle));
                    }
                }
            }
            // Global class instance property reads (e.g. re.source via InstanceMethod).
            if let Some(cls) = receiver_class.as_deref() {
                if let Some(spec) = crate::abi::global_class_lookup(cls) {
                    if let Some(member) = spec.instance_method(key) {
                        // Zero-arg instance method used as property accessor (e.g. .source)
                        if member.args.len() == 1 {
                            let sig = crate::abi::signature::lower_member(member);
                            let f = ctx.get_extern(member.symbol, &sig.params, sig.ret)?;
                            let inst = ctx.builder.ins().call(f, &[obj_handle]);
                            let ret_ty = ValTy::from_abi(member.returns);
                            let v = if sig.ret.is_some() {
                                ctx.builder.inst_results(inst)[0]
                            } else {
                                ctx.builder.ins().iconst(cl::I64, 0)
                            };
                            return Ok(TypedVal::new(v, ret_ty));
                        }
                    }
                }
            }
            // (#214) `(e as Error).message` — field_ty hint for map_get fallback.
            let mut field_ty = receiver_class
                .as_deref()
                .and_then(|c| field_type_in_hierarchy(ctx, c, key));
            if field_ty.is_none() && (key == "message" || key == "name") {
                if let Some(cls) = receiver_class.as_deref() {
                    if matches!(
                        cls,
                        "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
                    ) {
                        field_ty = Some(ValTy::Handle);
                    }
                }
            }
            // Fallback: tipo de campo registrado em var local (object literal).
            if field_ty.is_none() {
                if let Expr::Ident(obj_id) = m.obj.as_ref() {
                    let n = obj_id.sym.as_str();
                    if let Some(types) = ctx.local_obj_field_types.get(n) {
                        field_ty = types.get(key).copied();
                    }
                    // (#330) Global object literal (ex: enum string em fn user)
                    if field_ty.is_none() {
                        if let Some(types) = ctx.global_obj_field_types.get(n) {
                            field_ty = types.get(key).copied();
                        }
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

/// Resolve o `FieldSlot` para `field` percorrendo a hierarquia de
/// `class`. Retorna `None` se a classe (ou ancestrais ate o que declarou
/// o campo) nao tem layout nativo computado.
pub(crate) fn field_slot_in_hierarchy(
    ctx: &FnCtx,
    class: &str,
    field: &str,
) -> Option<FieldSlot> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(layout) = meta.layout.as_ref() {
            if let Some(slot) = layout.fields.iter().find(|s| s.name == field) {
                return Some(slot.clone());
            }
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

/// True quando a classe `class` (e a hierarquia ancestral relevante)
/// pode usar o caminho flat para acessar `field`.
pub(crate) fn class_field_uses_flat(ctx: &FnCtx, class: &str, field: &str) -> bool {
    if !is_class_flat_enabled(class) {
        return false;
    }
    // Bloqueio conservador: se a propria classe tem getter/setter dinamico
    // pra qualquer prop, nao desviamos para flat — mantemos hashmap como
    // escape hatch (passo 8 ira tratar dispatch virtual de getters).
    if let Some(meta) = ctx.classes.get(class) {
        if !meta.getters.is_empty() || !meta.setters.is_empty() {
            return false;
        }
    }
    field_slot_in_hierarchy(ctx, class, field).is_some()
}

/// Emite leitura tipada de um campo flat em `class.field`. Pre-condicao:
/// `class_field_uses_flat(ctx, class, field) == true`.
pub(crate) fn emit_flat_field_read(
    ctx: &mut FnCtx,
    recv_handle: cranelift_codegen::ir::Value,
    class: &str,
    field: &str,
) -> Result<TypedVal> {
    let slot = field_slot_in_hierarchy(ctx, class, field)
        .ok_or_else(|| anyhow!("flat field `{class}.{field}` sem slot"))?;
    let off = ctx
        .builder
        .ins()
        .iconst(cl::I32, slot.offset as i64);
    let (sym, ret_ty, ret_kind): (&'static str, _, ValTy) = match slot.ty {
        ValTy::F64 => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_F64",
            cl::F64,
            ValTy::F64,
        ),
        ValTy::I32 => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I32",
            cl::I32,
            ValTy::I32,
        ),
        ValTy::Bool => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
            cl::I64,
            ValTy::Bool,
        ),
        ValTy::Handle => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
            cl::I64,
            ValTy::Handle,
        ),
        ValTy::I64 => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
            cl::I64,
            ValTy::I64,
        ),
        ValTy::U64 => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
            cl::I64,
            ValTy::U64,
        ),
    };
    let fref = ctx.get_extern(sym, &[cl::I64, cl::I32], Some(ret_ty))?;
    let inst = ctx.builder.ins().call(fref, &[recv_handle, off]);
    let v = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(v, ret_kind))
}

/// Emite escrita tipada de um campo flat. Coage `value` para o ValTy do
/// slot antes de chamar a primitiva.
pub(crate) fn emit_flat_field_write(
    ctx: &mut FnCtx,
    recv_handle: cranelift_codegen::ir::Value,
    class: &str,
    field: &str,
    value: TypedVal,
) -> Result<()> {
    let slot = field_slot_in_hierarchy(ctx, class, field)
        .ok_or_else(|| anyhow!("flat field `{class}.{field}` sem slot"))?;
    let off = ctx
        .builder
        .ins()
        .iconst(cl::I32, slot.offset as i64);
    match slot.ty {
        ValTy::F64 => {
            let coerced = ctx.coerce_to_f64(value).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_GC_INSTANCE_STORE_F64",
                &[cl::I64, cl::I32, cl::F64],
                Some(cl::I64),
            )?;
            ctx.builder.ins().call(fref, &[recv_handle, off, coerced]);
        }
        ValTy::I32 => {
            let coerced = ctx.coerce_to_i32(value).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_GC_INSTANCE_STORE_I32",
                &[cl::I64, cl::I32, cl::I32],
                Some(cl::I64),
            )?;
            ctx.builder.ins().call(fref, &[recv_handle, off, coerced]);
        }
        ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => {
            let coerced = ctx.coerce_to_i64(value).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_GC_INSTANCE_STORE_I64",
                &[cl::I64, cl::I32, cl::I64],
                Some(cl::I64),
            )?;
            ctx.builder.ins().call(fref, &[recv_handle, off, coerced]);
        }
    }
    Ok(())
}

/// Le o handle do tag `__rts_class` de uma instancia. Centraliza o
/// dual-path do dispatch virtual (#147 passo 8): quando a classe estatica
/// e flat E tem layout, emite `gc.instance_class(recv)` (leitura direta
/// do struct Instance); caso contrario cai no `MAP_GET("__rts_class")`
/// legacy. Retorna i64 com o handle de string.
pub(crate) fn emit_class_tag_read(
    ctx: &mut FnCtx,
    recv_handle: cranelift_codegen::ir::Value,
    class_name: &str,
) -> Result<cranelift_codegen::ir::Value> {
    // Pre-condicao do dual-path: a classe estatica precisa ser flat
    // (env var ou prefixo `__Flat`). Se a hierarquia mistura
    // flat/HashMap, o usuario e responsavel por habilitar TODA a
    // hierarquia — instancias HashMap polimorficas sob receiver flat
    // retornariam 0 em `instance_class` e cairiam no metodo da base.
    // Mesma simetria reversa: instancias flat sob receiver HashMap nao
    // tem `__rts_class` no map. Nao tem como cobrir mistura sem
    // inspecao runtime do tipo de Entry.
    let is_flat = is_class_flat_enabled(class_name)
        && ctx
            .classes
            .get(class_name)
            .map(|m| m.layout.is_some())
            .unwrap_or(false);
    if is_flat {
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_INSTANCE_CLASS",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[recv_handle]);
        return Ok(ctx.builder.inst_results(inst)[0]);
    }
    let (key_ptr, key_len) = ctx.emit_str_literal(b"__rts_class")?;
    let map_get = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(map_get, &[recv_handle, key_ptr, key_len]);
    Ok(ctx.builder.inst_results(inst)[0])
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
                if ctx.classes.contains_key(name)
                    || crate::abi::global_class_lookup(name).is_some()
                {
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
        Expr::Call(call) => {
            // Resolve method chains like `expect(...).toBe(...)`:
            // if the callee is a user fn with a known class return type, use it.
            if let swc_ecma_ast::Callee::Expr(callee) = &call.callee {
                if let Expr::Ident(id) = callee.as_ref() {
                    if let Some(abi) = ctx.user_fns.get(id.sym.as_str()) {
                        return abi.ret_class.clone();
                    }
                }
            }
            None
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
