//! Lowering de `new ClassName(args)` e `new Function(...)`:
//! - lower_new: aloca instancia GC + chama __class_C__init + constructor.
//! - lower_new_function: `new Function("body")` via runtime.eval.
//! - lower_function_handle_method / lower_function_method_call:
//!   chamadas em handles `Function` reificados (.call/.apply/.bind/etc).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{CallExpr, Expr};

use crate::abi::types::AbiType;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::operators::to_f64;
use super::class_dispatch::{fn_name_has_this_param, resolve_init_owner};
use super::emit_user_fn_addr;
use crate::codegen::lower::compile::class::class_init_name;
use super::lower_call;

/// (cross-runtime #378) True if `class_name` is a user class whose `extends`
/// chain reaches the builtin `Array`.
pub(crate) fn class_extends_array(ctx: &FnCtx, class_name: &str) -> bool {
    let mut cur = class_name.to_string();
    let mut depth = 0;
    while depth < 32 {
        match ctx.classes.get(&cur).and_then(|m| m.super_class.clone()) {
            Some(s) if s == "Array" => return true,
            Some(s) => cur = s,
            None => return false,
        }
        depth += 1;
    }
    false
}

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
    let mut class_name = match callee_expr {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        // (Intl) `new Intl.NumberFormat(...)` — callee eh Member com obj=Ident.
        // Tratamos como classe global de nome composto "Intl.NumberFormat".
        Expr::Member(m) => {
            if let (Expr::Ident(ns), swc_ecma_ast::MemberProp::Ident(p)) =
                (m.obj.as_ref(), &m.prop)
            {
                if ns.sym.as_str() == "Intl" {
                    format!("Intl.{}", p.sym.as_str())
                } else {
                    return Err(anyhow!(
                        "`new` so suporta callee identifier (sem `new (expr)()`)"
                    ));
                }
            } else {
                return Err(anyhow!(
                    "`new` so suporta callee identifier (sem `new (expr)()`)"
                ));
            }
        }
        _ => {
            return Err(anyhow!(
                "`new` so suporta callee identifier (sem `new (expr)()`)"
            ));
        }
    };

    // (#69) SharedArrayBuffer eh tratado como ArrayBuffer no RTS (sem memoria
    // compartilhada entre threads via SAB — o backing eh o mesmo Buffer). Isso
    // destrava `new SharedArrayBuffer(n)` + Int32Array view + Atomics.* sobre
    // ela, que ja' funcionam para ArrayBuffer.
    if class_name == "SharedArrayBuffer" && !ctx.classes.contains_key(&class_name) {
        class_name = "ArrayBuffer".to_string();
    }

    // Function global (#359): `new Function(...params, body)` — variadic.
    // Empacota todos args excerto o ultimo em string CSV de params, ultimo
    // arg eh body. Aceita aridade 0+ (0 args = throw, 1 arg = body sem
    // params, 2+ args = (...params, body)).
    if class_name == "Function" {
        return lower_new_function(ctx, new_expr);
    }

    // `new Object()` / `Object(x)`: JS spec eh boxing.
    // - 0 args: novo Map vazio
    // - 1 arg null/undefined: novo Map vazio
    // - 1 arg object (Handle): passthrough
    // - 1 arg primitivo: Map vazio (perde valor mas \`typeof === \"object\"\` ok)
    // Cobre fixture #823. Boxing real de primitivos eh follow-up.
    if class_name == "Object" {
        let args = new_expr.args.as_ref();
        let n_args = args.map(|a| a.len()).unwrap_or(0);
        if n_args == 0 {
            let map_new = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
                &[],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(map_new, &[]);
            let h = ctx.builder.inst_results(inst)[0];
            return Ok(TypedVal::new(h, ValTy::Handle));
        }
        let arg = &args.unwrap()[0];
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in `new Object`"));
        }
        // Strings literais sao primitivos em JS — `new Object("s")` faz
        // boxing em objeto (typeof === "object"). Detecta antes de
        // lower_expr para nao confundir com passthrough de string Handle.
        let is_string_lit = matches!(
            arg.expr.as_ref(),
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
        );
        let tv = lower_expr(ctx, &arg.expr)?;
        if matches!(tv.ty, ValTy::Handle) && !is_string_lit {
            // null/undefined ou objeto: passthrough.
            return Ok(TypedVal::new(tv.val, ValTy::Handle));
        }
        // Primitivo (Bool, I64, F64): Map vazio para satisfazer
        // `typeof === "object"`. Valor primitivo descartado (TODO: boxing).
        let map_new = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
            &[],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(map_new, &[]);
        let h = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(h, ValTy::Handle));
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
        let result_val = ctx.builder.inst_results(inst)[0];
        // (cross-runtime panic fix) Usar from_abi pra preservar tipo Cranelift
        // do retorno do constructor. Antes assumia Handle (i64), mas Number/
        // Boolean retornam F64/I64 inteiros — declarar como Handle causa
        // Cranelift type mismatch panic.
        let result_ty = ValTy::from_abi(ctor.returns);
        return Ok(TypedVal::new(result_val, result_ty));
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

    // (#810) TypedArrays — `new Uint8Array([..])`, `new Int32Array(n)`, etc.
    // Backing store eh um Vec<i64> (collections.vec_*), reusando indexacao,
    // `.length` e `Array.from`. Os valores armazenados sao normalizados ao
    // alcance do tipo (mask/wrap) no momento do push, para casar com a
    // semantica JS (ex: Uint8 trunca a 0..255, Int8 estende sinal).
    if !ctx.classes.contains_key(&class_name) {
        if let Some(elem) = typed_array_kind(&class_name) {
            return lower_new_typed_array(ctx, new_expr, elem);
        }
    }

    // (#780) `new Array()`
    if class_name == "Array" && !ctx.classes.contains_key(&class_name) {
        let n_args = new_expr.args.as_ref().map(|a| a.len()).unwrap_or(0);
        if n_args == 1 {
            let arg = &new_expr.args.as_ref().unwrap()[0];
            let tv = super::super::lower_expr(ctx, &arg.expr)?;
            // Se argumento for numérico, aloca array vazio com aquele length
            if matches!(tv.ty, ValTy::I64 | ValTy::I32 | ValTy::F64 | ValTy::U64) {
                let len_i64 = ctx.coerce_to_i64(tv).val;
                let f = ctx.get_extern("__RTS_FN_GL_ARRAY_NEW_WITH_LENGTH", &[cl::I64], Some(cl::I64))?;
                let inst = ctx.builder.ins().call(f, &[len_i64]);
                let h = ctx.builder.inst_results(inst)[0];
                return Ok(crate::codegen::lower::ctx::TypedVal::new(h, ValTy::Handle));
            }
            // Se for handle (string/bool convertido), cai no fallback Array.of abaixo
        }
        
        // Fallback pra Array.of logic: aloca Vec e da push
        let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(new_fn, &[]);
        let vec_h = ctx.builder.inst_results(inst)[0];
        let push_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
        
        if let Some(args) = &new_expr.args {
            for arg in args {
                if arg.spread.is_some() {
                    return Err(anyhow!("spread not supported in new Array"));
                }
                let tv = super::super::lower_expr(ctx, &arg.expr)?;
                let v = if matches!(tv.ty, ValTy::Bool) {
                    ctx.coerce_to_handle(tv)?.val
                } else {
                    ctx.coerce_to_i64(tv).val
                };
                ctx.builder.ins().call(push_fn, &[vec_h, v]);
            }
        }
        return Ok(crate::codegen::lower::ctx::TypedVal::new(vec_h, ValTy::Handle));
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
            // Marca o kind do handle para que Object.prototype.toString
            // diferencie Map de Set (mesmo backing store).
            let mark_sym = if class_name == "Set" {
                "__RTS_FN_NS_COLLECTIONS_MARK_AS_SET"
            } else {
                "__RTS_FN_NS_COLLECTIONS_MARK_AS_MAP"
            };
            let mark_fn = ctx.get_extern(mark_sym, &[cl::I64], None)?;
            ctx.builder.ins().call(mark_fn, &[h]);
            // Initial entries: \`new Set([1,2,3])\` ou \`new Map([[\"a\",1],[\"b\",2]])\`.
            // Aceita Expr::Array literal (caminho estatico abaixo) E tambem
            // arg que NAO eh array literal (var/expr que resolve pra Vec em
            // runtime, ex: `new Map(entries)`) via MAP_FROM_ENTRIES.
            let init_arg = new_expr.args.as_ref().and_then(|args| args.first());
            // (374) `new Map(<expr-nao-literal>)` — popula via runtime.
            // Restrito a Ident/Member (var que ja' materializou o Vec de
            // pares). CallExpr inline (`new Map(arr.map(...))`) e' excluido:
            // o Vec temporario do .map+parallel ainda nao tem materializacao
            // estavel e crashava no MAP_FROM_ENTRIES — usar var intermediaria
            // (`const r = arr.map(...); new Map(r)`) funciona. Follow-up.
            if class_name == "Map" {
                if let Some(arg) = init_arg {
                    let arg_is_safe = matches!(
                        arg.expr.as_ref(),
                        Expr::Ident(_) | Expr::Member(_)
                            | Expr::Paren(_) | Expr::TsAs(_) | Expr::TsNonNull(_)
                    );
                    if arg.spread.is_none()
                        && !matches!(arg.expr.as_ref(), Expr::Array(_))
                        && arg_is_safe
                    {
                        let src_tv = lower_expr(ctx, &arg.expr)?;
                        let src_h = ctx.coerce_to_i64(src_tv).val;
                        let from_entries = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst_fe = ctx.builder.ins().call(from_entries, &[src_h]);
                        let m2 = ctx.builder.inst_results(inst_fe)[0];
                        let mark_fn2 = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MARK_AS_MAP",
                            &[cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(mark_fn2, &[m2]);
                        return Ok(TypedVal::new(m2, ValTy::Handle));
                    }
                }
            }
            // (cross-runtime) `new Set(<expr-nao-literal>)` — popula via runtime
            // SET_FROM_VEC. Antes so' literal `new Set([...])` populava; com var/
            // param (`new Set(arr)`) o Set ficava vazio. Restrito a Ident/Member/
            // etc (var que ja' materializou o Vec) — inline Call segue follow-up.
            if class_name == "Set" {
                if let Some(arg) = init_arg {
                    let arg_is_safe = matches!(
                        arg.expr.as_ref(),
                        Expr::Ident(_) | Expr::Member(_)
                            | Expr::Paren(_) | Expr::TsAs(_) | Expr::TsNonNull(_)
                    );
                    if arg.spread.is_none()
                        && !matches!(arg.expr.as_ref(), Expr::Array(_))
                        && arg_is_safe
                    {
                        let src_tv = lower_expr(ctx, &arg.expr)?;
                        let src_h = ctx.coerce_to_i64(src_tv).val;
                        let from_vec = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_SET_FROM_VEC",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst_fv = ctx.builder.ins().call(from_vec, &[src_h]);
                        let s2 = ctx.builder.inst_results(inst_fv)[0];
                        return Ok(TypedVal::new(s2, ValTy::Handle));
                    }
                }
            }
            if let Some(arg) = init_arg {
                if let Expr::Array(arr) = arg.expr.as_ref() {
                    if class_name == "Set" {
                        // (#394) add(elem) para cada item.
                        // - F64 (number): STRING_FROM_F64 preserva NaN/Infinity/-0
                        //   como keys distintas (#669/95); value = `one` (caminho
                        //   float tem repr propria, fora deste escopo).
                        // - resto (string/objeto/Set/int): SET_ADD deriva a KEY
                        //   estavel (conteudo p/ string, identidade p/ objeto,
                        //   decimal p/ int) e grava o VALOR original como value,
                        //   de modo que values()/[...set]/for-of recuperem a
                        //   identidade do elemento. Antes objetos viravam key
                        //   vazia (STRING_PTR de nao-string) e colidiam todos.
                        let from_f64 = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_FROM_F64",
                            &[cl::F64],
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
                        let set_add = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_SET_ADD",
                            &[cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let one = ctx.builder.ins().iconst(cl::I64, 1);
                        for elem in arr.elems.iter().flatten() {
                            if elem.spread.is_some() { continue; }
                            let tv = lower_expr(ctx, &elem.expr)?;
                            if matches!(tv.ty, ValTy::F64) {
                                let inst_s = ctx.builder.ins().call(from_f64, &[tv.val]);
                                let key_h = ctx.builder.inst_results(inst_s)[0];
                                let inst_p = ctx.builder.ins().call(str_ptr, &[key_h]);
                                let kp = ctx.builder.inst_results(inst_p)[0];
                                let inst_l = ctx.builder.ins().call(str_len, &[key_h]);
                                let kl = ctx.builder.inst_results(inst_l)[0];
                                ctx.builder.ins().call(map_set, &[h, kp, kl, one]);
                            } else {
                                let elem_raw = ctx.coerce_to_i64(tv).val;
                                ctx.builder.ins().call(set_add, &[h, elem_raw]);
                            }
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

    // (cross-runtime #344) `new <local>()` where the local holds a function
    // VALUE (a closure bound to a const — e.g. tsc __extends's captured `Ctor`,
    // which captures an outer param so it became a closure, not a named user
    // fn). Allocate the instance, install its prototype, invoke the function
    // handle with `this` via the thread-local slot, return the instance.
    if !ctx.classes.contains_key(&class_name)
        && !ctx.user_fns.contains_key(&class_name)
    {
        if let Some(fn_tv) = ctx.read_local(&class_name) {
            let fn_h = ctx.coerce_to_i64(fn_tv).val;
            let map_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
            let mi = ctx.builder.ins().call(map_new, &[]);
            let inst_h = ctx.builder.inst_results(mi)[0];
            // __proto__ = fn.prototype (FUNCTION_PROTOTYPE_GET lazily allocs).
            let proto_get =
                ctx.get_extern("__RTS_FN_GL_FUNCTION_PROTOTYPE_GET", &[cl::I64], Some(cl::I64))?;
            let pi = ctx.builder.ins().call(proto_get, &[fn_h]);
            let proto_h = ctx.builder.inst_results(pi)[0];
            let map_set = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                None,
            )?;
            let (pk, pl) = ctx.emit_str_literal(b"__proto__")?;
            ctx.builder.ins().call(map_set, &[inst_h, pk, pl, proto_h]);
            // Push `this`, build args, invoke the handle (INVOKE_AUTO detects
            // Function handle vs raw addr; bound captures travel via the handle).
            let push_fn = ctx.get_extern("__RTS_FN_RT_THIS_PUSH", &[cl::I64], None)?;
            ctx.builder.ins().call(push_fn, &[inst_h]);
            let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
            let ai = ctx.builder.ins().call(vec_new, &[]);
            let args_h = ctx.builder.inst_results(ai)[0];
            let vec_push =
                ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
            for a in new_expr.args.as_ref().map(|v| v.as_slice()).unwrap_or(&[]) {
                if a.spread.is_some() {
                    return Err(anyhow!("spread em `new <fn-value>` nao suportado"));
                }
                let tv = lower_expr(ctx, &a.expr)?;
                let v = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(vec_push, &[args_h, v]);
            }
            let invoke = ctx.get_extern(
                "__RTS_FN_RT_INVOKE_AUTO",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            ctx.builder.ins().call(invoke, &[fn_h, inst_h, args_h]);
            let pop_fn = ctx.get_extern("__RTS_FN_RT_THIS_POP", &[], None)?;
            ctx.builder.ins().call(pop_fn, &[]);
            return Ok(TypedVal::new(inst_h, ValTy::Handle));
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

    // (cross-runtime #378) Array subclass (`class PowerArray extends Array`):
    // the instance is backed by a Vec carrying the elements — `new
    // PowerArray(1,2,3,4)` is `[1,2,3,4]`. Class identity for `pa.method()` /
    // `pa instanceof PowerArray` comes from codegen's static type
    // (local_class_ty), and a derived array (`pa.map(...)`) is a plain
    // unregistered Vec, so Symbol.species naturally yields a plain Array.
    // (Only the no-explicit-constructor case; args become the elements.)
    if class_extends_array(ctx, &class_name) {
        let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(vec_new, &[]);
        let vec_h = ctx.builder.inst_results(inst)[0];
        let vec_push =
            ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
        let user_args = new_expr.args.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
        for a in user_args {
            if a.spread.is_some() {
                return Err(anyhow!("spread em `new` de subclasse de Array nao suportado"));
            }
            let tv = lower_expr(ctx, &a.expr)?;
            let v = match tv.ty {
                ValTy::Bool => ctx.coerce_to_handle(tv)?.val,
                ValTy::F64 => ctx.builder.ins().bitcast(
                    cl::I64,
                    cranelift_codegen::ir::MemFlags::new(),
                    tv.val,
                ),
                _ => ctx.coerce_to_i64(tv).val,
            };
            ctx.builder.ins().call(vec_push, &[vec_h, v]);
        }
        return Ok(TypedVal::new(vec_h, ValTy::Handle));
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

        // (cross-runtime #336) Instala `__proto__` chain: aloca o Map
        // prototype de class_name via FUNCTION_PROTOTYPE_GET e armazena
        // em instance.__proto__. Sem isso, `Object.getPrototypeOf(c)`
        // caia no default sentinel "[Object.prototype]" e perdia o
        // `.constructor` slot da classe — quebra iteracao de prototype
        // chain (`while (proto) { chain.push(proto.constructor.name); }`).
        //
        // Tambem encadeia recursivamente: `proto_C.__proto__ = proto_B`,
        // `proto_B.__proto__ = proto_A`, etc. — permitindo iteracao
        // multi-nivel da prototype chain.
        let proto_h_opt = emit_proto_for_class(ctx, &class_name, map_set)?;
        if let Some(proto_h) = proto_h_opt {
            let (proto_kp, proto_kl) = ctx.emit_str_literal(b"__proto__")?;
            ctx.builder.ins().call(map_set, &[handle, proto_kp, proto_kl, proto_h]);
        }

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
        // (cross-runtime #1057) Se user_args < expected, preenche com sentinel
        // (defaults). Cobre `class Dog extends Animal {}` onde __init herda
        // (name, energy=100) e `new Dog("Rex")` passa 1 arg.
        if user_args.len() > expected {
            return Err(anyhow!(
                "constructor de `{class_name}` espera ate {} argumento(s), recebeu {}",
                expected,
                user_args.len()
            ));
        }
        let mut args = vec![handle];
        for (i, expected_ty) in abi.params.iter().skip(1).copied().enumerate() {
            if i < user_args.len() {
                let a = &user_args[i];
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
            } else {
                let v = match expected_ty {
                    ValTy::F64 => ctx.builder.ins().f64const(f64::NAN),
                    _ => ctx.builder.ins().iconst(cl::I64, 0),
                };
                args.push(v);
            }
        }
        ctx.builder.ins().call(fref, &args);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

/// (#810) Elemento de um TypedArray: (bits, signed, is_float).
/// `None` quando o nome nao eh um TypedArray conhecido.
#[derive(Clone, Copy)]
pub(super) struct TaElem {
    pub(super) bits: u32,
    pub(super) signed: bool,
    pub(super) is_float: bool,
}

pub(super) fn typed_array_kind(name: &str) -> Option<TaElem> {
    let e = |bits, signed, is_float| Some(TaElem { bits, signed, is_float });
    match name {
        "Int8Array" => e(8, true, false),
        "Uint8Array" | "Uint8ClampedArray" => e(8, false, false),
        "Int16Array" => e(16, true, false),
        "Uint16Array" => e(16, false, false),
        "Int32Array" => e(32, true, false),
        "Uint32Array" => e(32, false, false),
        "Float32Array" | "Float64Array" => e(64, true, true),
        // (cross-runtime #65) BigInt typed arrays: 64-bit int elements.
        "BigInt64Array" => e(64, true, false),
        "BigUint64Array" => e(64, false, false),
        _ => None,
    }
}

/// (#810) `new <TypedArray>(arg)` — backing Vec<i64>.
/// - arg array literal `[a,b,c]`: cada elemento normalizado e empurrado.
/// - arg numerico `n`: Vec de `n` zeros.
/// - arg handle (outro array/typed array): copia elementos normalizados.
pub(super) fn lower_new_typed_array(
    ctx: &mut FnCtx,
    new_expr: &swc_ecma_ast::NewExpr,
    elem: TaElem,
) -> Result<TypedVal> {
    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let vec_h = ctx.builder.inst_results(inst)[0];

    let args = new_expr.args.as_deref().unwrap_or(&[]);
    if let Some(arg) = args.first() {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in `new TypedArray`"));
        }
        match arg.expr.as_ref() {
            // `new Uint8Array([1,2,3])` — elementos estaticos.
            Expr::Array(arr) => {
                let push_fn = ctx
                    .get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
                for el in &arr.elems {
                    let Some(el) = el else { continue };
                    if el.spread.is_some() {
                        return Err(anyhow!("spread not supported in `new TypedArray`"));
                    }
                    let tv = lower_expr(ctx, &el.expr)?;
                    let norm = normalize_ta_elem(ctx, tv, elem);
                    ctx.builder.ins().call(push_fn, &[vec_h, norm]);
                }
            }
            _ => {
                let tv = lower_expr(ctx, &arg.expr)?;
                if matches!(tv.ty, ValTy::I64 | ValTy::I32 | ValTy::U64 | ValTy::F64) {
                    // `new Uint16Array(3)` — Vec de N zeros, OU (#79) quando o
                    // arg tem tipo ambiguo (resultado de await) e na verdade eh
                    // um handle Buffer/Vec, copia os elementos. Decisao em
                    // runtime via VEC_FILL_TA_ARG (length vs handle vivo).
                    let arg_val = ctx.coerce_to_i64(tv).val;
                    let fill_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_FILL_TA_ARG",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    ctx.builder.ins().call(fill_fn, &[vec_h, arg_val]);
                } else if matches!(tv.ty, ValTy::Handle) {
                    let src_h = ctx.coerce_to_i64(tv).val;
                    // (#811/205) `new Uint8Array(arrayBuffer)` — VIEW VIVA: o
                    // valor da TypedArray eh o PROPRIO handle do buffer. `v[i]`
                    // le/escreve `elem_bytes` bytes via TA_GET/SET_ELEM
                    // (marcado em local_ta_view no decls). Escritas sao
                    // compartilhadas entre views do mesmo buffer.
                    // (#69) SharedArrayBuffer eh backing-identico a ArrayBuffer
                    // no RTS; ambos viram view-viva sobre o mesmo Buffer.
                    let is_array_buffer = matches!(
                        arg.expr.as_ref(),
                        Expr::Ident(id) if ctx
                            .local_class_ty
                            .get(id.sym.as_str())
                            .map(|c| c == "ArrayBuffer" || c == "SharedArrayBuffer")
                            .unwrap_or(false)
                    );
                    if is_array_buffer {
                        // Retorna o handle do buffer direto (view viva).
                        return Ok(TypedVal::new(src_h, ValTy::Handle));
                    }
                    // Array-like generico: copia para o Vec.
                    let copy_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    ctx.builder.ins().call(copy_fn, &[vec_h, src_h]);
                }
            }
        }
    }
    Ok(TypedVal::new(vec_h, ValTy::Handle))
}

/// Normaliza um valor ao alcance do elemento do TypedArray e devolve um i64
/// pronto para `vec_push`. Float* mantem como bits f64; inteiros aplicam
/// mask/extensao de sinal.
fn normalize_ta_elem(ctx: &mut FnCtx, tv: TypedVal, elem: TaElem) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::MemFlags;
    if elem.is_float {
        // Float32/64 — guarda bits f64 (Vec interpreta como number via INSPECT).
        let f = ctx.coerce_to_f64(tv).val;
        return ctx.builder.ins().bitcast(cl::I64, MemFlags::new(), f);
    }
    // Inteiro: trunca a `bits` via band, depois estende sinal se signed.
    let mut v = ctx.coerce_to_i64(tv).val;
    if elem.bits < 64 {
        let mask = (1i64 << elem.bits) - 1;
        v = ctx.builder.ins().band_imm(v, mask);
        if elem.signed {
            // sign-extend: shift left ate o topo e arithmetic shift right.
            let shift = 64 - elem.bits as i64;
            v = ctx.builder.ins().ishl_imm(v, shift);
            v = ctx.builder.ins().sshr_imm(v, shift);
        }
    }
    v
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
            // (cross-runtime #787) call/apply usam APPLY_TYPED que converte
            // args int->f64 bits baseado em param_kinds. Sem isso,
            // multiply.call({factor:1}, 3, 4) passa 3,4 como i64 puros que
            // invoke_typed interpretava como bits denormal f64.
            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_APPLY_TYPED",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY_TYPED",
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
            // (#180/#294) call/apply retornam i64 raw que pode ser handle de
            // string (fn retornou string) ou number bits. Marca como
            // var_member_call_values para que TPL_COERCE_AUTO renderize
            // correto em template literal / concat.
            if method == "call" || method == "apply" {
                ctx.var_member_call_values.insert(r);
            }
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
    // (cross-runtime #787) `has_this_param=true` quando user fn declarada
    // como `function f(this: any, ...)` — primeiro param Cranelift eh o
    // thisArg explicito. Sem isto, multiply.bind(obj) ignora obj porque
    // CALL empilha effective_this no slot e Cranelift recebe args=[a, b]
    // em vez de [this, a, b].
    let has_this_param_flag = fn_name_has_this_param(fn_name)
        || ctx
            .user_fns
            .get(fn_name)
            .map(|f| f.has_this_param)
            .unwrap_or(false);
    let has_this_v = ctx
        .builder
        .ins()
        .iconst(cl::I32, i64::from(has_this_param_flag));

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
            // (cross-runtime #359) Custom `toString` own-property override:
            // FUNCTION_TO_STRING_DYN(name, fn_handle) invokes an installed
            // override (set via `(f as any).toString = ...`) or falls back to
            // the native TO_STRING_HANDLE formatting.
            let (np, nl) = ctx.emit_str_literal(fn_name.as_bytes())?;
            let dyn_fn = ctx.get_extern(
                "__RTS_FN_RT_FUNCTION_TO_STRING_DYN",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(dyn_fn, &[np, nl, fn_handle]);
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

            // (cross-runtime #787) call/apply usam APPLY_TYPED que converte
            // args int->f64 bits baseado em param_kinds. Sem isso,
            // multiply.call({factor:1}, 3, 4) passa 3,4 como i64 puros que
            // invoke_typed interpretava como bits denormal f64.
            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_APPLY_TYPED",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY_TYPED",
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
            // (#180/#294) call/apply retornam i64 raw que pode ser handle de
            // string (fn retornou string) ou number bits. Marca como
            // var_member_call_values para que TPL_COERCE_AUTO renderize
            // correto em template literal / concat.
            if method == "call" || method == "apply" {
                ctx.var_member_call_values.insert(r);
            }
            Ok(Some(TypedVal::new(r, ty)))
        }
        _ => Ok(None),
    }
}

/// (cross-runtime #336) Emite IR para obter o handle do prototype Map de
/// `class_name`, garantindo que `proto.__proto__` aponta para o proto da
/// super classe (recursivo). Retorna None quando a classe nao tem
/// `__class_X__init` registrado (classes vazias sem super).
fn emit_proto_for_class(
    ctx: &mut FnCtx,
    class_name: &str,
    map_set: cranelift_codegen::ir::FuncRef,
) -> Result<Option<cranelift_codegen::ir::Value>> {
    let init_name = class_init_name(class_name);
    let Ok(fn_addr) = super::emit_user_fn_addr(ctx, &init_name) else {
        return Ok(None);
    };
    let arity = ctx.user_fns.get(&init_name).map(|f| f.params.len() as i64).unwrap_or(0);
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
    let inst_r = ctx.builder.ins().call(
        reify_fn,
        &[fn_addr.val, arity_v, n_ptr, n_len, is_arrow_v, has_this_v],
    );
    let fn_handle = ctx.builder.inst_results(inst_r)[0];
    let proto_get = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_PROTOTYPE_GET",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let inst_proto = ctx.builder.ins().call(proto_get, &[fn_handle]);
    let proto_h = ctx.builder.inst_results(inst_proto)[0];

    let super_name = ctx
        .classes
        .get(class_name)
        .and_then(|m| m.super_class.clone());
    if let Some(super_name) = super_name {
        if let Some(super_proto) = emit_proto_for_class(ctx, &super_name, map_set)? {
            let (proto_kp, proto_kl) = ctx.emit_str_literal(b"__proto__")?;
            ctx.builder.ins().call(map_set, &[proto_h, proto_kp, proto_kl, super_proto]);
        }
    } else {
        // (cross-runtime #336) Classe raiz: `proto.__proto__ = Object.prototype`.
        // Object.prototype singleton com `constructor.name === "Object"`.
        let obj_proto_fn = ctx.get_extern(
            "__RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE",
            &[],
            Some(cl::I64),
        )?;
        let inst_op = ctx.builder.ins().call(obj_proto_fn, &[]);
        let obj_proto = ctx.builder.inst_results(inst_op)[0];
        let (proto_kp, proto_kl) = ctx.emit_str_literal(b"__proto__")?;
        ctx.builder.ins().call(map_set, &[proto_h, proto_kp, proto_kl, obj_proto]);
    }
    Ok(Some(proto_h))
}
