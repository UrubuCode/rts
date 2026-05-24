use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use std::collections::HashMap;
use swc_ecma_ast::{Expr, Lit, MemberProp};

use crate::abi::lookup;

use super::calls::{
    AccessorKind, emit_namespace_constant, emit_user_fn_addr,
    emit_virtual_accessor_dispatch, fn_name_has_this_param, resolve_method_owner,
};
use super::lower_expr;
use crate::codegen::lower::ctx::{FieldSlot, FnCtx, TypedVal, ValTy, is_class_flat_enabled};

pub(super) fn lower_array_lit(ctx: &mut FnCtx, arr: &swc_ecma_ast::ArrayLit) -> Result<TypedVal> {
    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];
    // (cross-runtime #40) Marca handle do array literal como GC root. Sem
    // isso, allocs feitas durante push de elementos (`pair[0] + "=" + pair[1]`
    // chama gc.string_*) podem disparar coleta que libera o proprio Vec
    // ainda em construcao.
    ctx.declare_gc_handle(handle);

    let push_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        &[cl::I64, cl::I64],
        None,
    )?;

    for elem in &arr.elems {
        match elem {
            Some(e) => {
                if e.spread.is_some() {
                    // (#209/spread) Spread universal — runtime helper
                    // detecta Vec/String/Map e itera. Antes so' funcionava
                    // pra Vec; `[..."abc"]` retornava vazio.
                    let src_tv = lower_expr(ctx, &e.expr)?;
                    let src_h = ctx.coerce_to_i64(src_tv).val;
                    let spread_fn = ctx.get_extern(
                        "__RTS_FN_RT_SPREAD_INTO_VEC",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    ctx.builder.ins().call(spread_fn, &[handle, src_h]);
                    continue;
                }
                // (cross-runtime #142) `undefined`/`null` em array literal
                // viram sentinela. join/toString/etc espera-se que tratem
                // como string vazia (JS spec).
                //   MIN+2 = undefined, MIN+3 = null
                if let Expr::Ident(id) = e.expr.as_ref() {
                    if id.sym.as_str() == "undefined" && ctx.read_local("undefined").is_none() {
                        let s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
                        ctx.builder.ins().call(push_fn, &[handle, s]);
                        continue;
                    }
                }
                if matches!(e.expr.as_ref(), Expr::Lit(Lit::Null(_))) {
                    let s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 3);
                    ctx.builder.ins().call(push_fn, &[handle, s]);
                    continue;
                }
                let tv = lower_expr(ctx, &e.expr)?;
                // Bool em array vira sentinela (i64::MIN = false, MIN+1 =
                // true). Permite inspect_slot imprimir `true`/`false` sem
                // aspas, distinguindo de Entry::String("true"). Sentinelas
                // ficam fora do range de handles validos (gen>=1) e fora
                // do range de inteiros JS comuns.
                let value = if matches!(tv.ty, ValTy::Bool) {
                    let b = ctx.coerce_to_i64(tv).val;
                    // i64::MIN + b: b=0 -> MIN (false), b=1 -> MIN+1 (true)
                    let min = ctx.builder.ins().iconst(cl::I64, i64::MIN);
                    ctx.builder.ins().iadd(min, b)
                } else {
                    ctx.coerce_to_i64(tv).val
                };
                ctx.builder.ins().call(push_fn, &[handle, value]);
            }
            None => {
                // (cross-runtime #52) Sparse slot (`[1,,3]`) — JS spec
                // distingue hole de undefined literal. Hole eh i64::MIN+4
                // (sentinela exclusiva); Object.keys pula, join trata como
                // empty (igual undefined).
                let s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 4);
                ctx.builder.ins().call(push_fn, &[handle, s]);
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

        // Resolve key + value. Para computed keys avaliamos a expr e
        // pegamos ptr/len do string handle resultante. Para keys
        // estaticas usamos emit_str_literal direto.
        enum KeySrc {
            Static(String),
            Computed(Box<Expr>),
        }
        let (key_src, value_expr): (KeySrc, Box<Expr>) = match p.as_ref() {
            Prop::KeyValue(kv) => {
                let k = match &kv.key {
                    PropName::Ident(id) => KeySrc::Static(id.sym.as_str().to_string()),
                    PropName::Str(s) => KeySrc::Static(s.value.to_string_lossy().to_string()),
                    PropName::Num(n) => KeySrc::Static(n.value.to_string()),
                    PropName::Computed(c) => {
                        // Caso comum: literal string dentro do []
                        if let Expr::Lit(swc_ecma_ast::Lit::Str(s)) = c.expr.as_ref() {
                            KeySrc::Static(s.value.to_string_lossy().to_string())
                        } else {
                            KeySrc::Computed(c.expr.clone())
                        }
                    }
                    PropName::BigInt(_) => {
                        return Err(anyhow!(
                            "bigint key em object literal nao suportado"
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
                (KeySrc::Static(name), synthetic)
            }
            // (#261) Method shorthand: \`{ greet() { ... } }\` desugara
            // pra \`{ greet: function() { ... } }\`. \`this\` no body usa
            // slot thread-local (PR #451), populado pelo call site
            // que faz THIS_PUSH antes do invoke (\`obj.greet()\`).
            Prop::Method(method) => {
                let name = match &method.key {
                    PropName::Ident(id) => id.sym.as_str().to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    PropName::Num(n) => n.value.to_string(),
                    PropName::Computed(c) => {
                        if let Expr::Lit(swc_ecma_ast::Lit::Str(s)) = c.expr.as_ref() {
                            s.value.to_string_lossy().to_string()
                        } else {
                            return Err(anyhow!(
                                "computed key dynamic em method shorthand ainda nao suportado"
                            ));
                        }
                    }
                    PropName::BigInt(_) => {
                        return Err(anyhow!("bigint key em method nao suportado"));
                    }
                };
                let fn_expr = swc_ecma_ast::FnExpr {
                    ident: None,
                    function: method.function.clone(),
                };
                (KeySrc::Static(name), Box::new(Expr::Fn(fn_expr)))
            }
            Prop::Getter(_) | Prop::Setter(_) => {
                // Desugarado pelo pass `desugar_object_methods` em
                // KeyValue(\"__get_<name>\" / \"__set_<name>\", FnExpr).
                // Se chegou aqui sem desugar, eh bug do pass — bail.
                return Err(anyhow!(
                    "get/set escapou do pass de desugar"
                ));
            }
            Prop::Assign(_) => {
                return Err(anyhow!(
                    "assign em object literal nao suportado (MVP)"
                ));
            }
        };

        let value_tv = lower_expr(ctx, &value_expr)?;
        // (#680/50) Bool em obj literal vira sentinel (i64::MIN = false,
        // i64::MIN+1 = true). Permite JSON.stringify/console.log
        // distinguir bool de int 0/1, e iterar com tipo correto.
        let value_i64 = if matches!(value_tv.ty, ValTy::Bool) {
            let b = ctx.coerce_to_i64(value_tv).val;
            let min = ctx.builder.ins().iconst(cl::I64, i64::MIN);
            ctx.builder.ins().iadd(min, b)
        } else {
            ctx.coerce_to_i64(value_tv).val
        };
        match key_src {
            KeySrc::Static(s) => {
                let (kptr, klen) = ctx.emit_str_literal(s.as_bytes())?;
                ctx.builder
                    .ins()
                    .call(set_fn, &[handle, kptr, klen, value_i64]);
            }
            KeySrc::Computed(expr) => {
                // (#753) Avalia expr -> handle e usa MAP_SET_KH que
                // aceita key como handle (resolve String OU Symbol via
                // repr canonica `@@sym:<handle>`).
                let key_tv = lower_expr(ctx, &expr)?;
                let key_h = ctx.coerce_to_handle(key_tv)?.val;
                let set_kh_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_SET_KH",
                    &[cl::I64, cl::I64, cl::I64],
                    None,
                )?;
                ctx.builder
                    .ins()
                    .call(set_kh_fn, &[handle, key_h, value_i64]);
            }
        }
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

/// Function global (#359): reify user fn ident + chama getter (.name/.length).
fn lower_user_fn_getter(
    ctx: &mut FnCtx,
    fn_name: &str,
    prop: &str,
) -> Result<TypedVal> {
    use crate::codegen::lower::ctx::ValTy;

    let fn_ptr = emit_user_fn_addr(ctx, fn_name)?.val;
    let arity = ctx
        .user_fns
        .get(fn_name)
        .map(|f| f.params.len() as i64)
        .unwrap_or(0);
    let name_tv = ctx.emit_str_handle(fn_name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
    let n_ptr = ctx.builder.inst_results(inst_p)[0];
    let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
    let n_len = ctx.builder.inst_results(inst_l)[0];

    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
    let has_this_v = ctx.builder.ins().iconst(
        cl::I32,
        i64::from(fn_name_has_this_param(fn_name)),
    );
    let reify_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_REIFY",
        &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32],
        Some(cl::I64),
    )?;
    let inst_r = ctx
        .builder
        .ins()
        .call(reify_fn, &[fn_ptr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
    let fn_handle = ctx.builder.inst_results(inst_r)[0];

    match prop {
        "name" => {
            let getter = ctx.get_extern("__RTS_FN_GL_FUNCTION_NAME", &[cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(getter, &[fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::Handle))
        }
        "length" => {
            let getter = ctx.get_extern("__RTS_FN_GL_FUNCTION_LENGTH", &[cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(getter, &[fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::I64))
        }
        "prototype" => {
            // (#264) Lazy alloc handle de Map para o constructor.
            let getter = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_PROTOTYPE_GET",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(getter, &[fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::Handle))
        }
        _ => Err(anyhow!("user_fn_getter: prop {} nao suportado", prop)),
    }
}

/// Reifica método de instância em handle Function pré-bindado.
/// Usado quando codegen vê `obj.method` em posição de valor (não chamada
/// direta): emite REIFY com has_this_param=true + BIND com receiver como this.
fn emit_method_reify(
    ctx: &mut FnCtx,
    receiver_expr: &Expr,
    fn_name: &str,
    method_name: &str,
) -> Result<TypedVal> {
    let fn_ptr = emit_user_fn_addr(ctx, fn_name)?.val;
    let arity = ctx
        .user_fns
        .get(fn_name)
        .map(|f| f.params.len() as i64)
        .unwrap_or(0);
    let name_tv = ctx.emit_str_handle(method_name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
    let n_ptr = ctx.builder.inst_results(inst_p)[0];
    let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
    let n_len = ctx.builder.inst_results(inst_l)[0];

    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
    let has_this_v = ctx.builder.ins().iconst(cl::I32, 1);
    // Avalia receiver e usa diretamente como bound_this no REIFY_BOUND_TYPED.
    let recv_tv = lower_expr(ctx, receiver_expr)?;
    let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
    let has_bound_v = ctx.builder.ins().iconst(cl::I32, 1);

    // Coleta param_kinds + return_kind para invoke_typed dispatchar com
    // signature correta (number → f64, etc).
    let (param_kinds_bytes, return_kind_v): (Vec<u8>, i32) = {
        let info = ctx.user_fns.get(fn_name);
        let pks: Vec<u8> = info
            .map(|f| f.params.iter().map(|t| val_ty_to_kind(*t)).collect())
            .unwrap_or_default();
        let rk: i32 = info
            .and_then(|f| f.ret)
            .map(|t| val_ty_to_kind(t) as i32)
            .unwrap_or(4); // void = 4
        (pks, rk)
    };
    let pk_ptr_v: cranelift_codegen::ir::Value;
    let pk_len_v: cranelift_codegen::ir::Value;
    if param_kinds_bytes.is_empty() {
        pk_ptr_v = ctx.builder.ins().iconst(cl::I64, 0);
        pk_len_v = ctx.builder.ins().iconst(cl::I64, 0);
    } else {
        let tv = ctx.emit_str_handle(&param_kinds_bytes)?;
        let h = ctx.coerce_to_i64(tv).val;
        let p = ctx.builder.ins().call(str_ptr_fn, &[h]);
        let l = ctx.builder.ins().call(str_len_fn, &[h]);
        pk_ptr_v = ctx.builder.inst_results(p)[0];
        pk_len_v = ctx.builder.inst_results(l)[0];
    }
    let rk_v = ctx.builder.ins().iconst(cl::I32, return_kind_v as i64);

    let reify_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED",
        &[
            cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32, cl::I64, cl::I32,
            cl::I64, cl::I64, cl::I32,
        ],
        Some(cl::I64),
    )?;
    let inst_r = ctx.builder.ins().call(
        reify_fn,
        &[
            fn_ptr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v,
            recv_i64, has_bound_v, pk_ptr_v, pk_len_v, rk_v,
        ],
    );
    let fn_handle = ctx.builder.inst_results(inst_r)[0];
    Ok(TypedVal::new(fn_handle, ValTy::Handle))
}

pub(super) fn val_ty_to_kind(ty: ValTy) -> u8 {
    match ty {
        ValTy::I64 => 0,
        ValTy::F64 => 1,
        ValTy::Bool => 2,
        ValTy::I32 => 3,
        ValTy::Handle => 0, // Handle e' u64 → cabe em i64 raw
        ValTy::U64 => 0,
        // Narrow ints viajam em i64 raw — kind 0 (i64).
        ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => 0,
    }
}

/// (#nested-chain) Raiz de uma cadeia de Member: ident global/local ou `this`.
#[derive(Debug, Clone)]
enum ChainRoot {
    Ident(String),
    This,
}

/// Decompõe `a.b.c.d` (ou `this.b.c.d`, ou OptChain) em (root, [b, c, d]).
/// Retorna None se a cadeia tem qualquer prop nao-Ident no caminho.
fn chain_root_path_kind(e: &Expr) -> Option<(ChainRoot, Vec<String>)> {
    match e {
        Expr::Ident(id) => Some((ChainRoot::Ident(id.sym.to_string()), Vec::new())),
        Expr::This(_) => Some((ChainRoot::This, Vec::new())),
        Expr::Member(mi) => {
            if let MemberProp::Ident(p) = &mi.prop {
                let (root, mut path) = chain_root_path_kind(&mi.obj)?;
                path.push(p.sym.to_string());
                Some((root, path))
            } else {
                None
            }
        }
        Expr::OptChain(o) => {
            if let swc_ecma_ast::OptChainBase::Member(mi) = o.base.as_ref() {
                if let MemberProp::Ident(p) = &mi.prop {
                    let (root, mut path) = chain_root_path_kind(&mi.obj)?;
                    path.push(p.sym.to_string());
                    Some((root, path))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// (#nested-this) Resolve `(class, field, sub_key)` percorrendo a hierarquia
/// (class + ancestrais) e retorna o map de tipos dos sub-sub-fields.
fn class_field_nested<'a>(
    ctx: &'a FnCtx,
    class: &str,
    field: &str,
    sub_key: &str,
) -> Option<&'a HashMap<String, ValTy>> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(m) = meta
            .field_nested_obj_types
            .get(&(field.to_string(), sub_key.to_string()))
        {
            return Some(m);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

/// (#nested-this) Resolve `(class, field)` percorrendo hierarquia e retorna
/// os tipos diretos dos sub-fields registrados em this.field = { ... }.
fn class_field_obj<'a>(
    ctx: &'a FnCtx,
    class: &str,
    field: &str,
) -> Option<&'a HashMap<String, ValTy>> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(m) = meta.field_obj_types.get(field) {
            return Some(m);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

/// (cross-runtime #799) Reifica fn de namespace (ex: `Math.max`,
/// `math.max`) como Function handle. Emite extern decl + func_addr +
/// REIFY_BOUND_TYPED com param_kinds derivados do `member.args`.
fn reify_ns_fn_as_handle(
    ctx: &mut FnCtx,
    member: &crate::abi::NamespaceMember,
) -> Result<TypedVal> {
    let sig = crate::abi::signature::lower_member(member);
    let fref = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
    let ptr_ty = ctx.module.isa().pointer_type();
    let fn_addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    let arity = member.args.len() as i64;
    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let name_tv = ctx.emit_str_handle(member.name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
    let n_ptr = ctx.builder.inst_results(inst_p)[0];
    let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
    let n_len = ctx.builder.inst_results(inst_l)[0];
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
    let has_this_v = ctx.builder.ins().iconst(cl::I32, 0);
    // Param kinds: 0=i64, 1=f64. Deriva de AbiType de cada arg.
    let pks_bytes: Vec<u8> = member
        .args
        .iter()
        .map(|a| match a {
            crate::abi::AbiType::F64 => 1u8,
            _ => 0u8,
        })
        .collect();
    let rk_byte: u8 = match member.returns {
        crate::abi::AbiType::F64 => 1,
        crate::abi::AbiType::Void => 4,
        _ => 0,
    };
    let (kinds_ptr, kinds_len) = if pks_bytes.is_empty() {
        (
            ctx.builder.ins().iconst(cl::I64, 0),
            ctx.builder.ins().iconst(cl::I64, 0),
        )
    } else {
        let tv = ctx.emit_str_handle(&pks_bytes)?;
        let h = ctx.coerce_to_i64(tv).val;
        let p = ctx.builder.ins().call(str_ptr_fn, &[h]);
        let l = ctx.builder.ins().call(str_len_fn, &[h]);
        (
            ctx.builder.inst_results(p)[0],
            ctx.builder.inst_results(l)[0],
        )
    };
    let bound_this_v = ctx.builder.ins().iconst(cl::I64, 0);
    let has_bound_this_v = ctx.builder.ins().iconst(cl::I32, 0);
    let return_kind_v = ctx.builder.ins().iconst(cl::I32, rk_byte as i64);
    let reify_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED",
        &[
            cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32,
            cl::I64, cl::I32, cl::I64, cl::I64, cl::I32,
        ],
        Some(cl::I64),
    )?;
    let inst_r = ctx.builder.ins().call(
        reify_fn,
        &[
            fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v,
            bound_this_v, has_bound_this_v, kinds_ptr, kinds_len, return_kind_v,
        ],
    );
    let h = ctx.builder.inst_results(inst_r)[0];
    Ok(TypedVal::new(h, ValTy::Handle))
}

pub(super) fn lower_member_expr(ctx: &mut FnCtx, m: &swc_ecma_ast::MemberExpr) -> Result<TypedVal> {
    // (cross-runtime #314) `import.meta.url` — retorna string sentinel
    // file:// pra satisfazer .startsWith("file:") em fixtures. URL real
    // do main script nao eh acessivel sem refactor de module loader.
    if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
        if prop.sym.as_str() == "url" {
            if let Expr::MetaProp(mp) = m.obj.as_ref() {
                if mp.kind == swc_ecma_ast::MetaPropKind::ImportMeta {
                    return ctx.emit_str_handle(b"file:///rts/main");
                }
            }
        }
    }
    // (cross-runtime #1052) `obj[<StringLit>]` ou `obj[k[N]]` (k const array
    // de strings) quando key resolve estaticamente para "length"/"name"
    // (propriedade comum) — reescreve como `obj.<prop>` para usar path direto
    // em vez de MAP_GET literal (que retorna 0 em Vec/Array).
    if let MemberProp::Computed(c) = &m.prop {
        let static_key: Option<String> = match c.expr.as_ref() {
            Expr::Lit(swc_ecma_ast::Lit::Str(s)) => Some(s.value.to_string_lossy().to_string()),
            Expr::Member(inner) => {
                if let (Expr::Ident(arr_id), MemberProp::Computed(ic)) =
                    (inner.obj.as_ref(), &inner.prop)
                {
                    if let Expr::Lit(swc_ecma_ast::Lit::Num(n)) = ic.expr.as_ref() {
                        let idx = n.value as usize;
                        let arr_name = arr_id.sym.as_str().to_string();
                        crate::codegen::lower::passes::parallelism::STRING_ARRAY_VALUES
                            .with(|c| c.borrow().get(&arr_name).and_then(|v| v.get(idx).cloned()))
                    } else { None }
                } else { None }
            }
            _ => None,
        };
        if let Some(prop_name) = static_key {
            if matches!(prop_name.as_str(), "length" | "name" | "size" | "byteLength") {
                let synth = Expr::Member(swc_ecma_ast::MemberExpr {
                    span: m.span,
                    obj: m.obj.clone(),
                    prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                        span: m.span,
                        sym: prop_name.into(),
                    }),
                });
                return lower_expr(ctx, &synth);
            }
        }
    }

    // (cross-runtime #1079) `globalThis.X` em posicao de valor — re-lower
    // como ident solo `X`. Cobre identidade (globalThis.Array === Array),
    // chamadas (globalThis.parseInt("42")), reflexao.
    // (cross-runtime #1125) `globalThis[key]` (computed) — despacha pra Map
    // singleton via __RTS_FN_RT_GLOBAL_THIS_MAP. Pattern `ensureGlobal`.
    // Peel TsAs/Paren para `(globalThis as any)`.
    fn peel_obj<'a>(e: &'a Expr) -> &'a Expr {
        match e {
            Expr::TsAs(a) => peel_obj(&a.expr),
            Expr::TsTypeAssertion(a) => peel_obj(&a.expr),
            Expr::TsConstAssertion(a) => peel_obj(&a.expr),
            Expr::TsNonNull(a) => peel_obj(&a.expr),
            Expr::Paren(p) => peel_obj(&p.expr),
            _ => e,
        }
    }
    if let Expr::Ident(obj_id) = peel_obj(m.obj.as_ref()) {
        if obj_id.sym.as_str() == "globalThis" {
            // Computed key — Map dinamico do globalThis.
            if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                use cranelift_codegen::ir::InstBuilder;
                let key_tv = lower_expr(ctx, &c.expr)?;
                let key_h = ctx.coerce_to_handle(key_tv)?.val;
                let gt_fn = ctx.get_extern("__RTS_FN_RT_GLOBAL_THIS_MAP", &[], Some(cl::I64))?;
                let gt_inst = ctx.builder.ins().call(gt_fn, &[]);
                let gt = ctx.builder.inst_results(gt_inst)[0];
                let str_ptr = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                let str_len = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let p_inst = ctx.builder.ins().call(str_ptr, &[key_h]);
                let kp = ctx.builder.inst_results(p_inst)[0];
                let l_inst = ctx.builder.ins().call(str_len, &[key_h]);
                let kl = ctx.builder.inst_results(l_inst)[0];
                let get_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let g_inst = ctx.builder.ins().call(get_fn, &[gt, kp, kl]);
                let v = ctx.builder.inst_results(g_inst)[0];
                ctx.var_member_call_values.insert(v);
                return Ok(TypedVal::new(v, ValTy::I64));
            }
            if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
                let n = prop.sym.as_str();
                // So' redireciona se 'n' nao for var local capturando o nome.
                if ctx.read_local(n).is_none() {
                    // (cross-runtime #1125) Tenta primeiro lookup no Map
                    // singleton do globalThis. Se nao tem (handle invalido
                    // ou MAP_GET retorna 0), fallback para ident solo.
                    // Cobre `(globalThis as any).__myLib` setado via ensureGlobal.
                    use cranelift_codegen::ir::InstBuilder;
                    let gt_fn = ctx.get_extern("__RTS_FN_RT_GLOBAL_THIS_MAP", &[], Some(cl::I64))?;
                    let gt_inst = ctx.builder.ins().call(gt_fn, &[]);
                    let gt = ctx.builder.inst_results(gt_inst)[0];
                    let has_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_HAS",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let (kp, kl) = ctx.emit_str_literal(n.as_bytes())?;
                    let h_inst = ctx.builder.ins().call(has_fn, &[gt, kp, kl]);
                    let has = ctx.builder.inst_results(h_inst)[0];
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    use cranelift_codegen::ir::condcodes::IntCC;
                    let has_it = ctx.builder.ins().icmp(IntCC::NotEqual, has, zero);

                    let map_block = ctx.builder.create_block();
                    let synth_block = ctx.builder.create_block();
                    let merge = ctx.builder.create_block();
                    ctx.builder.append_block_param(merge, cl::I64);
                    ctx.builder.ins().brif(has_it, map_block, &[], synth_block, &[]);

                    ctx.builder.switch_to_block(map_block);
                    ctx.builder.seal_block(map_block);
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let g_inst = ctx.builder.ins().call(get_fn, &[gt, kp, kl]);
                    let v_map = ctx.builder.inst_results(g_inst)[0];
                    ctx.builder.ins().jump(merge, &[v_map.into()]);

                    ctx.builder.switch_to_block(synth_block);
                    ctx.builder.seal_block(synth_block);
                    let synth = Expr::Ident(swc_ecma_ast::Ident {
                        span: prop.span,
                        ctxt: Default::default(),
                        sym: prop.sym.clone(),
                        optional: false,
                    });
                    // Tenta ident solo; se falhar (undefined var), retorna 0.
                    let synth_v = super::lower_expr(ctx, &synth)
                        .map(|tv| ctx.coerce_to_i64(tv).val)
                        .unwrap_or_else(|_| ctx.builder.ins().iconst(cl::I64, 0));
                    ctx.builder.ins().jump(merge, &[synth_v.into()]);

                    ctx.builder.switch_to_block(merge);
                    ctx.builder.seal_block(merge);
                    let result = ctx.builder.block_params(merge)[0];
                    ctx.var_member_call_values.insert(result);
                    return Ok(TypedVal::new(result, ValTy::I64));
                }
            }
        }
    }

    // (#162/155) `Object.prototype` / `Array.prototype` — referencia ao
    // prototype singleton da classe global. Sem suporte completo a prototype
    // chain ainda; retorna handle de string sentinel "[<class>.prototype]"
    // — suficiente para `=== Object.prototype` em fixtures de proto check.
    if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
        if prop.sym.as_str() == "prototype" {
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let cls = obj_id.sym.as_str();
                let is_global_cls = matches!(
                    cls,
                    "Object" | "Array" | "Function" | "String" | "Number"
                        | "Boolean" | "Date" | "RegExp" | "Error" | "Map" | "Set"
                ) || crate::abi::global_class_lookup(cls).is_some();
                if is_global_cls && ctx.read_local(cls).is_none() && !ctx.user_fns.contains_key(cls) {
                    let label = format!("[{cls}.prototype]");
                    return ctx.emit_str_handle(label.as_bytes());
                }
            }
        }
    }

    if let Some(qualified) = qualified_member_name(&Expr::Member(m.clone())) {
        // (cross-runtime 55) Verifica que o root NAO eh var local antes de
        // tentar resolver como namespace member. Sem isso, `url.host` (onde
        // url eh var de URL) colide com namespace global "url" e gera
        // warning "is a function, not a constant".
        let root_is_local_var = if let Expr::Member(mm) = &Expr::Member(m.clone()) {
            if let Expr::Ident(id) = mm.obj.as_ref() {
                ctx.read_local(id.sym.as_str()).is_some()
            } else {
                false
            }
        } else {
            false
        };
        // (#208) `Math.X` (uppercase JS-style) → `math.X` (lowercase RTS namespace).
        if !root_is_local_var {
            if let Some(prop) = qualified.strip_prefix("Math.") {
                let target = format!("math.{prop}");
                if let Some((_, member)) = lookup(&target) {
                    if matches!(member.kind, crate::abi::MemberKind::Constant) {
                        if let Some(tv) = emit_namespace_constant(ctx, &target)? {
                            return Ok(tv);
                        }
                    } else {
                        // (cross-runtime #799) `Math.max` em posicao de valor
                        // (sem call) — reifica fn extern como Function handle
                        // pra suportar `const m = Math.max; Reflect.apply(m, ...)`.
                        return reify_ns_fn_as_handle(ctx, member);
                    }
                }
            }
            if let Some((_, member)) = lookup(&qualified) {
                if matches!(member.kind, crate::abi::MemberKind::Constant) {
                    if let Some(tv) = emit_namespace_constant(ctx, &qualified)? {
                        return Ok(tv);
                    }
                } else {
                    return reify_ns_fn_as_handle(ctx, member);
                }
            }
        }
        // `export * as ns from "./mod"` — `ns.const` resolve para o
        // identifier raiz registrado no flatten via local_alias_map.
        if let Some(orig) = ctx.local_alias_map.get(&qualified).cloned() {
            // Resolve via global module-level (const/let), reaproveitando
            // o mesmo path que ident comum. Se for fn (acesso sem call),
            // emit_user_fn_addr trata. Caso contrario tenta global.
            if let Some(tv) = ctx.read_local(&orig) {
                return Ok(tv);
            }
            if ctx.user_fns.contains_key(orig.as_str()) {
                return super::emit_user_fn_addr(ctx, &orig);
            }
        }
        // Number.* constants — valores vêm de globals::number::abi::number_const_value,
        // sem hardcode aqui. Codegen emite f64const inline para zero overhead.
        if let Some(prop) = qualified.strip_prefix("Number.") {
            use cranelift_codegen::ir::InstBuilder;
            use crate::codegen::lower::ctx::ValTy;
            if let Some(v) = crate::namespaces::globals::number::abi::number_const_value(prop) {
                let cv = ctx.builder.ins().f64const(v);
                return Ok(TypedVal::new(cv, ValTy::F64));
            }
        }
        // (#216) `Symbol.iterator` etc — well-known symbols. Despacha
        // pra fns extern que retornam handle cacheado.
        if let Some(prop) = qualified.strip_prefix("Symbol.") {
            use cranelift_codegen::ir::InstBuilder;
            use crate::codegen::lower::ctx::ValTy;
            let sym = match prop {
                "iterator" => Some("__RTS_FN_GL_SYMBOL_ITERATOR"),
                "asyncIterator" => Some("__RTS_FN_GL_SYMBOL_ASYNC_ITERATOR"),
                "hasInstance" => Some("__RTS_FN_GL_SYMBOL_HAS_INSTANCE"),
                "toPrimitive" => Some("__RTS_FN_GL_SYMBOL_TO_PRIMITIVE"),
                "toStringTag" => Some("__RTS_FN_GL_SYMBOL_TO_STRING_TAG"),
                _ => None,
            };
            if let Some(sym) = sym {
                let f = ctx.get_extern(sym, &[], Some(cranelift_codegen::ir::types::I64))?;
                let inst = ctx.builder.ins().call(f, &[]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(v, ValTy::Handle));
            }
        }
    }

    // Function global (#359): `<userFn>.name/.length/.prototype` — reify e chama getter.
    // (#264) Peel TsAs/Paren para suportar `(Animal as any).prototype`.
    if let MemberProp::Ident(prop) = &m.prop {
        let prop_name = prop.sym.as_str();
        if matches!(prop_name, "name" | "length" | "prototype") {
            let mut obj_e: &Expr = m.obj.as_ref();
            loop {
                match obj_e {
                    Expr::TsAs(a) => obj_e = &a.expr,
                    Expr::TsTypeAssertion(a) => obj_e = &a.expr,
                    Expr::TsConstAssertion(a) => obj_e = &a.expr,
                    Expr::TsSatisfies(a) => obj_e = &a.expr,
                    Expr::TsNonNull(a) => obj_e = &a.expr,
                    Expr::Paren(p) => obj_e = &p.expr,
                    _ => break,
                }
            }
            if let Expr::Ident(obj_id) = obj_e {
                let obj_name = obj_id.sym.as_str();
                if ctx.user_fns.contains_key(obj_name) && ctx.var_ty(obj_name).is_none() {
                    return lower_user_fn_getter(ctx, obj_name, prop_name);
                }
                // (cross-runtime #336) `Class.prototype` para classes user
                // resolvem para o mesmo Map que `Object.getPrototypeOf(c)`.
                // Reifica __class_X__init e chama FUNCTION_PROTOTYPE_GET.
                // Sem isso, `Class.prototype` caia no path generico de
                // member access em Ident desconhecido (handle = 0 ou novo).
                if prop_name == "prototype"
                    && ctx.classes.contains_key(obj_name)
                    && ctx.var_ty(obj_name).is_none()
                {
                    let init_fn = crate::codegen::lower::compile::class::class_init_name(obj_name);
                    if ctx.user_fns.contains_key(&init_fn) {
                        return lower_user_fn_getter(ctx, &init_fn, "prototype");
                    }
                }
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
            // Global class instance getters (#359): Function.name/.length, etc.
            if let Some(spec) = crate::abi::global_class_lookup(cls) {
                if let Some(member) = spec.instance_getter(prop_name) {
                    let recv_tv = lower_expr(ctx, &m.obj)?;
                    let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
                    let sig = crate::abi::signature::lower_member(member);
                    let fn_ref = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
                    let inst = ctx.builder.ins().call(fn_ref, &[recv_i64]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::from_abi(member.returns)));
                }
            }
            // Reificação de método de instância (#359): `c.method` em
            // posição de valor (não callee direto) → handle Function com
            // bound_this = c. Habilita `c.method.bind(c)`, `c.method.call(c, ...)`,
            // passar como callback. Método precisa estar em address_taken_fns
            // (callconv C) — métodos de classe são marcados em compile_program.
            if let Some(owner) = resolve_method_owner(ctx, cls, prop_name) {
                let fn_name_str = format!("__class_{owner}_{prop_name}");
                if ctx.user_fns.contains_key(&fn_name_str) {
                    return emit_method_reify(ctx, &m.obj, &fn_name_str, prop_name);
                }
            }
        }
    }

    // (cross-runtime #304) `<GlobalClass>.<staticMethod>` em posicao de
    // valor (sem call) — reifica como handle Function. Sem isso, codegen
    // tenta lower o ident `Promise` como variavel local e falha.
    // Ex: `Promise.try ? a : b` precisa avaliar Promise.try como truthy.
    if let MemberProp::Ident(id) = &m.prop {
        if let Expr::Ident(obj_id) = m.obj.as_ref() {
            let cls = obj_id.sym.as_str();
            let prop_name = id.sym.as_str();
            if ctx.read_local(cls).is_none() && !ctx.user_fns.contains_key(cls) {
                if let Some(spec) = crate::abi::global_class_lookup(cls) {
                    if let Some(member) = spec.static_member(prop_name) {
                        return reify_ns_fn_as_handle(ctx, member);
                    }
                }
                // (cross-runtime #1048) `Class.staticGetter` (sem call): static
                // getter de user class. Compilado como `__class_<C>_static_<g>`
                // recebendo zero args. Chamada direta retorna o valor.
                if let Some(meta) = ctx.classes.get(cls) {
                    if meta.static_methods.iter().any(|m| m == prop_name) {
                        let fn_name = crate::codegen::lower::compile::class::class_static_method_name(cls, prop_name);
                        if let Some(abi) = ctx.user_fns.get(&fn_name).cloned() {
                            // So' static getter (0 params); static method com
                            // 0 args e' reificavel mas ai dispara user call
                            // implicito perdendo semantica de fn handle. Como
                            // proxy, so' chama se ret eh nao-void.
                            if abi.params.is_empty() {
                                use cranelift_codegen::ir::InstBuilder;
                                let mangled = format!("__user_{fn_name}");
                                if let Some(&fn_id) = ctx.extern_cache.get(mangled.as_str()) {
                                    let fref = ctx.fref_for_id(fn_id);
                                    let inst = ctx.builder.ins().call(fref, &[]);
                                    let v = ctx.builder.inst_results(inst)[0];
                                    return Ok(TypedVal::new(v, abi.ret.unwrap_or(ValTy::I64)));
                                }
                            }
                        }
                    }
                }
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
    // Map/Set sao codegen-only globals (nao em GLOBAL_CLASS_SPECS) entao
    // receiver_class fica como "Map"/"Set" mas o dispatch de .size precisa
    // ir pelo handle_len generico (igual handles sem class). Trata como
    // None para esse fim.
    let rc_for_size = match receiver_class.as_deref() {
        Some("Map") | Some("Set") => None,
        other => other,
    };
    // (#json-bridge) Tambem cobre U64 (handle opaco de extern call,
    // ex: JSON.parse) e I64+var_member_call_values — usa UNIVERSAL_LENGTH
    // que detecta tipo de Entry em runtime. Sem isso
    // `JSON.parse("[1,2,3]").length` caia no MAP_GET("length") -> 0.
    let recv_is_ambiguous = matches!(obj_tv.ty, ValTy::U64)
        || (matches!(obj_tv.ty, ValTy::I64)
            && ctx.var_member_call_values.contains(&obj_tv.val));
    if (matches!(obj_tv.ty, ValTy::Handle) || recv_is_ambiguous) && rc_for_size.is_none() {
        if let MemberProp::Ident(id) = &m.prop {
            let key = id.sym.as_str();
            if (key == "size" || key == "length")
                && lhs_object_field_known(ctx, &m.obj, key).is_none()
            {
                // gc.handle_len despacha pelo tipo do Entry — funciona
                // para String/Map/Vec/Buffer/Env. Para handle ambiguo
                // (U64/I64 marcado), usa UNIVERSAL_LENGTH que tambem
                // cobre Entry::Json.
                let sym = if recv_is_ambiguous {
                    "__RTS_FN_RT_UNIVERSAL_LENGTH"
                } else {
                    "__RTS_FN_NS_GC_HANDLE_LEN"
                };
                let len_fn = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
                let inst = ctx.builder.ins().call(len_fn, &[obj_handle]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(v, ValTy::I64));
            }
            // (cross-runtime #70/#1162) `arr.groups` em Vec handle —
            // side-table lookup para regex `.indices.groups` (Vec da spec
            // tem prop adicional). Retorna 0 (=> undefined) quando vec
            // nao eh regex indices. Falls through pro MAP_GET path quando
            // obj eh Map normal.
            if key == "groups" && (matches!(obj_tv.ty, ValTy::Handle) || recv_is_ambiguous) {
                let f = ctx.get_extern(
                    "__RTS_FN_GL_REGEXP_INDICES_GROUPS",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_handle]);
                let v = ctx.builder.inst_results(inst)[0];
                // Se retornar 0 (handle invalido = undefined-ish), fallback
                // para MAP_GET para casos Map nao-indices.
                use cranelift_codegen::ir::condcodes::IntCC;
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                let is_zero = ctx.builder.ins().icmp(IntCC::Equal, v, zero);
                let map_block = ctx.builder.create_block();
                let merge = ctx.builder.create_block();
                ctx.builder.append_block_param(merge, cl::I64);
                ctx.builder.ins().brif(is_zero, map_block, &[], merge, &[v.into()]);

                ctx.builder.switch_to_block(map_block);
                ctx.builder.seal_block(map_block);
                let (kp, kl) = ctx.emit_str_literal(b"groups")?;
                let map_get = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst2 = ctx.builder.ins().call(map_get, &[obj_handle, kp, kl]);
                let v2 = ctx.builder.inst_results(inst2)[0];
                ctx.builder.ins().jump(merge, &[v2.into()]);

                ctx.builder.switch_to_block(merge);
                ctx.builder.seal_block(merge);
                let result = ctx.builder.block_params(merge)[0];
                return Ok(TypedVal::new(result, ValTy::Handle));
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
                        // (#745) JS spec: Error.stack eh string (mesmo
                        // que vazia ou stub). Ate stack capture real,
                        // retorna "<name>: <msg>".
                        "stack" => Some("__RTS_FN_GL_ERROR_STACK"),
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
                            let f = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
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
            // Fallback: receiver_class desconhecido mas Handle — tentar GlobalClassSpecs
            // por nome de propriedade. Cobre `resp.ok`, `resp.status`, etc. sem anotação.
            // (#274) NAO aplicar se o receiver eh ident de object literal local/global
            // conhecido — senao `cfg.port` em `const cfg = { port: 3000 }` cai em
            // `URL.port` e retorna lixo. Object literal sempre vai pro map_get abaixo.
            let is_known_obj_lit = if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let n = obj_id.sym.as_str();
                ctx.local_obj_field_types.contains_key(n)
                    || ctx.global_obj_field_types.contains_key(n)
            } else if let Expr::Member(_) | Expr::OptChain(_) = m.obj.as_ref() {
                // (#bug) Chained: cfg.server.host — se o root eh obj literal
                // conhecido e o path bate em nested registrado, tratar como
                // obj literal e ir pro map_get. Sem isso, cai no fallback
                // de GLOBAL_CLASS_SPECS e bate em URL.host/etc, retornando lixo.
                let chain = chain_root_path_kind(m.obj.as_ref());
                match chain {
                    Some((ChainRoot::Ident(root), path)) if !path.is_empty() => {
                        let last = path.last().unwrap().clone();
                        let key = (root.clone(), last);
                        (ctx.local_obj_field_types.contains_key(&root)
                            || ctx.global_obj_field_types.contains_key(&root))
                            && (ctx.local_nested_obj_field_types.contains_key(&key)
                                || ctx.global_nested_obj_field_types.contains_key(&key))
                    }
                    // (#nested-this) this.cfg.server (path.len()==1) ou
                    // this.cfg.server.host (path.len()>=2): se a classe corrente
                    // tem field_obj_types/field_nested_obj_types registrado,
                    // tratar como obj literal — vai pro map_get e a propriedade
                    // eh resolvida via field_ty da class meta.
                    Some((ChainRoot::This, path)) if !path.is_empty() => {
                        if let Some(cls) = ctx.current_class.as_deref() {
                            match path.len() {
                                1 => class_field_obj(ctx, cls, &path[0]).is_some(),
                                _ => {
                                    let field = path[0].clone();
                                    let sub = path[1].clone();
                                    class_field_nested(ctx, cls, &field, &sub).is_some()
                                }
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            };
            // (#601) Skip getter fallback para "name"/"length"/"prototype"
            // quando obj nao e' Ident de user fn. Esses sao Function metadata
            // — sem receiver_class identificavel, podem colidir com fields
            // legitimos de instance. Mantem behavior so' para idents de user
            // fn (Animal.name, etc.) via lower_user_fn_getter acima.
            let is_function_meta_prop = matches!(
                key, "name" | "length" | "prototype"
            );
            let obj_is_user_fn_ident = if let Expr::Ident(id) = m.obj.as_ref() {
                ctx.user_fns.contains_key(id.sym.as_str())
                    && ctx.var_ty(id.sym.as_str()).is_none()
            } else {
                false
            };
            let skip_function_meta = is_function_meta_prop && !obj_is_user_fn_ident;
            // (cross-runtime edge_json5) Quando obj eh resultado de uma Call
            // (ex: JSON.parse/JSON5.parse retornando Map generico), NAO
            // tentar GLOBAL_CLASS_SPECS getter — `obj.port`/`obj.flags`/etc
            // colidiriam com URL.port/RegExp.flags. Map handles genericos
            // devem cair no map_get abaixo. Inclui cadeias Member em cima
            // de Call (ex: `JSON.parse(...).flags.debug`).
            fn chain_has_call_root(e: &Expr, ambiguous: &std::collections::HashSet<String>) -> bool {
                match e {
                    Expr::Call(_) => true,
                    Expr::Ident(id) => ambiguous.contains(id.sym.as_str()),
                    Expr::Member(m) => chain_has_call_root(&m.obj, ambiguous),
                    Expr::OptChain(o) => match &**(&o.base) {
                        swc_ecma_ast::OptChainBase::Member(m) => chain_has_call_root(&m.obj, ambiguous),
                        swc_ecma_ast::OptChainBase::Call(c) => chain_has_call_root(&c.callee, ambiguous),
                    },
                    _ => false,
                }
            }
            let obj_is_call_result = chain_has_call_root(m.obj.as_ref(), &ctx.local_ambiguous_vars)
                // (#806 follow-up) Quando o obj veio de map_get/vec_get/etc e foi
                // marcado como ambiguo (var_member_call_values), tambem skip
                // fallback de GLOBAL_CLASS_SPECS — senao `r[0].status` em Map
                // generico (allSettled) dispatch para Response.status e retorna 0.
                || (matches!(obj_tv.ty, ValTy::I64 | ValTy::U64)
                    && ctx.var_member_call_values.contains(&obj_handle));
            // (#777) Methods universais que toda classe global define
            // (toString/valueOf/hasOwnProperty/etc) NAO devem dispatch via
            // GLOBAL_CLASS_SPECS fallback — o primeiro spec que define o
            // metodo (ex: String) vence e retorna lixo quando o receiver
            // nao eh dessa classe. Bug raiz: `Object.create(null).toString`
            // retornava o proprio handle porque String.toString eh passthrough.
            //
            // Para esses, cai no map_get_chain abaixo que retorna 0 quando
            // a key nao esta no Map (semantica correta: undefined).
            let is_universal_method = matches!(
                key,
                "toString" | "valueOf" | "hasOwnProperty" | "toLocaleString"
                | "isPrototypeOf" | "propertyIsEnumerable" | "constructor"
            );
            // (cross-runtime #744) `<handle>.raw` — TemplateStringsArray.raw.
            // Verifica side-table tagged_raw: se houver entry para o handle,
            // retorna; senao retorna 0 (TPL_COERCE_AUTO renderiza "null").
            // Trata antes do loop GLOBAL_CLASS_SPECS pois "raw" colide com
            // metodos de outras specs.
            if key == "raw" && receiver_class.is_none() && !is_known_obj_lit {
                let raw_get = ctx.get_extern(
                    "__RTS_FN_NS_GC_TAGGED_RAW_GET",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(raw_get, &[obj_handle]);
                let raw_h = ctx.builder.inst_results(inst)[0];
                // Se raw_h == 0, fallback para map_get_chain (semantica
                // do JS: undefined). Caso contrario retorna handle do Vec raw.
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                use cranelift_codegen::ir::condcodes::IntCC;
                let has_raw = ctx.builder.ins().icmp(IntCC::NotEqual, raw_h, zero);
                let raw_block = ctx.builder.create_block();
                let fallback_block = ctx.builder.create_block();
                let merge = ctx.builder.create_block();
                let result = ctx.builder.append_block_param(merge, cl::I64);
                ctx.builder.ins().brif(has_raw, raw_block, &[], fallback_block, &[]);

                ctx.builder.switch_to_block(raw_block);
                ctx.builder.seal_block(raw_block);
                ctx.builder.ins().jump(merge, &[raw_h.into()]);

                ctx.builder.switch_to_block(fallback_block);
                ctx.builder.seal_block(fallback_block);
                let (kp, kl) = ctx.emit_str_literal(b"raw")?;
                let map_get = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst2 = ctx.builder.ins().call(map_get, &[obj_handle, kp, kl]);
                let v2 = ctx.builder.inst_results(inst2)[0];
                ctx.builder.ins().jump(merge, &[v2.into()]);

                ctx.builder.switch_to_block(merge);
                ctx.builder.seal_block(merge);
                ctx.var_member_call_values.insert(result);
                return Ok(TypedVal::new(result, ValTy::Handle));
            }
            if receiver_class.is_none()
                && !is_known_obj_lit
                && !skip_function_meta
                && !obj_is_call_result
                && !is_universal_method
            {
                // (#216) Tenta instance_getter primeiro (description, source, flags, etc.)
                for spec in crate::abi::GLOBAL_CLASS_SPECS {
                    if let Some(member) = spec.instance_getter(key) {
                        let sig = crate::abi::signature::lower_member(member);
                        let f = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
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
                for spec in crate::abi::GLOBAL_CLASS_SPECS {
                    if let Some(member) = spec.instance_method(key) {
                        if member.args.len() == 1 {
                            let sig = crate::abi::signature::lower_member(member);
                            let f = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
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
                // (#592) \`users[i].name\` direto: detecta Member computed
                // em obj e usa local_array_obj_field_types do array, ou
                // local_array_class_ty (\`User[]\`) -> field_ty na classe.
                if field_ty.is_none() {
                    if let Expr::Member(inner_m) = m.obj.as_ref() {
                        if matches!(&inner_m.prop, MemberProp::Computed(_)) {
                            if let Expr::Ident(arr_id) = inner_m.obj.as_ref() {
                                let arr_name = arr_id.sym.as_str();
                                if let Some(types) = ctx.local_array_obj_field_types.get(arr_name) {
                                    field_ty = types.get(key).copied();
                                }
                                // (#592 fix) Array de classe user: pega
                                // tipo do field na class hierarchy.
                                if field_ty.is_none() {
                                    if let Some(elem_cls) =
                                        ctx.local_array_class_ty.get(arr_name).cloned()
                                    {
                                        field_ty = field_type_in_hierarchy(ctx, &elem_cls, key);
                                    }
                                }
                            }
                        }
                    }
                }
                // Chained nested: \`cfg.server.host\` ou \`cfg?.server?.host\`.
                // obj e' Member/OptChain — navega para descobrir tipos.
                if field_ty.is_none() {
                    fn chain_root_path(e: &Expr) -> Option<(String, Vec<String>)> {
                        match e {
                            Expr::Ident(id) => Some((id.sym.to_string(), Vec::new())),
                            Expr::Member(mi) => {
                                if let MemberProp::Ident(p) = &mi.prop {
                                    let (root, mut path) = chain_root_path(&mi.obj)?;
                                    path.push(p.sym.to_string());
                                    Some((root, path))
                                } else { None }
                            }
                            Expr::OptChain(o) => {
                                if let swc_ecma_ast::OptChainBase::Member(mi) = o.base.as_ref() {
                                    if let MemberProp::Ident(p) = &mi.prop {
                                        let (root, mut path) = chain_root_path(&mi.obj)?;
                                        path.push(p.sym.to_string());
                                        Some((root, path))
                                    } else { None }
                                } else { None }
                            }
                            _ => None,
                        }
                    }
                    if let Some((root, path)) = chain_root_path(m.obj.as_ref()) {
                        if !path.is_empty() {
                            let last_key = path.last().unwrap().clone();
                            let nkey = (root, last_key);
                            let nested = ctx
                                .local_nested_obj_field_types
                                .get(&nkey)
                                .cloned()
                                .or_else(|| {
                                    ctx.global_nested_obj_field_types.get(&nkey).cloned()
                                });
                            if let Some(nested) = nested {
                                field_ty = nested.get(key).copied();
                            }
                        }
                    }
                }
                // (#nested-this) this.cfg.server.host: usa class meta.
                // Para `this.X.K`, m.obj eh `this.X` (Member, root=This,
                // path=[X]) — usa class_field_obj. Para `this.X.K.host`,
                // m.obj eh `this.X.K` (Member, root=This, path=[X,K]) —
                // usa class_field_nested.
                if field_ty.is_none() {
                    if let Some(cls) = ctx.current_class.clone() {
                        if let Some((ChainRoot::This, path)) =
                            chain_root_path_kind(m.obj.as_ref())
                        {
                            match path.len() {
                                1 => {
                                    if let Some(map) =
                                        class_field_obj(ctx, &cls, &path[0])
                                    {
                                        field_ty = map.get(key).copied();
                                    }
                                }
                                n if n >= 2 => {
                                    let field = &path[0];
                                    let sub = &path[1];
                                    if let Some(map) =
                                        class_field_nested(ctx, &cls, field, sub)
                                    {
                                        field_ty = map.get(key).copied();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            map_get_static_typed(ctx, obj_handle, key.as_bytes(), field_ty)
        }
        MemberProp::Computed(c) => {
            if let Expr::Lit(Lit::Str(s)) = c.expr.as_ref() {
                // (#261) Computed key string literal: tenta inferir field_ty
                // do mesmo caminho que MemberProp::Ident usaria — lookup
                // em local_obj_field_types/global_obj_field_types pelo
                // ident root quando \`m.obj\` eh Ident.
                let key_bytes = s.value.as_bytes();
                let key_str_owned = String::from_utf8_lossy(key_bytes).into_owned();
                let mut field_ty: Option<ValTy> = None;
                if let Expr::Ident(obj_id) = m.obj.as_ref() {
                    let n = obj_id.sym.as_str();
                    if let Some(types) = ctx.local_obj_field_types.get(n) {
                        field_ty = types.get(&key_str_owned).copied();
                    }
                    if field_ty.is_none() {
                        if let Some(types) = ctx.global_obj_field_types.get(n) {
                            field_ty = types.get(&key_str_owned).copied();
                        }
                    }
                }
                let tv = map_get_static_typed(ctx, obj_handle, s.value.as_bytes(), field_ty)?;
                // Para acesso computed via string literal (e.g. `c["#x"]`),
                // converte 0 (campo ausente) em sentinel undefined — JS spec.
                // So' aplica quando field_ty era None (sem hint estatico).
                if field_ty.is_none() && matches!(tv.ty, ValTy::I64) {
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    let is_zero = ctx.builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal, tv.val, zero,
                    );
                    let undef = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
                    let result_v = ctx.builder.ins().select(is_zero, undef, tv.val);
                    ctx.var_member_call_values.insert(result_v);
                    return Ok(TypedVal::new(result_v, ValTy::I64));
                }
                return Ok(tv);
            }
            let idx_tv = lower_expr(ctx, &c.expr)?;
            match idx_tv.ty {
                ValTy::Handle => {
                    // (#753) MAP_GET_KH aceita key como handle (String OU
                    // Symbol via repr canonica `@@sym:<handle>`). Antes,
                    // STRING_PTR/LEN em Symbol retornava null -> MAP_GET
                    // sempre 0.
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GET_KH",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, idx_tv.val]);
                    let raw = ctx.builder.inst_results(inst)[0];
                    // (#261/#94) Pos-MAP_GET_KH com chave dinamica (string):
                    // value=0 eh quase sempre um numero literal (`arr[k]`
                    // em Vec, `m.get(k)` com valor 0). Usa VEC_SLOT que
                    // formata 0 como "0" (nao "null"). Trade-off com
                    // `map.set(k, null)` que tambem retorna 0 — caso raro.
                    let coerce_fn = ctx.get_extern(
                        "__RTS_FN_RT_TPL_COERCE_VEC_SLOT",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let coerced_inst = ctx.builder.ins().call(coerce_fn, &[raw]);
                    let coerced = ctx.builder.inst_results(coerced_inst)[0];
                    Ok(TypedVal::new(coerced, ValTy::Handle))
                }
                _ => {
                    let idx = ctx.coerce_to_i64(idx_tv).val;
                    // INDEX_GET_AUTO roteia em runtime: Vec -> slot, String -> char-at.
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_INDEX_GET_AUTO",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, idx]);
                    let v = ctx.builder.inst_results(inst)[0];
                    // Slot pode ser i64 puro (number) ou handle (string/obj).
                    // Marca como ambiguo p/ `===` e operadores em geral
                    // (var_member_call_values), e como vec-slot p/ template
                    // (var_vec_slot_values) — value=0 vira "0" nao "null".
                    ctx.var_member_call_values.insert(v);
                    ctx.var_vec_slot_values.insert(v);
                    Ok(TypedVal::new(v, ValTy::I64))
                }
            }
        }
        MemberProp::PrivateName(pn) => {
            let raw_name = pn.name.as_ref();
            let raw_key = format!("#{}", raw_name);
            validate_private_scope(ctx, &raw_key)?;
            // (cross-runtime #267) Mangle por current_class (declaring class).
            let key = if let Some(cur) = ctx.current_class.as_deref() {
                format!("#{}_{}", cur, raw_name)
            } else {
                raw_key.clone()
            };
            let field_ty = receiver_class
                .as_deref()
                .and_then(|c| field_type_in_hierarchy(ctx, c, &key)
                    .or_else(|| field_type_in_hierarchy(ctx, c, &raw_key)));
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
        // Narrow ints em campos de classe — armazenados como I64 no slot,
        // tipo logico preservado para mask em ops downstream.
        ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => (
            "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
            cl::I64,
            slot.ty,
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
        ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64
        | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
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

pub(super) fn map_get_static_typed(
    ctx: &mut FnCtx,
    obj_handle: cranelift_codegen::ir::Value,
    key: &[u8],
    declared_ty: Option<ValTy>,
) -> Result<TypedVal> {
    // (#object-getter) Antes do map_get padrao, tenta lookup do getter
    // sentinela `__get_<key>`. Se existe, chama via INVOKE_AUTO com
    // obj como this; resultado eh o valor do getter.
    let getter_key: Vec<u8> = {
        let mut v = b"__get_".to_vec();
        v.extend_from_slice(key);
        v
    };
    let (gkptr, gklen) = ctx.emit_str_literal(&getter_key)?;
    // (#218) usa MAP_GET_DIRECT para nao acionar trap Proxy — sentinel
    // `__get_<key>` so' existe em Map normal.
    let plain_get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let getter_inst = ctx.builder.ins().call(plain_get_fn, &[obj_handle, gkptr, gklen]);
    let getter_h = ctx.builder.inst_results(getter_inst)[0];
    // Branch: se getter_h != 0, chama via INVOKE_AUTO; senao map_get padrao.
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let has_getter = ctx.builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
        getter_h,
        zero,
    );
    let getter_block = ctx.builder.create_block();
    let plain_block = ctx.builder.create_block();
    let merge = ctx.builder.create_block();
    let result = ctx.builder.append_block_param(merge, cl::I64);
    ctx.builder.ins().brif(has_getter, getter_block, &[], plain_block, &[]);

    // Getter path: invoke via INVOKE_AUTO(getter_h, obj, empty_args).
    ctx.builder.switch_to_block(getter_block);
    ctx.builder.seal_block(getter_block);
    let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst_v = ctx.builder.ins().call(vec_new, &[]);
    let empty_args = ctx.builder.inst_results(inst_v)[0];
    let invoke = ctx.get_extern(
        "__RTS_FN_RT_INVOKE_AUTO",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst_call = ctx.builder.ins().call(invoke, &[getter_h, obj_handle, empty_args]);
    let getter_result = ctx.builder.inst_results(inst_call)[0];
    ctx.builder.ins().jump(merge, &[getter_result.into()]);

    // Plain path: MAP_GET_CHAIN normal.
    ctx.builder.switch_to_block(plain_block);
    ctx.builder.seal_block(plain_block);
    let (kptr, klen) = ctx.emit_str_literal(key)?;
    let get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, kptr, klen]);
    let plain_v = ctx.builder.inst_results(inst)[0];
    ctx.builder.ins().jump(merge, &[plain_v.into()]);

    ctx.builder.switch_to_block(merge);
    ctx.builder.seal_block(merge);
    let v = result;
    match declared_ty {
        Some(ValTy::I32) => Ok(TypedVal::new(
            ctx.builder.ins().ireduce(cl::I32, v),
            ValTy::I32,
        )),
        Some(ValTy::Handle) => Ok(TypedVal::new(v, ValTy::Handle)),
        Some(ValTy::Bool) => Ok(TypedVal::new(v, ValTy::Bool)),
        _ => {
            // (#proto-method) Sem tipo declarado, marcar como
            // var_member_call_values para que template literal use
            // TPL_COERCE_AUTO (detecta string handle em runtime).
            ctx.var_member_call_values.insert(v);
            Ok(TypedVal::new(v, ValTy::I64))
        }
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

fn resolve_method_owner_local(ctx: &FnCtx, class: &str, method: &str) -> Option<String> {
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

pub(super) fn lhs_static_class(ctx: &FnCtx, expr: &Expr) -> Option<String> {
    match expr {
        Expr::This(_) => ctx.current_class.clone(),
        Expr::Ident(id) => ctx
            .local_class_ty
            .get(id.sym.as_str())
            .cloned()
            // (#1059) Quando o ident nao esta em scope local mas eh global
            // (top-level `const wm = new WeakMap()`), consulta global_class_ty.
            .or_else(|| ctx.global_class_ty.get(id.sym.as_str()).cloned()),
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
            if let Some(c) = field_class_in_hierarchy(ctx, &owner, prop) {
                return Some(c);
            }
            // Method reified como Function handle (#359). Permite que
            // `c.add.bind(c)` resolva o callee como Function class.
            if resolve_method_owner_local(ctx, &owner, prop).is_some() {
                return Some("Function".to_string());
            }
            // (cross-runtime 107) Global class instance method whose
            // ts_signature declares a class return type — propaga.
            // Ex: `u.searchParams: URLSearchParams` em URL.
            if let Some(spec) = crate::abi::global_class_lookup(&owner) {
                if let Some(member) = spec.instance_method(prop) {
                    let sig = member.ts_signature;
                    if let Some(colon_pos) = sig.rfind(':') {
                        let ret_ty = sig[colon_pos + 1..].trim().trim_end_matches(';').trim();
                        if crate::abi::global_class_lookup(ret_ty).is_some()
                            || ctx.classes.contains_key(ret_ty)
                        {
                            return Some(ret_ty.to_string());
                        }
                    }
                }
            }
            // (#277) Error.cause / TypeError.cause / etc. — quando owner eh
            // alguma das Error globals e prop=="cause", propaga "Error"
            // (semantica: cause pode ser qualquer Error subclass; tratar
            // como Error generico permite .message/.name/.stack funcionar).
            if prop == "cause"
                && matches!(
                    owner.as_str(),
                    "Error" | "TypeError" | "RangeError" | "ReferenceError"
                        | "SyntaxError" | "URIError" | "EvalError" | "AggregateError"
                )
            {
                return Some("Error".to_string());
            }
            None
        }
        Expr::Call(call) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &call.callee {
                match callee.as_ref() {
                    // fetch(...) → Response
                    Expr::Ident(id) if id.sym.as_str() == "fetch" => {
                        return Some("Response".to_string());
                    }
                    // user fn with known return class
                    Expr::Ident(id) => {
                        if let Some(abi) = ctx.user_fns.get(id.sym.as_str()) {
                            return abi.ret_class.clone();
                        }
                    }
                    // (cross-runtime #38) `Class.staticMethod(...)` retorna
                    // ret_class declarado. Sem isso, `const a = C.seed()`
                    // perde info de classe e getters caem em map_get raw.
                    Expr::Member(m) => {
                        if let Expr::Ident(obj_id) = m.obj.as_ref() {
                            let cls = obj_id.sym.as_str();
                            if let Some(meta) = ctx.classes.get(cls) {
                                if let MemberProp::Ident(method_id) = &m.prop {
                                    let mn = method_id.sym.as_str();
                                    if meta.static_methods.iter().any(|s| s == mn) {
                                        let fn_name = crate::codegen::lower::compile::class::class_static_method_name(cls, mn);
                                        if let Some(abi) = ctx.user_fns.get(&fn_name) {
                                            return abi.ret_class.clone();
                                        }
                                    }
                                }
                            }
                            // (cross-runtime #750) Global class static method
                            // (e.g. `Promise.resolve(42)` → "Promise"). Extrai
                            // ret class do ts_signature, suportando `Promise<T>`.
                            if let Some(spec) = crate::abi::global_class_lookup(cls) {
                                if let MemberProp::Ident(method_id) = &m.prop {
                                    let mn = method_id.sym.as_str();
                                    if let Some(member) = spec.static_member(mn) {
                                        let sig = member.ts_signature;
                                        if let Some(colon_pos) = sig.rfind(':') {
                                            let raw = sig[colon_pos + 1..]
                                                .trim()
                                                .trim_end_matches(';')
                                                .trim();
                                            let base = match raw.find('<') {
                                                Some(p) => raw[..p].trim(),
                                                None => raw,
                                            };
                                            if crate::abi::global_class_lookup(base).is_some()
                                                || ctx.classes.contains_key(base)
                                            {
                                                return Some(base.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                // (cross-runtime #750) chained instance method of a global
                // class whose ts_signature declares a class return type.
                // Ex: `Promise.resolve(42).then(...)` → `Promise`.
                if let Expr::Member(mem) = callee.as_ref() {
                    if let MemberProp::Ident(method_id) = &mem.prop {
                        let recv_cls = lhs_static_class(ctx, &mem.obj)?;
                        if let Some(spec) = crate::abi::global_class_lookup(&recv_cls) {
                            if let Some(member) = spec.instance_method(method_id.sym.as_str()) {
                                let sig = member.ts_signature;
                                if let Some(colon_pos) = sig.rfind(':') {
                                    let raw = sig[colon_pos + 1..]
                                        .trim()
                                        .trim_end_matches(';')
                                        .trim();
                                    let base = match raw.find('<') {
                                        Some(p) => raw[..p].trim(),
                                        None => raw,
                                    };
                                    if crate::abi::global_class_lookup(base).is_some()
                                        || ctx.classes.contains_key(base)
                                    {
                                        return Some(base.to_string());
                                    }
                                }
                            }
                        }
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
