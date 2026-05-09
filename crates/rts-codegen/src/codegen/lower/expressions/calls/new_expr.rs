//! Lowering de `new ClassName(args)` e `new Function(...)`:
//! - lower_new: aloca instancia GC + chama __class_C__init + constructor.
//! - lower_new_function: `new Function("body")` via runtime.eval.
//! - lower_function_handle_method / lower_function_method_call:
//!   chamadas em handles `Function` reificados (.call/.apply/.bind/etc).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{CallExpr, Expr, MemberProp};

use crate::abi::lookup;
use crate::abi::types::AbiType;
use crate::codegen::lower::compile::class::class_init_name;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::operators::to_f64;
use super::class_dispatch::{fn_name_has_this_param, resolve_init_owner};
use super::emit_user_fn_addr;
use super::lower_call;

pub(crate) fn lower_new(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
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
                } else if *expected == AbiType::F64 {
                    arg_vals.push(ctx.coerce_to_f64(tv).val);
                } else {
                    arg_vals.push(ctx.coerce_to_i64(tv).val);
                }
            } else if *expected == AbiType::StrPtr {
                // Arg omitido: passa (ptr=0, len=0) — runtime trata como string vazia.
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                arg_vals.push(zero);
                arg_vals.push(zero);
            } else if *expected == AbiType::F64 {
                // Arg omitido em ctor F64: passa NaN como sentinela ("ausente").
                let nan = ctx.builder.ins().f64const(f64::NAN);
                arg_vals.push(nan);
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

    // (#218) `new Proxy(target, handler)` — aloca Entry::Proxy.
    // MAP_GET_CHAIN/MAP_SET/MAP_HAS/MAP_DELETE detectam o handle e
    // despacham pra trap correta no handler (ou forward pro target).
    if class_name == "Proxy" && !ctx.classes.contains_key(&class_name) {
        let args = new_expr.args.as_deref().unwrap_or(&[]);
        if args.len() != 2 {
            anyhow::bail!(
                "Proxy constructor: esperado 2 argumentos (target, handler), recebido {}",
                args.len()
            );
        }
        let target_tv = super::super::lower_expr(ctx, &args[0].expr)?;
        let target_h = ctx.coerce_to_i64(target_tv).val;
        let handler_tv = super::super::lower_expr(ctx, &args[1].expr)?;
        let handler_h = ctx.coerce_to_i64(handler_tv).val;
        let f = ctx.get_extern(
            "__RTS_FN_GL_PROXY_NEW",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[target_h, handler_h]);
        let h = ctx.builder.inst_results(inst)[0];
        return Ok(crate::codegen::lower::ctx::TypedVal::new(
            h,
            crate::codegen::lower::ctx::ValTy::Handle,
        ));
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
pub(super) fn lower_new_function(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
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
pub(super) fn lower_function_handle_method(
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
            // Dispatch runtime — TO_STRING_HANDLE inspeciona Entry
            // (Symbol/Function/String/Vec/Map) e formata corretamente.
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_RT_TO_STRING_HANDLE",
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
pub(super) fn lower_function_method_call(
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

    // Deriva param_kinds + return_kind da user fn pra que
    // FUNCTION_CALL/APPLY/BIND reinterpretem bits f64 corretamente em
    // vez de tratar tudo como i64.
    let (param_kinds_bytes, return_kind_byte): (Vec<u8>, u8) = {
        let info = ctx.user_fns.get(fn_name);
        let pks: Vec<u8> = info
            .map(|f| {
                f.params
                    .iter()
                    .map(|p| super::super::members::val_ty_to_kind(*p))
                    .collect()
            })
            .unwrap_or_default();
        let rk: u8 = info
            .and_then(|f| f.ret)
            .map(super::super::members::val_ty_to_kind)
            .unwrap_or(4); // void = 4
        (pks, rk)
    };
    let (kinds_ptr, kinds_len) = if param_kinds_bytes.is_empty() {
        (
            ctx.builder.ins().iconst(cl::I64, 0),
            ctx.builder.ins().iconst(cl::I64, 0),
        )
    } else {
        let tv = ctx.emit_str_handle(&param_kinds_bytes)?;
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
    let return_kind_v = ctx
        .builder
        .ins()
        .iconst(cl::I32, return_kind_byte as i64);

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
            fn_ptr,
            arity_v,
            n_ptr,
            n_len,
            is_arrow_v,
            has_this_v,
            bound_this_v,
            has_bound_this_v,
            kinds_ptr,
            kinds_len,
            return_kind_v,
        ],
    );
    let fn_handle = ctx.builder.inst_results(inst_r)[0];

    // 3. Despacha por metodo.
    match method {
        "toString" => {
            // Dispatch runtime — TO_STRING_HANDLE inspeciona Entry
            // (Symbol/Function/String/Vec/Map) e formata corretamente.
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_RT_TO_STRING_HANDLE",
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
