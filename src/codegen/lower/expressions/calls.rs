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
                // Console builtin precisa preceder o lookup (#380).
                if let Some(tv) = lower_console_call(ctx, &qualified, call)? {
                    return Ok(tv);
                }
                if lookup(&qualified).is_some() {
                    return lower_ns_call(ctx, &qualified, call);
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
            if let Some(method_name) = prop_method_name {
                if let Some(class_name) = lhs_static_class(ctx, &m.obj) {
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
                    // Global class instance methods (e.g. Date.getFullYear())
                    if let Some(spec) = crate::abi::global_class_lookup(&class_name) {
                        if let Some(member) = spec.instance_method(&method_name) {
                            let recv_tv = lower_expr(ctx, &m.obj)?;
                            let recv_i64 = ctx.coerce_to_i64(recv_tv).val;
                            return lower_global_instance_call(ctx, member, recv_i64, call);
                        }
                    }
                }
                // Numeric/string instance methods on literal/computed expressions:
                // (1000).toString(), (3.14).toFixed(2), "hi".toUpperCase().
                // Only when obj is NOT a plain Ident (those are handled via qualified_member_name
                // at the outer dispatch path which has the global_class_lookup).
                if !matches!(m.obj.as_ref(), Expr::Ident(_)) {
                    let recv_tv = lower_expr(ctx, &m.obj)?;
                    if matches!(recv_tv.ty, ValTy::F64 | ValTy::I64 | ValTy::I32) {
                        let recv_f = to_f64(ctx, recv_tv);
                        if let Some(tv) = lower_number_builtin(ctx, &method_name, recv_f, call)? {
                            return Ok(tv);
                        }
                    }
                    if matches!(recv_tv.ty, ValTy::Handle) {
                        let recv_h = ctx.coerce_to_i64(recv_tv).val;
                        if let Some(tv) = lower_string_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                    }
                }
            }
        }
        if let Some(qualified) = qualified_member_name(callee) {
            // Console builtin (#221, #380): console.log/info/debug → io.print,
            // console.error/warn → io.eprint. Args concatenados separados
            // por espaco. PRECISA vir antes do `lookup` generico porque
            // console.* tambem esta listado em SPECS (com aridade fixa
            // `StrPtr`) so' pra type-check / `rts apis` — passar 42 ali
            // dispararia "StrPtr argument must be a string value".
            if let Some(tv) = lower_console_call(ctx, &qualified, call)? {
                return Ok(tv);
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

            // Date static methods (#220): Date.now() / Date.parse() via GlobalClassSpec.
            if let Some((cls, method)) = qualified.split_once('.') {
                if let Some(spec) = crate::abi::global_class_lookup(cls) {
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
            // (#266) Object globals: Object.keys, Object.values, Object.hasOwn.
            if let Some(method) = qualified.strip_prefix("Object.") {
                let target = match method {
                    "keys" => "collections.map_keys",
                    "values" => "collections.map_values",
                    "hasOwn" => "collections.map_has",
                    _ => "",
                };
                if !target.is_empty() && lookup(target).is_some() {
                    return lower_ns_call(ctx, target, call);
                }
                // (#264 PR5) Object.create(proto) — aloca Map com __proto__.
                if method == "create" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let proto_h = ctx.coerce_to_i64(arg_tv).val;
                    let create_fn = ctx.get_extern(
                        "__RTS_FN_GL_OBJECT_CREATE",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(create_fn, &[proto_h]);
                    let v = ctx.builder.inst_results(inst)[0];
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
                // (#208 / #479) Object.freeze(obj) — v0 no-op, retorna handle.
                if method == "freeze" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_FREEZE",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[h]);
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
                    let a_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let b_tv = lower_expr(ctx, &call.args[1].expr)?;
                    // Se ambos sao F64, faz bitwise comparison (cobre NaN==NaN
                    // e 0!=-0 corretamente).
                    if matches!(a_tv.ty, ValTy::F64) || matches!(b_tv.ty, ValTy::F64) {
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
            }
            // (#218) Reflect API v0: get/set/has/deleteProperty/ownKeys.
            // Reusa as fns MAP_* (semantica identica a Object.*).
            // Nota: ownKeys retorna sorted (mesma limitacao de Object.keys).
            if let Some(method) = qualified.strip_prefix("Reflect.") {
                match method {
                    "get" if call.args.len() == 2 => {
                        return lower_ns_call(ctx, "collections.map_get", call);
                    }
                    "has" if call.args.len() == 2 => {
                        return lower_ns_call(ctx, "collections.map_has", call);
                    }
                    "ownKeys" if call.args.len() == 1 => {
                        return lower_ns_call(ctx, "collections.map_keys", call);
                    }
                    "deleteProperty" if call.args.len() == 2 => {
                        // map_delete retorna I64 (0/1). Reescreve como Bool.
                        let tv = lower_ns_call(ctx, "collections.map_delete", call)?;
                        return Ok(TypedVal::new(tv.val, ValTy::Bool));
                    }
                    "set" if call.args.len() == 3 => {
                        // map_set eh Void. Faz a chamada e retorna true.
                        let _ = lower_ns_call(ctx, "collections.map_set", call)?;
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    _ => {}
                }
            }
            // (#208 / #476) Array static globals: isArray, from.
            if let Some(method) = qualified.strip_prefix("Array.") {
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
                    // Detecta `{length: N}` literal.
                    if let Expr::Object(obj_lit) = first.expr.as_ref() {
                        let mut length_lit: Option<i64> = None;
                        for prop in &obj_lit.props {
                            if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                                if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                                    let key = match &kv.key {
                                        swc_ecma_ast::PropName::Ident(i) => Some(i.sym.as_str().to_string()),
                                        swc_ecma_ast::PropName::Str(s) => Some(s.value.to_string_lossy().to_string()),
                                        _ => None,
                                    };
                                    if key.as_deref() == Some("length") {
                                        if let Expr::Lit(swc_ecma_ast::Lit::Num(n)) = kv.value.as_ref() {
                                            length_lit = Some(n.value as i64);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(n_lit) = length_lit {
                            let n = ctx.builder.ins().iconst(cl::I64, n_lit);
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
                            if let Some(var_ty) = ctx.var_ty(obj_name) {
                                let is_primitive = matches!(
                                    var_ty,
                                    crate::codegen::lower::ctx::ValTy::Bool
                                        | crate::codegen::lower::ctx::ValTy::F64
                                        | crate::codegen::lower::ctx::ValTy::I32
                                );
                                if !is_primitive {
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
                        if ctx.var_ty(obj_name).is_some() {
                            if let Some(tv) = lower_function_handle_method(ctx, &m.obj, prop_name, call)? {
                                return Ok(tv);
                            }
                        }
                    }
                }
            }
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
        "isNaN" => lower_coerce_is_nan(ctx, call).map(Some),
        "isFinite" => lower_coerce_is_finite(ctx, call).map(Some),
        "Number" => lower_coerce_to_number(ctx, call),
        "String" => lower_coerce_to_string(ctx, call),
        "Boolean" => lower_coerce_to_boolean(ctx, call),
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
        "structuredClone" => Ok(Some(lower_ns_call(ctx, "text_encoding.structuredClone", call)?)),
        "queueMicrotask" => Ok(Some(lower_ns_call(ctx, "text_encoding.queueMicrotask", call)?)),

        _ => Ok(None),
    }
}

pub(super) fn resolve_method_owner(ctx: &FnCtx, class: &str, method: &str) -> Option<String> {
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

pub(super) fn lower_new(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
    // (#264) Peel TsAs/Paren etc para suportar `new (Animal as any)(...)`.
    let mut callee_expr: &Expr = new_expr.callee.as_ref();
    loop {
        match callee_expr {
            Expr::TsAs(a) => callee_expr = &a.expr,
            Expr::TsTypeAssertion(a) => callee_expr = &a.expr,
            Expr::TsConstAssertion(a) => callee_expr = &a.expr,
            Expr::TsSatisfies(a) => callee_expr = &a.expr,
            Expr::TsNonNull(a) => callee_expr = &a.expr,
            Expr::Paren(p) => callee_expr = &p.expr,
            _ => break,
        }
    }
    let class_name = match callee_expr {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => {
            return Err(anyhow!(
                "`new` so suporta callee identifier (sem `new (expr)()`)"
            ));
        }
    };

    // Function global (#359): `new Function(...params, body)` — variadic.
    // Empacota todos args excerto o ultimo em string CSV de params, ultimo
    // arg eh body. Aceita aridade 0+ (0 args = throw, 1 arg = body sem
    // params, 2+ args = (...params, body)).
    if class_name == "Function" {
        return lower_new_function(ctx, new_expr);
    }

    // Global class constructors: new Date(), new Date(ms), new Date(isoStr)
    if let Some(spec) = crate::abi::global_class_lookup(&class_name) {
        let n_args = new_expr.args.as_ref().map(|a| a.len()).unwrap_or(0);
        // When multiple constructors share the same arity (e.g. Date: I64 vs StrPtr),
        // prefer the one whose first arg type matches the AST arg expression kind:
        //   - string literal or Handle expr → StrPtr ctor
        //   - otherwise → first matching ctor (numeric/I64)
        let ctor = {
            let candidates: Vec<_> = spec.constructors()
                .filter(|m| m.args.len() == n_args)
                .collect();
            if candidates.len() > 1 {
                // Disambiguate by first arg: string literal → StrPtr ctor
                let first_is_str = new_expr.args.as_ref()
                    .and_then(|a| a.first())
                    .map(|a| matches!(a.expr.as_ref(), Expr::Lit(swc_ecma_ast::Lit::Str(_))))
                    .unwrap_or(false);
                if first_is_str {
                    candidates.into_iter().find(|m| m.args.first() == Some(&AbiType::StrPtr))
                        .or_else(|| spec.constructor_for_arity(n_args))
                } else {
                    candidates.into_iter().find(|m| m.args.first() != Some(&AbiType::StrPtr))
                        .or_else(|| spec.constructor_for_arity(n_args))
                }
            } else {
                spec.constructor_for_arity(n_args)
                    // Fallback: nenhum ctor com aridade exata. Pega o primeiro com
                    // aridade >= n_args para preencher os faltantes com defaults
                    // (e.g. `new Error()` resolve no ctor StrPtr passando ptr=0,
                    // len=0 que `str_from_raw` trata como String::new()).
                    .or_else(|| spec.constructors().find(|m| m.args.len() >= n_args))
            }
        }.ok_or_else(|| anyhow!("`new {}`: no constructor with {n_args} args", class_name))?;
        let sig = crate::abi::signature::lower_member(ctor);
        let mut arg_vals = Vec::new();
        let provided_args = new_expr.args.as_deref().unwrap_or(&[]);
        for (idx, expected) in ctor.args.iter().enumerate() {
            if let Some(arg) = provided_args.get(idx) {
                let tv = lower_expr(ctx, &arg.expr)?;
                if *expected == AbiType::StrPtr {
                    // expand handle to (ptr, len)
                    let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                    let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                    let h = ctx.coerce_to_i64(tv).val;
                    let pi = ctx.builder.ins().call(ptr_fn, &[h]);
                    let ptr = ctx.builder.inst_results(pi)[0];
                    let li = ctx.builder.ins().call(len_fn, &[h]);
                    let len = ctx.builder.inst_results(li)[0];
                    arg_vals.push(ptr);
                    arg_vals.push(len);
                } else {
                    arg_vals.push(ctx.coerce_to_i64(tv).val);
                }
            } else if *expected == AbiType::StrPtr {
                // Arg omitido: passa (ptr=0, len=0) — runtime trata como string vazia.
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                arg_vals.push(zero);
                arg_vals.push(zero);
            } else {
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                arg_vals.push(zero);
            }
        }
        let fn_ref = ctx.get_extern(ctor.symbol, &sig.params, sig.ret)?;
        let inst = ctx.builder.ins().call(fn_ref, &arg_vals);
        let handle = ctx.builder.inst_results(inst)[0];
        // Track local variable type for instance method dispatch
        // Caller (lower_var_decl) will store the binding name; we store the class name
        // via the return value annotation. Here we return a Handle tagged with class name.
        // The caller in lower_let_decl sets local_class_ty[bind] = class_name when it sees
        // a NewExpr whose callee is a known class. We need to ensure class_name is in
        // local_class_ty — do so by inserting via the returned TypedVal metadata.
        // Since we can't do it here directly (no bind name), the lower_let path handles it.
        // But we need to mark it as a global class so lhs_static_class can find it.
        // Store class_name in global_class_ty isn't mutable. Use local_class_ty trick:
        // The VarDecl lowering already calls `ctx.local_class_ty.insert(bind, class_name)`
        // when it detects a NewExpr with a known user class. We must ensure the same
        // happens for global classes. See lower/func.rs compile_user_fn var_decl handling.
        // For now return Handle — the VarDecl lowering will handle local_class_ty.
        return Ok(TypedVal::new(handle, ValTy::Handle));
    }

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

    // (#214) Error / TypeError / RangeError / ReferenceError / SyntaxError —
    // builtin error classes JS. Implementados como Map handle com keys
    // \`message\` (string) e \`name\` (string). Permite \`throw new Error(\"x\")\`,
    // \`(e as Error).message\`, comparacoes etc.
    let is_error_class = matches!(
        class_name.as_str(),
        "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
    );
    if is_error_class && !ctx.classes.contains_key(&class_name) {
        let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(new_fn, &[]);
        let h = ctx.builder.inst_results(inst)[0];

        let set_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            &[cl::I64, cl::I64, cl::I64, cl::I64],
            None,
        )?;

        // Set name = "Error" / "TypeError" / etc.
        let (name_kp, name_kl) = ctx.emit_str_literal(b"name")?;
        let (cls_ptr, cls_len) = ctx.emit_str_literal(class_name.as_bytes())?;
        let from_static = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(from_static, &[cls_ptr, cls_len]);
        let name_handle = ctx.builder.inst_results(inst)[0];
        ctx.builder.ins().call(set_fn, &[h, name_kp, name_kl, name_handle]);

        // Set message = primeiro argumento (string) ou "" se sem args.
        let msg_handle = if let Some(arg) = new_expr.args.as_ref().and_then(|args| args.first()) {
            if arg.spread.is_none() {
                let tv = super::lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_handle(tv)?.val
            } else {
                ctx.emit_str_handle(b"")?.val
            }
        } else {
            ctx.emit_str_handle(b"")?.val
        };
        let (msg_kp, msg_kl) = ctx.emit_str_literal(b"message")?;
        ctx.builder.ins().call(set_fn, &[h, msg_kp, msg_kl, msg_handle]);

        return Ok(TypedVal::new(h, ValTy::Handle));
    }

    // (#264 PR3) Constructor function: `new Animal(name)` quando Animal eh
    // user fn nao-classe. Aloca Map (instance), empilha como `this` no slot
    // thread-local, chama Animal(args), desempilha. Retorna o Map handle.
    //
    // Limitacao desta PR: nao instala __proto__ chain — `instance.method()`
    // ainda nao acha methods em Animal.prototype.method (PR 4 cobre).
    // Mas `this.field = v` no body do constructor ja persiste no Map
    // (assignment usa MAP_SET sobre o handle do `this`).
    if !ctx.classes.contains_key(&class_name)
        && ctx.user_fns.contains_key(&class_name)
    {
        // 1. Aloca Map vazio (instance).
        let map_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(map_new, &[]);
        let inst_h = ctx.builder.inst_results(inst)[0];

        // 1.5 (#264 PR4) Instala __proto__ chain: aloca o prototype Map
        // de Animal e armazena em instance.__proto__ pra lookup chain
        // walk em member access subsequentes.
        let proto_get = ctx.get_extern(
            "__RTS_FN_GL_FUNCTION_PROTOTYPE_GET",
            &[cl::I64],
            Some(cl::I64),
        )?;
        // Para chamar PROTOTYPE_GET precisa do Function handle — reify
        // a partir do user fn.
        let fn_addr = emit_user_fn_addr(ctx, &class_name)?.val;
        let arity = ctx
            .user_fns
            .get(&class_name)
            .map(|f| f.params.len() as i64)
            .unwrap_or(0);
        let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
        let name_tv = ctx.emit_str_handle(class_name.as_bytes())?;
        let name_h = ctx.coerce_to_i64(name_tv).val;
        let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
        let n_ptr = ctx.builder.inst_results(inst_p)[0];
        let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
        let n_len = ctx.builder.inst_results(inst_l)[0];
        let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
        let has_this_v = ctx.builder.ins().iconst(cl::I32, 0);
        let reify_fn = ctx.get_extern(
            "__RTS_FN_GL_FUNCTION_REIFY",
            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32],
            Some(cl::I64),
        )?;
        let inst_r = ctx
            .builder
            .ins()
            .call(reify_fn, &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
        let fn_handle = ctx.builder.inst_results(inst_r)[0];
        let inst_proto = ctx.builder.ins().call(proto_get, &[fn_handle]);
        let proto_h = ctx.builder.inst_results(inst_proto)[0];
        // map_set(inst_h, "__proto__", proto_h)
        let map_set_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            &[cl::I64, cl::I64, cl::I64, cl::I64],
            None,
        )?;
        let (proto_kp, proto_kl) = ctx.emit_str_literal(b"__proto__")?;
        ctx.builder.ins().call(map_set_fn, &[inst_h, proto_kp, proto_kl, proto_h]);

        // 2. Push this slot (thread-local, usado por `Expr::This` no body).
        let push_fn = ctx.get_extern(
            "__RTS_FN_RT_THIS_PUSH",
            &[cl::I64],
            None,
        )?;
        ctx.builder.ins().call(push_fn, &[inst_h]);

        // 3. Chama a user fn como call direto. Args sao os do new_expr.
        // Reusa o caminho de chamada normal sintetizando um CallExpr.
        let synthetic_callee = Box::new(Expr::Ident(swc_ecma_ast::Ident::new(
            class_name.as_str().into(),
            new_expr.span,
            new_expr.ctxt,
        )));
        let synthetic_args = new_expr.args.clone().unwrap_or_default();
        let synthetic_call = swc_ecma_ast::CallExpr {
            span: new_expr.span,
            ctxt: new_expr.ctxt,
            callee: swc_ecma_ast::Callee::Expr(synthetic_callee),
            args: synthetic_args,
            type_args: new_expr.type_args.clone(),
        };
        let _call_result = lower_call(ctx, &synthetic_call)?;

        // 4. Pop this slot.
        let pop_fn = ctx.get_extern("__RTS_FN_RT_THIS_POP", &[], None)?;
        ctx.builder.ins().call(pop_fn, &[]);

        // 5. Retorna o instance handle.
        return Ok(TypedVal::new(inst_h, ValTy::Handle));
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
        let mangled: String = format!("__user_{init_fn_name}");
        let fn_id = *ctx
            .extern_cache
            .get(mangled.as_str())
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

/// Function global (#359): `new Function(...params, body)` variadic.
/// Concatena params em CSV e chama __RTS_FN_GL_FUNCTION_NEW(params_str, body).
fn lower_new_function(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
    use crate::codegen::lower::ctx::ValTy;

    let args = new_expr.args.as_ref();
    let n = args.map(|a| a.len()).unwrap_or(0);
    if n == 0 {
        return Err(anyhow!("new Function() requer pelo menos 1 arg (body)"));
    }
    let args_vec = args.unwrap();
    let body_idx = n - 1;

    // Concatena params 0..body_idx em CSV. Para isso constroi handle string
    // em runtime via gc.string_concat com ",". Caso comum: 1-3 params.
    let params_handle = if body_idx == 0 {
        // sem params — string vazia
        ctx.emit_str_handle(b"")?.val
    } else {
        // Acumulador comeca com primeiro param.
        let first_tv = lower_expr(ctx, &args_vec[0].expr)?;
        let mut acc = ctx.coerce_to_i64(first_tv).val;
        let comma_h = ctx.emit_str_handle(b",")?.val;
        let concat_fn = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        for i in 1..body_idx {
            let tv = lower_expr(ctx, &args_vec[i].expr)?;
            let p = ctx.coerce_to_i64(tv).val;
            let inst1 = ctx.builder.ins().call(concat_fn, &[acc, comma_h]);
            let acc1 = ctx.builder.inst_results(inst1)[0];
            let inst2 = ctx.builder.ins().call(concat_fn, &[acc1, p]);
            acc = ctx.builder.inst_results(inst2)[0];
        }
        acc
    };

    let body_tv = lower_expr(ctx, &args_vec[body_idx].expr)?;
    let body_h = ctx.coerce_to_i64(body_tv).val;

    let new_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_NEW",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(new_fn, &[params_handle, body_h]);
    let r = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(r, ValTy::Handle))
}

/// Function global (#359): chamada de metodo em handle Function ja' reificado
/// (var de `bind()` ou `new Function`). Sem reify — receiver eh o handle direto.
fn lower_function_handle_method(
    ctx: &mut FnCtx,
    obj: &Expr,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::codegen::lower::ctx::ValTy;

    let obj_tv = lower_expr(ctx, obj)?;
    let fn_handle = ctx.coerce_to_i64(obj_tv).val;

    match method {
        "toString" => {
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_TO_STRING",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(to_str_fn, &[fn_handle]);
            let r = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(r, ValTy::Handle)))
        }
        "call" | "apply" | "bind" => {
            let this_arg = if let Some(arg) = call.args.first() {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let args_vec_h = if method == "apply" {
                if let Some(arg) = call.args.get(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    ctx.coerce_to_i64(tv).val
                } else {
                    ctx.builder.ins().iconst(cl::I64, 0)
                }
            } else {
                let vec_new_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                    &[],
                    Some(cl::I64),
                )?;
                let push_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                    &[cl::I64, cl::I64],
                    None,
                )?;
                let inst_v = ctx.builder.ins().call(vec_new_fn, &[]);
                let vec_h = ctx.builder.inst_results(inst_v)[0];
                for arg in call.args.iter().skip(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    let v = ctx.coerce_to_i64(tv).val;
                    ctx.builder.ins().call(push_fn, &[vec_h, v]);
                }
                vec_h
            };
            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_CALL",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY",
                "bind" => "__RTS_FN_GL_FUNCTION_BIND",
                _ => unreachable!(),
            };
            let target_fn = ctx.get_extern(
                symbol,
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx
                .builder
                .ins()
                .call(target_fn, &[fn_handle, this_arg, args_vec_h]);
            let r = ctx.builder.inst_results(inst)[0];
            let ty = if method == "bind" { ValTy::Handle } else { ValTy::I64 };
            Ok(Some(TypedVal::new(r, ty)))
        }
        _ => Ok(None),
    }
}

/// Function global (#359): emite reify + chamada do metodo (call/apply/bind/toString)
/// pra um ident de user fn. Retorna `Ok(None)` se algo nao se encaixa (caller
/// segue pro fallback). Args sao empacotados em Vec handle pra call/apply/bind.
fn lower_function_method_call(
    ctx: &mut FnCtx,
    fn_name: &str,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::codegen::lower::ctx::ValTy;

    // 1. emit_user_fn_addr garante callconv C (address_taken_fns).
    let fn_ptr = emit_user_fn_addr(ctx, fn_name)?.val;

    // 2. Reify: __RTS_FN_GL_FUNCTION_REIFY(fn_ptr, arity, name_ptr, name_len, is_arrow).
    let arity = ctx
        .user_fns
        .get(fn_name)
        .map(|f| f.params.len() as i64)
        .unwrap_or(0);
    let name_tv = ctx.emit_str_handle(fn_name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    // emit_str_handle retorna handle GC — pra REIFY precisamos de (ptr, len).
    // Usamos gc.string_ptr/string_len no handle.
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

    // 3. Despacha por metodo.
    match method {
        "toString" => {
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_TO_STRING",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(to_str_fn, &[fn_handle]);
            let r = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(r, ValTy::Handle)))
        }
        "call" | "apply" | "bind" => {
            // Args: primeiro arg eh thisArg, demais sao argumentos da fn (call/bind)
            // ou argsArray (apply). Empacotamos demais em Vec.
            let this_arg = if let Some(arg) = call.args.first() {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };

            let args_vec_h = if method == "apply" {
                // apply(this, [a, b, c]) — segundo arg ja' eh array.
                if let Some(arg) = call.args.get(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    ctx.coerce_to_i64(tv).val
                } else {
                    ctx.builder.ins().iconst(cl::I64, 0)
                }
            } else {
                // call/bind: empacota call.args[1..] em Vec.
                let vec_new_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                    &[],
                    Some(cl::I64),
                )?;
                let push_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                    &[cl::I64, cl::I64],
                    None,
                )?;
                let inst_v = ctx.builder.ins().call(vec_new_fn, &[]);
                let vec_h = ctx.builder.inst_results(inst_v)[0];
                for arg in call.args.iter().skip(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    let v = ctx.coerce_to_i64(tv).val;
                    ctx.builder.ins().call(push_fn, &[vec_h, v]);
                }
                vec_h
            };

            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_CALL",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY",
                "bind" => "__RTS_FN_GL_FUNCTION_BIND",
                _ => unreachable!(),
            };
            let ret_ty = if method == "bind" { cl::I64 } else { cl::I64 };
            let target_fn = ctx.get_extern(
                symbol,
                &[cl::I64, cl::I64, cl::I64],
                Some(ret_ty),
            )?;
            let inst = ctx
                .builder
                .ins()
                .call(target_fn, &[fn_handle, this_arg, args_vec_h]);
            let r = ctx.builder.inst_results(inst)[0];
            let ty = if method == "bind" { ValTy::Handle } else { ValTy::I64 };
            Ok(Some(TypedVal::new(r, ty)))
        }
        _ => Ok(None),
    }
}

pub(super) fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
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

    // Builtins de Number (n.toFixed(), n.toString(), etc.) em receiver numeric.
    if matches!(obj_tv.ty, ValTy::F64 | ValTy::I64 | ValTy::I32) {
        let recv_f = to_f64(ctx, obj_tv);
        if let Some(tv) = lower_number_builtin(ctx, prop, recv_f, call)? {
            return Ok(tv);
        }
    }

    // (#208) Quando a var e' sabidamente um array (declarada `T[]` /
    // `Array<T>`, ou inicializada com array literal), prefere
    // `lower_array_builtin` antes de string/map. Sem isso, `arr.indexOf(2)`
    // cai em `__RTS_FN_GL_STRING_INDEX_OF` e retorna lixo.
    let is_array_var = ctx.local_array_vars.contains(obj_name);
    if is_array_var {
        if let Some(tv) = lower_array_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
    }

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
    // (Fallback caso `is_array_var` seja false e o tipo runtime seja vec.)
    if !is_array_var {
        if let Some(tv) = lower_array_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
    }

    // (#264 PR5) `obj.hasOwnProperty(key)` — verifica own props sem chain.
    if prop == "hasOwnProperty" && call.args.len() == 1 {
        let key_tv = lower_expr(ctx, &call.args[0].expr)?;
        let key_h = ctx.coerce_to_i64(key_tv).val;
        let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let inst_p = ctx.builder.ins().call(str_ptr_fn, &[key_h]);
        let kptr = ctx.builder.inst_results(inst_p)[0];
        let inst_l = ctx.builder.ins().call(str_len_fn, &[key_h]);
        let klen = ctx.builder.inst_results(inst_l)[0];
        let has_own = ctx.get_extern(
            "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(has_own, &[obj_h, kptr, klen]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(v, ValTy::Bool));
    }

    let (kp, kl) = ctx.emit_str_literal(prop.as_bytes())?;
    // (#264 PR5) MAP_GET_CHAIN: lookup own + __proto__ chain. Permite
    // `instance.method()` resolver methods em \`Animal.prototype.method\`.
    let map_get = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(map_get, &[obj_h, kp, kl]);
    let callee_val = ctx.builder.inst_results(inst)[0];

    // Guard runtime: se obj nao for um Map (e.g. Vec sem o metodo
    // requested como `arr.filter` antes da feature #267 estar pronta),
    // map_get retorna 0 e o call_indirect saltaria pra endereco 0
    // → segfault silencioso. Trap explicito da diagnostico claro em
    // vez de access violation.
    ctx.builder
        .ins()
        .trapz(callee_val, cranelift_codegen::ir::TrapCode::user(1).unwrap());

    // (#proto-method) Empacota args em Vec<i64> handle e chama
    // INVOKE_AUTO que decide entre handle Function (typed via
    // invoke_typed com return_kind) e fn ptr raw (invoke_n i64-only).
    // Cobre o caso de \`Animal.prototype.x = userFn\` armazenado como
    // handle Function (PR proto-method).
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
            return Err(anyhow!("spread not supported in var.member call"));
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
    let inst = ctx.builder.ins().call(invoke_auto, &[callee_val, obj_h, args_h]);
    let v = ctx.builder.inst_results(inst)[0];

    // (#proto-method) Marca o resultado para que lower_tpl use
    // TPL_COERCE_AUTO (detecta handle de string em runtime).
    ctx.var_member_call_values.insert(v);

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
    // Helper: lower arg[idx] -> handle i64.
    fn arg_handle(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in string builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_handle(tv)?.val)
    }

    fn arg_i64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in string builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    // Macro para chamadas simples: symbol(recv [, args...]) -> ret_ty
    macro_rules! call_h {
        ($sym:expr, $params:expr, $ret:expr, $args:expr) => {{
            let f = ctx.get_extern($sym, $params, $ret)?;
            let i = ctx.builder.ins().call(f, $args);
            ctx.builder.inst_results(i)[0]
        }};
    }

    match method {
        // ── length as method call (parens) ── kept for compat
        "length" => {
            let v = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // ── search ──────────────────────────────────────────────────────
        "indexOf" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_INDEX_OF", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "lastIndexOf" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_LAST_INDEX_OF", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "includes" | "contains" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_INCLUDES", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "startsWith" | "starts_with" => {
            let prefix = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_STARTS_WITH", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, prefix]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "endsWith" | "ends_with" => {
            let suffix = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_ENDS_WITH", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, suffix]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        // ── indexing ─────────────────────────────────────────────────────
        "charAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CHAR_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "charCodeAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CHAR_CODE_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "codePointAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CODE_POINT_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "at" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // ── slicing ───────────────────────────────────────────────────────
        "slice" => {
            let start = arg_i64(ctx, call, 0)?;
            let end = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SLICE", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, end]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "substring" => {
            let start = arg_i64(ctx, call, 0)?;
            let end = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SUBSTRING", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, end]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "substr" => {
            let start = arg_i64(ctx, call, 0)?;
            let len = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SUBSTR", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, len]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // ── transform ─────────────────────────────────────────────────────
        "toLowerCase" | "toLocaleLowerCase" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_LOWER_CASE", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toUpperCase" | "toLocaleUpperCase" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_UPPER_CASE", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trim" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimStart" | "trimLeft" | "trim_start" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM_START", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimEnd" | "trimRight" | "trim_end" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM_END", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "repeat" => {
            let n = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPEAT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, n]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "replace" => {
            let from = arg_handle(ctx, call, 0)?;
            let to   = arg_handle(ctx, call, 1)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPLACE", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, from, to]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "replaceAll" => {
            let from = arg_handle(ctx, call, 0)?;
            let to   = arg_handle(ctx, call, 1)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPLACE_ALL", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, from, to]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "concat" => {
            let other = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CONCAT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, other]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "padStart" => {
            let target = arg_i64(ctx, call, 0)?;
            let pad = if call.args.len() > 1 {
                arg_handle(ctx, call, 1)?
            } else {
                ctx.emit_str_handle(b" ")?.val
            };
            let v = call_h!("__RTS_FN_GL_STRING_PAD_START", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, target, pad]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "padEnd" => {
            let target = arg_i64(ctx, call, 0)?;
            let pad = if call.args.len() > 1 {
                arg_handle(ctx, call, 1)?
            } else {
                ctx.emit_str_handle(b" ")?.val
            };
            let v = call_h!("__RTS_FN_GL_STRING_PAD_END", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, target, pad]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "split" => {
            let sep = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_SPLIT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, sep]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `s.match(pattern)` — primeiro match, retorna string handle ou 0.
        "match" => {
            let pattern = arg_handle(ctx, call, 0)?;
            // Converte recv_h e pattern de handle pra (ptr, len).
            let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
            let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            let p2 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[pattern]);
            let l2 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[pattern]);
            let v = call_h!(
                "__RTS_FN_NS_STRING_MATCH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
                &[p1, l1, p2, l2]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `s.search(pattern)` — index do primeiro match, ou -1.
        "search" => {
            let pattern = arg_handle(ctx, call, 0)?;
            let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
            let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            let p2 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[pattern]);
            let l2 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[pattern]);
            let v = call_h!(
                "__RTS_FN_NS_STRING_SEARCH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
                &[p1, l1, p2, l2]
            );
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "localeCompare" => {
            let other = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_LOCALE_COMPARE", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, other]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "toString" | "valueOf" | "toWellFormed" | "normalize" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_STRING", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "isWellFormed" => {
            let v = call_h!("__RTS_FN_GL_STRING_IS_WELL_FORMED", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        _ => Ok(None),
    }
}

/// Number instance methods em receiver F64/I64 (n.toFixed(), n.toString(), etc.).
/// Retorna `Some` quando reconheceu o metodo — semântica idêntica ao lower_string_builtin.
fn lower_number_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_f: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // arg[idx] como i64 (digits, radix)
    fn arg_i64_opt(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
        default: i64,
    ) -> Result<cranelift_codegen::ir::Value> {
        if let Some(arg) = call.args.get(idx) {
            if arg.spread.is_some() {
                return Ok(ctx.builder.ins().iconst(cl::I64, default));
            }
            let tv = lower_expr(ctx, &arg.expr)?;
            Ok(ctx.coerce_to_i64(tv).val)
        } else {
            Ok(ctx.builder.ins().iconst(cl::I64, default))
        }
    }

    macro_rules! call_num {
        ($sym:expr, $params:expr, $ret:expr, $args:expr) => {{
            let f = ctx.get_extern($sym, $params, $ret)?;
            let i = ctx.builder.ins().call(f, $args);
            ctx.builder.inst_results(i)[0]
        }};
    }

    match method {
        "toFixed" => {
            let digits = arg_i64_opt(ctx, call, 0, 0)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_FIXED",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toPrecision" => {
            let digits = arg_i64_opt(ctx, call, 0, 0)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_PRECISION",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toExponential" => {
            let digits = arg_i64_opt(ctx, call, 0, 6)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toString" => {
            // toString(radix?) — sem radix ou radix=10 usa string_from_f64.
            // Com radix usa __RTS_FN_GL_NUMBER_TO_STRING_RADIX.
            let radix = arg_i64_opt(ctx, call, 0, 10)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_STRING_RADIX",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, radix]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "valueOf" => Ok(Some(TypedVal::new(recv_f, ValTy::F64))),
        "toLocaleString" => {
            // Stub: mesma saída que toString() sem localização.
            let v = call_num!(
                "__RTS_FN_NS_GC_STRING_FROM_F64",
                &[cl::F64],
                Some(cl::I64),
                &[recv_f]
            );
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

    // Arity check: a heuristica so' deve disparar quando o nº de args
    // bate com a assinatura JS de Map/Set. Sem isso, qualquer obj literal
    // com chave `add`/`set`/`get`/`has`/`delete` que carregue uma fn de
    // user com aridade diferente cai aqui e retorna lixo (#311). Caller
    // recebe None e tenta o path generico de map_get + call_indirect.
    let arity = call.args.len();
    let expected_arity = match method {
        "set" => 2, // Map.set(key, value)
        "add" | "delete" | "has" | "get" => 1,
        _ => usize::MAX, // outros metodos nao tem checagem aqui
    };
    if expected_arity != usize::MAX && arity != expected_arity {
        return Ok(None);
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
        // (#222) Map iteration methods. Reusam fns de Object.keys/values/entries.
        "entries" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "keys" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "values" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
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
        "join" => {
            // arr.join(sep): converte sep para string handle, chama runtime.
            let sep_h = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() {
                    return Ok(None);
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_handle(tv)?.val
            } else {
                // Default JS: separador "," sem argumento.
                ctx.emit_str_handle(b",")?.val
            };
            let join_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(join_fn, &[obj_h, sep_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
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
        // (#208 / #476) Array methods sem callback — args concretos só.
        "indexOf" | "lastIndexOf" | "includes" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
            let needle = ctx.coerce_to_i64(needle_tv).val;
            let sym = match method {
                "indexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF",
                "lastIndexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF",
                _ => "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES",
            };
            let fref = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, needle]);
            let v = ctx.builder.inst_results(inst)[0];
            let ty = if method == "includes" { ValTy::Bool } else { ValTy::I64 };
            Ok(Some(TypedVal::new(v, ty)))
        }
        "reverse" | "flat" => {
            if !call.args.is_empty() {
                // flat(depth) e reverse() ambos sem args na versao v0.
                // depth diferente de 1 cai em outro PR.
                if method == "reverse" {
                    return Ok(None);
                }
            }
            let sym = match method {
                "reverse" => "__RTS_FN_NS_COLLECTIONS_VEC_REVERSE",
                _ => "__RTS_FN_NS_COLLECTIONS_VEC_FLAT",
            };
            let fref = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(fref, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "shift" => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SHIFT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "unshift" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let arg = ctx.coerce_to_i64(arg_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, arg]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "slice" => {
            // arr.slice(start?, end?). end omitido = i64::MIN sentinel.
            let start_tv = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() { return Ok(None); }
                lower_expr(ctx, &arg.expr)?
            } else {
                TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64)
            };
            let start = ctx.coerce_to_i64(start_tv).val;
            let end = if let Some(arg) = call.args.get(1) {
                if arg.spread.is_some() { return Ok(None); }
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SLICE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "concat" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let other_tv = lower_expr(ctx, &call.args[0].expr)?;
            let other = ctx.coerce_to_i64(other_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_CONCAT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, other]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "fill" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let val_tv = lower_expr(ctx, &call.args[0].expr)?;
            let value = ctx.coerce_to_i64(val_tv).val;
            let start = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let end = if let Some(arg) = call.args.get(2) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_FILL",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, value, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "splice" => {
            // splice(start, deleteCount). Versao com ...items vai em PR separada.
            if call.args.is_empty() || call.args.len() > 2
                || call.args.iter().any(|a| a.spread.is_some())
            {
                return Ok(None);
            }
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                // splice(start) sem count = remove tudo do start em diante.
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, count]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.sort()` sem comparator. `arr.sort(fn)` com comparator
        // precisa do lifter de arrow (PR separada — usuario passa Ident
        // de user fn por enquanto, que `address_taken_fns` captura).
        "sort" => {
            let fn_ptr = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() { return Ok(None); }
                if let Expr::Ident(id) = arg.expr.as_ref() {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    }
                } else {
                    return Ok(None);
                }
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SORT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.copyWithin(target, start?, end?)` — args concretos.
        "copyWithin" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let target_tv = lower_expr(ctx, &call.args[0].expr)?;
            let target = ctx.coerce_to_i64(target_tv).val;
            let start = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let end = if let Some(arg) = call.args.get(2) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, target, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.findLast/findLastIndex/reduceRight/flatMap` com user fn ident.
        // Lifting de arrow inline pra esses fica pra outra PR — segue padrao
        // do lift_inline_arrows_in_array_methods em func.rs.
        "findLast" | "findLastIndex" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let sym = if method == "findLast" {
                "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST"
            } else {
                "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX"
            };
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "reduceRight" => {
            if call.args.len() != 2 || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let init_tv = lower_expr(ctx, &call.args[1].expr)?;
            let init = ctx.coerce_to_i64(init_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, init, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "flatMap" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}

fn lower_indirect_call(ctx: &mut FnCtx, callee_expr: &Expr, call: &CallExpr) -> Result<TypedVal> {
    let callee = lower_expr(ctx, callee_expr)?;

    // Quando callee é Handle (var que recebeu fn handle de bind/REIFY/
    // new Function), despacha via __RTS_FN_GL_FUNCTION_CALL — esse path
    // entende bound_args, has_this_param, is_arrow, etc. Caso contrário
    // trata como fn pointer raw (call_indirect direto).
    if matches!(callee.ty, ValTy::Handle) {
        return emit_function_handle_indirect_call(ctx, callee.val, call);
    }

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

/// Despacha chamada via handle Function (bind/REIFY/new Function) através
/// de __RTS_FN_GL_FUNCTION_CALL. Empacota args em Vec collections.
fn emit_function_handle_indirect_call(
    ctx: &mut FnCtx,
    handle_val: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<TypedVal> {
    // Empacota args em Vec<i64>.
    let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst_v = ctx.builder.ins().call(vec_new, &[]);
    let args_handle = ctx.builder.inst_results(inst_v)[0];
    let vec_push = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        &[cl::I64, cl::I64],
        None,
    )?;
    for a in &call.args {
        if a.spread.is_some() {
            return Err(anyhow!("spread em call de handle Function nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        // Args numéricos são empacotados como `f64::to_bits` (i64) para
        // preservar precisão na travessia do Vec<i64>. O invoke_typed em
        // runtime usa `f64::from_bits` quando param_kinds[i]=1 (F64).
        // Handles/Bool ficam como i64 puro (param_kinds[i]=0).
        let v = match tv.ty {
            ValTy::F64 => ctx.builder.ins().bitcast(
                cl::I64,
                cranelift_codegen::ir::MemFlags::new(),
                tv.val,
            ),
            ValTy::I32 | ValTy::I64 => {
                // Literal int em TS é `number` (F64). Promove e empacota
                // como bits para que invoke_typed leia f64 corretamente.
                let as_f = ctx.coerce_to_f64(tv).val;
                ctx.builder.ins().bitcast(
                    cl::I64,
                    cranelift_codegen::ir::MemFlags::new(),
                    as_f,
                )
            }
            ValTy::Bool | ValTy::Handle | ValTy::U64 => ctx.coerce_to_i64(tv).val,
        };
        ctx.builder.ins().call(vec_push, &[args_handle, v]);
    }
    // thisArg = 0 (chamada direta, sem this); FUNCTION_CALL respeita
    // bound_this/has_this_param se setados em bind().
    let this_zero = ctx.builder.ins().iconst(cl::I64, 0);
    let call_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_CALL",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst_c = ctx.builder.ins().call(call_fn, &[handle_val, this_zero, args_handle]);
    let v_i64 = ctx.builder.inst_results(inst_c)[0];
    // FUNCTION_CALL retorna i64 contendo bits f64 quando o método tem
    // return_kind=1 (F64). Para métodos number (caso comum em RTS), bitcast
    // para F64. Métodos void/i64 têm bits 0 — `to_bits` de 0.0 = 0, então
    // F64 wraps continua semanticamente correto.
    let v_f64 = ctx.builder.ins().bitcast(
        cl::F64,
        cranelift_codegen::ir::MemFlags::new(),
        v_i64,
    );
    Ok(TypedVal::new(v_f64, ValTy::F64))
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
        ctx.extern_cache.insert(member.symbol.to_string(), id);
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
    }
}

fn lower_ns_call_member(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    call: &CallExpr,
) -> Result<TypedVal> {
    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }
    lower_ns_call_body(ctx, member, call)
}

fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }

    lower_ns_call_body(ctx, member, call)
}

fn lower_ns_call_body(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    call: &CallExpr,
) -> Result<TypedVal> {
    let qualified = member.symbol;
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
        ctx.extern_cache.insert(member.symbol.to_string(), id);
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
                // U64 e tipo opaco — handle de runtime ou ponteiro.
                // Quando o input e f64 (variavel `number` carregando um
                // handle), converte numericamente via fcvt_to_uint_sat:
                // o valor f64 ja' foi produzido por uma conversao i64→f64,
                // entao fcvt_to_uint_sat recupera o inteiro original.
                // Bitcast daria o bit-pattern IEEE 754, que e' um valor
                // completamente diferente — causaria handles invalidos.
                //
                // Excecao (#435): `thread.spawn*(fp, arg)` — quando o
                // worker pede `arg: number` (f64), o lifter cria
                // trampolim com param `__rts_spawn_arg_f64` que faz
                // bitcast i64→f64 ao receber. Nesse caso o caller deve
                // passar os BITS do f64, nao a conversao numerica.
                // Detectado pelo simbolo do membro (todas variantes
                // de spawn que aceitam fn_ptr+arg).
                let is_spawn_arg = matches!(
                    member.symbol,
                    "__RTS_FN_NS_THREAD_SPAWN"
                    | "__RTS_FN_NS_THREAD_SPAWN_ASYNC"
                    | "__RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN"
                    | "__RTS_FN_NS_THREAD_SPAWN_DETACHED"
                    | "__RTS_FN_NS_THREAD_SPAWN_WITH_UD"
                ) && values.len() == 1; // segundo arg = `arg` (apos fn_ptr)
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = match tv.ty {
                    crate::codegen::lower::ctx::ValTy::F64 if is_spawn_arg => {
                        ctx.builder.ins().bitcast(
                            cl::I64,
                            cranelift_codegen::ir::MemFlags::new(),
                            tv.val,
                        )
                    }
                    crate::codegen::lower::ctx::ValTy::F64 => {
                        ctx.builder.ins().fcvt_to_uint_sat(cl::I64, tv.val)
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

/// Lowers a call to a `node:*` member via nodespace lookup.
///
/// `qualified` is the codegen-internal name like `"node_fs.readFileSync"`.
/// The nodespace member maps directly to an existing RTS ABI symbol, so
/// this function builds the same extern call as `lower_ns_call` but sources
/// the metadata from `crate::nodespace::node_lookup` instead of `abi::lookup`.
fn lower_node_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    use crate::abi::signature::{lower_params, lower_return};

    let member = crate::nodespace::node_lookup(qualified)
        .ok_or_else(|| anyhow!("unknown node namespace member `{qualified}`"))?;

    let lowered_params = lower_params(member.args);
    let lowered_ret = lower_return(member.returns);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        for &p in &lowered_params {
            sig.params.push(AbiParam::new(p));
        }
        if let Some(r) = lowered_ret {
            sig.returns.push(AbiParam::new(r));
        }
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol.to_string(), id);
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
            .ok_or_else(|| anyhow!("too few arguments for node `{qualified}`"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in node namespace calls"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                fn unwrap_paren(e: &Expr) -> &Expr {
                    match e {
                        Expr::Paren(p) => unwrap_paren(&p.expr),
                        _ => e,
                    }
                }
                let lit_bytes: Option<Vec<u8>> = match unwrap_paren(&arg.expr) {
                    Expr::Lit(swc_ecma_ast::Lit::Str(s)) => Some(s.value.as_bytes().to_vec()),
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
                        let ptr_fref = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_PTR",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let len_fref = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_LEN",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
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
            AbiType::U64 | AbiType::I64 | AbiType::Handle | AbiType::Bool => {
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
    if lowered_ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        Ok(TypedVal::new(v, ValTy::from_abi(member.returns)))
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
}

/// Emits a call to a global class instance method (e.g. `d.getFullYear()`).
/// `recv` is the already-lowered Handle value. The InstanceMethod ABI has the
/// Handle as its first arg (slot 0 of member.args), so we prepend it and pass
/// the remaining TS args in order.
fn lower_global_instance_call(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    recv: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<TypedVal> {
    use crate::abi::signature::lower_member;

    let sig = lower_member(member);
    let fn_ref = ctx.get_extern(member.symbol, &sig.params, sig.ret)?;

    // slot 0 = Handle receiver; slots 1.. = TS call args
    let mut values = vec![recv];
    let abi_args = &member.args[1..]; // skip Handle slot
    let mut arg_iter = call.args.iter();
    for &abi_ty in abi_args {
        let arg = arg_iter
            .next()
            .ok_or_else(|| anyhow!("too few arguments for `{}`", member.name))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in global class method call"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                let tv = lower_expr(ctx, &arg.expr)?;
                let h = ctx.coerce_to_i64(tv).val;
                let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let pi = ctx.builder.ins().call(ptr_fn, &[h]);
                values.push(ctx.builder.inst_results(pi)[0]);
                let li = ctx.builder.ins().call(len_fn, &[h]);
                values.push(ctx.builder.inst_results(li)[0]);
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(to_f64(ctx, tv));
            }
            _ => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
        }
    }

    let inst = ctx.builder.ins().call(fn_ref, &values);
    if sig.ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        Ok(TypedVal::new(v, ValTy::from_abi(member.returns)))
    } else {
        Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64))
    }
}

fn lower_user_call(ctx: &mut FnCtx, name: &str, call: &CallExpr) -> Result<TypedVal> {
    let abi = ctx
        .user_fns
        .get(name)
        .ok_or_else(|| anyhow!("call to undeclared user function `{name}`"))?
        .clone();

    let mangled: String = format!("__user_{name}");
    if !ctx.extern_cache.contains_key(mangled.as_str()) {
        return Err(anyhow!("call to undeclared user function `{name}`"));
    }
    let func_id = *ctx.extern_cache.get(mangled.as_str()).unwrap();
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

    // Tail calls: \`return_call\` substitui o frame atual no lugar de
    // empilhar — semantica de loop iterativo. NAO incrementar stack
    // depth, senao tail recursion de N iteracoes estoura limite (10K
    // default) mesmo nao consumindo stack real do C. Tambem NAO pop —
    // o frame atual sera substituido, sem retorno ao caller pra fazer
    // pop. (Antes Daniel emitia push aqui, fazendo loopTco(500000)
    // overflow desnecessariamente.)
    if ctx.is_tail_conv && ctx.in_tail_position {
        let ty = abi.ret.unwrap_or(ValTy::I64);
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

fn lower_coerce_is_nan(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    use cranelift_codegen::ir::condcodes::FloatCC;
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    let arg = call.args.first().ok_or_else(|| anyhow!("isNaN requires 1 arg"))?;
    if arg.spread.is_some() {
        return Err(anyhow!("isNaN: spread arg not supported"));
    }
    let tv = super::lower_expr(ctx, &arg.expr)?;
    let f = super::operators::to_f64(ctx, tv);
    let result = ctx.builder.ins().fcmp(FloatCC::Unordered, f, f);
    Ok(TypedVal::new(result, ValTy::Bool))
}

fn lower_coerce_is_finite(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    use cranelift_codegen::ir::condcodes::FloatCC;
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    let arg = call.args.first().ok_or_else(|| anyhow!("isFinite requires 1 arg"))?;
    if arg.spread.is_some() {
        return Err(anyhow!("isFinite: spread arg not supported"));
    }
    let tv = super::lower_expr(ctx, &arg.expr)?;
    let f = super::operators::to_f64(ctx, tv);
    let abs_f = ctx.builder.ins().fabs(f);
    let inf = ctx.builder.ins().f64const(f64::INFINITY);
    let result = ctx.builder.ins().fcmp(FloatCC::LessThan, abs_f, inf);
    Ok(TypedVal::new(result, ValTy::Bool))
}

fn lower_coerce_to_number(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        if matches!(tv.ty, ValTy::Handle) {
            // Delega para __RTS_FN_GL_NUMBER_FROM_STR(handle) -> f64
            let from_str = ctx.get_extern("__RTS_FN_GL_NUMBER_FROM_STR", &[cl::I64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(from_str, &[tv.val]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::F64)));
        }
        let f = super::operators::to_f64(ctx, tv);
        return Ok(Some(TypedVal::new(f, ValTy::F64)));
    }
    let v = ctx.builder.ins().f64const(0.0);
    Ok(Some(TypedVal::new(v, ValTy::F64)))
}

fn lower_coerce_to_string(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        let h = ctx.coerce_to_handle(tv)?;
        return Ok(Some(h));
    }
    let h = ctx.emit_str_handle(b"")?;
    Ok(Some(h))
}

fn lower_coerce_to_boolean(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        if matches!(tv.ty, ValTy::F64) {
            let zero = ctx.builder.ins().f64const(0.0);
            let ne_zero = ctx.builder.ins().fcmp(FloatCC::NotEqual, tv.val, zero);
            return Ok(Some(TypedVal::new(ne_zero, ValTy::Bool)));
        }
        let v = ctx.coerce_to_i64(tv).val;
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let result = ctx.builder.ins().icmp(IntCC::NotEqual, v, zero);
        return Ok(Some(TypedVal::new(result, ValTy::Bool)));
    }
    let v = ctx.builder.ins().iconst(cl::I64, 0);
    Ok(Some(TypedVal::new(v, ValTy::Bool)))
}

/// Retorna true quando a função compilada tem `this` como primeiro parâmetro.
/// Isso ocorre em métodos de classe não-estáticos: nome começa com `__class_`
/// e não contém `_static_` e não é o init synthetic (`__init`).
pub(super) fn fn_name_has_this_param(name: &str) -> bool {
    name.starts_with("__class_")
        && !name.contains("_static_")
        && !name.ends_with("__init")
}
