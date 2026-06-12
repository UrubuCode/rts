mod builtins;
mod class_dispatch;
mod coerce;
mod indirect;
mod new_expr;
mod ns_call;
mod super_calls;

use self::indirect::{
    lower_curry_call, lower_indirect_call, lower_var_member_call,
};
use self::new_expr::{lower_function_handle_method, lower_function_method_call};

pub(super) use self::new_expr::lower_new;

use self::builtins::{
    lower_array_builtin, lower_console_call, lower_map_set_builtin, lower_math_builtin,
    lower_string_builtin,
};
use self::ns_call::{
    lower_global_instance_call, lower_node_ns_call,
    lower_ns_call, lower_ns_call_member,
};
use self::super_calls::{lower_super_call, lower_super_method_call};

pub(super) use self::ns_call::emit_namespace_constant;
pub(super) use self::super_calls::{lower_super_prop_assign, lower_super_prop_read};

pub(super) use class_dispatch::{
    AccessorKind, emit_virtual_accessor_dispatch,
    fn_name_has_this_param, lower_class_method_call_with_recv, resolve_method_owner, resolve_setter_owner,
};

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use cranelift_module::Module;
use swc_ecma_ast::{CallExpr, Callee, Expr, MemberProp};

use self::coerce::{
    lower_coerce_is_finite, lower_coerce_is_nan, lower_coerce_to_boolean, lower_coerce_to_number,
    lower_coerce_to_string,
};

use crate::abi::lookup;

use super::lower_expr;
use super::members::{
    lhs_static_class,
    qualified_member_name,
};
use super::operators::to_f64;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::compile::class::class_static_method_name;

/// (#json-bool) Tenta serializar um literal AST diretamente para JSON
/// em compile time. Suporta: Lit (bool/num/str/null), Object literal e
/// Array literal recursivamente. Retorna None quando encontra qualquer
/// expressao nao-literal (var, call, etc.) — caller cai no caminho
/// runtime que perde tipo de bool/null escalar dentro de containers.
pub(super) fn try_const_stringify(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(lit) => match lit {
            swc_ecma_ast::Lit::Bool(b) => Some(if b.value { "true".into() } else { "false".into() }),
            swc_ecma_ast::Lit::Null(_) => Some("null".into()),
            swc_ecma_ast::Lit::Num(n) => {
                // JS spec: NaN/Infinity -> null em JSON.
                if n.value.is_nan() || n.value.is_infinite() {
                    Some("null".into())
                } else if n.value == n.value.trunc() && n.value.abs() < 1e21 {
                    Some(format!("{}", n.value as i64))
                } else {
                    Some(format!("{}", n.value))
                }
            }
            swc_ecma_ast::Lit::Str(s) => Some(json_escape_str(&s.value.to_string_lossy())),
            _ => None,
        },
        Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus) => {
            // -42 -> "-42"
            if let Expr::Lit(swc_ecma_ast::Lit::Num(n)) = u.arg.as_ref() {
                let v = -n.value;
                if v.is_nan() || v.is_infinite() {
                    return Some("null".into());
                }
                if v == v.trunc() && v.abs() < 1e21 {
                    return Some(format!("{}", v as i64));
                }
                return Some(format!("{}", v));
            }
            None
        }
        Expr::Object(obj) => {
            let mut out = String::from("{");
            let mut first = true;
            for prop in &obj.props {
                let p = match prop {
                    swc_ecma_ast::PropOrSpread::Prop(p) => p,
                    _ => return None, // spread nao constante
                };
                let (key, val_expr) = match p.as_ref() {
                    swc_ecma_ast::Prop::KeyValue(kv) => {
                        let k = match &kv.key {
                            swc_ecma_ast::PropName::Ident(id) => id.sym.to_string(),
                            swc_ecma_ast::PropName::Str(s) => s.value.to_string_lossy().to_string(),
                            swc_ecma_ast::PropName::Num(n) => n.value.to_string(),
                            _ => return None,
                        };
                        (k, kv.value.as_ref())
                    }
                    _ => return None, // method/getter/setter/shorthand bail
                };
                let val_json = try_const_stringify(val_expr)?;
                if !first { out.push(','); }
                first = false;
                out.push_str(&json_escape_str(&key));
                out.push(':');
                out.push_str(&val_json);
            }
            out.push('}');
            Some(out)
        }
        Expr::Array(arr) => {
            let mut out = String::from("[");
            let mut first = true;
            for elem in &arr.elems {
                let e = match elem {
                    Some(e) => e,
                    None => return None, // sparse array bail
                };
                if e.spread.is_some() { return None; }
                let val_json = try_const_stringify(&e.expr)?;
                if !first { out.push(','); }
                first = false;
                out.push_str(&val_json);
            }
            out.push(']');
            Some(out)
        }
        _ => None,
    }
}

fn json_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn lower_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    if matches!(&call.callee, Callee::Super(_)) {
        return lower_super_call(ctx, call);
    }
    // Dynamic import(expr) — lowers to runtime.eval_file(path).
    if matches!(&call.callee, Callee::Import(_)) {
        return lower_dynamic_import(ctx, call);
    }
    if let Callee::Expr(callee) = &call.callee {
        // (generators) `__RTS_GEN_FINISH(buf, ret)` — sentinela do
        // generator_desugar no `return`. Registra o ret_value (devolvido pelo
        // `.next()` ao esgotar) e retorna o Vec `buf`.
        if let Expr::Ident(id) = callee.as_ref() {
            // (#195 mutable closures) cell intrinsics emitted by the
            // box_mutable_captures pass. A captured-AND-mutated local lives in a
            // heap cell; reads/writes go through these so capturers share state.
            match id.sym.as_str() {
                "__cell_new" if call.args.len() == 1 => {
                    use cranelift_codegen::ir::types as cl;
                    let init_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let init = ctx.coerce_to_i64(init_tv).val;
                    let f = ctx.get_extern("__RTS_FN_RT_CELL_NEW", &[cl::I64], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(f, &[init]);
                    let h = ctx.builder.inst_results(inst)[0];
                    ctx.declare_gc_handle(h);
                    return Ok(TypedVal::new(h, ValTy::Handle));
                }
                "__cell_get" if call.args.len() == 1 => {
                    use cranelift_codegen::ir::types as cl;
                    let cell_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let cell = ctx.coerce_to_i64(cell_tv).val;
                    let f = ctx.get_extern("__RTS_FN_RT_CELL_GET", &[cl::I64], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(f, &[cell]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::I64));
                }
                "__cell_set" if call.args.len() == 2 => {
                    use cranelift_codegen::ir::types as cl;
                    let cell_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let cell = ctx.coerce_to_i64(cell_tv).val;
                    let val_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let val = ctx.coerce_to_i64(val_tv).val;
                    let f = ctx.get_extern("__RTS_FN_RT_CELL_SET", &[cl::I64, cl::I64], None)?;
                    ctx.builder.ins().call(f, &[cell, val]);
                    // Assignment expression evaluates to the assigned value.
                    return Ok(TypedVal::new(val, ValTy::I64));
                }
                _ => {}
            }
            if id.sym.as_str() == "__RTS_GEN_FINISH" && call.args.len() == 2 {
                use cranelift_codegen::ir::types as cl;
                let buf_tv = lower_expr(ctx, &call.args[0].expr)?;
                let buf = ctx.coerce_to_i64(buf_tv).val;
                let ret_tv = lower_expr(ctx, &call.args[1].expr)?;
                let ret = ctx.coerce_to_i64(ret_tv).val;
                let set_ret = ctx.get_extern(
                    "__RTS_FN_NS_GC_GENERATOR_SET_RET",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(set_ret, &[buf, ret]);
                let h = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(h, ValTy::Handle));
            }
            // (#275/#379) `__RTS_GEN_GET_RET(vec)` — sentinela do
            // generator_desugar p/ `const r = yield* gen()`: devolve o
            // ret_value (`return X`) registrado pela generator delegada.
            if id.sym.as_str() == "__RTS_GEN_GET_RET" && call.args.len() == 1 {
                use cranelift_codegen::ir::types as cl;
                let vec_tv = lower_expr(ctx, &call.args[0].expr)?;
                let vec = ctx.coerce_to_i64(vec_tv).val;
                let get_ret = ctx.get_extern(
                    "__RTS_FN_NS_GC_GENERATOR_GET_RET",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(get_ret, &[vec]);
                let v = ctx.builder.inst_results(inst)[0];
                // ret_value eh ambiguo (i64/handle); marca para coercao auto.
                ctx.var_member_call_values.insert(v);
                return Ok(TypedVal::new(v, ValTy::I64));
            }
            // (#477) Sentinelas da state-machine de generators. Roteiam para os
            // simbolos runtime reais `__RTS_FN_NS_GC_GEN_SM_*`.
            if let Some((sym, ret)) = gen_sm_sentinel(id.sym.as_str(), call.args.len()) {
                use cranelift_codegen::ir::types as cl;
                let mut argv: Vec<_> = Vec::with_capacity(call.args.len());
                let sig: Vec<cl::Type> = vec![cl::I64; call.args.len()];
                for (i, a) in call.args.iter().enumerate() {
                    if i == 0
                        && (sym == "__RTS_FN_NS_GC_GEN_SM_NEW"
                            || sym == "__RTS_FN_NS_GC_ASYNC_SM_NEW"
                            || sym == "__RTS_FN_NS_GC_AGEN_NEW")
                    {
                        if let Expr::Ident(fid) = a.expr.as_ref() {
                            let addr = emit_user_fn_addr(ctx, fid.sym.as_str())?;
                            argv.push(ctx.coerce_to_i64(addr).val);
                            continue;
                        }
                    }
                    let tv = lower_expr(ctx, &a.expr)?;
                    argv.push(ctx.coerce_to_i64(tv).val);
                }
                let fref = ctx.get_extern(sym, &sig, ret)?;
                let inst = ctx.builder.ins().call(fref, &argv);
                if ret.is_some() {
                    let r = ctx.builder.inst_results(inst)[0];
                    let vt = if sym == "__RTS_FN_NS_GC_GEN_SM_NEW"
                        || sym == "__RTS_FN_NS_GC_ASYNC_SM_NEW"
                        || sym == "__RTS_FN_NS_GC_ASYNC_SM_START"
                        || sym == "__RTS_FN_NS_GC_AGEN_NEW"
                    {
                        ValTy::Handle
                    } else {
                        // value yieldado/ret eh ambiguo i64/handle.
                        ctx.var_member_call_values.insert(r);
                        ValTy::I64
                    };
                    return Ok(TypedVal::new(r, vt));
                } else {
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    return Ok(TypedVal::new(zero, ValTy::I64));
                }
            }
        }
        // (generators) `<genVar>.next()` — protocolo de iterador finito.
        // Roteia para GENERATOR_NEXT (cursor lateral sobre o Vec retornado
        // pela generator fn), devolvendo `{value,done}` (Map). So' dispara
        // para vars marcadas como generator (`const it = g()`), evitando
        // conflito com objetos que tem `.next()` proprio.
        if let Expr::Member(m) = callee.as_ref() {
            if let MemberProp::Ident(prop) = &m.prop {
                if prop.sym.as_str() == "next" && call.args.len() <= 1 {
                    if let Expr::Ident(obj_id) = m.obj.as_ref() {
                        let nm = obj_id.sym.as_str();
                        // (cross-runtime #344) Route `.next()` to GENERATOR_NEXT
                        // not only for vars marked `generator_vars` (`const it =
                        // g()`) but also for ambiguous handle-typed locals/globals
                        // whose value can be a generator obtained indirectly
                        // (`g = generator.call(this)`, a promoted-to-global capture
                        // typed F64, etc.). GENERATOR_NEXT runtime-detects
                        // GenState/Vec/Map-custom-iterator, so over-routing a plain
                        // object iterator is handled there. Class instances (own
                        // `.next` method) are excluded — they keep the method path.
                        let ambiguous = {
                            let ty = ctx
                                .read_local_info(nm)
                                .map(|l| l.ty)
                                .or_else(|| ctx.globals.get(nm).map(|g| g.ty));
                            matches!(
                                ty,
                                None | Some(ValTy::I64)
                                    | Some(ValTy::U64)
                                    | Some(ValTy::Handle)
                                    | Some(ValTy::F64)
                            ) && !ctx.local_array_vars.contains(nm)
                                && ctx.local_class_ty.get(nm).is_none()
                        };
                        if ctx.generator_vars.contains(nm) || ambiguous {
                            use cranelift_codegen::ir::types as cl;
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let recv = ctx.coerce_to_i64(recv_tv).val;
                            // (#211 value-passing) `g.next(v)`: injeta `v` como
                            // `sent` e avanca (NEXT_SENT). Sem arg: NEXT puro.
                            let h = if let Some(arg) = call.args.first() {
                                let arg_tv = lower_expr(ctx, &arg.expr)?;
                                let arg_v = ctx.coerce_to_i64(arg_tv).val;
                                let next_fn = ctx.get_extern(
                                    "__RTS_FN_NS_GC_GENERATOR_NEXT_SENT",
                                    &[cl::I64, cl::I64],
                                    Some(cl::I64),
                                )?;
                                let inst = ctx.builder.ins().call(next_fn, &[recv, arg_v]);
                                ctx.builder.inst_results(inst)[0]
                            } else {
                                let next_fn = ctx.get_extern(
                                    "__RTS_FN_NS_GC_GENERATOR_NEXT",
                                    &[cl::I64],
                                    Some(cl::I64),
                                )?;
                                let inst = ctx.builder.ins().call(next_fn, &[recv]);
                                ctx.builder.inst_results(inst)[0]
                            };
                            ctx.declare_gc_handle(h);
                            return Ok(TypedVal::new(h, ValTy::Handle));
                        }
                    }
                }
                // (#306) `<iterVar>.toArray()` — consome iterator wrapper a
                // partir do cursor lateral, devolve Vec do restante. So' p/
                // vars marcadas Iterator ou chain inline `Iterator.from(x)`.
                if prop.sym.as_str() == "toArray" && call.args.is_empty() {
                    let is_iter = match m.obj.as_ref() {
                        Expr::Ident(obj_id) => ctx
                            .local_class_ty
                            .get(obj_id.sym.as_str())
                            .map(|c| c == "Iterator")
                            .unwrap_or(false),
                        Expr::Call(c) => {
                            if let Callee::Expr(cb) = &c.callee {
                                if let Expr::Member(mm) = cb.as_ref() {
                                    let prop_from = matches!(&mm.prop,
                                        MemberProp::Ident(p) if p.sym.as_str() == "from");
                                    let obj_iter = matches!(mm.obj.as_ref(),
                                        Expr::Ident(o) if o.sym.as_str() == "Iterator");
                                    prop_from && obj_iter
                                } else { false }
                            } else { false }
                        }
                        _ => false,
                    };
                    if is_iter {
                        use cranelift_codegen::ir::types as cl;
                        let recv_tv = lower_expr(ctx, &m.obj)?;
                        let recv = ctx.coerce_to_i64(recv_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ITERATOR_TO_ARRAY",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[recv]);
                        let h = ctx.builder.inst_results(inst)[0];
                        ctx.declare_gc_handle(h);
                        return Ok(TypedVal::new(h, ValTy::Handle));
                    }
                }
                // (generators) `<genVar>.return(v)` — encerra o generator,
                // devolve `{value:v, done:true}` e marca esgotado. So' p/
                // generator_vars.
                if prop.sym.as_str() == "return" && call.args.len() == 1 {
                    if let Expr::Ident(obj_id) = m.obj.as_ref() {
                        if ctx.generator_vars.contains(obj_id.sym.as_str()) {
                            use cranelift_codegen::ir::types as cl;
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let recv = ctx.coerce_to_i64(recv_tv).val;
                            let val_tv = lower_expr(ctx, &call.args[0].expr)?;
                            let val = ctx.coerce_to_i64(val_tv).val;
                            let ret_fn = ctx.get_extern(
                                "__RTS_FN_NS_GC_GENERATOR_RETURN",
                                &[cl::I64, cl::I64],
                                Some(cl::I64),
                            )?;
                            let inst = ctx.builder.ins().call(ret_fn, &[recv, val]);
                            let h = ctx.builder.inst_results(inst)[0];
                            ctx.declare_gc_handle(h);
                            return Ok(TypedVal::new(h, ValTy::Handle));
                        }
                    }
                }
                // (#477 fatia 2) `<genVar>.throw(e)` — injeta excecao no ponto
                // suspenso; se ha' finally ativo, roda o finally (yield absorve).
                if prop.sym.as_str() == "throw" && call.args.len() == 1 {
                    if let Expr::Ident(obj_id) = m.obj.as_ref() {
                        if ctx.generator_vars.contains(obj_id.sym.as_str()) {
                            use cranelift_codegen::ir::types as cl;
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let recv = ctx.coerce_to_i64(recv_tv).val;
                            let val_tv = lower_expr(ctx, &call.args[0].expr)?;
                            let val = ctx.coerce_to_i64(val_tv).val;
                            let throw_fn = ctx.get_extern(
                                "__RTS_FN_NS_GC_GENERATOR_THROW",
                                &[cl::I64, cl::I64],
                                Some(cl::I64),
                            )?;
                            let inst = ctx.builder.ins().call(throw_fn, &[recv, val]);
                            let h = ctx.builder.inst_results(inst)[0];
                            ctx.declare_gc_handle(h);
                            return Ok(TypedVal::new(h, ValTy::Handle));
                        }
                    }
                }
            }
        }
        if let Expr::SuperProp(sp) = callee.as_ref() {
            return lower_super_method_call(ctx, sp, call);
        }
        // (cross-runtime #304) `<GlobalClass>.<staticMethod>.call(thisArg, ...args)`
        // — reescreve para `<GlobalClass>.<staticMethod>(...args)`. RTS user
        // fns nao usam thisArg (sem slot reservado em namespace fns), entao
        // call/apply sao equivalentes a chamada direta para static methods.
        if let Expr::Member(outer) = callee.as_ref() {
            if let MemberProp::Ident(call_id) = &outer.prop {
                let call_method = call_id.sym.as_str();
                if call_method == "call" {
                    if let Expr::Member(mid) = outer.obj.as_ref() {
                        if let (Expr::Ident(cls_id), MemberProp::Ident(static_id)) =
                            (mid.obj.as_ref(), &mid.prop)
                        {
                            let cls = cls_id.sym.as_str();
                            if ctx.read_local(cls).is_none()
                                && !ctx.user_fns.contains_key(cls)
                            {
                                if let Some(spec) = crate::abi::global_class_lookup(cls) {
                                    if spec.static_member(static_id.sym.as_str()).is_some() {
                                        let rest: Vec<_> = call.args.iter().skip(1).cloned().collect();
                                        let synth_call = swc_ecma_ast::CallExpr {
                                            span: call.span,
                                            ctxt: call.ctxt,
                                            callee: Callee::Expr(Box::new(Expr::Member(
                                                (*mid).clone(),
                                            ))),
                                            args: rest,
                                            type_args: None,
                                        };
                                        return lower_call(ctx, &synth_call);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // (cross-runtime #81/#99) `/regex/.test(s)`/`/regex/.exec(s)` em
        // literal — codegen avalia regex literal pra handle, depois usa
        // builtin. Sem isso, lower_indirect_call falha procurando var.
        if let Expr::Member(m) = callee.as_ref() {
            if let (Expr::Lit(swc_ecma_ast::Lit::Regex(_)), MemberProp::Ident(prop)) =
                (m.obj.as_ref(), &m.prop)
            {
                // Receiver é um literal regex → definitivamente RegExp; resolve
                // o método pelo Registry (sem hardcode de "test"/"exec").
                let pn = prop.sym.as_str();
                let recv_tv = lower_expr(ctx, &m.obj)?;
                if let Some(tv) =
                    ns_call::try_global_class_instance_method(ctx, "RegExp", pn, recv_tv, call)?
                {
                    return Ok(tv);
                }
            }
        }
        // (cross-runtime #79) `crypto.subtle.digest("SHA-256", data)` -> Promise
        // resolvido com Buffer dos 32 bytes do SHA-256. `data` eh handle
        // Buffer/Vec (TextEncoder.encode). `await` unwrappa pro Buffer.
        if let Expr::Member(outer) = callee.as_ref() {
            if let MemberProp::Ident(prop_id) = &outer.prop {
                if prop_id.sym.as_str() == "digest" {
                    if let Expr::Member(mid) = outer.obj.as_ref() {
                        if let (Expr::Ident(crypto_id), MemberProp::Ident(sub_id)) =
                            (mid.obj.as_ref(), &mid.prop)
                        {
                            if crypto_id.sym.as_str() == "crypto"
                                && sub_id.sym.as_str() == "subtle"
                                && ctx.read_local("crypto").is_none()
                                && call.args.len() >= 2
                            {
                                let data_tv = lower_expr(ctx, &call.args[1].expr)?;
                                let data_h = ctx.coerce_to_i64(data_tv).val;
                                let f = ctx.get_extern(
                                    "__RTS_FN_NS_CRYPTO_SHA256_DIGEST",
                                    &[cl::I64],
                                    Some(cl::I64),
                                )?;
                                let inst = ctx.builder.ins().call(f, &[data_h]);
                                let h = ctx.builder.inst_results(inst)[0];
                                return Ok(TypedVal::new(h, ValTy::Handle));
                            }
                        }
                    }
                }
            }
        }
        // `Object.prototype.toString.call(value)` — gera "[object Type]"
        // via __RTS_FN_RT_OBJECT_TO_STRING. Tag conforme tipo estatico.
        if let Some(tv) = try_lower_object_to_string_call(ctx, callee, call)? {
            return Ok(tv);
        }
        // (#247) `<Class>.prototype.isPrototypeOf(obj)` -> equivalente a
        // `obj instanceof <Class>` para classes globais conhecidas.
        // Sem isso, Object.prototype eh um string sentinel handle e o
        // .isPrototypeOf falha em "unsupported call expression form".
        if let Expr::Member(outer) = callee.as_ref() {
            if let MemberProp::Ident(prop_id) = &outer.prop {
                if prop_id.sym.as_str() == "isPrototypeOf" && call.args.len() == 1 {
                    if let Expr::Member(inner) = outer.obj.as_ref() {
                        let inner_is_proto = matches!(
                            &inner.prop,
                            MemberProp::Ident(id) if id.sym.as_str() == "prototype"
                        );
                        if inner_is_proto {
                            if let Expr::Ident(cls_id) = inner.obj.as_ref() {
                                let cls = cls_id.sym.as_str();
                                let known = matches!(
                                    cls,
                                    "Object" | "Array" | "Function" | "String"
                                    | "Number" | "Boolean" | "Date" | "RegExp"
                                    | "Error" | "Map" | "Set"
                                ) || crate::abi::global_class_lookup(cls).is_some();
                                if known {
                                    // Sintetiza `arg instanceof <cls>`.
                                    let arg_expr = call.args[0].expr.clone();
                                    let cls_ident = swc_ecma_ast::Ident {
                                        span: cls_id.span,
                                        ctxt: Default::default(),
                                        sym: cls.into(),
                                        optional: false,
                                    };
                                    let synth = swc_ecma_ast::BinExpr {
                                        span: cls_id.span,
                                        op: swc_ecma_ast::BinaryOp::InstanceOf,
                                        left: arg_expr,
                                        right: Box::new(Expr::Ident(cls_ident)),
                                    };
                                    return super::lower_bin(ctx, &synth);
                                }
                            }
                        }
                    }
                }
            }
        }
        // (#40) `Object.prototype.<method>.call(obj, ...args)` -> reescreve
        // como `obj.<method>(...args)` quando method eh universal
        // (hasOwnProperty, propertyIsEnumerable). isPrototypeOf ja' tratado
        // acima via inferencia de instanceof. Sem isso o callee
        // `Object.prototype.hasOwnProperty` vira sentinel string e o
        // `.call(...)` falha em "unsupported call expression form".
        // (cross-runtime #1064) Tambem suporta `Array.prototype.<method>.call(obj, ...args)`
        // -> `obj.<method>(...args)` para slice/push/concat/etc (pattern
        // tsc/esbuild __spreadArray helper).
        if let Expr::Member(outer) = callee.as_ref() {
            if let MemberProp::Ident(call_id) = &outer.prop {
                if call_id.sym.as_str() == "call" && !call.args.is_empty() {
                    if let Expr::Member(mid) = outer.obj.as_ref() {
                        if let MemberProp::Ident(method_id) = &mid.prop {
                            let method = method_id.sym.as_str();
                            let is_universal = matches!(
                                method,
                                "hasOwnProperty" | "propertyIsEnumerable"
                            );
                            let is_array_proto_method = matches!(
                                method,
                                "slice" | "concat" | "push" | "pop" | "shift" | "unshift"
                                | "indexOf" | "lastIndexOf" | "includes" | "join"
                                | "reverse" | "filter" | "map" | "forEach" | "find"
                                | "findIndex" | "every" | "some" | "reduce" | "flat"
                            );
                            if is_universal || is_array_proto_method {
                                if let Expr::Member(inner) = mid.obj.as_ref() {
                                    let proto_match = matches!(
                                        &inner.prop,
                                        MemberProp::Ident(id) if id.sym.as_str() == "prototype"
                                    );
                                    let obj_is_object = matches!(
                                        inner.obj.as_ref(),
                                        Expr::Ident(id) if matches!(id.sym.as_str(), "Object" | "Array")
                                    );
                                    if proto_match && obj_is_object {
                                        // Sintetiza `<arg0>.<method>(...rest)`.
                                        let recv_expr = call.args[0].expr.clone();
                                        let rest_args: Vec<_> = call
                                            .args
                                            .iter()
                                            .skip(1)
                                            .cloned()
                                            .collect();
                                        let synth_callee = Expr::Member(
                                            swc_ecma_ast::MemberExpr {
                                                span: method_id.span,
                                                obj: recv_expr,
                                                prop: MemberProp::Ident(
                                                    swc_ecma_ast::IdentName {
                                                        span: method_id.span,
                                                        sym: method.into(),
                                                    },
                                                ),
                                            },
                                        );
                                        let synth_call = swc_ecma_ast::CallExpr {
                                            span: call.span,
                                            ctxt: call.ctxt,
                                            callee: Callee::Expr(Box::new(synth_callee)),
                                            args: rest_args,
                                            type_args: None,
                                        };
                                        return super::lower_call(ctx, &synth_call);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // (#264 PR5) Fast path: `(var as any).method(...)` — TsAs/Paren etc
        // antes do member. Peel e despacha como var.method().
        // (#fix) NAO sequestra quando \`lhs_static_class\` resolve via TsAs
        // para uma classe — esse caso deve ir pelo caminho de class method
        // (lower_class_method_call_with_recv) via lhs_static_class.
        if let Expr::Member(m) = callee.as_ref() {
            let class_via_assertion = lhs_static_class(ctx, &m.obj);
            if class_via_assertion.is_none() {
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
                if !matches!(m.obj.as_ref(), Expr::Ident(_)) {
                    if let Expr::Ident(obj_id) = obj_e {
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
            }
        }
        if let Expr::Member(m) = callee.as_ref() {
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let cn = obj_id.sym.as_str();
                if let Some(meta) = ctx.classes.get(cn) {
                    // (cross-runtime #1056) Suporta MemberProp::PrivateName
                    // alem de Ident — `ClassName.#staticPrivate(...)`.
                    let method_name_opt: Option<String> = match &m.prop {
                        MemberProp::Ident(id) => Some(id.sym.as_str().to_string()),
                        MemberProp::PrivateName(pn) => Some(format!("#{}", pn.name.as_ref())),
                        _ => None,
                    };
                    if let Some(mn) = method_name_opt {
                        if meta.static_methods.iter().any(|m| m == &mn) {
                            let fn_name = class_static_method_name(cn, &mn);
                            return lower_user_call(ctx, &fn_name, call);
                        }
                    }
                }
            }
            // (#68) `view.set(srcArray, offset?)` onde view eh TypedArray-view
            // sobre (Shared)ArrayBuffer (local_ta_view). O path generico
            // (VEC_SET_FROM) so' escreve em Entry::Vec — sobre Buffer era no-op
            // silencioso. Rota dedicada TA_SET_FROM escreve os elementos como
            // `elem_bytes` bytes little-endian no buffer subjacente.
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    if prop.sym.as_str() == "set"
                        && matches!(
                            call.args.first().map(|a| a.expr.as_ref()),
                            Some(Expr::Array(_))
                        )
                        && call.args.iter().all(|a| a.spread.is_none())
                    {
                        if let Some(&(eb, _sg, fl)) =
                            ctx.local_ta_view.get(obj_id.sym.as_str())
                        {
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let buf_h = ctx.coerce_to_i64(recv_tv).val;
                            let src_tv = lower_expr(ctx, &call.args[0].expr)?;
                            let src_h = ctx.coerce_to_i64(src_tv).val;
                            let offset = if let Some(a) = call.args.get(1) {
                                let tv = lower_expr(ctx, &a.expr)?;
                                ctx.coerce_to_i64(tv).val
                            } else {
                                ctx.builder.ins().iconst(cl::I64, 0)
                            };
                            let eb_v = ctx.builder.ins().iconst(cl::I64, eb);
                            let fl_v = ctx.builder.ins().iconst(cl::I64, fl);
                            let f = ctx.get_extern(
                                "__RTS_FN_GL_TA_SET_FROM",
                                &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
                                None,
                            )?;
                            ctx.builder
                                .ins()
                                .call(f, &[buf_h, src_h, offset, eb_v, fl_v]);
                            return Ok(TypedVal::new(buf_h, ValTy::Handle));
                        }
                    }
                }
            }
            // (#68) `new Uint8Array(buf).set([...])` — receiver ANONIMO
            // (view sem binding). `new TypedArray(arraybuffer-ident)` lowera
            // pro handle do buffer (view-viva); extrai elem_bytes do nome da
            // classe e roteia pra TA_SET_FROM. Espelha o caso Ident acima.
            if let Expr::New(ne) = m.obj.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    if prop.sym.as_str() == "set"
                        && matches!(
                            call.args.first().map(|a| a.expr.as_ref()),
                            Some(Expr::Array(_))
                        )
                        && call.args.iter().all(|a| a.spread.is_none())
                    {
                        // (eb, signed, is_float) via typed_array_kind (bits->bytes).
                        let ta_meta = match ne.callee.as_ref() {
                            Expr::Ident(cid) => self::new_expr::typed_array_kind(cid.sym.as_str())
                                .map(|e| ((e.bits / 8) as i64, e.signed as i64, e.is_float as i64)),
                            _ => None,
                        };
                        // o arg do new precisa ser um (Shared)ArrayBuffer ident.
                        let arg_is_buf = ne.args.as_ref()
                            .and_then(|a| a.first())
                            .map(|a| matches!(a.expr.as_ref(),
                                Expr::Ident(id) if ctx.local_class_ty.get(id.sym.as_str())
                                    .map(|c| c == "ArrayBuffer" || c == "SharedArrayBuffer")
                                    .unwrap_or(false)))
                            .unwrap_or(false);
                        if let (Some((eb, _sg, fl)), true) = (ta_meta, arg_is_buf) {
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let buf_h = ctx.coerce_to_i64(recv_tv).val;
                            let src_tv = lower_expr(ctx, &call.args[0].expr)?;
                            let src_h = ctx.coerce_to_i64(src_tv).val;
                            let offset = if let Some(a) = call.args.get(1) {
                                let tv = lower_expr(ctx, &a.expr)?;
                                ctx.coerce_to_i64(tv).val
                            } else {
                                ctx.builder.ins().iconst(cl::I64, 0)
                            };
                            let eb_v = ctx.builder.ins().iconst(cl::I64, eb);
                            let fl_v = ctx.builder.ins().iconst(cl::I64, fl);
                            let f = ctx.get_extern(
                                "__RTS_FN_GL_TA_SET_FROM",
                                &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
                                None,
                            )?;
                            ctx.builder.ins().call(f, &[buf_h, src_h, offset, eb_v, fl_v]);
                            return Ok(TypedVal::new(buf_h, ValTy::Handle));
                        }
                    }
                }
            }
            if let Some(qualified) = qualified_member_name(callee) {
                // (cross-runtime #218/#354) `t.apply(...)` / `.call` / `.bind`
                // onde `t` eh um LOCAL/param (valor de fn — handle Function,
                // Proxy, ou func_addr cru) e NAO um namespace/classe/global/
                // user-fn. Roteia via FUNCTION_APPLY_TYPED/BIND (detectam o tipo
                // em runtime). Sem isto, `t.apply` cai no MAP_GET("apply")+trapz
                // generico -> TRAP (ILLEGAL_INSTRUCTION). Precisa preceder os
                // builtins/lookup pq `t.apply` tem qualified name "t.apply".
                if let Some((obj_name, meth)) = qualified.split_once('.') {
                    if matches!(meth, "apply" | "call" | "bind")
                        && ctx.read_local(obj_name).is_some()
                        && !ctx.user_fns.contains_key(obj_name)
                        && crate::abi::global_class_lookup(obj_name).is_none()
                    {
                        if let Expr::Member(mem) = callee.as_ref() {
                            if let Some(tv) =
                                lower_function_handle_method(ctx, &mem.obj, meth, call)?
                            {
                                return Ok(tv);
                            }
                        }
                    }
                }
                // Console builtin precisa preceder o lookup (#380).
                if let Some(tv) = lower_console_call(ctx, &qualified, call)? {
                    return Ok(tv);
                }
                // Math builtin variádico (#760).
                if let Some(tv) = lower_math_builtin(ctx, &qualified, call)? {
                    return Ok(tv);
                }
                // (cross-runtime #228) JSON.parse(text, reviver) — JS 2-arg form.
                if qualified == "JSON.parse" && call.args.len() >= 2 {
                    if call.args.iter().any(|a| a.spread.is_some()) {
                        return Err(anyhow!("spread not supported in JSON.parse"));
                    }
                    use crate::codegen::lower::ctx::{TypedVal, ValTy};
                    let s_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let s_h = ctx.coerce_to_i64(s_tv).val;
                    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                    let p_inst = ctx.builder.ins().call(str_ptr_fn, &[s_h]);
                    let s_ptr = ctx.builder.inst_results(p_inst)[0];
                    let l_inst = ctx.builder.ins().call(str_len_fn, &[s_h]);
                    let s_len = ctx.builder.inst_results(l_inst)[0];
                    let r_h = lower_callable_target_h(ctx, &call.args[1].expr)?;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_JSON_PARSE_REVIVER",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[s_ptr, s_len, r_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    // Handle (nao U64) para que a var seja registrada como
                    // GC root e o Map/Vec resultante nao seja coletado antes
                    // do consumer (ex: JSON.stringify subsequente).
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // JSON.stringify(value, replacer) — 2-arg. Replacer pode ser
                // array de keys (filtra props) ou fn (transforma valores).
                // Detecta tipo do arg em AST: array literal/Ident array -> KEYS,
                // arrow/fn -> REPLACER_FN, senao tenta dispatch generico (array).
                if qualified == "JSON.stringify" && call.args.len() == 2 {
                    if call.args.iter().any(|a| a.spread.is_some()) {
                        return Err(anyhow!("spread not supported in JSON.stringify"));
                    }
                    let v_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let v_h = ctx.coerce_to_i64(v_tv).val;
                    let arg1 = call.args[1].expr.as_ref();
                    // Detecta fn replacer: Arrow/Fn literal, OR Ident que
                    // resolve para user fn (incl. hoisted arrows).
                    let is_fn_replacer = matches!(arg1, Expr::Arrow(_) | Expr::Fn(_))
                        || matches!(arg1, Expr::Ident(id)
                            if ctx.user_fns.contains_key(id.sym.as_str())
                            && ctx.var_ty(id.sym.as_str()).is_none());
                    if is_fn_replacer {
                        let r_h = lower_callable_target_h(ctx, &call.args[1].expr)?;
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_JSON_STRINGIFY_REPLACER_FN",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[v_h, r_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(crate::codegen::lower::ctx::TypedVal::new(
                            v,
                            crate::codegen::lower::ctx::ValTy::Handle,
                        ));
                    }
                    let r_tv = lower_expr(ctx, &call.args[1].expr)?;
                    if matches!(r_tv.ty, ValTy::Handle) {
                        let r_h = r_tv.val;
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_JSON_STRINGIFY_KEYS",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[v_h, r_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(crate::codegen::lower::ctx::TypedVal::new(
                            v,
                            crate::codegen::lower::ctx::ValTy::Handle,
                        ));
                    }
                }
                // JSON.stringify(value, replacer, indent) — JS 3-arg form.
                // Replacer ignorado (v0); indent vai pra STRINGIFY_PRETTY.
                if qualified == "JSON.stringify" && call.args.len() >= 3 {
                    if call.args.iter().any(|a| a.spread.is_some()) {
                        return Err(anyhow!("spread not supported in JSON.stringify"));
                    }
                    let v_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let v_h = ctx.coerce_to_i64(v_tv).val;
                    let i_tv = lower_expr(ctx, &call.args[2].expr)?;
                    // JS spec: indent pode ser number ou string. Detecta
                    // pelo tipo e roteia para PRETTY (i64) ou PRETTY_STR
                    // (handle de string).
                    let sym = if matches!(i_tv.ty, ValTy::Handle) {
                        "__RTS_FN_NS_JSON_STRINGIFY_PRETTY_STR"
                    } else {
                        "__RTS_FN_NS_JSON_STRINGIFY_PRETTY"
                    };
                    let indent = if matches!(i_tv.ty, ValTy::Handle) {
                        i_tv.val
                    } else {
                        ctx.coerce_to_i64(i_tv).val
                    };
                    let f = ctx.get_extern(
                        sym,
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[v_h, indent]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(crate::codegen::lower::ctx::TypedVal::new(v, crate::codegen::lower::ctx::ValTy::Handle));
                }
                // JSON.stringify(value) — 1-arg typed dispatch para preservar
                // semantica JS spec (Bool -> "true"/"false", Null -> "null").
                if qualified == "JSON.stringify" && call.args.len() == 1 {
                    if call.args[0].spread.is_some() {
                        return Err(anyhow!("spread not supported in JSON.stringify"));
                    }
                    use crate::codegen::lower::ctx::{TypedVal, ValTy};
                    // (#json-bool) Pre-stringify literais em compile time.
                    // Bool/Number/String/null/Object/Array de literais geram
                    // JSON correto sem passar pelo Map<String,i64> que perde
                    // tipo de bool/null. Cobre o caso comum
                    // `JSON.stringify({nested: {x: true}})`.
                    if let Some(json_str) = try_const_stringify(&call.args[0].expr) {
                        let h = ctx.emit_str_handle(json_str.as_bytes())?;
                        return Ok(h);
                    }
                    // (cross-runtime #292) JS spec: se o valor eh uma class
                    // instance com metodo `toJSON()`, chama e usa o retorno.
                    // Detectamos em compile-time se o arg eh `new C()` ou ident
                    // tipado de classe registrada que tem `toJSON`.
                    if let Some(rewritten) = rewrite_to_json_call(ctx, &call.args[0].expr) {
                        let v_tv = lower_expr(ctx, &rewritten)?;
                        let raw = ctx.coerce_to_i64(v_tv).val;
                        let kind_v = ctx.builder.ins().iconst(cl::I32, 0);
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_JSON_STRINGIFY_TYPED",
                            &[cl::I64, cl::I32],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[raw, kind_v]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Handle));
                    }
                    let v_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let (raw, kind) = match v_tv.ty {
                        ValTy::Bool => (ctx.coerce_to_i64(v_tv).val, 2i64),
                        ValTy::F64 => {
                            let bits = ctx.builder.ins().bitcast(
                                cl::I64,
                                cranelift_codegen::ir::MemFlags::new(),
                                v_tv.val,
                            );
                            (bits, 1i64)
                        }
                        _ => (ctx.coerce_to_i64(v_tv).val, 0i64),
                    };
                    let kind_v = ctx.builder.ins().iconst(cl::I32, kind);
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_JSON_STRINGIFY_TYPED",
                        &[cl::I64, cl::I32],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[raw, kind_v]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#207 async try/catch) `await x` foi reescrito pra
                // `promise.wait(x)`. Se a Promise awaited rejeitar, PROMISE_WAIT
                // seta o error slot thread-local. Dentro de um `try` com catch,
                // isso deve saltar pro catch IMEDIATAMENTE (semantica de `throw`)
                // em vez de continuar executando o try-body. Emite o check + brif
                // pro catch logo apos a call (espelha lower_throw_stmt, mas
                // condicional pq `await` produz um valor usado adiante).
                if qualified == "promise.wait" && !ctx.catch_target_stack.is_empty()
                {
                    let tv = lower_ns_call(ctx, &qualified, call)?;
                    let get_fref =
                        ctx.get_extern("__RTS_FN_RT_ERROR_GET", &[], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(get_fref, &[]);
                    let err = ctx.builder.inst_results(inst)[0];
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    let is_err = ctx.builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                        err,
                        zero,
                    );
                    let cont = ctx.builder.create_block();
                    let cb = {
                        let (catch_blk, targeted) =
                            ctx.catch_target_stack.last_mut().unwrap();
                        *targeted = true;
                        *catch_blk
                    };
                    ctx.builder.ins().brif(is_err, cb, &[], cont, &[]);
                    ctx.builder.switch_to_block(cont);
                    ctx.builder.seal_block(cont);
                    return Ok(tv);
                }
                if lookup(&qualified).is_some() {
                    return lower_ns_call(ctx, &qualified, call);
                }
                // `export * as ns from "./mod"` registrou `ns.foo` -> `foo`
                // no local_alias_map durante o flatten. Resolve aqui para
                // user call do nome original.
                if let Some(orig) = ctx.local_alias_map.get(&qualified).cloned() {
                    if ctx.user_fns.contains_key(orig.as_str()) {
                        return lower_user_call(ctx, &orig, call);
                    }
                }
                // node: namespace imports: `import * as fs from "node:fs"` → `fs.readFileSync()`
                // node_import_map["fs"] = "node_fs" (prefix only, no dot)
                if let Some((obj_name, fn_name)) = qualified.split_once('.') {
                    if let Some(prefix) = ctx.node_import_map.get(obj_name) {
                        if !prefix.contains('.') {
                            let node_qualified = format!("{prefix}.{fn_name}");
                            if crate::nodespace::node_lookup(&node_qualified).is_some() {
                                return lower_node_ns_call(ctx, &node_qualified, call);
                            }
                        }
                    }
                }
            }
            let prop_method_name: Option<String> = match &m.prop {
                MemberProp::Ident(id) => Some(id.sym.as_str().to_string()),
                MemberProp::PrivateName(pn) => Some(format!("#{}", pn.name.as_ref())),
                _ => None,
            };
            // (cross-runtime #268) Private methods sao escopados por
            // declaring class — sem dispatch virtual. Em \`this.#m()\` dentro
            // de A.call, queremos A.#m (nao B.#m). Forca a resolucao para
            // current_class ao acessar private de \`this\`.
            let is_private = matches!(&m.prop, MemberProp::PrivateName(_));
            let force_class_for_private: Option<String> = if is_private {
                match m.obj.as_ref() {
                    Expr::This(_) => ctx.current_class.clone(),
                    // (cross-runtime #1056) `ClassName.#priv()` em static method —
                    // chamada para private static. obj eh ident == nome da classe.
                    Expr::Ident(id) => {
                        let n = id.sym.as_str();
                        if ctx.classes.contains_key(n) { Some(n.to_string()) } else { None }
                    }
                    _ => None,
                }
            } else {
                None
            };
            if let Some(method_name) = prop_method_name {
                // (cross-runtime #268) Para private method (this.#m), forca
                // class_name = current_class para evitar dispatch virtual.
                let class_name_opt = force_class_for_private
                    .clone()
                    .or_else(|| lhs_static_class(ctx, &m.obj));
                if let Some(class_name) = class_name_opt {
                    if resolve_method_owner(ctx, &class_name, &method_name).is_some() {
                        let recv_tv = lower_expr(ctx, &m.obj)?;
                        let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
                        return lower_class_method_call_with_recv(
                            ctx,
                            &class_name,
                            &method_name,
                            recv_i64,
                            call,
                        );
                    }
                    // Function global (#359): variadic — empacota args em Vec.
                    // PRECISA preceder lower_global_instance_call generico que
                    // mapeia 1:1 TS args -> ABI args.
                    if class_name == "Function"
                        && matches!(method_name.as_str(), "call" | "apply" | "bind" | "toString")
                    {
                        if let Some(tv) = lower_function_handle_method(ctx, &m.obj, &method_name, call)? {
                            return Ok(tv);
                        }
                    }
                    // (cross-runtime #378) Array-subclass instance: a method NOT
                    // defined on the subclass (reduce/map/join/filter/...) routes
                    // to the Array builtin on the backing Vec. Covers both
                    // `this.reduce(...)` inside a method and `pa.map(...)`
                    // externally.
                    if self::new_expr::class_extends_array(ctx, &class_name)
                        && resolve_method_owner(ctx, &class_name, &method_name).is_none()
                    {
                        let recv_tv = lower_expr(ctx, &m.obj)?;
                        let recv = ctx.coerce_to_i64(recv_tv).val;
                        if let Some(tv) = lower_array_builtin(ctx, &method_name, recv, call)? {
                            return Ok(tv);
                        }
                    }
                    // Global class instance methods (e.g. Date.getFullYear())
                    if let Some(spec) = crate::abi::global_class_lookup(&class_name) {
                        // Overload por aridade (alias/variadic/optional-tail) +
                        // fallback first-by-name — centralizado em
                        // resolve_instance_method (RTS_ENGINE.md §4.3 / F2b).
                        // Ex: `view.setUint16(o, v, le)` (3 args) tem que pegar o
                        // overload de 3 args, nao o big-endian de 2 (cross-runtime 57).
                        let n_call_args = call.args.len();
                        let member = spec.resolve_instance_method(&method_name, n_call_args);
                        if let Some(member) = member {
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            // (#245) Quando member.args[0] eh F64 (Number/etc.
                            // primitives wrapped), passa recv como F64 raw.
                            // Senao coerce para i64 (Handle/string/etc.).
                            let first_arg_is_f64 = member.args
                                .first()
                                .map(|a| matches!(a, crate::abi::types::AbiType::F64))
                                .unwrap_or(false);
                            let recv_raw = if first_arg_is_f64 {
                                to_f64(ctx, recv_tv)
                            } else {
                                ctx.coerce_to_i64(recv_tv).val
                            };
                            return lower_global_instance_call(ctx, member, recv_raw, call);
                        }
                    }
                }
                // (cross-runtime #359) `(<userFn> as any).call/apply/bind/toString(...)`
                // — the `as any` cast makes m.obj a TsAs (not Ident), so without
                // this it falls into the generic numeric/string method block
                // below and mishandles the function (`.toString()` → garbage
                // handle). Peel the cast and route bare user-fn idents to the
                // dedicated function-method path, identical to uncast `f.toString()`.
                if matches!(method_name.as_str(), "call" | "apply" | "bind" | "toString") {
                    let mut oe: &Expr = m.obj.as_ref();
                    loop {
                        match oe {
                            Expr::Paren(p) => oe = &p.expr,
                            Expr::TsAs(a) => oe = &a.expr,
                            Expr::TsTypeAssertion(a) => oe = &a.expr,
                            Expr::TsConstAssertion(a) => oe = &a.expr,
                            Expr::TsNonNull(n) => oe = &n.expr,
                            _ => break,
                        }
                    }
                    if let Expr::Ident(id) = oe {
                        let nm = id.sym.as_str();
                        if ctx.user_fns.contains_key(nm) && ctx.var_ty(nm).is_none() {
                            if let Some(tv) =
                                self::new_expr::lower_function_method_call(ctx, nm, &method_name, call)?
                            {
                                return Ok(tv);
                            }
                        }
                    }
                }
                // Numeric/string instance methods on literal/computed expressions:
                // (1000).toString(), (3.14).toFixed(2), "hi".toUpperCase().
                // Only when obj is NOT a plain Ident (those are handled via qualified_member_name
                // at the outer dispatch path which has the global_class_lookup).
                if !matches!(m.obj.as_ref(), Expr::Ident(_)) {
                    let mut recv_tv = lower_expr(ctx, &m.obj)?;
                    // (#394) Peel Paren/TsAs/TsNonNull do receiver p/ os checks
                    // de chained-Call abaixo. `(m.get(k) as Set).has(v)` tem
                    // m.obj = Paren/TsAs(Call); sem peelar, nenhum branch
                    // Call-receiver disparava e o `.has` caia num path que
                    // deixava block dangling -> verifier "invalid block reference".
                    let obj_peeled: &Expr = {
                        let mut e: &Expr = m.obj.as_ref();
                        loop {
                            match e {
                                Expr::Paren(p) => e = &p.expr,
                                Expr::TsAs(a) => e = &a.expr,
                                Expr::TsTypeAssertion(a) => e = &a.expr,
                                Expr::TsConstAssertion(a) => e = &a.expr,
                                Expr::TsNonNull(n) => e = &n.expr,
                                _ => break,
                            }
                        }
                        e
                    };
                    // (cross-runtime) `mk().get(k)`/`mk().has(k)` chain direto:
                    // mk() retorna i64 nao-tipado-Handle, entao o receiver caia
                    // no caminho number_builtin e crashava. Se mk eh fn em
                    // FNS_RET_MAPSET, re-tipa o receiver como Handle p/ entrar no
                    // bloco de map_set_builtin.
                    if matches!(recv_tv.ty, ValTy::I64) {
                        let recv_is_mapcall = matches!(m.obj.as_ref(),
                            Expr::Call(ce) if matches!(&ce.callee,
                                swc_ecma_ast::Callee::Expr(callee) if matches!(callee.as_ref(),
                                    Expr::Ident(fid) if crate::codegen::lower::passes::parallelism::FNS_RET_MAPSET
                                        .with(|c| c.borrow().contains(fid.sym.as_str()))
                                )
                            )
                        );
                        if recv_is_mapcall {
                            recv_tv = TypedVal::new(recv_tv.val, ValTy::Handle);
                        }
                    }
                    // (#550 parte 2) (true).toString() / (false).valueOf() em literal.
                    if matches!(recv_tv.ty, ValTy::Bool) && call.args.is_empty() {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        match method_name.as_str() {
                            "toString" => {
                                let true_h = ctx.emit_str_handle(b"true")?.val;
                                let false_h = ctx.emit_str_handle(b"false")?.val;
                                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                                let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, recv_tv.val, zero);
                                let r = ctx.builder.ins().select(is_true, true_h, false_h);
                                return Ok(TypedVal::new(r, ValTy::Handle));
                            }
                            "valueOf" => {
                                return Ok(recv_tv);
                            }
                            _ => {}
                        }
                    }
                    if matches!(recv_tv.ty, ValTy::F64 | ValTy::I64 | ValTy::I32) {
                        // GENÉRICO via Registry — sem nomes de método hardcoded.
                        if let Some(tv) = ns_call::try_global_class_instance_method(
                            ctx,
                            "Number",
                            &method_name,
                            recv_tv,
                            call,
                        )? {
                            return Ok(tv);
                        }
                    }
                    if matches!(recv_tv.ty, ValTy::Handle) {
                        let recv_h = ctx.coerce_to_i64(recv_tv).val;
                        // (#549) Array literal/expr como receiver: tentar
                        // array_builtin antes de string_builtin. Heuristica:
                        // se m.obj e' Expr::Array OU Expr::Call cuja propria
                        // chamada retorna Handle (ex: outro .reverse()),
                        // tratamos como array. Para garantir, tenta array primeiro.
                        // (cross-runtime #106) Excecao: Call para coerce builtin
                        // `String(...)` retorna string handle — array_builtin
                        // matchando "includes"/"indexOf" antes de string_builtin
                        // chamaria VEC_INCLUDES e retornaria false. Detecta
                        // callee Ident "String"/"Number"/"Boolean" para skip.
                        let call_returns_string_coerce = if let Expr::Call(c) = m.obj.as_ref() {
                            matches!(&c.callee, swc_ecma_ast::Callee::Expr(ce)
                                if matches!(ce.as_ref(), Expr::Ident(id)
                                    if matches!(id.sym.as_str(), "String")))
                        } else {
                            false
                        };
                        // (#376) Receiver `(x ?? [])` / `(x || [])` — nullish/or
                        // com array literal como fallback eh um array. Sem isto,
                        // `(map.get(k) ?? []).includes(v)` caia em string_builtin
                        // (VEC vs STRING) e retornava false. Detecta o padrao.
                        let obj_is_coalesce_array = matches!(
                            m.obj.as_ref(),
                            Expr::Paren(p) if expr_is_coalesce_with_array(&p.expr)
                        ) || expr_is_coalesce_with_array(m.obj.as_ref());
                        if matches!(m.obj.as_ref(), Expr::Array(_) | Expr::Call(_))
                            && !call_returns_string_coerce
                            || obj_is_coalesce_array
                        {
                            if let Some(tv) = lower_array_builtin(ctx, &method_name, recv_h, call)? {
                                return Ok(tv);
                            }
                        }
                        // (cross-runtime #302) `new Set([...]).isSubsetOf(...)`:
                        // m.obj eh NewExpression Map/Set. Tenta map_set_builtin
                        // ANTES de string_builtin (que tem methods conflitantes
                        // tipo "includes"/"indexOf").
                        let obj_is_new_map_set = matches!(
                            m.obj.as_ref(),
                            Expr::New(n) if matches!(
                                n.callee.as_ref(),
                                Expr::Ident(id) if matches!(id.sym.as_str(), "Map" | "Set" | "WeakMap" | "WeakSet")
                            )
                        )
                        // (cross-runtime) `mk().get(k)` chain direto: receiver eh
                        // call de fn que retorna Map/Set (FNS_RET_MAPSET). Sem
                        // isto cai em string/array builtin e crasha (SIGILL).
                        || matches!(m.obj.as_ref(),
                            Expr::Call(ce) if matches!(&ce.callee,
                                swc_ecma_ast::Callee::Expr(callee) if matches!(callee.as_ref(),
                                    Expr::Ident(fid) if crate::codegen::lower::passes::parallelism::FNS_RET_MAPSET
                                        .with(|c| c.borrow().contains(fid.sym.as_str()))
                                )
                            )
                        );
                        if obj_is_new_map_set {
                            if let Some(tv) = lower_map_set_builtin(ctx, &method_name, recv_h, call)? {
                                return Ok(tv);
                            }
                        }
                        // (cross-runtime #1533) `[].hasOwnProperty(k)` / literal
                        // receiver: metodos universais de Object.prototype com
                        // receiver handle arbitrario. Sem isto caia em
                        // "unsupported call expression form".
                        if matches!(
                            method_name.as_str(),
                            "hasOwnProperty" | "propertyIsEnumerable"
                        ) && call.args.len() == 1
                        {
                            let key_tv = lower_expr(ctx, &call.args[0].expr)?;
                            let key_h = ctx.coerce_to_handle(key_tv)?.val;
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
                            let inst_p = ctx.builder.ins().call(str_ptr_fn, &[key_h]);
                            let kptr = ctx.builder.inst_results(inst_p)[0];
                            let inst_l = ctx.builder.ins().call(str_len_fn, &[key_h]);
                            let klen = ctx.builder.inst_results(inst_l)[0];
                            let sym = if method_name == "hasOwnProperty" {
                                "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY"
                            } else {
                                "__RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE"
                            };
                            let f = ctx.get_extern(sym, &[cl::I64, cl::I64, cl::I64], Some(cl::I64))?;
                            let inst = ctx.builder.ins().call(f, &[recv_h, kptr, klen]);
                            let v = ctx.builder.inst_results(inst)[0];
                            return Ok(TypedVal::new(v, ValTy::Bool));
                        }
                        if let Some(tv) = lower_string_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                        // Fallback array para chains (call que pode ser map/filter/etc).
                        if let Some(tv) = lower_array_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                        // Fallback final: tenta map_set_builtin para isSubsetOf/etc.
                        if let Some(tv) = lower_map_set_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                    }
                    // (cross-runtime #222) Receiver Call que devolve i64
                    // AMBIGUO (ex: `m.get(k)` retorna o handle do array como
                    // i64, nao ValTy::Handle) seguido de array method
                    // (`.join`/`.map`/etc): tenta lower_array_builtin com o
                    // recv coerido ANTES do fallback chain-Map abaixo. Sem
                    // isto, `m.get(k).join(",")` caia em MAP_GET("join") +
                    // trapz -> SIGILL. Var intermediaria ja' funcionava.
                    if matches!(obj_peeled, Expr::Call(_))
                        && matches!(recv_tv.ty, ValTy::I64 | ValTy::U64)
                    {
                        let is_array_method = matches!(
                            method_name.as_str(),
                            "join" | "map" | "filter" | "forEach" | "reduce"
                            | "slice" | "concat" | "indexOf" | "lastIndexOf"
                            | "includes" | "reverse" | "find" | "findIndex"
                            | "every" | "some" | "flat" | "flatMap" | "at"
                            | "fill" | "sort" | "push" | "pop" | "shift"
                            | "unshift" | "splice" | "keys" | "values" | "entries"
                            // (#305) Iterator helpers eager: chain sobre array.
                            | "take" | "drop" | "toArray"
                        );
                        if is_array_method {
                            let recv_h = ctx.coerce_to_i64(recv_tv).val;
                            if let Some(tv) = lower_array_builtin(ctx, &method_name, recv_h, call)? {
                                return Ok(tv);
                            }
                        }
                    }
                    // (#480 chain) Method chain em Call result: \`c.add(5).add(3)\`.
                    // Receiver i64 (return this) que e' handle de Map. Faz map_get +
                    // INVOKE_AUTO em qualquer Call obj. (#394) usa obj_peeled
                    // p/ cobrir `(call as T).method()` / `(call).method()`.
                    if matches!(obj_peeled, Expr::Call(_)) {
                        let recv_h = ctx.coerce_to_i64(recv_tv).val;
                        // (#394) Quando o receiver call retorna uma COLECAO
                        // (Map/Set) — ex: `m.get(k).has(v)` onde o valor do Map
                        // eh Set — `.has`/`.get` devem despachar SET_HAS/MAP_GET
                        // (runtime detecta o tipo do Entry), nao procurar um
                        // method-handle no Set (=> trapz). So' intercepta quando
                        // o callee NAO eh metodo de classe user (preserva chains
                        // de operator/metodo `c.add(5).add(3)`): a heuristica eh
                        // que `m.get(...)` (Map.get) retorna valor de colecao.
                        let recv_is_map_get = matches!(obj_peeled,
                            Expr::Call(ce) if matches!(&ce.callee,
                                swc_ecma_ast::Callee::Expr(cb) if matches!(cb.as_ref(),
                                    Expr::Member(mm) if matches!(&mm.prop,
                                        MemberProp::Ident(p) if matches!(p.sym.as_str(), "get" | "at" | "pop" | "shift"))
                                )
                            )
                        );
                        if recv_is_map_get
                            && matches!(method_name.as_str(),
                                "has" | "get" | "set" | "add" | "delete" | "clear"
                                | "keys" | "values" | "entries" | "forEach")
                        {
                            if let Some(tv) = lower_map_set_builtin(ctx, &method_name, recv_h, call)? {
                                return Ok(tv);
                            }
                        }
                        let (kp, kl) = ctx.emit_str_literal(method_name.as_bytes())?;
                        let map_get = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst_g = ctx.builder.ins().call(map_get, &[recv_h, kp, kl]);
                        let callee_val = ctx.builder.inst_results(inst_g)[0];
                        ctx.builder.ins().trapz(
                            callee_val,
                            cranelift_codegen::ir::TrapCode::user(1).unwrap(),
                        );
                        let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
                        let inst_v = ctx.builder.ins().call(vec_new, &[]);
                        let args_h = ctx.builder.inst_results(inst_v)[0];
                        let vec_push = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                            &[cl::I64, cl::I64],
                            None,
                        )?;
                        for arg in &call.args {
                            if arg.spread.is_some() {
                                return Err(anyhow!("spread not supported in chained method call"));
                            }
                            let tv = lower_expr(ctx, &arg.expr)?;
                            let v = ctx.coerce_to_i64(tv).val;
                            ctx.builder.ins().call(vec_push, &[args_h, v]);
                        }
                        let invoke_auto = ctx.get_extern(
                            "__RTS_FN_RT_INVOKE_AUTO",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(invoke_auto, &[callee_val, recv_h, args_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                }
            }
        }
        if let Some(qualified) = qualified_member_name(callee) {
            // (#195) parallel.{map,filter,for_each}_bound(arr, __lifted_cap_N):
            // o callback captura vars por-valor. Reifica a fn liftada com as
            // capturas (lidas do escopo via LIFTED_CAPTURES) em bound_args e
            // chama a variante BOUND do runtime (que invoca via Entry::Function).
            if matches!(
                qualified.as_str(),
                "parallel.map_bound" | "parallel.filter_bound" | "parallel.for_each_bound"
                | "parallel.reduce_bound" | "parallel.reduce_no_init_bound"
                | "parallel.find_bound" | "parallel.find_index_bound"
                | "parallel.some_bound" | "parallel.every_bound"
                | "parallel.reduce_right_bound" | "parallel.reduce_right_no_init_bound"
                | "parallel.find_last_bound" | "parallel.find_last_index_bound"
            ) {
                if let Some(tv) = lower_parallel_bound_call(ctx, &qualified, call)? {
                    return Ok(tv);
                }
            }
            // Console builtin (#221, #380): console.log/info/debug → io.print,
            // console.error/warn → io.eprint. Args concatenados separados
            // por espaco. PRECISA vir antes do `lookup` generico porque
            // console.* tambem esta listado em SPECS (com aridade fixa
            // `StrPtr`) so' pra type-check / `rts apis` — passar 42 ali
            // dispararia "StrPtr argument must be a string value".
            if let Some(tv) = lower_console_call(ctx, &qualified, call)? {
                return Ok(tv);
            }
            // JSON.stringify(value, replacer, indent) — JS 3-arg form.
            // Replacer ignorado (v0); indent vai pra STRINGIFY_PRETTY.
            // Tem que ser ANTES de lookup (que resolveria a forma 1-arg).
            if qualified == "JSON.stringify" && call.args.len() >= 3 {
                if call.args.iter().any(|a| a.spread.is_some()) {
                    return Err(anyhow!("spread not supported in JSON.stringify"));
                }
                let v_tv = lower_expr(ctx, &call.args[0].expr)?;
                let v_h = ctx.coerce_to_i64(v_tv).val;
                let i_tv = lower_expr(ctx, &call.args[2].expr)?;
                let indent = ctx.coerce_to_i64(i_tv).val;
                let f = ctx.get_extern(
                    "__RTS_FN_NS_JSON_STRINGIFY_PRETTY",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[v_h, indent]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(v, ValTy::Handle));
            }
            if lookup(&qualified).is_some() {
                return lower_ns_call(ctx, &qualified, call);
            }
            // node: namespace/default imports: `import fs from "node:fs"` → `fs.readFileSync()`
            if let Some((obj_name, fn_name)) = qualified.split_once('.') {
                if let Some(prefix) = ctx.node_import_map.get(obj_name) {
                    if !prefix.contains('.') {
                        let node_qualified = format!("{prefix}.{fn_name}");
                        if crate::nodespace::node_lookup(&node_qualified).is_some() {
                            return lower_node_ns_call(ctx, &node_qualified, call);
                        }
                    }
                }
            }
            // JSON global (#215): JSON.* — spec name="JSON" is in SPECS, so
            // `lookup("JSON.parse")` resolves directly above. No fallback needed.

            // (#208) Math.hypot(...args) — JS spec N args. hypot e' associativo:
            // hypot(a,b,c) = hypot(hypot(a,b), c).
            if qualified == "Math.hypot" && call.args.len() > 2 {
                if call.args.iter().any(|a| a.spread.is_some()) {
                    return Err(anyhow!("spread not supported in Math.hypot"));
                }
                let f = ctx.get_extern(
                    "__RTS_FN_NS_MATH_HYPOT",
                    &[cl::F64, cl::F64],
                    Some(cl::F64),
                )?;
                let mut acc: Option<cranelift_codegen::ir::Value> = None;
                for a in &call.args {
                    let tv = lower_expr(ctx, &a.expr)?;
                    let val = ctx.coerce_to_f64(tv).val;
                    acc = Some(match acc {
                        None => val,
                        Some(prev) => {
                            let inst = ctx.builder.ins().call(f, &[prev, val]);
                            ctx.builder.inst_results(inst)[0]
                        }
                    });
                }
                return Ok(TypedVal::new(acc.unwrap(), ValTy::F64));
            }
            // Math.max() = -Infinity; Math.min() = +Infinity (JS spec).
            if qualified == "Math.max" && call.args.is_empty() {
                let v = ctx.builder.ins().f64const(f64::NEG_INFINITY);
                return Ok(TypedVal::new(v, ValTy::F64));
            }
            if qualified == "Math.min" && call.args.is_empty() {
                let v = ctx.builder.ins().f64const(f64::INFINITY);
                return Ok(TypedVal::new(v, ValTy::F64));
            }
            // (#208) Math.max/min variadico — JS spec aceita N args.
            // ABI namespace fixou em 2 args; aqui suportamos N reduzindo
            // pairwise. Caso 1 arg: retorna `Number(arg)` (sem call).
            if (qualified == "Math.max" || qualified == "Math.min") && call.args.len() == 1 {
                // (cross-runtime) `Math.min(...arr)` / `Math.max(...arr)`: o
                // unico arg eh spread de um array -> reduz sobre o Vec via
                // VEC_MIN/MAX. Sem isto, coerce_to_f64(arr_handle) virava lixo
                // (handle interpretado como f64). Standalone passava por outro
                // caminho; em fn caia aqui.
                if call.args[0].spread.is_some() {
                    let src_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let src_h = ctx.coerce_to_i64(src_tv).val;
                    let sym = if qualified == "Math.max" {
                        "__RTS_FN_NS_COLLECTIONS_VEC_MAX"
                    } else {
                        "__RTS_FN_NS_COLLECTIONS_VEC_MIN"
                    };
                    let f = ctx.get_extern(sym, &[cl::I64], Some(cl::F64))?;
                    let inst = ctx.builder.ins().call(f, &[src_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::F64));
                }
                let tv = lower_expr(ctx, &call.args[0].expr)?;
                let v = ctx.coerce_to_f64(tv).val;
                return Ok(TypedVal::new(v, ValTy::F64));
            }
            if (qualified == "Math.max" || qualified == "Math.min") && call.args.len() > 2 {
                if call.args.iter().any(|a| a.spread.is_some()) {
                    return Err(anyhow!("spread not supported in Math.max/min"));
                }
                let sym = if qualified == "Math.max" {
                    "__RTS_FN_NS_MATH_MAX_F64"
                } else {
                    "__RTS_FN_NS_MATH_MIN_F64"
                };
                let f = ctx.get_extern(sym, &[cl::F64, cl::F64], Some(cl::F64))?;
                let mut acc: Option<cranelift_codegen::ir::Value> = None;
                for a in &call.args {
                    let tv = lower_expr(ctx, &a.expr)?;
                    let val = ctx.coerce_to_f64(tv).val;
                    acc = Some(match acc {
                        None => val,
                        Some(prev) => {
                            let inst = ctx.builder.ins().call(f, &[prev, val]);
                            ctx.builder.inst_results(inst)[0]
                        }
                    });
                }
                return Ok(TypedVal::new(acc.unwrap(), ValTy::F64));
            }
            // (#208) String.fromCharCode/fromCodePoint variadico.
            // Default em GlobalClassSpec aceita so 1 arg; aqui suportamos N
            // chamando a fn unitaria pra cada e concatenando.
            if (qualified == "String.fromCharCode" || qualified == "String.fromCodePoint")
                && call.args.len() > 1
            {
                if call.args.iter().any(|a| a.spread.is_some()) {
                    return Err(anyhow!("spread not supported in String.fromXxx"));
                }
                let single_sym = if qualified == "String.fromCharCode" {
                    "__RTS_FN_GL_STRING_FROM_CHAR_CODE"
                } else {
                    "__RTS_FN_GL_STRING_FROM_CODE_POINT"
                };
                let single_fn = ctx.get_extern(single_sym, &[cl::I64], Some(cl::I64))?;
                let concat_fn = ctx.get_extern(
                    "__RTS_FN_NS_GC_STRING_CONCAT",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let mut acc: Option<cranelift_codegen::ir::Value> = None;
                for a in &call.args {
                    let tv = lower_expr(ctx, &a.expr)?;
                    let code = ctx.coerce_to_i64(tv).val;
                    let inst = ctx.builder.ins().call(single_fn, &[code]);
                    let part = ctx.builder.inst_results(inst)[0];
                    acc = Some(match acc {
                        None => part,
                        Some(prev) => {
                            let i = ctx.builder.ins().call(concat_fn, &[prev, part]);
                            ctx.builder.inst_results(i)[0]
                        }
                    });
                }
                return Ok(TypedVal::new(acc.unwrap(), ValTy::Handle));
            }
            // (#220) Date.UTC(year, month, day?, hour?, min?, sec?, ms?) —
            // arity 1-7, fill defaults: month=0 nao aceito (spec ECMA),
            // day=1, demais=0.
            if qualified == "Date.UTC" && (1..=7).contains(&call.args.len()) {
                if call.args.iter().any(|a| a.spread.is_some()) {
                    return Err(anyhow!("spread not supported in Date.UTC"));
                }
                let mut vals: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(7);
                for a in &call.args {
                    let tv = lower_expr(ctx, &a.expr)?;
                    vals.push(ctx.coerce_to_i64(tv).val);
                }
                // Pad com defaults: day=1, demais=0.
                while vals.len() < 7 {
                    let default = if vals.len() == 2 {
                        // day default = 1
                        ctx.builder.ins().iconst(cl::I64, 1)
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    };
                    vals.push(default);
                }
                let f = ctx.get_extern(
                    "__RTS_FN_NS_DATE_FROM_PARTS",
                    &[cl::I64; 7],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &vals);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(TypedVal::new(v, ValTy::I64));
            }
            // `Number.parseInt` / `Number.parseFloat` aliases globais —
            // delega para o handler de \`parseInt\` (com suporte a radix)
            // em vez do static_member do namespace (que tem arity fixa).
            // JS spec: \`Number.parseInt === parseInt\`.
            if qualified == "Number.parseInt" || qualified == "Number.parseFloat" {
                let alias = if qualified == "Number.parseInt" { "parseInt" } else { "parseFloat" };
                if let Some(tv) = lower_js_global_call(ctx, alias, call)? {
                    return Ok(tv);
                }
            }
            // (#879/130) `Number.isFinite/isNaN/isInteger/isSafeInteger`:
            // JS spec — NAO coage o argumento. Se tipo != number, retorna false.
            // Distinto do global `isFinite`/`isNaN` (que coagem).
            if matches!(
                qualified.as_str(),
                "Number.isFinite" | "Number.isNaN" | "Number.isInteger" | "Number.isSafeInteger"
            ) {
                use crate::codegen::lower::ctx::ValTy;
                if let Some(arg) = call.args.first() {
                    if arg.spread.is_none() {
                        let tv = lower_expr(ctx, &arg.expr)?;
                        if !matches!(tv.ty, ValTy::F64 | ValTy::I32 | ValTy::I64) {
                            // Nao-numero → false. (Handle/Bool/U64/Str etc.)
                            let v = ctx.builder.ins().iconst(cl::I64, 0);
                            return Ok(TypedVal::new(v, ValTy::Bool));
                        }
                        // Number type: delega pra impl normal preservando tv.
                        let f = match tv.ty {
                            ValTy::F64 => tv.val,
                            _ => {
                                let i = ctx.coerce_to_i64(tv).val;
                                ctx.builder.ins().fcvt_from_sint(cl::F64, i)
                            }
                        };
                        let sym = match qualified.as_str() {
                            "Number.isFinite" => "__RTS_FN_GL_NUMBER_IS_FINITE",
                            "Number.isNaN" => "__RTS_FN_GL_NUMBER_IS_NAN",
                            "Number.isInteger" => "__RTS_FN_GL_NUMBER_IS_INTEGER",
                            "Number.isSafeInteger" => "__RTS_FN_GL_NUMBER_IS_SAFE_INT",
                            _ => unreachable!(),
                        };
                        let fref = ctx.get_extern(sym, &[cl::F64], Some(cl::I64))?;
                        let inst = ctx.builder.ins().call(fref, &[f]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Bool));
                    }
                }
            }
            // Date static methods (#220): Date.now() / Date.parse() via GlobalClassSpec.
            if let Some((cls, method)) = qualified.split_once('.') {
                if let Some(spec) = crate::abi::global_class_lookup(cls) {
                    // (cross-runtime #746) Multiplos static members com mesmo
                    // nome (overloads por arity TS): prefere o que combina.
                    // Cada AbiType::StrPtr conta como 1 arg TS (expandida em
                    // 2 slots na ABI Cranelift).
                    let n_args = call.args.len();
                    fn ts_arity(args: &[crate::abi::types::AbiType]) -> usize {
                        args.len()
                    }
                    let arity_match = spec.members.iter().find(|m| {
                        if m.name != method { return false; }
                        if !matches!(m.kind,
                            crate::abi::member::MemberKind::Function
                            | crate::abi::member::MemberKind::Constant
                            | crate::abi::member::MemberKind::StaticMethod) {
                            return false;
                        }
                        ts_arity(m.args) == n_args
                    });
                    if let Some(member) = arity_match {
                        return lower_ns_call_member(ctx, member, call);
                    }
                    if let Some(member) = spec.static_member(method) {
                        return lower_ns_call_member(ctx, member, call);
                    }
                }
            }
            // (#208) `Math.X` → `math.X` (lowercase namespace). RTS usa
            // `math` namespace pra tudo; expor JS-style `Math.sqrt` etc.
            // sem duplicar codigo de impl.
            if let Some(method) = qualified.strip_prefix("Math.") {
                let target = format!("math.{method}");
                if lookup(&target).is_some() {
                    return lower_ns_call(ctx, &target, call);
                }
            }
            // (#69) Atomics.* sobre TypedArray-view inteiro (backing
            // (Shared)ArrayBuffer). Single-thread: cada op eh RMW nao-
            // concorrente via ATOMICS_RMW/CAS/LOAD/STORE. O 1o arg eh a
            // view (Ident em local_ta_view => elem_bytes/signed); demais
            // sao index/operand i64.
            if let Some(method) = qualified.strip_prefix("Atomics.") {
                // Resolve (elem_bytes, signed) da view (1o arg).
                let ta_meta = call.args.first().and_then(|a| match a.expr.as_ref() {
                    Expr::Ident(id) => ctx
                        .local_ta_view
                        .get(id.sym.as_str())
                        .map(|&(eb, sg, _fl)| (eb, sg)),
                    _ => None,
                });
                if let Some((eb, sg)) = ta_meta {
                    // Lower view handle + index.
                    let view_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let view_h = ctx.coerce_to_i64(view_tv).val;
                    let idx_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let idx = ctx.coerce_to_i64(idx_tv).val;
                    let eb_v = ctx.builder.ins().iconst(cl::I64, eb);
                    let sg_v = ctx.builder.ins().iconst(cl::I64, sg);
                    // op codes: add=0 sub=1 and=2 or=3 xor=4 exchange=5
                    let rmw_op = match method {
                        "add" => Some(0i64),
                        "sub" => Some(1),
                        "and" => Some(2),
                        "or" => Some(3),
                        "xor" => Some(4),
                        "exchange" => Some(5),
                        _ => None,
                    };
                    if let Some(op) = rmw_op {
                        let operand_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let operand = ctx.coerce_to_i64(operand_tv).val;
                        let op_v = ctx.builder.ins().iconst(cl::I64, op);
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ATOMICS_RMW",
                            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx
                            .builder
                            .ins()
                            .call(f, &[view_h, idx, eb_v, sg_v, op_v, operand]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    if method == "compareExchange" {
                        let exp_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let exp = ctx.coerce_to_i64(exp_tv).val;
                        let rep_tv = lower_expr(ctx, &call.args[3].expr)?;
                        let rep = ctx.coerce_to_i64(rep_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ATOMICS_CAS",
                            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx
                            .builder
                            .ins()
                            .call(f, &[view_h, idx, eb_v, sg_v, exp, rep]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    if method == "load" {
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ATOMICS_LOAD",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[view_h, idx, eb_v, sg_v]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    if method == "store" {
                        let val_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let val = ctx.coerce_to_i64(val_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ATOMICS_STORE",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[view_h, idx, eb_v, val]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                }
            }
            // (#266) Object globals: Object.keys, Object.values, Object.hasOwn.
            if let Some(method) = qualified.strip_prefix("Object.") {
                // Object.keys: usa OBJECT_KEYS_AUTO que cobre Map e Vec
                // (arrays retornam ["0","1",...] como JS spec).
                if method == "keys" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (cross-runtime #789) Object.getOwnPropertyNames — inclui
                // non-enumerable (diferenca para keys). Para arrays inclui
                // "length" como own property name.
                if method == "getOwnPropertyNames" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_OBJECT_OWN_PROPERTY_NAMES",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#798) Object.getOwnPropertySymbols(obj) — apos #753, Symbol
                // keys sao gravadas como `@@sym:<handle>` no Map. Decodifica
                // essas keys de volta para Vec de Symbol handles.
                if method == "getOwnPropertySymbols" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_SYMBOLS",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (cross-runtime #749) Object.getOwnPropertyDescriptors(obj).
                if method == "getOwnPropertyDescriptors" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // Object.getOwnPropertyDescriptor(obj, key) — reusa Reflect.
                // (#98) versao _PROXY: dispara o trap quando obj for Proxy,
                // senao cai em forward_get_own_property_descriptor.
                if method == "getOwnPropertyDescriptor" && call.args.len() == 2 {
                    let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let key_h = ctx.coerce_to_handle(key_tv)?.val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[obj_h, key_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                let target = match method {
                    "values" => "collections.map_values",
                    "hasOwn" => "collections.map_has",
                    _ => "",
                };
                if !target.is_empty() && lookup(target).is_some() {
                    return lower_ns_call(ctx, target, call);
                }
                // (#264 PR5) Object.create(proto) — aloca Map com __proto__.
                // (#162) Object.create(proto, descriptors) — 2-arg variant.
                // O segundo arg eh um Map de { key: { value, writable, ... } }.
                // Iteramos cada key e fazemos MAP_SET com value.
                if method == "create" && (call.args.len() == 1 || call.args.len() == 2) {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let proto_h = ctx.coerce_to_i64(arg_tv).val;
                    let create_fn = ctx.get_extern(
                        "__RTS_FN_GL_OBJECT_CREATE",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(create_fn, &[proto_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    if call.args.len() == 2 {
                        // Aplica descriptors via runtime helper.
                        let descs_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let descs_h = ctx.coerce_to_i64(descs_tv).val;
                        let apply_fn = ctx.get_extern(
                            "__RTS_FN_GL_OBJECT_APPLY_DESCRIPTORS",
                            &[cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(apply_fn, &[v, descs_h]);
                    }
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208 / #479) Object.entries(obj).
                if method == "entries" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208 / #479) Object.freeze/seal — v0 no-op, retorna handle.
                if (method == "freeze" || method == "seal") && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let sym = if method == "freeze" {
                        "__RTS_FN_NS_COLLECTIONS_MAP_FREEZE"
                    } else {
                        "__RTS_FN_NS_COLLECTIONS_MAP_SEAL"
                    };
                    let f = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208) Object.isFrozen/isSealed — v0 sempre false.
                if (method == "isFrozen" || method == "isSealed") && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let sym = if method == "isFrozen" {
                        "__RTS_FN_NS_COLLECTIONS_MAP_IS_FROZEN"
                    } else {
                        "__RTS_FN_NS_COLLECTIONS_MAP_IS_SEALED"
                    };
                    let f = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Bool));
                }
                // (#208) Object.getPrototypeOf(obj) — handle de __proto__ ou 0.
                if method == "getPrototypeOf" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (cross-runtime #772) Object.setPrototypeOf(obj, proto) —
                // reusa o impl de Reflect.setPrototypeOf.
                if method == "setPrototypeOf" && call.args.len() == 2 {
                    let target_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let target = ctx.coerce_to_i64(target_tv).val;
                    let proto_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let proto = ctx.coerce_to_i64(proto_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[target, proto]);
                    let _ = ctx.builder.inst_results(inst)[0];
                    // JS spec: Object.setPrototypeOf retorna o proprio target.
                    return Ok(TypedVal::new(target, ValTy::Handle));
                }
                // (#771) Object.isExtensible/preventExtensions — backed por
                // Set thread-safe de handles em collections.
                if method == "isExtensible" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_IS_EXTENSIBLE",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Bool));
                }
                if method == "preventExtensions" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_PREVENT_EXTENSIONS",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208) Object.defineProperty(obj, key, descriptor) — v0
                // suporta apenas { value: x }.
                // (#98) roteia via REFLECT_DEFINE_PROPERTY_PROXY: quando obj
                // for Proxy, dispara o trap `defineProperty`; senao cai em
                // forward_define_property (mesma semantica do antigo
                // MAP_DEFINE_PROPERTY, extraindo value+writable+enumerable).
                if method == "defineProperty" && call.args.len() == 3 {
                    let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let obj = ctx.coerce_to_i64(obj_tv).val;
                    let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let key_h = ctx.coerce_to_handle(key_tv)?.val;
                    let desc_tv = lower_expr(ctx, &call.args[2].expr)?;
                    let desc = ctx.coerce_to_i64(desc_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[obj, key_h, desc]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208 / #479) Object.fromEntries(arr).
                if method == "fromEntries" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                // (#208) Object.is(a, b) — egal-style equality:
                //   - NaN === NaN
                //   - 0 !== -0
                //   - mesma identidade caso contrario
                if method == "is" && call.args.len() == 2 {
                    use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
                    // (#872/134) Detecta `-N` literal ANTES de lower — em RTS
                    // `-0` lowera para iconst 0 perdendo o sinal. Materializa
                    // direto como f64const com sinal preservado quando arg
                    // for `-<num_lit>`.
                    let neg_num_lit_f64 = |e: &swc_ecma_ast::Expr| -> Option<f64> {
                        if let swc_ecma_ast::Expr::Unary(u) = e {
                            if u.op == swc_ecma_ast::UnaryOp::Minus {
                                if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(n)) = &*u.arg {
                                    return Some(-n.value);
                                }
                            }
                        }
                        None
                    };
                    let a_neg = neg_num_lit_f64(&call.args[0].expr);
                    let b_neg = neg_num_lit_f64(&call.args[1].expr);
                    let force_f64 = a_neg.is_some() || b_neg.is_some();
                    let a_tv = if let Some(v) = a_neg {
                        TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64)
                    } else {
                        lower_expr(ctx, &call.args[0].expr)?
                    };
                    let b_tv = if let Some(v) = b_neg {
                        TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64)
                    } else {
                        lower_expr(ctx, &call.args[1].expr)?
                    };
                    if force_f64 || matches!(a_tv.ty, ValTy::F64) || matches!(b_tv.ty, ValTy::F64) {
                        let af = if matches!(a_tv.ty, ValTy::F64) {
                            a_tv.val
                        } else {
                            let i = ctx.coerce_to_i64(a_tv).val;
                            ctx.builder.ins().fcvt_from_sint(cl::F64, i)
                        };
                        let bf = if matches!(b_tv.ty, ValTy::F64) {
                            b_tv.val
                        } else {
                            let i = ctx.coerce_to_i64(b_tv).val;
                            ctx.builder.ins().fcvt_from_sint(cl::F64, i)
                        };
                        // Bitwise: bitcast f64 → i64 e compara.
                        let abits = ctx.builder.ins().bitcast(cl::I64, cranelift_codegen::ir::MemFlags::new(), af);
                        let bbits = ctx.builder.ins().bitcast(cl::I64, cranelift_codegen::ir::MemFlags::new(), bf);
                        let eq = ctx.builder.ins().icmp(IntCC::Equal, abits, bbits);
                        // Excecao JS: NaN.is(NaN) === true. Bitwise sao iguais
                        // se sao a mesma representacao de NaN, mas spec diz
                        // que QUALQUER NaN.is(QUALQUER NaN). Detectamos via
                        // is_nan && is_nan.
                        let a_nan = ctx.builder.ins().fcmp(FloatCC::NotEqual, af, af);
                        let b_nan = ctx.builder.ins().fcmp(FloatCC::NotEqual, bf, bf);
                        let both_nan = ctx.builder.ins().band(a_nan, b_nan);
                        let result = ctx.builder.ins().bor(eq, both_nan);
                        return Ok(TypedVal::new(result, ValTy::Bool));
                    }
                    // (#1024) Strings sao handles distintos no HandleTable
                    // mesmo com conteudo igual. Object.is("a","a") deve ser
                    // true por spec — compara via gc.string_eq quando ambos
                    // sao Handle (string ou outro objeto). Para String, eh
                    // egal por conteudo (interning JS); para outros handles
                    // (objeto/array/Map), eq do handle equivale a identity.
                    if matches!(a_tv.ty, ValTy::Handle) && matches!(b_tv.ty, ValTy::Handle) {
                        let a = a_tv.val;
                        let b = b_tv.val;
                        // Fast path: identity equal -> true.
                        let id_eq = ctx.builder.ins().icmp(IntCC::Equal, a, b);
                        // Slow path: gc.string_eq compara conteudo (retorna
                        // 0/1 mesmo para handles que nao sao string).
                        let str_eq_fn = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_EQ",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(str_eq_fn, &[a, b]);
                        let content_eq_i64 = ctx.builder.inst_results(inst)[0];
                        let zero = ctx.builder.ins().iconst(cl::I64, 0);
                        let content_eq = ctx.builder.ins().icmp(IntCC::NotEqual, content_eq_i64, zero);
                        let result = ctx.builder.ins().bor(id_eq, content_eq);
                        return Ok(TypedVal::new(result, ValTy::Bool));
                    }
                    // Ambos integers — compara diretamente.
                    let a = ctx.coerce_to_i64(a_tv).val;
                    let b = ctx.coerce_to_i64(b_tv).val;
                    let eq = ctx.builder.ins().icmp(IntCC::Equal, a, b);
                    return Ok(TypedVal::new(eq, ValTy::Bool));
                }
                // (#208 / #479) Object.assign(target, ...sources). Loop por cada source.
                if method == "assign" && call.args.len() >= 2 {
                    let target_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let mut acc = ctx.coerce_to_i64(target_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_ASSIGN",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    for arg in &call.args[1..] {
                        if arg.spread.is_some() {
                            return Err(anyhow!("spread not supported in Object.assign"));
                        }
                        let s_tv = lower_expr(ctx, &arg.expr)?;
                        let s = ctx.coerce_to_i64(s_tv).val;
                        let inst = ctx.builder.ins().call(f, &[acc, s]);
                        acc = ctx.builder.inst_results(inst)[0];
                    }
                    return Ok(TypedVal::new(acc, ValTy::Handle));
                }
                // (#678/89) Object.groupBy(arr, fn) - ES2024. Agrupa elementos
                // por key retornado de fn(elem, idx). Retorna obj { key: [items] }.
                if method == "groupBy" && call.args.len() == 2 {
                    let arr_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let arr_h = ctx.coerce_to_i64(arr_tv).val;
                    let fn_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let fn_h = ctx.coerce_to_i64(fn_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_OBJECT_GROUP_BY",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[arr_h, fn_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
            }
            // (#678/89) Map.groupBy(arr, fn) - ES2024. Igual Object.groupBy mas retorna Map.
            if let Some(method) = qualified.strip_prefix("Map.") {
                if method == "groupBy" && call.args.len() == 2 {
                    let arr_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let arr_h = ctx.coerce_to_i64(arr_tv).val;
                    let fn_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let fn_h = ctx.coerce_to_i64(fn_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GROUP_BY",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[arr_h, fn_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
            }
            // (#218) Reflect API v0: get/set/has/deleteProperty/ownKeys.
            // Reusa as fns MAP_* (semantica identica a Object.*).
            // Nota: ownKeys retorna sorted (mesma limitacao de Object.keys).
            if let Some(method) = qualified.strip_prefix("Reflect.") {
                match method {
                    "get" if call.args.len() >= 2 && call.args.len() <= 3 => {
                        // (cross-runtime #53) Usa MAP_GET_KH que aceita key
                        // como string handle (em vez de StrPtr ptr+len que
                        // requer literal/ann conhecido em compile-time).
                        // Cobre `Reflect.get(t, prop)` em fn body onde prop
                        // eh param Handle (string em runtime).
                        // 3o arg (receiver) ignorado.
                        // (#795) Marca como ambiguo pra TPL_COERCE_AUTO
                        // resolver bool sentinels e string handles em concat.
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_handle(key_tv)?.val;
                        let get_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_GET_KH",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(get_fn, &[obj_h, key_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        ctx.var_member_call_values.insert(v);
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    "has" if call.args.len() == 2 => {
                        // (cross-runtime #53) Usa OBJ_HAS que aceita key handle
                        // (em vez de StrPtr ptr+len). Tambem dispatch Proxy.
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_handle(key_tv)?.val;
                        let has_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_OBJ_HAS",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(has_fn, &[obj_h, key_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Bool));
                    }
                    "ownKeys" if call.args.len() == 1 => {
                        return lower_ns_call(ctx, "collections.map_keys", call);
                    }
                    "deleteProperty" if call.args.len() == 2 => {
                        // (cross-runtime #53) MAP_DELETE_AUTO aceita key handle
                        // (vs StrPtr) e dispatcha Proxy `deleteProperty` trap.
                        // JS spec: Reflect.deleteProperty retorna true tanto
                        // para chave existente quanto inexistente (so' falha
                        // em props nao-configurable, que RTS v0 nao distingue).
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_handle(key_tv)?.val;
                        let del_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_DELETE_AUTO",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        ctx.builder.ins().call(del_fn, &[obj_h, key_h]);
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    "set" if call.args.len() == 3 || call.args.len() == 4 => {
                        // (cross-runtime #53) OBJ_SET aceita key handle (vs
                        // StrPtr) e dispatcha Proxy `set` trap.
                        // 4o arg (receiver) eh ignorado — RTS v0 nao tem
                        // receiver-aware getter/setter dispatch via Proxy.
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_handle(key_tv)?.val;
                        let val_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let val = ctx.coerce_to_i64(val_tv).val;
                        let set_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_OBJ_SET",
                            &[cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(set_fn, &[obj_h, key_h, val]);
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    // (#218) Reflect.getPrototypeOf — reusa Object.getPrototypeOf.
                    "getPrototypeOf" if call.args.len() == 1 => {
                        let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let h = ctx.coerce_to_i64(arg_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Handle));
                    }
                    // (#218 phase3) Reflect.setPrototypeOf — wrapper proxy-aware.
                    "setPrototypeOf" if call.args.len() == 2 => {
                        let target_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let target = ctx.coerce_to_i64(target_tv).val;
                        let proto_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let proto = ctx.coerce_to_i64(proto_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[target, proto]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Bool));
                    }
                    // (#771) Reflect.isExtensible — backed pelo mesmo Set.
                    "isExtensible" if call.args.len() == 1 => {
                        let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let h = ctx.coerce_to_i64(arg_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_IS_EXTENSIBLE",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Bool));
                    }
                    // (#771) Reflect.preventExtensions — retorna true (success).
                    "preventExtensions" if call.args.len() == 1 => {
                        let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let h = ctx.coerce_to_i64(arg_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_PREVENT_EXTENSIONS",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        ctx.builder.ins().call(f, &[h]);
                        let t = ctx.builder.ins().iconst(cl::I8, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    // (#218) Reflect.apply(fn, thisArg, argsArray) — reusa
                    // __RTS_FN_GL_FUNCTION_APPLY (mesma assinatura).
                    "apply" if call.args.len() == 3 => {
                        let fn_h = lower_callable_target_h(ctx, &call.args[0].expr)?;
                        let this_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let this_v = ctx.coerce_to_i64(this_tv).val;
                        let args_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let args_h = ctx.coerce_to_i64(args_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_FUNCTION_APPLY_TYPED",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[fn_h, this_v, args_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        // (cross-runtime #799) FUNCTION_APPLY retorna i64 que
                        // pode ser bits f64 (callee f64-typed) ou int direto.
                        // Marca como var_member_call_value pra que
                        // TPL_COERCE_AUTO formate certo em concat.
                        ctx.var_member_call_values.insert(v);
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    // (#218) Reflect.construct(Target, args) — semantica de
                    // `new Target(...args)`. Reusa o trampolim Function:
                    // aloca Map (instancia), chama o constructor com THIS_PUSH
                    // implicito via FUNCTION_APPLY com `this = inst`. Deixa
                    // newTarget como follow-up (afeta prototype chain).
                    "construct" if matches!(call.args.len(), 2 | 3) => {
                        // (cross-runtime #1127) User class como target: monta
                        // NewExpr sintetico e chama lower_new. Sem isso, Reflect.construct
                        // tratava class como Function handle e retornava instancia com
                        // campos null.
                        if let Expr::Ident(target_id) = call.args[0].expr.as_ref() {
                            let cls = target_id.sym.as_str();
                            if ctx.classes.contains_key(cls) {
                                // Args eh array literal? Extrai elementos pra passar como new args.
                                if let Expr::Array(arr) = call.args[1].expr.as_ref() {
                                    let mut new_args: Vec<swc_ecma_ast::ExprOrSpread> = Vec::new();
                                    for elem_opt in &arr.elems {
                                        if let Some(elem) = elem_opt {
                                            new_args.push(elem.clone());
                                        }
                                    }
                                    let new_expr_synth = swc_ecma_ast::NewExpr {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        callee: Box::new(Expr::Ident(target_id.clone())),
                                        args: Some(new_args),
                                        type_args: None,
                                    };
                                    return self::new_expr::lower_new(ctx, &new_expr_synth);
                                }
                            }
                        }
                        let target_h = lower_callable_target_h(ctx, &call.args[0].expr)?;
                        let args_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let args_h = ctx.coerce_to_i64(args_tv).val;
                        // (#218 phase2) Wrapper detecta Proxy e dispara trap;
                        // senao, faz alocacao+apply como antes. Centraliza
                        // pra evitar dois caminhos divergindo.
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_CONSTRUCT",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[target_h, args_h]);
                        let h = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(h, ValTy::Handle));
                    }
                    // (#218) Reflect.getOwnPropertyDescriptor(obj, key) — v0:
                    // retorna { value, writable: true, enumerable: true,
                    // configurable: true } sintetizado. Descriptors reais
                    // exigiriam metadata por slot no Map (out of scope v0).
                    "getOwnPropertyDescriptor" if call.args.len() == 2 => {
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_i64(key_tv).val;
                        // (#218 phase3) Wrapper proxy-aware substitui o
                        // REFLECT_GET_OWN_PROPERTY_DESCRIPTOR antigo.
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[obj_h, key_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Handle));
                    }
                    // (#218) Reflect.defineProperty(obj, key, descriptor) —
                    // v0: extrai `value` do descriptor (Map) e faz map_set.
                    // Ignora writable/enumerable/configurable e get/set
                    // accessors (descriptors com fns sao convertidos como
                    // value=undefined). Retorna true.
                    "defineProperty" if call.args.len() == 3 => {
                        let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let key_h = ctx.coerce_to_i64(key_tv).val;
                        let desc_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let desc_h = ctx.coerce_to_i64(desc_tv).val;
                        // (#218 phase3) Wrapper proxy-aware substitui o
                        // REFLECT_DEFINE_PROPERTY antigo.
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[obj_h, key_h, desc_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::Bool));
                    }
                    _ => {}
                }
            }
            // (#306/#305) `Iterator.from(arr)` — modelo EAGER: devolve o proprio
            // array (Vec) marcado i64-ambiguo, para que a chain de helpers
            // (`.map/.filter/.take/.drop/.reduce/.toArray`) despache como array
            // methods (o receiver-Call dispatch em ~1144 reconhece i64+array
            // method). take/drop/toArray sao array methods adicionados.
            if qualified == "Iterator.from" && call.args.len() == 1
                && call.args[0].spread.is_none()
            {
                let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                let src_h = ctx.coerce_to_i64(arg_tv).val;
                ctx.var_member_call_values.insert(src_h);
                return Ok(TypedVal::new(src_h, ValTy::I64));
            }
            // (#208 / #476) Array static globals: isArray, from.
            if let Some(method) = qualified.strip_prefix("Array.") {
                // (#861) Array.fromAsync(iter, mapper?) — Promise<Array>.
                // Suporte parcial: array sync de promises (ou valores). Async
                // generator depende de #211 (state machine generator).
                if method == "fromAsync" && (call.args.len() == 1 || call.args.len() == 2) {
                    if call.args.iter().any(|a| a.spread.is_some()) {
                        return Err(anyhow!("spread not supported in Array.fromAsync"));
                    }
                    let iter_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let iter_h = ctx.coerce_to_i64(iter_tv).val;
                    let mapper_h = if call.args.len() == 2 {
                        lower_callable_target_h(ctx, &call.args[1].expr)?
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    };
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_ARRAY_FROM_ASYNC",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[iter_h, mapper_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
                if method == "isArray" && call.args.len() == 1
                    && call.args[0].spread.is_none()
                {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let arg_h = ctx.coerce_to_i64(arg_tv).val;
                    let is_vec_fn = ctx.get_extern(
                        "__RTS_FN_NS_GC_IS_VEC",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(is_vec_fn, &[arg_h]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Bool));
                }
                // (#208) Array.of(...args) — cria Vec com cada arg.
                if method == "of" {
                    if call.args.iter().any(|a| a.spread.is_some()) {
                        return Err(anyhow!("spread not supported in Array.of"));
                    }
                    let new_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                        &[],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(new_fn, &[]);
                    let vec_h = ctx.builder.inst_results(inst)[0];
                    let push_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    for arg in &call.args {
                        let tv = lower_expr(ctx, &arg.expr)?;
                        // Mesma sentinela de array literal (members.rs).
                        let v = if matches!(tv.ty, ValTy::Bool) {
                            let b = ctx.coerce_to_i64(tv).val;
                            let min = ctx.builder.ins().iconst(cl::I64, i64::MIN);
                            ctx.builder.ins().iadd(min, b)
                        } else {
                            ctx.coerce_to_i64(tv).val
                        };
                        ctx.builder.ins().call(push_fn, &[vec_h, v]);
                    }
                    return Ok(TypedVal::new(vec_h, ValTy::Handle));
                }
                // (#208) Array.from(arrayLike, mapper?). Aceita 2 formas:
                //   1. Array.from({length: N}, fn?) — gera Vec [fn(0,0), fn(1,1), ...]
                //      Detecta object literal com unica key "length".
                //   2. Array.from(vecHandle, fn?) — converte/mapeia Vec existente.
                // mapper e' Ident de user fn (lift de arrow inline fica em
                // PR separada — usuario pode definir `function f(_, i)` antes).
                if method == "from" && (call.args.len() == 1 || call.args.len() == 2) {
                    let first = &call.args[0];
                    if first.spread.is_some() {
                        return Err(anyhow!("spread not supported in Array.from"));
                    }
                    // (#216/#271) `Array.from(c)` onde `c` eh instancia de classe
                    // iteravel (`[Symbol.iterator]`): drena via
                    // `c.__rts_sym_iterator()` (que devolve um iterator/Vec).
                    // Reescreve para `Array.from(c.__rts_sym_iterator())`.
                    // Peel `c as any`/`(c)`/`c!` no source.
                    fn peel_src(e: &Expr) -> &Expr {
                        match e {
                            Expr::TsAs(a) => peel_src(&a.expr),
                            Expr::TsConstAssertion(a) => peel_src(&a.expr),
                            Expr::TsNonNull(a) => peel_src(&a.expr),
                            Expr::Paren(p) => peel_src(&p.expr),
                            _ => e,
                        }
                    }
                    if let Expr::Ident(src_id) = peel_src(first.expr.as_ref()) {
                        let is_iter_inst = ctx
                            .local_class_ty
                            .get(src_id.sym.as_str())
                            .map(|cls| {
                                crate::codegen::lower::passes::custom_iterator::is_iter_class(cls)
                            })
                            .unwrap_or(false);
                        if is_iter_inst {
                            let it_call = Expr::Call(swc_ecma_ast::CallExpr {
                                span: call.span,
                                ctxt: call.ctxt,
                                callee: Callee::Expr(Box::new(Expr::Member(
                                    swc_ecma_ast::MemberExpr {
                                        span: call.span,
                                        obj: Box::new(Expr::Ident(src_id.clone())),
                                        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                                            span: call.span,
                                            sym: "__rts_sym_iterator".into(),
                                        }),
                                    },
                                ))),
                                args: Vec::new(),
                                type_args: None,
                            });
                            let mut new_args = call.args.clone();
                            new_args[0] = swc_ecma_ast::ExprOrSpread {
                                spread: None,
                                expr: Box::new(it_call),
                            };
                            let synth = swc_ecma_ast::CallExpr {
                                span: call.span,
                                ctxt: call.ctxt,
                                callee: call.callee.clone(),
                                args: new_args,
                                type_args: None,
                            };
                            return super::lower_call(ctx, &synth);
                        }
                    }
                    // Resolve mapper fn_ptr (0 se ausente).
                    let fn_ptr = if call.args.len() == 2 {
                        let arg = &call.args[1];
                        if arg.spread.is_some() {
                            return Err(anyhow!("spread not supported in Array.from"));
                        }
                        match arg.expr.as_ref() {
                            Expr::Ident(id) => {
                                let fn_name = id.sym.as_str().to_string();
                                if ctx.user_fns.contains_key(&fn_name)
                                    && ctx.var_ty(&fn_name).is_none()
                                {
                                    let tv = emit_user_fn_addr(ctx, &fn_name)?;
                                    ctx.coerce_to_i64(tv).val
                                } else {
                                    ctx.builder.ins().iconst(cl::I64, 0)
                                }
                            }
                            _ => ctx.builder.ins().iconst(cl::I64, 0),
                        }
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    };
                    // (cross-runtime) Array.from(src, mapper) onde mapper captura
                    // var local (`__lifted_cap_*`). O fn_ptr nu nao carrega os
                    // bound_args -> mapper recebia o idx no slot da captura e
                    // produzia lixo. Roteia ao caminho BOUND: gera o Vec fonte e
                    // chama PARALLEL_MAP_BOUND(vec, fn_handle), que invoca via
                    // invoke_array_callback (prepende bound_args ++ [val, idx]).
                    // Para `{length:n}` o Vec fonte eh [0..n-1]; o mapper
                    // `(_, i) => ...` le i=idx == val, equivalente.
                    if call.args.len() == 2 {
                        if let Expr::Ident(mid) = call.args[1].expr.as_ref() {
                            let mname = mid.sym.as_str();
                            if mname.starts_with("__lifted_cap_") {
                                let caps: Vec<String> =
                                    crate::codegen::lower::passes::parallelism::LIFTED_CAPTURES
                                        .with(|c| c.borrow().get(mname).cloned())
                                        .unwrap_or_default();
                                if !caps.is_empty() {
                                    let mut cap_vals: Vec<TypedVal> = Vec::with_capacity(caps.len());
                                    let mut ok = true;
                                    for cap in &caps {
                                        match ctx.read_local(cap) {
                                            Some(tv) => cap_vals.push(tv),
                                            None => { ok = false; break; }
                                        }
                                    }
                                    if ok {
                                        // Vec fonte: {length:n} -> [0..n-1]; senao src.
                                        let src_vec = build_array_from_source_vec(ctx, &first.expr)?;
                                        let fn_handle = emit_lifted_arrow_handle_with_captures(
                                            ctx, mname, &cap_vals,
                                        )?.val;
                                        let f = ctx.get_extern(
                                            "__RTS_FN_NS_PARALLEL_MAP_BOUND",
                                            &[cl::I64, cl::I64],
                                            Some(cl::I64),
                                        )?;
                                        let inst = ctx.builder.ins().call(f, &[src_vec, fn_handle]);
                                        let v = ctx.builder.inst_results(inst)[0];
                                        ctx.declare_gc_handle(v);
                                        return Ok(TypedVal::new(v, ValTy::Handle));
                                    }
                                }
                            }
                        }
                    }
                    // Detecta `{length: <expr>}` — N literal OU expressao
                    // dinamica (`m + 1`). (#363) Antes so' literal; expr caia no
                    // caminho BOUND/source_vec que nao persistia handles de linha
                    // (matriz 2D `Array.from({length:m+1}, () => new Array(n+1))`
                    // lia 0). ARRAY_FROM_LENGTH(n, fn_ptr) aplica o mapper e
                    // armazena os handles corretamente (mesmo path do literal).
                    if let Expr::Object(obj_lit) = first.expr.as_ref() {
                        let mut length_expr: Option<&Expr> = None;
                        for prop in &obj_lit.props {
                            if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                                if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                                    let key = match &kv.key {
                                        swc_ecma_ast::PropName::Ident(i) => Some(i.sym.as_str().to_string()),
                                        swc_ecma_ast::PropName::Str(s) => Some(s.value.to_string_lossy().to_string()),
                                        _ => None,
                                    };
                                    if key.as_deref() == Some("length") {
                                        length_expr = Some(kv.value.as_ref());
                                    }
                                }
                            }
                        }
                        if let Some(len_e) = length_expr {
                            let n = if let Expr::Lit(swc_ecma_ast::Lit::Num(num)) = len_e {
                                ctx.builder.ins().iconst(cl::I64, num.value as i64)
                            } else {
                                let tv = lower_expr(ctx, len_e)?;
                                ctx.coerce_to_i64(tv).val
                            };
                            let f = ctx.get_extern(
                                "__RTS_FN_GL_ARRAY_FROM_LENGTH",
                                &[cl::I64, cl::I64],
                                Some(cl::I64),
                            )?;
                            let inst = ctx.builder.ins().call(f, &[n, fn_ptr]);
                            let v = ctx.builder.inst_results(inst)[0];
                            return Ok(TypedVal::new(v, ValTy::Handle));
                        }
                    }
                    // Array.from(string): split em chars via STRING_SPLIT(s, "").
                    // Se ha mapper, aplica via ARRAY_FROM_VEC apos split.
                    let is_string_arg = matches!(
                        first.expr.as_ref(),
                        Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
                    );
                    if is_string_arg {
                        let s_tv = lower_expr(ctx, &first.expr)?;
                        let s_h = ctx.coerce_to_handle(s_tv)?.val;
                        let empty = ctx.emit_str_handle(b"")?.val;
                        let split_fn = ctx.get_extern(
                            "__RTS_FN_GL_STRING_SPLIT",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(split_fn, &[s_h, empty]);
                        let split_h = ctx.builder.inst_results(inst)[0];
                        // Sem mapper: passthrough do vec gerado.
                        let zero = ctx.builder.ins().iconst(cl::I64, 0);
                        let has_mapper = call.args.len() == 2;
                        if !has_mapper {
                            return Ok(TypedVal::new(split_h, ValTy::Handle));
                        }
                        // Aplica mapper via ARRAY_FROM_VEC (mesma rotina
                        // do caso Vec normal).
                        let from_vec = ctx.get_extern(
                            "__RTS_FN_GL_ARRAY_FROM_VEC",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let _ = zero;
                        let inst2 = ctx.builder.ins().call(from_vec, &[split_h, fn_ptr]);
                        let v = ctx.builder.inst_results(inst2)[0];
                        return Ok(TypedVal::new(v, ValTy::Handle));
                    }
                    // Fallback: src e' Vec handle.
                    let src_tv = lower_expr(ctx, &first.expr)?;
                    let src = ctx.coerce_to_i64(src_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_ARRAY_FROM_VEC",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[src, fn_ptr]);
                    let v = ctx.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(v, ValTy::Handle));
                }
            }
            // Function global (#359): `<fn>.call/.apply/.bind/.toString(...)`.
            // Dois caminhos:
            //   (a) <userFn>.method(...) — reifica o ident e despacha
            //   (b) <var>.method(...) onde var eh handle Function — chamada direta
            if let Expr::Member(m) = callee.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    let prop_name = prop.sym.as_str();
                    if matches!(prop_name, "call" | "apply" | "bind" | "toString") {
                        // Peel TsAs/Paren em obj para suportar
                        // \`(Animal as any).call(this, ...)\`.
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
                            // (a) user fn direta
                            if ctx.user_fns.contains_key(obj_name) && ctx.var_ty(obj_name).is_none() {
                                if let Some(tv) = lower_function_method_call(ctx, obj_name, prop_name, call)? {
                                    return Ok(tv);
                                }
                            }
                            // (b) var handle (de bind ou new Function) — pula
                            // primitivos (Bool/F64/I32) pra deixar dispatch
                            // correto cair em lower_var_member_call.
                            // (#proto-instance) Tambem pula proto-instance pra
                            // que `a.toString()` use prototype.toString do user.
                            let is_proto_instance = ctx
                                .local_class_ty
                                .get(obj_name)
                                .map(|s| s == "__proto_instance")
                                .unwrap_or(false);
                            if let Some(var_ty) = ctx.var_ty(obj_name) {
                                // (cross-runtime #58) I64 tambem eh primitivo
                                // numerico aqui — sem isso, `b.toString(16)`
                                // em param ambíguo de map() cai em
                                // lower_function_handle_method e crasha.
                                let is_primitive = matches!(
                                    var_ty,
                                    crate::codegen::lower::ctx::ValTy::Bool
                                        | crate::codegen::lower::ctx::ValTy::F64
                                        | crate::codegen::lower::ctx::ValTy::I32
                                        | crate::codegen::lower::ctx::ValTy::I64
                                );
                                if !is_primitive && !is_proto_instance {
                                    if let Some(tv) = lower_function_handle_method(ctx, &m.obj, prop_name, call)? {
                                        return Ok(tv);
                                    }
                                }
                            }
                        }
                        // (c) `<expr>.bind/call/apply/toString` onde <expr> não é
                        // Ident — ex: `c.add.bind(c)`. Avalia obj e se for Handle,
                        // despacha via FUNCTION_*.
                        if !matches!(obj_e, Expr::Ident(_)) {
                            if let Some(tv) = lower_function_handle_method(ctx, &m.obj, prop_name, call)? {
                                return Ok(tv);
                            }
                        }
                    }
                }
            }
            // Fallback: ident.fn(...) onde ident e var (ex: namespace TS
            // desugared para const Foo = { ... }). Faz map_get pela key
            // e despacha via call_indirect.
            // (#264 PR5) Peel TsAs/Paren no obj para `(c as any).method(...)`.
            if let Expr::Member(m) = callee.as_ref() {
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
                    // (cross-runtime #46) `NaN.toFixed(2)` / `Infinity.toFixed(2)`
                    // — globais f64 nao sao var, mas precisam aceitar
                    // Number.prototype methods. Lower obj como F64 e usa
                    // number builtin path.
                    let global_name = obj_id.sym.as_str();
                    if matches!(global_name, "NaN" | "Infinity") {
                        if let MemberProp::Ident(prop) = &m.prop {
                            let obj_tv = crate::codegen::lower::expressions::lower_expr(ctx, &m.obj)?;
                            // GENÉRICO via Registry — sem nomes de método hardcoded.
                            if let Some(tv) = ns_call::try_global_class_instance_method(
                                ctx,
                                "Number",
                                prop.sym.as_str(),
                                obj_tv,
                                call,
                            )? {
                                return Ok(tv);
                            }
                        }
                    }
                }
            }
            return lower_ns_call(ctx, &qualified, call);
        }
        // (#264) Fallback fora do qualified path: \`(Animal as any).call(this, ...)\`
        // — qualified_member_name retorna None por causa do TsAs, entao
        // este block roda separado pra peelar e roteár.
        if let Expr::Member(m) = callee.as_ref() {
            if let MemberProp::Ident(prop) = &m.prop {
                let prop_name = prop.sym.as_str();
                if matches!(prop_name, "call" | "apply" | "bind" | "toString") {
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
                            if let Some(tv) = lower_function_method_call(ctx, obj_name, prop_name, call)? {
                                return Ok(tv);
                            }
                        }
                        // (#proto-instance) Var marcada como instance via constructor
                        // function — skip Function.toString builtin pra que o lookup
                        // de prototype.toString pelo MAP_GET_CHAIN prevaleça.
                        let is_proto_instance = ctx
                            .local_class_ty
                            .get(obj_name)
                            .map(|s| s == "__proto_instance")
                            .unwrap_or(false);
                        if ctx.var_ty(obj_name).is_some() && !is_proto_instance {
                            if let Some(tv) = lower_function_handle_method(ctx, &m.obj, prop_name, call)? {
                                return Ok(tv);
                            }
                        }
                    }
                }
            }
        }
        // (cross-runtime #746) Chained call em obj.prop (obj nao Ident):
        // `u.searchParams.get("q")` — m.obj eh Member. Lower o obj
        // como expr (gera handle), depois tenta GLOBAL_CLASS_SPECS
        // instance_method e mapeia args.
        if let Expr::Member(m) = callee.as_ref() {
            if matches!(m.obj.as_ref(), Expr::Member(_) | Expr::OptChain(_) | Expr::Call(_)) {
                if let MemberProp::Ident(prop_id) = &m.prop {
                    let prop_name = prop_id.sym.as_str();
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    // (#97) Antes de tentar GLOBAL_CLASS_SPECS, tenta
                    // array/string/map builtins — cobre o caso comum
                    // `copy.list.push(4)` onde `copy.list` retorna handle
                    // mas push nao esta em nenhum spec global.
                    if let Some(tv) = self::builtins::lower_array_builtin(
                        ctx, prop_name, obj_h, call,
                    )? {
                        return Ok(tv);
                    }
                    if let Some(tv) = self::builtins::lower_string_builtin(
                        ctx, prop_name, obj_h, call,
                    )? {
                        return Ok(tv);
                    }
                    if let Some(tv) = self::builtins::lower_map_set_builtin(
                        ctx, prop_name, obj_h, call,
                    )? {
                        return Ok(tv);
                    }
                    for spec in crate::abi::registry_classes_ordered() {
                        if let Some(member) = spec.resolve_instance_method(prop_name, call.args.len()) {
                            let sig = crate::abi::signature::lower_member(member);
                            let f = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;
                            let mut args: Vec<cranelift_codegen::ir::Value> = vec![obj_h];
                            // Coerce args conforme assinatura (skip o `Handle` do receiver).
                            for (i, abi_ty) in member.args.iter().skip(1).enumerate() {
                                if let Some(arg) = call.args.get(i) {
                                    let tv = lower_expr(ctx, &arg.expr)?;
                                    match abi_ty {
                                        crate::abi::AbiType::StrPtr => {
                                            let h = ctx.coerce_to_handle(tv)?.val;
                                            let pf = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                                            let lf = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                                            let ip = ctx.builder.ins().call(pf, &[h]);
                                            let pp = ctx.builder.inst_results(ip)[0];
                                            let il = ctx.builder.ins().call(lf, &[h]);
                                            let ll = ctx.builder.inst_results(il)[0];
                                            args.push(pp);
                                            args.push(ll);
                                        }
                                        _ => {
                                            let v = ctx.coerce_to_i64(tv).val;
                                            args.push(v);
                                        }
                                    }
                                }
                            }
                            let inst = ctx.builder.ins().call(f, &args);
                            let v = if sig.ret.is_some() {
                                ctx.builder.inst_results(inst)[0]
                            } else {
                                ctx.builder.ins().iconst(cl::I64, 0)
                            };
                            let ret_ty = ValTy::from_abi(member.returns);
                            // Marca como ambiguo pra template literal funcionar.
                            ctx.var_member_call_values.insert(v);
                            return Ok(TypedVal::new(v, ret_ty));
                        }
                    }
                }
            }
        }
        // (cross-runtime #300 / #1281 curry) Call em result de outra Call
        // (`add3(1)(2)(3)`, `Function("body")()`, `obj.getFn()()`). O callee-call
        // retorna handle de arrow liftada i64-ABI (le params via fcvt_from_sint,
        // espera INTEIRO). lower_curry_call empaca os args como inteiro p/ casar
        // com as capturas (REIFY) — sem isso o nivel final recebia bits-f64 e
        // corrompia (`add3(1)(2)(3)` dava 3.0000…013).
        if matches!(callee.as_ref(), Expr::Call(_)) {
            return self::indirect::lower_curry_call(ctx, callee, call);
        }
        if let Expr::Ident(id) = callee.as_ref() {
            let name = id.sym.as_str();
            // Globais JS \`isNaN\`/\`isFinite\`/\`Number\`/\`String\`/\`Boolean\`
            // resolvidos antes de cair em user_call (que falharia com
            // \"undeclared user function\").
            if let Some(tv) = lower_js_global_call(ctx, name, call)? {
                return Ok(tv);
            }
            // node: named imports: `import { readFileSync } from "node:fs"`
            // node_import_map["readFileSync"] = "node_fs.readFileSync"
            if let Some(qualified) = ctx.node_import_map.get(name).cloned() {
                if crate::nodespace::node_lookup(&qualified).is_some() {
                    return lower_node_ns_call(ctx, &qualified, call);
                }
            }
            // Resolve alias antes de lookup user_fns: `plus(...)` quando
            // `import { add as plus }` foi registrado redireciona pra `add`.
            let resolved = ctx.resolve_alias(name).to_string();
            if ctx.user_fns.contains_key(resolved.as_str()) && ctx.var_ty(name).is_none() {
                return lower_user_call(ctx, &resolved, call);
            }
            if ctx.var_ty(name).is_some() {
                return lower_indirect_call(ctx, callee, call);
            }
            return lower_user_call(ctx, &resolved, call);
        }
        // (cross-runtime #1067) Callee eh member computed (`obj[k](args)`,
        // `arr[i](x)`) ou outra expressao que produz fn handle. Lower o
        // callee como expressao, depois faz indirect call via FUNCTION_APPLY.
        if let Expr::Member(m) = callee.as_ref() {
            if let MemberProp::Computed(c) = &m.prop {
                // (#211/#222) `arr[Symbol.iterator]()` — emite ARRAY_VALUES_ITER
                // direto com `this=arr`. Sem isto cai no indirect call via
                // FUNCTION_APPLY, que NAO liga `this`, e ARRAY_VALUES_ITER roda
                // com this=0 -> ITERATOR_FROM(0) -> iterator vazio (zip/iterator
                // protocol rendia 0 elementos).
                let key_is_symbol_iterator = matches!(
                    c.expr.as_ref(),
                    Expr::Member(sm)
                        if matches!(
                            (sm.obj.as_ref(), &sm.prop),
                            (Expr::Ident(o), MemberProp::Ident(p))
                                if o.sym.as_str() == "Symbol" && p.sym.as_str() == "iterator"
                        )
                );
                if key_is_symbol_iterator && call.args.is_empty() {
                    use cranelift_codegen::ir::types as cl;
                    let recv_tv = lower_expr(ctx, &m.obj)?;
                    let recv = ctx.coerce_to_i64(recv_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_GL_ARRAY_VALUES_ITER",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[recv]);
                    let h = ctx.builder.inst_results(inst)[0];
                    ctx.declare_gc_handle(h);
                    return Ok(TypedVal::new(h, ValTy::Handle));
                }
                // (cross-runtime #1052) `arr[<methodName>](args)` quando a key
                // resolve estaticamente para string method name (push/slice/etc):
                // reescreve como `arr.<method>(args)` em compile time para
                // ir pelo path otimizado (sem crashar em fn handle invalido).
                let static_key: Option<String> = match c.expr.as_ref() {
                    Expr::Lit(swc_ecma_ast::Lit::Str(s)) => Some(s.value.to_string_lossy().to_string()),
                    // (cross-runtime #1052) Constant propagation: `k[N]`
                    // quando `k` eh top-level const array de strings.
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
                if let Some(method) = static_key {
                    let is_array_method = matches!(
                        method.as_str(),
                        "slice" | "concat" | "push" | "pop" | "shift" | "unshift"
                        | "indexOf" | "lastIndexOf" | "includes" | "join"
                        | "reverse" | "filter" | "map" | "forEach" | "find"
                        | "findIndex" | "every" | "some" | "reduce" | "flat"
                        | "splice" | "sort" | "fill" | "copyWithin" | "at"
                    );
                    if is_array_method {
                        let synth_callee = Expr::Member(swc_ecma_ast::MemberExpr {
                            span: m.span,
                            obj: m.obj.clone(),
                            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                                span: m.span,
                                sym: method.into(),
                            }),
                        });
                        let synth_call = swc_ecma_ast::CallExpr {
                            span: call.span,
                            ctxt: call.ctxt,
                            callee: Callee::Expr(Box::new(synth_callee)),
                            args: call.args.clone(),
                            type_args: None,
                        };
                        return super::lower_call(ctx, &synth_call);
                    }
                }
                // (#216) `obj[X]()` onde X eh `const X = "k"` / `Symbol.for(..)`
                // / `Symbol.iterator` -> despacha como metodo `obj.<canonical>()`.
                // Cobre `c[reg]()` (271). A chave canonica casa com o nome do
                // metodo resolvido em prop_name_to_string.
                if let Expr::Ident(key_id) = c.expr.as_ref() {
                    if let Some(canon) =
                        crate::parser::const_string_value(key_id.sym.as_str())
                    {
                        let synth_callee = Expr::Member(swc_ecma_ast::MemberExpr {
                            span: m.span,
                            obj: m.obj.clone(),
                            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                                span: m.span,
                                sym: canon.into(),
                            }),
                        });
                        let synth_call = swc_ecma_ast::CallExpr {
                            span: call.span,
                            ctxt: call.ctxt,
                            callee: Callee::Expr(Box::new(synth_callee)),
                            args: call.args.clone(),
                            type_args: None,
                        };
                        return super::lower_call(ctx, &synth_call);
                    }
                }
                return lower_indirect_call(ctx, callee, call);
            }
        }
    }
    // (#41 closures_deep) Callee eh uma expr callable nao coberta pelos casos
    // acima — tipicamente `f(x)(y)` (call-of-call) ou `(expr)(args)`. O callee
    // produz um fn_ptr/handle Function; despacha via lower_indirect_call.
    if let Callee::Expr(callee) = &call.callee {
        // (#1281 curry N-nivel) callee-call (`add3(1)(2)(3)`) retorna arrow
        // liftada i64-ABI — empaca args como INTEIRO via lower_curry_call.
        if matches!(callee.as_ref(), Expr::Call(_)) {
            return lower_curry_call(ctx, callee, call);
        }
        if matches!(callee.as_ref(),
            Expr::Paren(_) | Expr::Cond(_) | Expr::Bin(_)
        ) {
            return lower_indirect_call(ctx, callee, call);
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

    // `import(path)` returns a handle to the module's exports namespace object
    // (collected in-process by runtime_import_module_jit). Member access
    // (`mod.named`, `mod.default(...)`, `mod.state.count`) then resolves via the
    // generic object/map dispatch on the Handle.
    lower_ns_call(ctx, "runtime.import_module", &CallExpr {
        span: call.span,
        callee: call.callee.clone(),
        args: vec![path_arg.clone()],
        type_args: None,
        ctxt: Default::default(),
    })
    .map(|tv| crate::codegen::lower::ctx::TypedVal { val: tv.val, ty: ValTy::Handle })
}

/// Globais JS funcionais: \`isNaN\`, \`isFinite\`, \`Number\`, \`String\`, \`Boolean\`.
/// Retornam Some(tv) quando match, None pra deixar o caller resolver como
/// user fn / indirect.
fn lower_js_global_call(
    ctx: &mut FnCtx,
    name: &str,
    call: &CallExpr,
) -> Result<Option<crate::codegen::lower::ctx::TypedVal>> {
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    match name {
        // (cross-runtime #178) `Array(n)` / `Array(...)` sem `new` — JS spec
        // trata identico a `new Array(...)`. 1 arg numerico = length, senao push de cada arg.
        "Array" => {
            use crate::codegen::lower::ctx::{TypedVal, ValTy};
            let n_args = call.args.len();
            if n_args == 1 && call.args[0].spread.is_none() {
                let tv = super::lower_expr(ctx, &call.args[0].expr)?;
                if matches!(tv.ty, ValTy::I64 | ValTy::I32 | ValTy::F64 | ValTy::U64) {
                    let len_i64 = ctx.coerce_to_i64(tv).val;
                    let f = ctx.get_extern("__RTS_FN_GL_ARRAY_NEW_WITH_LENGTH", &[cl::I64], Some(cl::I64))?;
                    let inst = ctx.builder.ins().call(f, &[len_i64]);
                    let h = ctx.builder.inst_results(inst)[0];
                    return Ok(Some(TypedVal::new(h, ValTy::Handle)));
                }
                // Handle: cai pro fallback abaixo (Array.of-like, push do unico arg).
            }
            let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(new_fn, &[]);
            let vec_h = ctx.builder.inst_results(inst)[0];
            let push_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
            for arg in &call.args {
                if arg.spread.is_some() {
                    return Ok(None);
                }
                let tv = super::lower_expr(ctx, &arg.expr)?;
                let v = if matches!(tv.ty, ValTy::Bool) {
                    ctx.coerce_to_handle(tv)?.val
                } else {
                    ctx.coerce_to_i64(tv).val
                };
                ctx.builder.ins().call(push_fn, &[vec_h, v]);
            }
            return Ok(Some(TypedVal::new(vec_h, ValTy::Handle)));
        }
        "isNaN" => lower_coerce_is_nan(ctx, call).map(Some),
        "isFinite" => lower_coerce_is_finite(ctx, call).map(Some),
        "Number" => lower_coerce_to_number(ctx, call),
        "String" => lower_coerce_to_string(ctx, call),
        "Boolean" => lower_coerce_to_boolean(ctx, call),
        // (cross-runtime #300) Function(args, body) — equivalente a
        // `new Function(args, body)` em JS spec. Reusa lower_new_function.
        "Function" => {
            let synth_new = swc_ecma_ast::NewExpr {
                span: call.span,
                ctxt: call.ctxt,
                callee: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: call.span,
                    ctxt: Default::default(),
                    sym: "Function".into(),
                    optional: false,
                })),
                args: Some(call.args.clone()),
                type_args: None,
            };
            let r = self::new_expr::lower_new_function(ctx, &synth_new)?;
            return Ok(Some(r));
        }
        // Object(x) / Object() — coercion JS:
        // - 0 args ou null/undefined: novo Map vazio
        // - Handle (object/array): passthrough
        // - primitivo: Map vazio (boxing real eh follow-up)
        "Object" => {
            let n_args = call.args.len();
            if n_args == 0 {
                let map_new = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
                    &[],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(map_new, &[]);
                let h = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(h, ValTy::Handle)));
            }
            if call.args[0].spread.is_some() {
                return Ok(None);
            }
            let tv = super::lower_expr(ctx, &call.args[0].expr)?;
            if matches!(tv.ty, ValTy::Handle) {
                return Ok(Some(TypedVal::new(tv.val, ValTy::Handle)));
            }
            let map_new = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
                &[],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(map_new, &[]);
            let h = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(h, ValTy::Handle)))
        }
        // (#216 partial) Symbol("desc") chamado como funcao em vez de
        // global member. Encaminha para __RTS_FN_GL_SYMBOL_NEW(ptr, len).
        "Symbol" if call.args.len() <= 1 && call.args.iter().all(|a| a.spread.is_none()) => {
            let (ptr_v, len_v) = if let Some(arg) = call.args.first() {
                let tv = super::lower_expr(ctx, &arg.expr)?;
                let h = ctx.coerce_to_handle(tv)?.val;
                let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let ip = ctx.builder.ins().call(ptr_fn, &[h]);
                let p = ctx.builder.inst_results(ip)[0];
                let il = ctx.builder.ins().call(len_fn, &[h]);
                let l = ctx.builder.inst_results(il)[0];
                (p, l)
            } else {
                let z = ctx.builder.ins().iconst(cl::I64, 0);
                let neg = ctx.builder.ins().iconst(cl::I64, -1);
                (z, neg)
            };
            let sym_new = ctx.get_extern(
                "__RTS_FN_GL_SYMBOL_NEW",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(sym_new, &[ptr_v, len_v]);
            let r = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(r, ValTy::Handle)))
        }
        // (#208) parseInt(s, radix?) — JS spec com radix opcional, tolerante.
        // Retorna NaN quando parse falha (runtime retorna i64::MIN sentinel).
        "parseInt" if (1..=2).contains(&call.args.len())
            && call.args.iter().all(|a| a.spread.is_none()) =>
        {
            let arg_tv = super::lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_handle(arg_tv)?.val;
            let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
            let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let ip = ctx.builder.ins().call(ptr_fn, &[h]);
            let p = ctx.builder.inst_results(ip)[0];
            let il = ctx.builder.ins().call(len_fn, &[h]);
            let l = ctx.builder.inst_results(il)[0];
            // Radix arg ou 0 (auto-detect).
            let radix = if call.args.len() == 2 {
                let r_tv = super::lower_expr(ctx, &call.args[1].expr)?;
                ctx.coerce_to_i64(r_tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_FMT_PARSE_INT_RADIX",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[p, l, radix]);
            let result = ctx.builder.inst_results(inst)[0];
            
            // Se result == i64::MIN (sentinel de erro), retorna NaN.
            // Caso contrário, converte i64 para f64.
            let i64_min = ctx.builder.ins().iconst(cl::I64, i64::MIN);
            let is_error = ctx.builder.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                result,
                i64_min,
            );
            let nan = ctx.builder.ins().f64const(f64::NAN);
            let result_f64 = ctx.builder.ins().fcvt_from_sint(cl::F64, result);
            let final_result = ctx.builder.ins().select(is_error, nan, result_f64);
            
            Ok(Some(TypedVal::new(final_result, ValTy::F64)))
        }
        "parseFloat" if call.args.len() == 1 && call.args[0].spread.is_none() => {
            let arg_tv = super::lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_handle(arg_tv)?.val;
            let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
            let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let ip = ctx.builder.ins().call(ptr_fn, &[h]);
            let p = ctx.builder.inst_results(ip)[0];
            let il = ctx.builder.ins().call(len_fn, &[h]);
            let l = ctx.builder.inst_results(il)[0];
            let f = ctx.get_extern(
                "__RTS_FN_NS_FMT_PARSE_F64",
                &[cl::I64, cl::I64],
                Some(cl::F64),
            )?;
            let inst = ctx.builder.ins().call(f, &[p, l]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        // (#208) encodeURIComponent / decodeURIComponent globais.
        // (#775) encodeURI / decodeURI globais (preservam reserved chars).
        "encodeURIComponent" | "decodeURIComponent" | "encodeURI" | "decodeURI"
            if call.args.len() == 1 && call.args[0].spread.is_none() =>
        {
            let arg_tv = super::lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_handle(arg_tv)?.val;
            let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
            let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let ip = ctx.builder.ins().call(ptr_fn, &[h]);
            let p = ctx.builder.inst_results(ip)[0];
            let il = ctx.builder.ins().call(len_fn, &[h]);
            let l = ctx.builder.inst_results(il)[0];
            let sym = match name {
                "encodeURIComponent" => "__RTS_FN_GL_ENCODE_URI_COMPONENT",
                "decodeURIComponent" => "__RTS_FN_GL_DECODE_URI_COMPONENT",
                "encodeURI" => "__RTS_FN_GL_ENCODE_URI",
                "decodeURI" => "__RTS_FN_GL_DECODE_URI",
                _ => unreachable!(),
            };
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[p, l]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // getPointer(fn) — materializa o endereço de uma user fn como i64.
        // Substitui o padrão `fn as unknown as number` nos call sites.
        "getPointer" => {
            let arg = call
                .args
                .first()
                .ok_or_else(|| anyhow!("getPointer requires 1 argument"))?;
            if arg.spread.is_some() {
                return Ok(None);
            }
            // Peel type assertions: getPointer(fn as SomeType) -> getPointer(fn)
            fn peel_ty(e: &Expr) -> &Expr {
                match e {
                    Expr::TsAs(a) => peel_ty(&a.expr),
                    Expr::TsTypeAssertion(a) => peel_ty(&a.expr),
                    Expr::TsConstAssertion(a) => peel_ty(&a.expr),
                    Expr::Paren(p) => peel_ty(&p.expr),
                    _ => e,
                }
            }
            let inner = peel_ty(&arg.expr);
            if let Expr::Ident(id) = inner {
                let name = id.sym.as_str();
                // Se for user fn direta, emite func_addr.
                // Se for var local (alias de fp), lower_expr já devolve o i64 armazenado.
                if ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none() {
                    return Ok(Some(emit_user_fn_addr(ctx, name)?));
                }
            }
            // Fallback: expressão que já contém um ponteiro (var local, param, etc)
            let tv = super::lower_expr(ctx, inner)?;
            Ok(Some(TypedVal::new(ctx.coerce_to_i64(tv).val, ValTy::I64)))
        }
        // ── Timers ────────────────────────────────────────────────────────────
        "setTimeout" => Ok(Some(lower_ns_call(ctx, "timers.setTimeout", call)?)),
        "clearTimeout" => Ok(Some(lower_ns_call(ctx, "timers.clearTimeout", call)?)),
        "setInterval" => Ok(Some(lower_ns_call(ctx, "timers.setInterval", call)?)),
        "clearInterval" => Ok(Some(lower_ns_call(ctx, "timers.clearInterval", call)?)),
        "setImmediate" => Ok(Some(lower_ns_call(ctx, "timers.setImmediate", call)?)),
        "clearImmediate" => Ok(Some(lower_ns_call(ctx, "timers.clearImmediate", call)?)),

        // ── fetch(url, opts?) ─────────────────────────────────────────────────
        // Assinatura interna: __RTS_FN_GL_FETCH(url_ptr, url_len, opts_h: u64)
        // opts_h = 0 quando chamado com 1 arg.
        "fetch" => {
            use crate::codegen::lower::ctx::{TypedVal, ValTy};
            let member = crate::abi::lookup("fetch.fetch")
                .ok_or_else(|| anyhow!("fetch.fetch not in SPECS"))?
                .1;
            // Resolve URL arg (StrPtr → ptr, len)
            let url_arg = call.args.first()
                .ok_or_else(|| anyhow!("fetch() requires at least 1 argument (url)"))?;
            if url_arg.spread.is_some() {
                return Ok(None);
            }
            let (url_ptr, url_len) = {
                let tv = super::lower_expr(ctx, &url_arg.expr)?;
                match tv.ty {
                    ValTy::Handle => {
                        let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                        let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                        let pi = ctx.builder.ins().call(ptr_fn, &[tv.val]);
                        let ptr = ctx.builder.inst_results(pi)[0];
                        let li = ctx.builder.ins().call(len_fn, &[tv.val]);
                        let len = ctx.builder.inst_results(li)[0];
                        (ptr, len)
                    }
                    _ => {
                        // Literal string already materialized as (ptr,len) via lower_expr?
                        // Fallback: treat as i64 ptr with len from string_from_static.
                        // Best effort: emit as gc handle.
                        let h = ctx.coerce_to_i64(tv).val;
                        let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                        let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                        let pi = ctx.builder.ins().call(ptr_fn, &[h]);
                        let ptr = ctx.builder.inst_results(pi)[0];
                        let li = ctx.builder.ins().call(len_fn, &[h]);
                        let len = ctx.builder.inst_results(li)[0];
                        (ptr, len)
                    }
                }
            };
            // opts arg: second arg or 0
            let opts_h = if call.args.len() >= 2 {
                let opts_arg = &call.args[1];
                if opts_arg.spread.is_none() {
                    let tv = super::lower_expr(ctx, &opts_arg.expr)?;
                    ctx.coerce_to_i64(tv).val
                } else {
                    ctx.builder.ins().iconst(cl::I64, 0)
                }
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            // Emit call __RTS_FN_GL_FETCH(url_ptr, url_len, opts_h) -> i64 (handle)
            let func_id = {
                use cranelift_codegen::ir::types::I64 as CL_I64;
                use cranelift_codegen::ir::{AbiParam, Signature};
                use cranelift_module::Linkage;
                let sym = member.symbol;
                if !ctx.extern_cache.contains_key(sym) {
                    let mut sig = Signature::new(ctx.module.isa().default_call_conv());
                    sig.params.push(AbiParam::new(CL_I64)); // url_ptr
                    sig.params.push(AbiParam::new(CL_I64)); // url_len
                    sig.params.push(AbiParam::new(CL_I64)); // opts_h
                    sig.returns.push(AbiParam::new(CL_I64)); // Promise<Response> handle
                    let id = ctx.module.declare_function(sym, Linkage::Import, &sig)
                        .map_err(|e| anyhow!("{e}"))?;
                    ctx.extern_cache.insert(sym.to_string(), id);
                    id
                } else {
                    *ctx.extern_cache.get(sym).unwrap()
                }
            };
            let fref = ctx.fref_for_id(func_id);
            let inst = ctx.builder.ins().call(fref, &[url_ptr, url_len, opts_h]);
            let val = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(val, ValTy::Handle)))
        }

        // ── Text encoding / global utils ──────────────────────────────────────
        "atob" => Ok(Some(lower_ns_call(ctx, "text_encoding.atob", call)?)),
        "btoa" => Ok(Some(lower_ns_call(ctx, "text_encoding.btoa", call)?)),
        "structuredClone" => {
            // (#1042) Primitivos (bool/num/str) sao retornados direto sem
            // passar pelo dispatch de handle — STRUCTURED_CLONE em runtime
            // so' clona Map/Vec/String/Buffer e retorna handle original
            // para outros, mas o codegen passa Bool/F64 coerced para i64
            // (perdendo a marca de tipo) e o resultado eh impresso como
            // numero (`1`/`null`) em vez de "true"/"false".
            if let Some(arg) = call.args.first() {
                if arg.spread.is_none() {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    if matches!(tv.ty, ValTy::Bool | ValTy::F64 | ValTy::I64 | ValTy::I32) {
                        return Ok(Some(tv));
                    }
                }
            }
            // (#68) structuredClone(buf, { transfer: [buf] }) — JS spec: o
            // buffer fonte fica DETACHED (byteLength -> 0). Detecta o 2o arg
            // com key `transfer` e, se o 1o arg eh um (Shared)ArrayBuffer
            // ident, emite BUFFER_DETACH(src) APOS o clone.
            let detach_src: Option<cranelift_codegen::ir::Value> = if call.args.len() >= 2 {
                let has_transfer = matches!(call.args[1].expr.as_ref(),
                    Expr::Object(o) if o.props.iter().any(|p| matches!(p,
                        swc_ecma_ast::PropOrSpread::Prop(pp) if matches!(pp.as_ref(),
                            swc_ecma_ast::Prop::KeyValue(kv) if matches!(&kv.key,
                                swc_ecma_ast::PropName::Ident(id) if id.sym.as_str() == "transfer")))));
                let src_is_buf = matches!(call.args[0].expr.as_ref(),
                    Expr::Ident(id) if ctx.local_class_ty.get(id.sym.as_str())
                        .map(|c| c == "ArrayBuffer" || c == "SharedArrayBuffer").unwrap_or(false));
                if has_transfer && src_is_buf {
                    let src_tv = lower_expr(ctx, &call.args[0].expr)?;
                    Some(ctx.coerce_to_i64(src_tv).val)
                } else {
                    None
                }
            } else {
                None
            };
            let cloned = lower_ns_call(ctx, "text_encoding.structuredClone", call)?;
            if let Some(src) = detach_src {
                let detach = ctx.get_extern("__RTS_FN_GL_BUFFER_DETACH", &[cl::I64], None)?;
                ctx.builder.ins().call(detach, &[src]);
            }
            Ok(Some(cloned))
        }
        "queueMicrotask" => Ok(Some(lower_ns_call(ctx, "text_encoding.queueMicrotask", call)?)),

        // (cross-runtime #267/#268) Global `eval(src)` — RTS nao tem dynamic
        // eval em-line. Para compatibilidade com codigo que usa eval dentro
        // de try/catch esperando SyntaxError (acesso a #private fora da
        // classe, etc), seta error slot com SyntaxError e retorna 0. Assim
        // o catch dispara e e.name === "SyntaxError" funciona.
        "eval" => {
            use cranelift_codegen::ir::{InstBuilder, types as cl};
            // Lower o argumento (descartado, mas preserva side effects)
            if let Some(arg0) = call.args.first() {
                if arg0.spread.is_none() {
                    let _ = super::lower_expr(ctx, &arg0.expr)?;
                }
            }
            let msg = b"eval not supported";
            let msg_h = ctx.emit_str_handle(msg)?.val;
            // SYNTAX_ERROR_NEW(msg_ptr, msg_len, cause_handle) — passar
            // o handle de string como (ptr,len) requer expand. Mais
            // simples: chamar alloc_error_with_cause via wrapper helper
            // ja existente? Em vez disso, usa ERROR_NEW generico com
            // name="SyntaxError". Mas nao temos esse helper exposto.
            // Solucao: chamar __RTS_FN_GL_SYNTAX_ERROR_NEW via StrPtr,
            // que toma ptr+len. Vamos extrair do handle.
            let ptr_fn = ctx.get_extern(
                "__RTS_FN_NS_GC_STRING_PTR",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_GC_STRING_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let p = {
                let i = ctx.builder.ins().call(ptr_fn, &[msg_h]);
                ctx.builder.inst_results(i)[0]
            };
            let l = {
                let i = ctx.builder.ins().call(len_fn, &[msg_h]);
                ctx.builder.inst_results(i)[0]
            };
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let new_fn = ctx.get_extern(
                "__RTS_FN_GL_SYNTAX_ERROR_NEW",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(new_fn, &[p, l, zero]);
            let err_h = ctx.builder.inst_results(inst)[0];
            let set_fn = ctx.get_extern(
                "__RTS_FN_RT_ERROR_SET",
                &[cl::I64],
                None,
            )?;
            ctx.builder.ins().call(set_fn, &[err_h]);
            // Retorna 0 (sentinel). Caller deveria estar em try/catch.
            let z = ctx.builder.ins().iconst(cl::I64, 0);
            Ok(Some(crate::codegen::lower::ctx::TypedVal::new(
                z,
                crate::codegen::lower::ctx::ValTy::I64,
            )))
        }

        _ => Ok(None),
    }
}


/// (cross-runtime #799) Reflect.apply/construct e similares precisam de
/// Function handle (Entry::Function) — read_function_data falha em
/// fn_ptr nu. Quando target eh ident de user fn, reifica em handle com
/// param_kinds/return_kind corretos pra invoke_typed reinterpretar bits
/// f64 vs i64; senao usa coerce normal (handle ja eh i64).
/// (issue-pai invoke/param_kinds) `expr` eh um ident de user fn NOMEADA pelo
/// usuario (nao var local, nao fn sintetica hoistada/liftada)? So' essas tem
/// kinds confiaveis p/ reificacao; function expressions anonimas hoistadas
/// seguem o caminho antigo (func_addr) que ja' funcionava.
pub(in crate::codegen::lower::expressions) fn arg_is_bare_user_fn(ctx: &FnCtx, expr: &swc_ecma_ast::Expr) -> bool {
    if let swc_ecma_ast::Expr::Ident(id) = expr {
        let name = id.sym.as_str();
        if name.starts_with("__hoisted_")
            || name.starts_with("__lifted_")
            || name.starts_with("__async_inner_")
        {
            return false;
        }
        return ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none();
    }
    false
}

pub(in crate::codegen::lower::expressions) fn lower_callable_target_h(
    ctx: &mut FnCtx,
    expr: &swc_ecma_ast::Expr,
) -> Result<cranelift_codegen::ir::Value> {
    use cranelift_codegen::ir::types as cl;
    if let swc_ecma_ast::Expr::Ident(id) = expr {
        let name = id.sym.as_str();
        if ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none() {
            let fn_addr = emit_user_fn_addr(ctx, name)?.val;
            let arity = ctx
                .user_fns
                .get(name)
                .map(|f| f.params.len() as i64)
                .unwrap_or(0);
            let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
            let name_tv = ctx.emit_str_handle(name.as_bytes())?;
            let name_h = ctx.coerce_to_i64(name_tv).val;
            let str_ptr_fn =
                ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
            let str_len_fn =
                ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
            let n_ptr = ctx.builder.inst_results(inst_p)[0];
            let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
            let n_len = ctx.builder.inst_results(inst_l)[0];
            let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
            // (cross-runtime #799) `has_this_param=true` quando user fn declarada
            // como `function f(this: any, ...)` — primeiro param Cranelift eh
            // o thisArg explicito. Caller (FUNCTION_CALL) precisa prepender
            // o thisArg como arg em vez de empilhar no slot.
            let has_this_param_flag = fn_name_has_this_param(name)
                || ctx
                    .user_fns
                    .get(name)
                    .map(|f| f.has_this_param)
                    .unwrap_or(false);
            let has_this_v = ctx
                .builder
                .ins()
                .iconst(cl::I32, i64::from(has_this_param_flag));
            // Deriva param_kinds + return_kind (mesmo padrao de
            // lower_function_method_call em new_expr.rs).
            let (pks_bytes, rk_byte): (Vec<u8>, u8) = {
                let info = ctx.user_fns.get(name);
                let pks: Vec<u8> = info
                    .map(|f| {
                        f.params
                            .iter()
                            .map(|p| super::members::val_ty_to_kind(*p))
                            .collect()
                    })
                    .unwrap_or_default();
                let rk: u8 = info
                    .and_then(|f| f.ret)
                    .map(super::members::val_ty_to_kind)
                    .unwrap_or(4);
                (pks, rk)
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
            return Ok(ctx.builder.inst_results(inst_r)[0]);
        }
    }
    let tv = lower_expr(ctx, expr)?;
    Ok(ctx.coerce_to_i64(tv).val)
}

pub(crate) fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    // User fns cujo endereço é tomado são declaradas com C callconv
    // (ver `address_taken_fns` em compile_program / #206) — seguro para
    // `thread.spawn` e FFI.
    let mangled: String = format!("__user_{name}");
    let func_id = *ctx
        .extern_cache
        .get(mangled.as_str())
        .ok_or_else(|| anyhow!("user function `{name}` has no cached id"))?;
    let fref = ctx.fref_for_id(func_id);
    let ptr_ty = ctx.module.isa().pointer_type();
    let addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    Ok(TypedVal::new(addr, ValTy::I64))
}

/// Reifica uma arrow hoistada (`__hoisted_arrow_N`) como Function handle
/// com `is_arrow=1`.
///
/// `bound_this`: quando `Some(val)`, usa REIFY_BOUND para capturar o
/// receiver do escopo envolvente no momento da criação (ex: classe method).
/// INVOKE_AUTO empurra `bound_this` ao slot antes de invocar, então
/// `THIS_GET()` no body da arrow lê o valor correto mesmo quando a arrow
/// é armazenada e chamada fora do escopo original.
///
/// Quando `None`, usa REIFY simples — arrow em escopo sem `this` (top-level
/// ou fn plain).
pub(super) fn emit_hoisted_arrow_handle(
    ctx: &mut FnCtx,
    name: &str,
    bound_this: Option<cranelift_codegen::ir::Value>,
) -> Result<TypedVal> {
    use cranelift_codegen::ir::types as cl;
    let fn_addr_tv = emit_user_fn_addr(ctx, name)?;
    let fn_addr = fn_addr_tv.val;
    let arity = ctx
        .user_fns
        .get(name)
        .map(|f| f.params.len() as i64)
        .unwrap_or(0);
    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let name_tv = ctx.emit_str_handle(name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
    let n_ptr = ctx.builder.inst_results(inst_p)[0];
    let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
    let n_len = ctx.builder.inst_results(inst_l)[0];
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 1);
    let has_this_v = ctx.builder.ins().iconst(cl::I32, 0);

    // (A+C — #1281) Deriva param_kinds + return_kind da UserFnAbi. Quando ha
    // algum kind!=0 (param/ret number/bool, ex arrow `(i:number)=>i+100`), o
    // handle precisa carregar os kinds p/ invoke_typed fazer from_bits — senao
    // o raw fn_ptr `(f64)->f64` e' invocado via invoke_all_i64 (ABI i64) e
    // corrompe. SE todo-zero: caminho BYTE-IDENTICO ao de hoje (REIFY/REIFY_BOUND).
    let (pks_bytes, rk_byte): (Vec<u8>, u8) = {
        let info = ctx.user_fns.get(name);
        let pks: Vec<u8> = info
            .map(|f| {
                f.params
                    .iter()
                    .map(|p| super::members::val_ty_to_kind(*p))
                    .collect()
            })
            .unwrap_or_default();
        let rk: u8 = info
            .and_then(|f| f.ret)
            .map(super::members::val_ty_to_kind)
            .unwrap_or(0);
        (pks, rk)
    };
    let has_nonzero_kind = pks_bytes.iter().any(|&k| k != 0) || rk_byte != 0;

    let handle = if has_nonzero_kind {
        // TYPED: usa REIFY_BOUND_TYPED (cobre bound_this opcional + kinds).
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
            (ctx.builder.inst_results(p)[0], ctx.builder.inst_results(l)[0])
        };
        let (bound_this_v, has_bound_v) = match bound_this {
            Some(this_val) => (this_val, ctx.builder.ins().iconst(cl::I32, 1)),
            None => (
                ctx.builder.ins().iconst(cl::I64, 0),
                ctx.builder.ins().iconst(cl::I32, 0),
            ),
        };
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
                bound_this_v, has_bound_v, kinds_ptr, kinds_len, return_kind_v,
            ],
        );
        ctx.builder.inst_results(inst_r)[0]
    } else if let Some(this_val) = bound_this {
        // Captura `this` do escopo envolvente no handle — arrow de longa duração.
        let has_bound_v = ctx.builder.ins().iconst(cl::I32, 1);
        let reify_fn = ctx.get_extern(
            "__RTS_FN_GL_FUNCTION_REIFY_BOUND",
            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32, cl::I64, cl::I32],
            Some(cl::I64),
        )?;
        let inst_r = ctx.builder.ins().call(
            reify_fn,
            &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v, this_val, has_bound_v],
        );
        ctx.builder.inst_results(inst_r)[0]
    } else {
        let reify_fn = ctx.get_extern(
            "__RTS_FN_GL_FUNCTION_REIFY",
            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32],
            Some(cl::I64),
        )?;
        let inst_r = ctx
            .builder
            .ins()
            .call(reify_fn, &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
        ctx.builder.inst_results(inst_r)[0]
    };
    Ok(TypedVal::new(handle, ValTy::Handle))
}

/// (#195) Reifica `__lifted_arrow_N` que captura variaveis livres POR VALOR.
/// Empacota os valores capturados num Vec (bitcast f64->bits) e chama
/// REIFY_CAPTURED — bound_args sao prepended em cada invocacao, dando
/// captura-por-ativacao correta (curry/recursao). Os capturados sao os
/// params INICIAIS da fn liftada (ver lift_arrow_to_ident).
pub(crate) fn emit_lifted_arrow_handle_with_captures(
    ctx: &mut FnCtx,
    name: &str,
    capture_vals: &[TypedVal],
) -> Result<TypedVal> {
    use cranelift_codegen::ir::types as cl;
    let fn_addr = emit_user_fn_addr(ctx, name)?.val;
    let arity = ctx.user_fns.get(name).map(|f| f.params.len() as i64).unwrap_or(0);
    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let name_tv = ctx.emit_str_handle(name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let n_ptr = {
        let i = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
        ctx.builder.inst_results(i)[0]
    };
    let n_len = {
        let i = ctx.builder.ins().call(str_len_fn, &[name_h]);
        ctx.builder.inst_results(i)[0]
    };
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 1);
    let has_this_v = ctx.builder.ins().iconst(cl::I32, 0);

    // Empacota capturas num Vec<i64> (f64 -> bits). param_kinds reflete o
    // tipo de cada capture + zeros para os params proprios (codegen ja' passa
    // valores como i64; f64 reinterpretado via param_kinds[i]=1).
    // (A+C+D — #1281) Deriva param_kinds + return_kind da UserFnAbi da fn
    // liftada (mesmo padrao de lower_callable_target_h:3306-3321). A fn liftada
    // tem params = [capturas..., params proprios]; com a anotacao number/boolean
    // preservada (this_arrow.rs PARTE A), os kinds refletem o tipo real.
    let (pks_bytes, rk_byte): (Vec<u8>, u8) = {
        let info = ctx.user_fns.get(name);
        let pks: Vec<u8> = info
            .map(|f| {
                f.params
                    .iter()
                    .map(|p| super::members::val_ty_to_kind(*p))
                    .collect()
            })
            .unwrap_or_default();
        let rk: u8 = info
            .and_then(|f| f.ret)
            .map(super::members::val_ty_to_kind)
            .unwrap_or(0);
        (pks, rk)
    };
    // SE todo-zero (incl. ret): caminho BYTE-IDENTICO ao de hoje (curry-i64
    // intacto). SE algum kind!=0: TYPED com kinds reais + capturas f64-bits.
    let has_nonzero_kind = pks_bytes.iter().any(|&k| k != 0) || rk_byte != 0;

    let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let bound_h = {
        let i = ctx.builder.ins().call(vec_new, &[]);
        ctx.builder.inst_results(i)[0]
    };
    let vec_push = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
    // Empacota cada captura. As capturas ocupam os PRIMEIROS slots dos params
    // (ver lift_arrow_to_ident). Quando o slot correspondente e' kind=1 (f64),
    // empacota como bits-f64 (bitcast) p/ o runtime fazer from_bits; senao
    // coerce_to_i64 como hoje (handle/i64/bool i64 cru).
    for (i, cv) in capture_vals.iter().enumerate() {
        let raw = if has_nonzero_kind && pks_bytes.get(i).copied() == Some(1) {
            let f = ctx.coerce_to_f64(*cv).val;
            ctx.builder.ins().bitcast(cl::I64, cranelift_codegen::ir::MemFlags::new(), f)
        } else {
            ctx.coerce_to_i64(*cv).val
        };
        ctx.builder.ins().call(vec_push, &[bound_h, raw]);
    }
    // Mantem o Vec vivo durante a reificacao.
    ctx.declare_gc_handle(bound_h);

    let reify_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_REIFY_CAPTURED",
        &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32],
        Some(cl::I64),
    )?;
    let (kinds_ptr, kinds_len) = if has_nonzero_kind && !pks_bytes.is_empty() {
        let tv = ctx.emit_str_handle(&pks_bytes)?;
        let h = ctx.coerce_to_i64(tv).val;
        let p = ctx.builder.ins().call(str_ptr_fn, &[h]);
        let l = ctx.builder.ins().call(str_len_fn, &[h]);
        (ctx.builder.inst_results(p)[0], ctx.builder.inst_results(l)[0])
    } else {
        (
            ctx.builder.ins().iconst(cl::I64, 0),
            ctx.builder.ins().iconst(cl::I64, 0),
        )
    };
    let return_kind_v = ctx
        .builder
        .ins()
        .iconst(cl::I32, if has_nonzero_kind { rk_byte as i64 } else { 0 });
    // (#195) rest_param_idx: indice do `...rest` (after capturas prepended) ou
    // -1. expand_rest_args ja' calculou sobre os params finais.
    let rest_idx_v = {
        let ri = crate::codegen::lower::passes::args::rest_args::fn_rest_idx(name)
            .map(|i| i as i64)
            .unwrap_or(-1);
        ctx.builder.ins().iconst(cl::I32, ri)
    };
    let inst = ctx.builder.ins().call(
        reify_fn,
        &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v, bound_h, kinds_ptr, kinds_len, return_kind_v, rest_idx_v],
    );
    let handle = ctx.builder.inst_results(inst)[0];
    ctx.declare_gc_handle(handle);
    Ok(TypedVal::new(handle, ValTy::Handle))
}

/// (cross-runtime) Gera o Vec fonte de um `Array.from(src, mapper)` para o
/// caminho BOUND: `{length:n}` -> [0..n-1] (via ARRAY_FROM_LENGTH com fn_ptr=0);
/// string literal/tpl -> split em chars; senao trata src como Vec handle.
fn build_array_from_source_vec(
    ctx: &mut FnCtx,
    src: &Expr,
) -> Result<cranelift_codegen::ir::Value> {
    use cranelift_codegen::ir::types as cl;
    // {length:N} literal -> [0..N-1].
    if let Expr::Object(obj_lit) = src {
        for prop in &obj_lit.props {
            if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                    let key = match &kv.key {
                        swc_ecma_ast::PropName::Ident(i) => Some(i.sym.as_str().to_string()),
                        swc_ecma_ast::PropName::Str(s) => Some(s.value.to_string_lossy().to_string()),
                        _ => None,
                    };
                    if key.as_deref() == Some("length") {
                        // (#363) `{length: N}` — N literal OU expressao dinamica
                        // (`m + 1`). Antes so' literal era tratado; expr caia no
                        // fallback "src eh Vec handle" que lia o object literal
                        // como handle e produzia length -1. Agora lower a expr e
                        // gera [0..len-1] via ARRAY_FROM_LENGTH.
                        let n_v = if let Expr::Lit(swc_ecma_ast::Lit::Num(n)) = kv.value.as_ref() {
                            ctx.builder.ins().iconst(cl::I64, n.value as i64)
                        } else {
                            let tv = lower_expr(ctx, &kv.value)?;
                            ctx.coerce_to_i64(tv).val
                        };
                        let zero = ctx.builder.ins().iconst(cl::I64, 0);
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_ARRAY_FROM_LENGTH",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[n_v, zero]);
                        return Ok(ctx.builder.inst_results(inst)[0]);
                    }
                }
            }
        }
    }
    // string literal/tpl -> split em chars.
    if matches!(src, Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)) {
        let s_tv = lower_expr(ctx, src)?;
        let s_h = ctx.coerce_to_handle(s_tv)?.val;
        let empty = ctx.emit_str_handle(b"")?.val;
        let split_fn =
            ctx.get_extern("__RTS_FN_GL_STRING_SPLIT", &[cl::I64, cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(split_fn, &[s_h, empty]);
        return Ok(ctx.builder.inst_results(inst)[0]);
    }
    // Fallback: src eh Vec handle.
    let src_tv = lower_expr(ctx, src)?;
    Ok(ctx.coerce_to_i64(src_tv).val)
}

/// (#195) Lower de `parallel.{map,filter,for_each}_bound(arr, __lifted_cap_N)`.
/// Lê as capturas registradas em LIFTED_CAPTURES, lower seus valores do
/// escopo atual, reifica a fn liftada com bound_args via
/// emit_lifted_arrow_handle_with_captures, e chama a variante BOUND do
/// runtime. Retorna None se nao casar (deixa caminho generico tentar).
fn lower_parallel_bound_call(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use cranelift_codegen::ir::types as cl;
    // reduce_bound/reduce_right_bound: (arr, init, fn). Demais: (arr, fn).
    let is_reduce_init =
        qualified == "parallel.reduce_bound" || qualified == "parallel.reduce_right_bound";
    let expected_args = if is_reduce_init { 3 } else { 2 };
    if call.args.len() != expected_args {
        return Ok(None);
    }
    // O callback eh sempre o ULTIMO arg.
    let fn_idx = expected_args - 1;
    let fn_name = match call.args[fn_idx].expr.as_ref() {
        Expr::Ident(id) if id.sym.as_str().starts_with("__lifted_cap_") => id.sym.to_string(),
        _ => return Ok(None),
    };
    let captures: Vec<String> = crate::codegen::lower::passes::parallelism::LIFTED_CAPTURES
        .with(|c| c.borrow().get(&fn_name).cloned())
        .unwrap_or_default();
    if captures.is_empty() {
        return Ok(None);
    }
    // Lower os valores das capturas no escopo atual.
    let mut capture_vals: Vec<TypedVal> = Vec::with_capacity(captures.len());
    for cap in &captures {
        // (#376 camada 3) `__captured_this` -> `this` atual.
        let lookup = if cap == "__captured_this" { "this" } else { cap.as_str() };
        match ctx.read_local(lookup) {
            Some(tv) => capture_vals.push(tv),
            None => return Ok(None), // captura nao resolve no escopo — bail
        }
    }
    // Reifica a fn liftada com as capturas em bound_args.
    let fn_handle = emit_lifted_arrow_handle_with_captures(ctx, &fn_name, &capture_vals)?.val;
    // Lower o array.
    let arr_tv = lower_expr(ctx, &call.args[0].expr)?;
    let arr_h = ctx.coerce_to_i64(arr_tv).val;

    // reduce/reduceRight com init: chama *_REDUCE[_RIGHT]_BOUND(arr, init, fn).
    if is_reduce_init {
        let sym = if qualified == "parallel.reduce_right_bound" {
            "__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_BOUND"
        } else {
            "__RTS_FN_NS_PARALLEL_REDUCE_BOUND"
        };
        let init_tv = lower_expr(ctx, &call.args[1].expr)?;
        let init = ctx.coerce_to_i64(init_tv).val;
        let f = ctx.get_extern(sym, &[cl::I64, cl::I64, cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(f, &[arr_h, init, fn_handle]);
        let v = ctx.builder.inst_results(inst)[0];
        ctx.var_member_call_values.insert(v);
        return Ok(Some(TypedVal::new(v, ValTy::I64)));
    }

    // Chama a variante BOUND (2-arg).
    let (sym, kind) = match qualified {
        "parallel.map_bound" => ("__RTS_FN_NS_PARALLEL_MAP_BOUND", 0u8),
        "parallel.filter_bound" => ("__RTS_FN_NS_PARALLEL_FILTER_BOUND", 0),
        "parallel.for_each_bound" => ("__RTS_FN_NS_PARALLEL_FOR_EACH_BOUND", 1),
        "parallel.reduce_no_init_bound" => ("__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT_BOUND", 2),
        // find/findIndex/some/every -> i64. find eh ambiguo (val ou handle
        // "undefined"); os demais sao int/bool puros (kind 2 marca ambiguo
        // tambem, inofensivo p/ int — TPL_COERCE_AUTO formata 0/1 como num).
        // find eh ambiguo (val ou handle "undefined"): kind 2 (marca ambiguo).
        // findIndex -> int; some/every -> bool. kind 4 = i64 puro sem marca.
        "parallel.find_bound" => ("__RTS_FN_NS_PARALLEL_FIND_BOUND", 2),
        "parallel.find_index_bound" => ("__RTS_FN_NS_PARALLEL_FIND_INDEX_BOUND", 4),
        "parallel.some_bound" => ("__RTS_FN_NS_PARALLEL_SOME_BOUND", 4),
        "parallel.every_bound" => ("__RTS_FN_NS_PARALLEL_EVERY_BOUND", 4),
        // reduceRight sem init -> ambiguo (val ou handle). findLast ambiguo;
        // findLastIndex -> int puro.
        "parallel.reduce_right_no_init_bound" => {
            ("__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_NO_INIT_BOUND", 2)
        }
        "parallel.find_last_bound" => ("__RTS_FN_NS_PARALLEL_FIND_LAST_BOUND", 2),
        "parallel.find_last_index_bound" => ("__RTS_FN_NS_PARALLEL_FIND_LAST_INDEX_BOUND", 4),
        _ => return Ok(None),
    };
    match kind {
        0 => {
            // map/filter -> handle Vec.
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[arr_h, fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            ctx.declare_gc_handle(v);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        2 => {
            // reduce_no_init/find -> i64 ambiguo (val ou handle).
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[arr_h, fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            ctx.var_member_call_values.insert(v);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        4 => {
            // findIndex/some/every -> i64 puro (int/bool), sem marca ambigua.
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[arr_h, fn_handle]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        _ => {
            // for_each -> undefined.
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], None)?;
            ctx.builder.ins().call(f, &[arr_h, fn_handle]);
            let u = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
            Ok(Some(TypedVal::new(u, ValTy::I64)))
        }
    }
}

/// `obj.fn(...)` onde `obj` e uma var local (HashMap-like, ex: namespace
/// TS desugared). Faz map_get(obj, "fn") -> i64 (funcptr) e
/// call_indirect com signature i64-only.

/// (#376) `(x ?? [])` ou `(x || [])` — nullish/logical-or onde um dos lados eh
/// array literal. O resultado eh sempre um array, entao member calls (`.includes`
/// etc) devem despachar pelos builtins de array. Cobre o caso de fallback
/// `map.get(k) ?? []`.
fn expr_is_coalesce_with_array(e: &Expr) -> bool {
    if let Expr::Bin(b) = e {
        if matches!(
            b.op,
            swc_ecma_ast::BinaryOp::NullishCoalescing | swc_ecma_ast::BinaryOp::LogicalOr
        ) {
            let side_is_array = |x: &Expr| {
                matches!(x, Expr::Array(_))
                    || matches!(x, Expr::Paren(p) if matches!(p.expr.as_ref(), Expr::Array(_)))
            };
            return side_is_array(&b.left) || side_is_array(&b.right);
        }
    }
    false
}

pub(crate) fn lower_user_call(ctx: &mut FnCtx, name: &str, call: &CallExpr) -> Result<TypedVal> {
    let abi = ctx
        .user_fns
        .get(name)
        .ok_or_else(|| anyhow!("call to undeclared user function `{name}`"))?
        .clone();

    // (cross-runtime #348) Spread em chamada a user fn: o call direto eh
    // posicional (aridade fixa), incompativel com `f(...xs)` onde o numero
    // de args so' se conhece em runtime. Roteia via INVOKE_AUTO sobre um
    // handle Function reificado COM param_kinds/return_kind (lower_callable
    // _target_h) — assim invoke_typed reinterpreta f64-bits corretamente.
    // Monta o args Vec: push normal (f64 -> bits) + VEC_EXTEND_FROM p/ spread.
    if call.args.iter().any(|a| a.spread.is_some()) {
        let callee = lower_callable_target_h(
            ctx,
            &swc_ecma_ast::Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: name.into(),
                optional: false,
            }),
        )?;
        let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
        let vec_push = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            &[cl::I64, cl::I64],
            None,
        )?;
        let vec_extend = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
            &[cl::I64, cl::I64],
            None,
        )?;
        let new_inst = ctx.builder.ins().call(vec_new, &[]);
        let args_vec = ctx.builder.inst_results(new_inst)[0];
        ctx.declare_gc_handle(args_vec);
        for arg in &call.args {
            let tv = lower_expr(ctx, &arg.expr)?;
            if arg.spread.is_some() {
                // Source eh um Vec/array — extend com seus elementos.
                let src = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(vec_extend, &[args_vec, src]);
            } else {
                // Arg posicional: f64 viaja como BITS (invoke_typed
                // reinterpreta via param_kinds[i]==1); o resto como i64.
                let v = if matches!(tv.ty, ValTy::F64) {
                    ctx.builder.ins().bitcast(
                        cl::I64,
                        cranelift_codegen::ir::MemFlags::new(),
                        tv.val,
                    )
                } else {
                    ctx.coerce_to_i64(tv).val
                };
                ctx.builder.ins().call(vec_push, &[args_vec, v]);
            }
        }
        let invoke = ctx.get_extern(
            "__RTS_FN_RT_INVOKE_AUTO",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let inst = ctx.builder.ins().call(invoke, &[callee, zero, args_vec]);
        let v = ctx.builder.inst_results(inst)[0];
        ctx.var_member_call_values.insert(v);
        return Ok(TypedVal::new(v, ValTy::I64));
    }

    let mangled: String = format!("__user_{name}");
    if !ctx.extern_cache.contains_key(mangled.as_str()) {
        return Err(anyhow!("call to undeclared user function `{name}`"));
    }
    let func_id = *ctx.extern_cache.get(mangled.as_str()).unwrap();
    let fref = ctx.fref_for_id(func_id);

    // (cross-runtime #299/#270) JS spec: chamada com MENOS args completa com
    // undefined; MAIS args sao acessiveis via `arguments` (RTS nao
    // tem `arguments` real mas silenciosamente ignora extras pra
    // compat com pattern \`function f() { ... arguments... }\`).
    // Fill com sentinel undefined (i64::MIN+2) quando faltam args.
    let mut values = Vec::new();
    for (i, expected_ty) in abi.params.iter().copied().enumerate() {
        let value = if let Some(arg) = call.args.get(i) {
            if arg.spread.is_some() {
                return Err(anyhow!("spread not supported"));
            }
            // (issue-pai invoke/param_kinds, metade a) Arg que eh ident de user
            // fn NOMEADA passado a param nao-numerico: reifica como handle
            // Function COM param_kinds/return_kind (lower_callable_target_h) em
            // vez de func_addr cru. Resolve HOF: `apply(inc,10)` invoca inc com
            // a ABI certa. Function expressions hoistadas seguem o caminho antigo
            // (arg_is_bare_user_fn as exclui).
            if !matches!(expected_ty, ValTy::F64) && arg_is_bare_user_fn(ctx, &arg.expr) {
                lower_callable_target_h(ctx, &arg.expr)?
            } else {
                let tv = lower_expr(ctx, &arg.expr)?;
                match expected_ty {
                    ValTy::I32 => ctx.coerce_to_i32(tv).val,
                    ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64
                    | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => ctx.coerce_to_i64(tv).val,
                    ValTy::F64 => to_f64(ctx, tv),
                }
            }
        } else {
            // Param ausente — fill com undefined sentinel (ou 0.0 pra F64).
            match expected_ty {
                ValTy::F64 => ctx.builder.ins().f64const(f64::NAN),
                ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
                _ => ctx.builder.ins().iconst(cl::I64, i64::MIN + 2),
            }
        };
        values.push(value);
    }

    // Tail calls: \`return_call\` substitui o frame atual no lugar de
    // empilhar — semantica de loop iterativo. NAO incrementar stack
    // depth, senao tail recursion de N iteracoes estoura limite (10K
    // default) mesmo nao consumindo stack real do C. Tambem NAO pop —
    // o frame atual sera substituido, sem retorno ao caller pra fazer
    // pop. (Antes Daniel emitia push aqui, fazendo loopTco(500000)
    // overflow desnecessariamente.)
    // (cross-runtime #348) `return_call` exige que a assinatura de RETORNO do
    // callee bata com a do caller (mesma repr Cranelift) — senao o verifier
    // rejeita ("result 0 has type f64, must match i64"). Acontece quando um
    // forwarder `(...a) => fn(...a)` (caller inferido i64) chama em tail uma
    // fn variadic cujo retorno virou f64 (elementos do rest array). Quando
    // divergem, cai no call normal abaixo + coercao no `return` do caller.
    let cl_ret_class = |t: ValTy| -> u8 {
        match t {
            ValTy::F64 => 2,
            ValTy::I32 => 1,
            _ => 0,
        }
    };
    let callee_ret = abi.ret.unwrap_or(ValTy::I64);
    let caller_ret = ctx.return_ty.unwrap_or(ValTy::I64);
    // (cross-runtime closures) `return_call` exige tambem que a CALLING
    // CONVENTION do callee bata com a do caller. Uma user fn address-taken
    // (passada como valor — ex. `sq`/`out` em `h(sq, out, 5)`) ou callback
    // liftada usa `windows_fastcall`, nao `Tail`; tail-call cross-conv quebra o
    // verifier ("calling convention windows_fastcall does not support tail
    // calls"). Consulta a conv real da assinatura ja' declarada do callee.
    let callee_is_tail = {
        let sig_ref = ctx.builder.func.dfg.ext_funcs[fref].signature;
        ctx.builder.func.dfg.signatures[sig_ref].call_conv
            == cranelift_codegen::isa::CallConv::Tail
    };
    if ctx.is_tail_conv && ctx.in_tail_position
        && callee_is_tail
        && cl_ret_class(callee_ret) == cl_ret_class(caller_ret)
    {
        let ty = callee_ret;
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

/// Detecta `Object.prototype.toString.call(value)` e emite chamada para
/// `__RTS_FN_RT_OBJECT_TO_STRING(value, tag)`. Tag conforme tipo estatico
/// do arg: Number/String/Boolean/Function literais; Handle delega ao
/// runtime (tag=0) que inspeciona Entry. Retorna Some quando match,
/// None pra continuar com dispatch normal.
fn try_lower_object_to_string_call(
    ctx: &mut FnCtx,
    callee: &Expr,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    // Padrao: Member { obj: Member { obj: Member { obj: Ident("Object"),
    //                                              prop: "prototype" },
    //                                  prop: "toString" },
    //                  prop: "call" }
    let outer = match callee {
        Expr::Member(m) => m,
        _ => return Ok(None),
    };
    let prop_call = match &outer.prop {
        MemberProp::Ident(id) if id.sym.as_str() == "call" => true,
        _ => false,
    };
    if !prop_call { return Ok(None); }
    let mid = match outer.obj.as_ref() {
        Expr::Member(m) => m,
        _ => return Ok(None),
    };
    let prop_tostring = match &mid.prop {
        MemberProp::Ident(id) if id.sym.as_str() == "toString" => true,
        _ => false,
    };
    if !prop_tostring { return Ok(None); }
    let inner = match mid.obj.as_ref() {
        Expr::Member(m) => m,
        _ => return Ok(None),
    };
    let prop_proto = match &inner.prop {
        MemberProp::Ident(id) if id.sym.as_str() == "prototype" => true,
        _ => false,
    };
    if !prop_proto { return Ok(None); }
    let is_object = matches!(inner.obj.as_ref(), Expr::Ident(id) if id.sym.as_str() == "Object");
    if !is_object { return Ok(None); }

    // Match. Resolve arg + tag.
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        return Ok(None);
    }
    // Pre-detecta literais nullish e funcoes antes do lower_expr.
    let is_null_lit = matches!(call.args[0].expr.as_ref(), Expr::Lit(swc_ecma_ast::Lit::Null(_)));
    let is_undef_ident = matches!(
        call.args[0].expr.as_ref(),
        Expr::Ident(id) if id.sym.as_str() == "undefined"
    );
    let is_function = match call.args[0].expr.as_ref() {
        Expr::Arrow(_) | Expr::Fn(_) => true,
        Expr::Ident(id) => {
            let n = id.sym.as_str();
            // Idents que sao user fn ou hoisted arrow/fn.
            n.starts_with("__hoisted_arrow_")
                || n.starts_with("__hoisted_fn_")
                || n.starts_with("__lifted_arr_method_")
                || ctx.user_fns.contains_key(n)
        }
        _ => false,
    };
    let tag: i64 = if is_null_lit { 4 }
        else if is_undef_ident { 5 }
        else if is_function { 6 }
        else { 0 };

    let (value_i64, computed_tag) = if tag != 0 {
        // Para null/undefined, valor nao importa.
        let v = ctx.builder.ins().iconst(cl::I64, 0);
        (v, tag)
    } else {
        let tv = lower_expr(ctx, &call.args[0].expr)?;
        let t = match tv.ty {
            ValTy::I64 | ValTy::I32 | ValTy::F64 | ValTy::U64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => 1, // Number
            ValTy::Bool => 3,
            ValTy::Handle => 0, // runtime decide
        };
        let v = ctx.coerce_to_i64(tv).val;
        (v, t)
    };
    let tag_v = ctx.builder.ins().iconst(cl::I64, computed_tag);
    let fref = ctx.get_extern(
        "__RTS_FN_RT_OBJECT_TO_STRING",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(fref, &[value_i64, tag_v]);
    let r = ctx.builder.inst_results(inst)[0];
    Ok(Some(TypedVal::new(r, ValTy::Handle)))
}



/// (cross-runtime #292) Se `expr` for `new C(...)` ou ident tipado de classe
/// registrada com metodo `toJSON`, retorna `<expr>.toJSON()` para que o
/// codegen lower invoque o metodo antes de stringify.
fn rewrite_to_json_call(ctx: &FnCtx, expr: &swc_ecma_ast::Expr) -> Option<swc_ecma_ast::Expr> {
    use swc_ecma_ast::{Expr, MemberExpr, MemberProp, CallExpr, Callee};
    let class_name = match expr {
        Expr::New(n) => match n.callee.as_ref() {
            Expr::Ident(id) => Some(id.sym.to_string()),
            _ => None,
        },
        Expr::Ident(id) => {
            let name = id.sym.as_str();
            ctx.local_class_ty.get(name).cloned()
        }
        _ => None,
    }?;
    let meta = ctx.classes.get(&class_name)?;
    let has_to_json = meta.methods.iter().any(|m| m == "toJSON")
        || resolve_method_owner(ctx, &class_name, "toJSON").is_some();
    if !has_to_json {
        return None;
    }
    Some(Expr::Call(CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
            span: Default::default(),
            obj: Box::new(expr.clone()),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                span: Default::default(),
                sym: "toJSON".into(),
            }),
        }))),
        args: Vec::new(),
        type_args: None,
    }))
}

/// (#477) Mapeia o nome-sentinela da state-machine de generators para o
/// simbolo runtime real + tipo de retorno (None = void). Devolve None se o
/// ident nao for uma sentinela conhecida ou a aridade nao bater.
fn gen_sm_sentinel(
    name: &str,
    argc: usize,
) -> Option<(&'static str, Option<cranelift_codegen::ir::Type>)> {
    use cranelift_codegen::ir::types as cl;
    match (name, argc) {
        ("__RTS_GEN_SM_NEW", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_NEW", Some(cl::I64))),
        ("__RTS_GEN_SM_FGET", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_FGET", Some(cl::I64))),
        ("__RTS_GEN_SM_FSET", 3) => Some(("__RTS_FN_NS_GC_GEN_SM_FSET", None)),
        ("__RTS_GEN_SM_STATE", 1) => Some(("__RTS_FN_NS_GC_GEN_SM_STATE", Some(cl::I64))),
        ("__RTS_GEN_SM_SETSTATE", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_SETSTATE", None)),
        ("__RTS_GEN_SM_YIELD", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_YIELD", Some(cl::I64))),
        ("__RTS_GEN_SM_DONE", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_DONE", Some(cl::I64))),
        ("__RTS_GEN_SM_ENTER_TRY", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY", None)),
        ("__RTS_GEN_SM_ENTER_TRY_CATCH", 2) => Some(("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH", None)),
        ("__RTS_GEN_SM_EXIT_TRY_CATCH", 1) => Some(("__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH", None)),
        ("__RTS_GEN_SM_CAUGHT", 1) => Some(("__RTS_FN_NS_GC_GEN_SM_CAUGHT", Some(cl::I64))),
        ("__RTS_GEN_SM_END_FINALLY", 1) => Some(("__RTS_FN_NS_GC_GEN_SM_END_FINALLY", Some(cl::I64))),
        // (#477/#211) yield* lazy delegation: itera a fonte 1 valor por vez.
        ("__RTS_GEN_SM_SENT", 1) => Some(("__RTS_FN_NS_GC_GEN_SM_SENT", Some(cl::I64))),
        ("__RTS_GEN_DELEGATE_START", 1) => Some(("__RTS_FN_NS_GC_GEN_DELEGATE_START", Some(cl::I64))),
        ("__RTS_GEN_DELEGATE_NEXT", 1) => Some(("__RTS_FN_NS_GC_GEN_DELEGATE_NEXT", Some(cl::I64))),
        ("__RTS_GEN_DELEGATE_DONE", 1) => Some(("__RTS_FN_NS_GC_GEN_DELEGATE_DONE", Some(cl::I64))),
        // (#207 async-SM) Sentinelas da state-machine de async functions.
        ("__RTS_ASYNC_SM_NEW", 2) => Some(("__RTS_FN_NS_GC_ASYNC_SM_NEW", Some(cl::I64))),
        ("__RTS_ASYNC_SM_START", 1) => Some(("__RTS_FN_NS_GC_ASYNC_SM_START", Some(cl::I64))),
        ("__RTS_ASYNC_SM_SUSPEND", 2) => Some(("__RTS_FN_NS_GC_ASYNC_SM_SUSPEND", Some(cl::I64))),
        ("__RTS_ASYNC_SM_AWAITED", 1) => Some(("__RTS_FN_NS_GC_ASYNC_SM_AWAITED", Some(cl::I64))),
        ("__RTS_ASYNC_SM_RESOLVE", 2) => Some(("__RTS_FN_NS_GC_ASYNC_SM_RESOLVE", Some(cl::I64))),
        // (cross-runtime #392) async generator: ctor lazy.
        ("__RTS_AGEN_NEW", 2) => Some(("__RTS_FN_NS_GC_AGEN_NEW", Some(cl::I64))),
        _ => None,
    }
}
