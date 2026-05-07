mod builtins;
mod class_dispatch;
mod coerce;
mod super_calls;

use self::builtins::{
    lower_array_builtin, lower_console_call, lower_map_set_builtin, lower_number_builtin,
    lower_string_builtin,
};
use self::super_calls::{lower_super_call, lower_super_method_call};

pub(super) use self::super_calls::{lower_super_prop_assign, lower_super_prop_read};

pub(super) use class_dispatch::{
    AccessorKind, emit_method_call, emit_named_method_call, emit_virtual_accessor_dispatch,
    fn_name_has_this_param, lower_class_method_call_with_recv, resolve_getter_owner,
    resolve_init_owner, resolve_method_owner, resolve_setter_owner,
};

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{CallExpr, Callee, Expr, MemberProp};

use self::coerce::{
    lower_coerce_is_finite, lower_coerce_is_nan, lower_coerce_to_boolean, lower_coerce_to_number,
    lower_coerce_to_string,
};

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
use crate::codegen::lower::compile::class::{
    class_getter_name, class_setter_name, class_static_method_name,
};

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
                // JSON.stringify(value, replacer, indent) — JS 3-arg form.
                // Replacer ignorado (v0); indent vai pra STRINGIFY_PRETTY.
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
                    return Ok(crate::codegen::lower::ctx::TypedVal::new(v, crate::codegen::lower::ctx::ValTy::Handle));
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
                        let recv_f = to_f64(ctx, recv_tv);
                        if let Some(tv) = lower_number_builtin(ctx, &method_name, recv_f, call)? {
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
                        if matches!(m.obj.as_ref(), Expr::Array(_) | Expr::Call(_)) {
                            if let Some(tv) = lower_array_builtin(ctx, &method_name, recv_h, call)? {
                                return Ok(tv);
                            }
                        }
                        if let Some(tv) = lower_string_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                        // Fallback array para chains (call que pode ser map/filter/etc).
                        if let Some(tv) = lower_array_builtin(ctx, &method_name, recv_h, call)? {
                            return Ok(tv);
                        }
                    }
                    // (#480 chain) Method chain em Call result: \`c.add(5).add(3)\`.
                    // Receiver i64 (return this) que e' handle de Map. Faz map_get +
                    // INVOKE_AUTO em qualquer Call obj.
                    if matches!(m.obj.as_ref(), Expr::Call(_)) {
                        let recv_h = ctx.coerce_to_i64(recv_tv).val;
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
            // pairwise.
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
                let target = match method {
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
                // (#208) Object.isExtensible/preventExtensions — v0 stubs
                // (sempre true; preventExtensions e' no-op).
                if method == "isExtensible" && call.args.len() == 1 {
                    let _ = lower_expr(ctx, &call.args[0].expr)?;
                    let t = ctx.builder.ins().iconst(cl::I64, 1);
                    return Ok(TypedVal::new(t, ValTy::Bool));
                }
                if method == "preventExtensions" && call.args.len() == 1 {
                    let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let h = ctx.coerce_to_i64(arg_tv).val;
                    return Ok(TypedVal::new(h, ValTy::Handle));
                }
                // (#208) Object.defineProperty(obj, key, descriptor) — v0
                // suporta apenas { value: x }.
                if method == "defineProperty" && call.args.len() == 3 {
                    let obj_tv = lower_expr(ctx, &call.args[0].expr)?;
                    let obj = ctx.coerce_to_i64(obj_tv).val;
                    let key_tv = lower_expr(ctx, &call.args[1].expr)?;
                    let key_h = ctx.coerce_to_i64(key_tv).val;
                    let desc_tv = lower_expr(ctx, &call.args[2].expr)?;
                    let desc = ctx.coerce_to_i64(desc_tv).val;
                    let kp_fn = ctx.get_extern(
                        "__RTS_FN_NS_GC_STRING_PTR",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let kl_fn = ctx.get_extern(
                        "__RTS_FN_NS_GC_STRING_LEN",
                        &[cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst_p = ctx.builder.ins().call(kp_fn, &[key_h]);
                    let kp = ctx.builder.inst_results(inst_p)[0];
                    let inst_l = ctx.builder.ins().call(kl_fn, &[key_h]);
                    let kl = ctx.builder.inst_results(inst_l)[0];
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY",
                        &[cl::I64, cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(f, &[obj, kp, kl, desc]);
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
                    // (#218) Reflect.setPrototypeOf — escreve __proto__ no map.
                    // Retorna sempre true (sem tracking de extensibilidade v0).
                    "setPrototypeOf" if call.args.len() == 2 => {
                        let target_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let target = ctx.coerce_to_i64(target_tv).val;
                        let proto_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let proto = ctx.coerce_to_i64(proto_tv).val;
                        let key_h = ctx.emit_str_handle(b"__proto__")?.val;
                        let kp_fn = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_PTR",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let kl_fn = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_LEN",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst_p = ctx.builder.ins().call(kp_fn, &[key_h]);
                        let kp = ctx.builder.inst_results(inst_p)[0];
                        let inst_l = ctx.builder.ins().call(kl_fn, &[key_h]);
                        let kl = ctx.builder.inst_results(inst_l)[0];
                        let set_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(set_fn, &[target, kp, kl, proto]);
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    // (#218) Reflect.isExtensible — v0 sempre true.
                    "isExtensible" if call.args.len() == 1 => {
                        // Avalia arg pra side-effects mas ignora.
                        let _ = lower_expr(ctx, &call.args[0].expr)?;
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
                    }
                    // (#218) Reflect.preventExtensions — v0 no-op, retorna true.
                    "preventExtensions" if call.args.len() == 1 => {
                        let _ = lower_expr(ctx, &call.args[0].expr)?;
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
                        let v = ctx.coerce_to_i64(tv).val;
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
                    // Array.from(string): split em chars via STRING_SPLIT(s, "").
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
                        let v = ctx.builder.inst_results(inst)[0];
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
                                let is_primitive = matches!(
                                    var_ty,
                                    crate::codegen::lower::ctx::ValTy::Bool
                                        | crate::codegen::lower::ctx::ValTy::F64
                                        | crate::codegen::lower::ctx::ValTy::I32
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
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
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
        "encodeURIComponent" | "decodeURIComponent"
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
            let sym = if name == "encodeURIComponent" {
                "__RTS_FN_GL_ENCODE_URI_COMPONENT"
            } else {
                "__RTS_FN_GL_DECODE_URI_COMPONENT"
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
        "structuredClone" => Ok(Some(lower_ns_call(ctx, "text_encoding.structuredClone", call)?)),
        "queueMicrotask" => Ok(Some(lower_ns_call(ctx, "text_encoding.queueMicrotask", call)?)),

        _ => Ok(None),
    }
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
        let fn_ref = ctx.get_extern_abi(ctor.symbol, &sig.params, sig.ret)?;
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
    // lower_var_member_call.
    if class_name == "Map" || class_name == "Set" {
        if !ctx.classes.contains_key(&class_name) {
            let new_fn =
                ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(new_fn, &[]);
            let h = ctx.builder.inst_results(inst)[0];
            // Initial entries: \`new Set([1,2,3])\` ou \`new Map([[\"a\",1],[\"b\",2]])\`.
            // v0: aceita Expr::Array literal; outros casos sao no-op.
            let init_arg = new_expr.args.as_ref().and_then(|args| args.first());
            if let Some(arg) = init_arg {
                if let Expr::Array(arr) = arg.expr.as_ref() {
                    if class_name == "Set" {
                        // add(elem) para cada item.
                        let from_i64 = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_FROM_I64",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let str_ptr = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_PTR",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let str_len = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_LEN",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let map_set = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        let one = ctx.builder.ins().iconst(cl::I64, 1);
                        for elem in arr.elems.iter().flatten() {
                            if elem.spread.is_some() { continue; }
                            let tv = lower_expr(ctx, &elem.expr)?;
                            let i = ctx.coerce_to_i64(tv).val;
                            // Converte i64 para string-key.
                            let inst_s = ctx.builder.ins().call(from_i64, &[i]);
                            let key_h = ctx.builder.inst_results(inst_s)[0];
                            let inst_p = ctx.builder.ins().call(str_ptr, &[key_h]);
                            let kp = ctx.builder.inst_results(inst_p)[0];
                            let inst_l = ctx.builder.ins().call(str_len, &[key_h]);
                            let kl = ctx.builder.inst_results(inst_l)[0];
                            ctx.builder.ins().call(map_set, &[h, kp, kl, one]);
                        }
                    } else {
                        // Map: cada elem e' [key, value].
                        let str_ptr = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_PTR",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let str_len = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_LEN",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let map_set = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        for entry in arr.elems.iter().flatten() {
                            if entry.spread.is_some() { continue; }
                            if let Expr::Array(pair) = entry.expr.as_ref() {
                                if pair.elems.len() != 2 { continue; }
                                let k_el = pair.elems[0].as_ref();
                                let v_el = pair.elems[1].as_ref();
                                let (Some(k), Some(v)) = (k_el, v_el) else { continue; };
                                let k_tv = lower_expr(ctx, &k.expr)?;
                                let key_h = ctx.coerce_to_handle(k_tv)?.val;
                                let inst_p = ctx.builder.ins().call(str_ptr, &[key_h]);
                                let kp = ctx.builder.inst_results(inst_p)[0];
                                let inst_l = ctx.builder.ins().call(str_len, &[key_h]);
                                let kl = ctx.builder.inst_results(inst_l)[0];
                                let v_tv = lower_expr(ctx, &v.expr)?;
                                let v_i = ctx.coerce_to_i64(v_tv).val;
                                ctx.builder.ins().call(map_set, &[h, kp, kl, v_i]);
                            }
                        }
                    }
                }
            }
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

    let handle = if let Some(this_val) = bound_this {
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

    // (#208) Boolean.prototype methods em receiver Bool.
    if matches!(obj_tv.ty, ValTy::Bool) {
        match prop {
            "toString" if call.args.is_empty() => {
                // Emite condicional: bool ? "true" : "false".
                use cranelift_codegen::ir::condcodes::IntCC;
                let true_h = ctx.emit_str_handle(b"true")?.val;
                let false_h = ctx.emit_str_handle(b"false")?.val;
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, obj_h, zero);
                let result = ctx.builder.ins().select(is_true, true_h, false_h);
                return Ok(TypedVal::new(result, ValTy::Handle));
            }
            "valueOf" if call.args.is_empty() => {
                return Ok(TypedVal::new(obj_h, ValTy::Bool));
            }
            _ => {}
        }
    }

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

    // (#proto-instance) Var marcada como instance via constructor function:
    // `new Animal(...)` cujo Animal eh user fn. Skipar string/map-set builtins
    // pra que o lookup de prototype (MAP_GET_CHAIN abaixo) prevaleça.
    let is_proto_instance = ctx
        .local_class_ty
        .get(obj_name)
        .map(|s| s == "__proto_instance")
        .unwrap_or(false);

    // Builtins de string em receiver Handle: s.indexOf(...), s.startsWith(...), etc.
    // Tem que vir antes do map_get porque uma string handle nao e um map —
    // map_get retornaria lixo, e o call_indirect subsequente saltaria pra
    // endereco invalido. (#235: indexOf travava/SIGSEGV em string com \0)
    if matches!(obj_tv.ty, ValTy::Handle) && !is_proto_instance {
        if let Some(tv) = lower_string_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
        // #222 — Map/Set methods em receiver Handle. Heuristica conservadora:
        // so age quando o nome do metodo eh tipico de Map/Set e nao colide
        // com classes do usuario. Ergonomia v0 — usuario que tem classe
        // chamada `set()` em var Handle precisa anotar tipo da var pra
        // resolver dispatch antes do builtin.
        // (#480) Skip se obj_name e' object literal local com o prop como
        // field key registrado — significa user definiu \`obj.add()\` como
        // method, nao Set.add.
        let user_method_field = ctx
            .local_obj_field_types
            .get(obj_name)
            .map(|fs| fs.contains_key(prop))
            .unwrap_or(false);
        if !user_method_field {
            if let Some(tv) = lower_map_set_builtin(ctx, prop, obj_h, call)? {
                return Ok(tv);
            }
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
            ValTy::I32 | ValTy::I64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
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
    // FUNCTION_CALL retorna i64. Para return_kind=0 (default) é plain i64;
    // para return_kind=1 (F64) seria bits f64 — mas esse caso é raro e
    // resolvido pelo coerce_value_to_ty do caller quando necessário.
    Ok(TypedVal::new(v_i64, ValTy::I64))
}

fn emit_constant_load(ctx: &mut FnCtx, member: &crate::abi::NamespaceMember) -> Result<TypedVal> {
    use crate::abi::signature::scalar_to_cl;
    let lowered = lower_member(member);
    let ret_abi = lowered
        .ret
        .ok_or_else(|| anyhow!("constant `{}` has no return type", member.name))?;
    let ret_cl = scalar_to_cl(ret_abi);

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
    use crate::abi::signature::scalar_to_cl;
    let qualified = member.symbol;
    let lowered = lower_member(member);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        for &p in &lowered.params {
            sig.params.push(AbiParam::new(scalar_to_cl(p)));
        }
        if let Some(r) = lowered.ret {
            sig.returns.push(AbiParam::new(scalar_to_cl(r)));
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
    use crate::abi::signature::{lower_params, lower_return, scalar_to_cl};

    let member = crate::nodespace::node_lookup(qualified)
        .ok_or_else(|| anyhow!("unknown node namespace member `{qualified}`"))?;

    let lowered_params = lower_params(member.args);
    let lowered_ret = lower_return(member.returns);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        for &p in &lowered_params {
            sig.params.push(AbiParam::new(scalar_to_cl(p)));
        }
        if let Some(r) = lowered_ret {
            sig.returns.push(AbiParam::new(scalar_to_cl(r)));
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
    let fn_ref = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;

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
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => ctx.coerce_to_i64(tv).val,
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
