mod builtins;
mod class_dispatch;
mod coerce;
mod indirect;
mod new_expr;
mod ns_call;
mod super_calls;

use self::indirect::{
    emit_function_handle_indirect_call, lower_indirect_call, lower_var_member_call,
};
use self::new_expr::{lower_function_handle_method, lower_function_method_call, lower_new_function};

pub(super) use self::new_expr::lower_new;

use self::builtins::{
    lower_array_builtin, lower_console_call, lower_map_set_builtin, lower_number_builtin,
    lower_string_builtin,
};
use self::ns_call::{
    emit_constant_load, lower_global_instance_call, lower_intrinsic, lower_node_ns_call,
    lower_ns_call, lower_ns_call_body, lower_ns_call_member,
};
use self::super_calls::{lower_super_call, lower_super_method_call};

pub(super) use self::ns_call::emit_namespace_constant;
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
                // JSON.stringify(value) — 1-arg typed dispatch para preservar
                // semantica JS spec (Bool -> "true"/"false", Null -> "null").
                if qualified == "JSON.stringify" && call.args.len() == 1 {
                    if call.args[0].spread.is_some() {
                        return Err(anyhow!("spread not supported in JSON.stringify"));
                    }
                    use crate::codegen::lower::ctx::{TypedVal, ValTy};
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
                        // Returna I64 (slot raw). Slots podem carregar handle
                        // de string/map ou int direto; sem analise de tipo no
                        // call site, deixamos o caller decidir via cast/use.
                        // Combina com testes pre-existentes que usam
                        // `Reflect.get(o,k).toString()` em ints.
                        return lower_ns_call(ctx, "collections.map_get", call);
                    }
                    "has" if call.args.len() == 2 => {
                        return lower_ns_call(ctx, "collections.map_has", call);
                    }
                    "ownKeys" if call.args.len() == 1 => {
                        return lower_ns_call(ctx, "collections.map_keys", call);
                    }
                    "deleteProperty" if call.args.len() == 2 => {
                        // JS spec: Reflect.deleteProperty retorna true tanto
                        // para chave existente quanto inexistente (so' falha
                        // em props nao-configurable, que RTS v0 nao distingue).
                        // map_delete devolve 1/0 (existente/inexistente);
                        // forcamos true e descartamos o resultado.
                        let _ = lower_ns_call(ctx, "collections.map_delete", call)?;
                        let t = ctx.builder.ins().iconst(cl::I64, 1);
                        return Ok(TypedVal::new(t, ValTy::Bool));
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
                    // (#218) Reflect.apply(fn, thisArg, argsArray) — reusa
                    // __RTS_FN_GL_FUNCTION_APPLY (mesma assinatura).
                    "apply" if call.args.len() == 3 => {
                        let fn_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let fn_h = ctx.coerce_to_i64(fn_tv).val;
                        let this_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let this_v = ctx.coerce_to_i64(this_tv).val;
                        let args_tv = lower_expr(ctx, &call.args[2].expr)?;
                        let args_h = ctx.coerce_to_i64(args_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_FUNCTION_APPLY",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst = ctx.builder.ins().call(f, &[fn_h, this_v, args_h]);
                        let v = ctx.builder.inst_results(inst)[0];
                        return Ok(TypedVal::new(v, ValTy::I64));
                    }
                    // (#218) Reflect.construct(Target, args) — semantica de
                    // `new Target(...args)`. Reusa o trampolim Function:
                    // aloca Map (instancia), chama o constructor com THIS_PUSH
                    // implicito via FUNCTION_APPLY com `this = inst`. Deixa
                    // newTarget como follow-up (afeta prototype chain).
                    "construct" if matches!(call.args.len(), 2 | 3) => {
                        let target_tv = lower_expr(ctx, &call.args[0].expr)?;
                        let target_h = ctx.coerce_to_i64(target_tv).val;
                        let args_tv = lower_expr(ctx, &call.args[1].expr)?;
                        let args_h = ctx.coerce_to_i64(args_tv).val;
                        // Cria Map vazio para servir como instancia.
                        let inst_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
                            &[],
                            Some(cl::I64),
                        )?;
                        let inst_call = ctx.builder.ins().call(inst_fn, &[]);
                        let inst_h = ctx.builder.inst_results(inst_call)[0];
                        // Chama target.apply(inst, args) — como construtor.
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_FUNCTION_APPLY",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        ctx.builder.ins().call(f, &[target_h, inst_h, args_h]);
                        // Retorna a instancia construida.
                        return Ok(TypedVal::new(inst_h, ValTy::Handle));
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
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR",
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
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REFLECT_DEFINE_PROPERTY",
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

