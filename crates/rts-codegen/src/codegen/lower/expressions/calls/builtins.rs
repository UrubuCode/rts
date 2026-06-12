//! Builtins de tipo: string/number/array/console/map+set.
//!
//! Cada `lower_*_builtin` reescreve uma chamada `recv.method(...)` em
//! IR Cranelift direto quando o codegen pode resolver pelo tipo do
//! receiver. Quando nao consegue, retorna `None` e o caller cai em
//! caminhos genericos (lower_var_member_call etc).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{CallExpr, Expr};

use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::emit_user_fn_addr;

pub(super) fn lower_string_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
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
        // indexOf drenado pro Registry (INDEX_OF_FROM + default from=0 cobre as
        // duas aridades) — cai no fallback `_`.
        // includes/startsWith drenados pro Registry (INCLUDES_AT/STARTS_WITH_AT +
        // default pos=0 cobrem as duas aridades) — caem no fallback `_`.
        // ── indexing ─────────────────────────────────────────────────────
        // charAt drenado pro Registry (CHAR_AT idêntico) — cai no fallback `_`.
        // at drenado pro Registry (STRING_AT idêntico) — cai no fallback `_`.
        // ── slicing ───────────────────────────────────────────────────────
        // ── transform ─────────────────────────────────────────────────────
        // case/trim drenados pro Registry (sem overload por tipo de arg) — caem
        // no fallback genérico do `_ =>` no fim. Símbolos GL_STRING_* idênticos.
        // localeCompare drenado pro Registry (LOCALE_COMPARE idêntico) — fallback.
        // toString (→TO_STRING) e valueOf (→ReceiverIdentity) drenados pro
        // Registry — caem no fallback `_`.
        // toWellFormed drenado pro Registry (TO_WELL_FORMED idêntico) — fallback.
        // isWellFormed drenado pro Registry (IS_WELL_FORMED idêntico) — fallback.
        // GENÉRICO via Registry: métodos de String SEM dispatch por tipo-de-arg
        // (case/trim/normalize/...) resolvem aqui pelo Registry, sem braço
        // hardcoded. Os métodos com overload por tipo de arg (replace str-vs-
        // regex-vs-fn, split, match/search, indexOf-from) permanecem como braços
        // dedicados acima — o Registry resolve por (nome, aridade), não por tipo
        // de arg, então não pode escolher o símbolo certo deles ainda.
        _ => super::ns_call::try_global_class_instance_method(
            ctx,
            "String",
            method,
            TypedVal::new(recv_h, ValTy::Handle),
            call,
        ),
    }
}

/// Map/Set methods (#222) em receiver Handle. v0 mapeia direto pra
/// collections.map_* (mesmo backing store). Set usa Map<key, 1> com
/// key sempre string — limitacao aceita de v0.
///
/// Reconhecidos: set/get/has/delete/clear/add/size. Para `m.size`
/// (sem parens) ainda nao tem caminho — usuario chama `m.size()` em v0.
pub(super) fn lower_map_set_builtin(
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

    // (#394) Como `arg_strptr`, mas lowera o arg UMA vez e devolve tambem o
    // valor i64 cru do elemento (`raw`). Necessario p/ has()/delete() unificados
    // Map/Set: Map usa (p,l) key-string, Set usa `raw` p/ a key estavel de
    // identidade. Evita lowerar o arg duas vezes (efeitos colaterais / handles
    // duplicados em `set.has(f())` ou `set.has({...})`).
    fn arg_strptr_and_raw(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<(
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
    )> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let raw = ctx.coerce_to_i64(tv).val;
        let h = ctx.coerce_to_handle(tv)?.val;
        let ptr_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fref, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fref, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l, raw))
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
            // (#1275) value float fracionario literal -> bits f64.
            let val_is_frac = super::super::members::expr_is_frac_float_lit(&val_arg.expr);
            let val_tv = lower_expr(ctx, &val_arg.expr)?;
            let val_i64 = if matches!(val_tv.ty, ValTy::F64) && val_is_frac {
                ctx.builder.ins().bitcast(
                    cl::I64,
                    cranelift_codegen::ir::MemFlags::new(),
                    val_tv.val,
                )
            } else {
                ctx.coerce_to_i64(val_tv).val
            };
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
            // (#394) Set.add(value) → SET_ADD(h, elemRaw). O runtime deriva a
            // KEY estavel (set_stable_key): conteudo p/ string, "\0obj#<h>" p/
            // objeto/Set (dedup por IDENTIDADE), decimal p/ numero. Armazena o
            // VALOR i64 original como value, de modo que values()/[...set]/for-of
            // recuperem a identidade do elemento (handle de objeto/Set aninhado).
            // Antes objetos viravam key vazia e colidiam todos numa entrada.
            let val_tv = lower_expr(ctx, &call.args[0].expr)?;
            let elem_raw = ctx.coerce_to_i64(val_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_SET_ADD",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, elem_raw]);
            let h = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(h, ValTy::Handle)))
        }
        "get" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let raw = ctx.builder.inst_results(inst)[0];
            // JS semantics: Map.get(missing) -> undefined. RTS map_get retorna
            // 0; convertemos pro sentinel undefined (i64::MIN+2) para que
            // INSPECT/TPL_COERCE_AUTO mostrem "undefined" corretamente.
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_zero = ctx.builder.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal, raw, zero,
            );
            let undef = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
            let v = ctx.builder.ins().select(is_zero, undef, raw);
            // Marca ambiguo pra template literal usar TPL_COERCE_AUTO.
            ctx.var_member_call_values.insert(v);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "has" => {
            // (#394) Unificado Map/Set. Passa key-string (kp/kl) E elemento cru
            // (elem_raw); o runtime escolhe por handle_is_set_kind: Set usa key
            // estavel (identidade p/ objetos), Map usa key-string (inalterado).
            let (kp, kl, elem_raw) = arg_strptr_and_raw(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_HAS",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl, elem_raw]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "delete" => {
            // (#394) Unificado Map/Set (ver `has`).
            let (kp, kl, elem_raw) = arg_strptr_and_raw(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_DELETE",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl, elem_raw]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        // Métodos LIMPOS (clear/size/keys/values/entries + set-ops ES2025
        // union/intersection/difference/symmetricDifference/isSubsetOf/
        // isSupersetOf/isDisjointFrom) resolvem pela classe global "Map" no
        // Registry (sem braço hardcoded) — ver o `_ =>` fallback abaixo e
        // `collections::register_mapset_class_spec`.
        // (cross-runtime #793) map.forEach((value, key, map) => ...).
        // Aceita arrow inline ou ident de user fn — passa ptr/handle pra
        // MAP_FOR_EACH no runtime que invoca via transmute.
        "forEach" if call.args.len() == 1 && call.args[0].spread.is_none() => {
            // Se o callback eh Ident de user fn, reify pra Function handle
            // (com calling conv tail preservada) — INVOKE_AUTO interno do
            // MAP_FOR_EACH precisa disso pra invocar com a aridade correta.
            let cb_tv = if let Expr::Ident(id) = call.args[0].expr.as_ref() {
                let name = id.sym.as_str().to_string();
                // (#376) Callback liftado COM captura (`__lifted_cap_N` do
                // parallelism pass, ou `__lifted_arrow_N` do this_arrow): reifica
                // com bound_args via REIFY_CAPTURED. Sem isso ia por
                // emit_hoisted_arrow_handle (sem bound_args) e a captura (`value`)
                // colidia com o item do Set passado pelo MAP_FOR_EACH.
                let caps_opt = crate::codegen::lower::passes::parallelism::LIFTED_CAPTURES
                    .with(|c| c.borrow().get(&name).cloned())
                    .or_else(|| crate::codegen::lower::passes::this_arrow::lifted_arrow_captures(&name));
                if let Some(caps) = caps_opt {
                    let cap_vals: Vec<TypedVal> = caps
                        .iter()
                        .filter_map(|c| {
                            if c == "__captured_this" { ctx.read_local("this") }
                            else { ctx.read_local(c) }
                        })
                        .collect();
                    if cap_vals.len() == caps.len() {
                        super::emit_lifted_arrow_handle_with_captures(ctx, &name, &cap_vals)?
                    } else if ctx.user_fns.contains_key(&name) {
                        super::emit_hoisted_arrow_handle(ctx, &name, None)?
                    } else {
                        lower_expr(ctx, &call.args[0].expr)?
                    }
                } else if ctx.user_fns.contains_key(&name) {
                    super::emit_hoisted_arrow_handle(ctx, &name, None)?
                } else {
                    lower_expr(ctx, &call.args[0].expr)?
                }
            } else {
                lower_expr(ctx, &call.args[0].expr)?
            };
            let cb_ptr = ctx.coerce_to_i64(cb_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH",
                &[cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h, cb_ptr]);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            )))
        }
        // Fallback genérico: métodos limpos (clear/size/keys/values/entries +
        // set-ops) resolvem pela classe global "Map" no Registry, sem braço
        // hardcoded. O receiver é o handle do Map/Set.
        _ => super::ns_call::try_global_class_instance_method(
            ctx,
            "Map",
            method,
            TypedVal::new(recv_h, ValTy::Handle),
            call,
        ),
    }
}

/// Console object (#221) — mapeia console.log/info/debug → io.print
/// e console.error/warn → io.eprint. Args sao concatenados como string
/// separados por espaco (semantica JS). Implementado em codegen direto
/// pra que `console.X(...)` funcione sem import explicito.
pub(super) fn lower_console_call(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let Some(method) = qualified.strip_prefix("console.") else {
        return Ok(None);
    };

    // (#310/#311/#312) Override runtime: se `(console as any).<method>` foi
    // reatribuido, despacha pro handle custom via INVOKE_AUTO. Checa em
    // runtime (GET_OVERRIDE != 0) e cai no nativo caso contrario. So' para
    // metodos sem spread nos args (variadic empacotado em Vec).
    if call.args.iter().all(|a| a.spread.is_none()) {
        let (mp, ml) = ctx.emit_str_literal(method.as_bytes())?;
        let get_ov = ctx.get_extern(
            "__RTS_FN_RT_CONSOLE_GET_OVERRIDE",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let ov_inst = ctx.builder.ins().call(get_ov, &[mp, ml]);
        let ov = ctx.builder.inst_results(ov_inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let has_ov = ctx.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            ov,
            zero,
        );
        let ov_block = ctx.builder.create_block();
        let native_block = ctx.builder.create_block();
        let merge = ctx.builder.create_block();
        ctx.builder.append_block_param(merge, cl::I64);
        ctx.builder.ins().brif(has_ov, ov_block, &[], native_block, &[]);

        // Override block: empacota args em Vec e INVOKE_AUTO(ov, 0, vec).
        ctx.builder.switch_to_block(ov_block);
        ctx.builder.seal_block(ov_block);
        let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
        let vn_inst = ctx.builder.ins().call(vec_new, &[]);
        let args_vec = ctx.builder.inst_results(vn_inst)[0];
        let push = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            &[cl::I64, cl::I64],
            None,
        )?;
        for arg in &call.args {
            let tv = lower_expr(ctx, &arg.expr)?;
            let v = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(push, &[args_vec, v]);
        }
        // (#310) Callback variadic (`...args`): a lifted fn espera UM param
        // (o array de args, pos expand_rest_args). Empacota args_vec dentro
        // de outro Vec [args_vec] e passa esse como args. Senao (aridade
        // fixa) passa os args individuais direto.
        let is_var_fn = ctx.get_extern(
            "__RTS_FN_RT_CONSOLE_OVERRIDE_IS_VARIADIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let iv_inst = ctx.builder.ins().call(is_var_fn, &[mp, ml]);
        let is_var = ctx.builder.inst_results(iv_inst)[0];
        let is_var_b = ctx.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            is_var,
            zero,
        );
        // wrapped = [args_vec]
        let vn2 = ctx.builder.ins().call(vec_new, &[]);
        let wrapped = ctx.builder.inst_results(vn2)[0];
        ctx.builder.ins().call(push, &[wrapped, args_vec]);
        let final_args = ctx.builder.ins().select(is_var_b, wrapped, args_vec);
        let invoke = ctx.get_extern(
            "__RTS_FN_RT_INVOKE_AUTO",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inv_inst = ctx.builder.ins().call(invoke, &[ov, zero, final_args]);
        let inv_ret = ctx.builder.inst_results(inv_inst)[0];
        ctx.builder.ins().jump(merge, &[inv_ret.into()]);

        // Native block: dispatch builtin original (recursao com o mesmo
        // qualified — mas agora ja' dentro do native_block, sem re-checar
        // override pois a recursao chega aqui de novo... evitamos recursao:
        // movemos a logica nativa para uma fn separada).
        ctx.builder.switch_to_block(native_block);
        ctx.builder.seal_block(native_block);
        let native_tv = lower_console_call_native(ctx, method, call)?;
        let native_v = match native_tv {
            Some(tv) => ctx.coerce_to_i64(tv).val,
            None => ctx.builder.ins().iconst(cl::I64, i64::MIN + 2),
        };
        ctx.builder.ins().jump(merge, &[native_v.into()]);

        ctx.builder.switch_to_block(merge);
        ctx.builder.seal_block(merge);
        let result = ctx.builder.block_params(merge)[0];
        return Ok(Some(TypedVal::new(result, ValTy::I64)));
    }

    lower_console_call_native(ctx, method, call)
}

/// Dispatch nativo dos metodos de console (sem checar override runtime).
/// Separado de lower_console_call p/ evitar recursao no path de override.
fn lower_console_call_native(
    ctx: &mut FnCtx,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let target_symbol: &str = match method {
        "log" | "info" | "debug" => "__RTS_FN_NS_IO_PRINT",
        "error" | "warn" => "__RTS_FN_NS_IO_EPRINT",
        // (#686/309) console.assert(cond, ...msg) — se cond eh truthy, noop.
        // Senao imprime "Assertion failed: <msg>" em stderr. Nao throw.
        // dir(arg) — imprime arg via INSPECT (= console.log um arg).
        "assert" => {
            // Sem args: noop.
            if call.args.is_empty() {
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                return Ok(Some(TypedVal::new(zero, ValTy::I64)));
            }
            // Avalia cond.
            let cond_tv = lower_expr(ctx, &call.args[0].expr)?;
            // Truthy via __RTS_FN_RT_TRUTHY (cobre handle/string/0/sentinels).
            let cond_i64 = ctx.coerce_to_i64(cond_tv).val;
            let truthy_fn = ctx.get_extern(
                "__RTS_FN_RT_TRUTHY",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst_t = ctx.builder.ins().call(truthy_fn, &[cond_i64]);
            let truthy = ctx.builder.inst_results(inst_t)[0];
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_falsy = ctx.builder.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                truthy,
                zero,
            );
            let print_block = ctx.builder.create_block();
            let merge = ctx.builder.create_block();
            ctx.builder.ins().brif(is_falsy, print_block, &[], merge, &[]);

            ctx.builder.switch_to_block(print_block);
            ctx.builder.seal_block(print_block);
            // Constroi "Assertion failed: <msg>" (msgs concat com " ").
            let prefix = ctx.emit_str_handle(b"Assertion failed:")?.val;
            let space = ctx.emit_str_handle(b" ")?.val;
            let concat = ctx.get_extern(
                "__RTS_FN_NS_GC_STRING_CONCAT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let auto_coerce = ctx.get_extern(
                "__RTS_FN_RT_INSPECT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let mut msg = prefix;
            for arg in &call.args[1..] {
                if arg.spread.is_some() {
                    return Err(anyhow!("spread not supported in console.assert args"));
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                let is_known_str = matches!(
                    arg.expr.as_ref(),
                    Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
                );
                let needs_auto = matches!(tv.ty, ValTy::Handle | ValTy::U64) && !is_known_str;
                let h = ctx.coerce_to_handle(tv)?.val;
                let h = if needs_auto {
                    let inst = ctx.builder.ins().call(auto_coerce, &[h]);
                    ctx.builder.inst_results(inst)[0]
                } else {
                    h
                };
                let with_space = ctx.builder.ins().call(concat, &[msg, space]);
                let prev = ctx.builder.inst_results(with_space)[0];
                let combined = ctx.builder.ins().call(concat, &[prev, h]);
                msg = ctx.builder.inst_results(combined)[0];
            }
            let ptr_fref =
                ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
            let len_fref =
                ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let pi = ctx.builder.ins().call(ptr_fref, &[msg]);
            let p = ctx.builder.inst_results(pi)[0];
            let li = ctx.builder.ins().call(len_fref, &[msg]);
            let l = ctx.builder.inst_results(li)[0];
            let eprint = ctx.get_extern("__RTS_FN_NS_IO_EPRINT", &[cl::I64, cl::I64], None)?;
            ctx.builder.ins().call(eprint, &[p, l]);
            ctx.builder.ins().jump(merge, &[]);

            ctx.builder.switch_to_block(merge);
            ctx.builder.seal_block(merge);
            return Ok(Some(TypedVal::new(zero, ValTy::I64)));
        }
        // (#686) dir(obj) — alias de console.log com 1 arg + INSPECT.
        "dir" => "__RTS_FN_NS_IO_PRINT",
        // (cross-runtime #311/#310) Console methods nao implementados —
        // tratados como noop ou alias de log/error pra evitar
        // 'unknown namespace member'. table/group/groupEnd/groupCollapsed/
        // count/countReset/trace/time*/clear/profile/profileEnd/timeStamp.
        "table" | "group" | "groupCollapsed" | "groupEnd" |
        "count" | "countReset" | "trace" | "time" | "timeEnd" | "timeLog" |
        "clear" | "profile" | "profileEnd" | "timeStamp" => "__RTS_FN_NS_IO_PRINT",
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

    // Para console.* usamos INSPECT (pretty-print estilo Node/Bun):
    // arrays viram `[ 1, 2, 'a' ]`, objetos `{ k: v }`, strings top-level
    // sem aspas. Literals string/template ja sao Handle conhecido, skip
    // da coercao via heuristica simples: arg e' Lit::Str ou Tpl.
    let auto_coerce = ctx.get_extern(
        "__RTS_FN_RT_INSPECT",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let num_bias = ctx.get_extern(
        "__RTS_FN_RT_TPL_COERCE_NUM_BIAS",
        &[cl::I64],
        Some(cl::I64),
    )?;
    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in console.* args"));
        }
        let is_known_str = matches!(
            arg.expr.as_ref(),
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
        );
        let tv = lower_expr(ctx, &arg.expr)?;
        // (#573) U64 tambem pode ser handle ambiguo (ex: JSON.parse retorno
        // de '42' eh i64 raw com tipo U64). Auto-coerce decide em runtime.
        let needs_auto = matches!(tv.ty, ValTy::Handle | ValTy::U64) && !is_known_str;
        // (cross-runtime #335/#1056) I64 marcado var_member_call_values (ex:
        // `obj.field`, `getter()`, member call result) — coerce com bias
        // numerico: value=0 vira "0" em vez de "null" (TPL_COERCE_AUTO).
        // Casos legitimos de `null` continuam cobertos via sentinel `i64::MIN+3`.
        let needs_num_bias = matches!(tv.ty, ValTy::I64)
            && ctx.var_member_call_values.contains(&tv.val)
            && !is_known_str;
        let h = if needs_num_bias {
            let val_i64 = ctx.coerce_to_i64(tv).val;
            let inst = ctx.builder.ins().call(num_bias, &[val_i64]);
            ctx.builder.inst_results(inst)[0]
        } else {
            let h0 = ctx.coerce_to_handle(tv)?.val;
            if needs_auto {
                let inst = ctx.builder.ins().call(auto_coerce, &[h0]);
                ctx.builder.inst_results(inst)[0]
            } else {
                h0
            }
        };
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
pub(super) fn lower_array_builtin(
    ctx: &mut FnCtx,
    method: &str,
    obj_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // (cross-runtime #132/#159/#233/#235/#248, issue #195) Quando um arrow
    // inline com captura mutavel (`count++`, `sum += x`) falha no lifter
    // de `passes/parallelism.rs`, o arrow fica no call site e o fallback
    // generico tenta tratar Vec como Map (MAP_GET_CHAIN) e SIGILL. Aqui
    // damos um default seguro: some=false, every=true, forEach=undefined.
    // Nao implementa a semantica (closure captura nao funciona ate #195),
    // mas evita crash hostil.
    if matches!(method, "some" | "every" | "forEach")
        && call.args.len() == 1
        && matches!(call.args[0].expr.as_ref(), Expr::Arrow(_))
    {
        let _ = obj_h;
        let v = match method {
            "every" => ctx.builder.ins().iconst(cl::I64, 1),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        let ty = if method == "forEach" { ValTy::I64 } else { ValTy::Bool };
        return Ok(Some(TypedVal::new(v, ty)));
    }
    match method {
        // (cross-runtime #365) `.map(cb)` fallback quando o array_methods_pass
        // NAO reescreveu pra `parallel.map` — caso tipico: corpo de async fn
        // ja' convertido em state machine pelo `generator_sm` no parser, cujo
        // switch interno os passes de codegen nao varrem. Sem este arm, `.map`
        // cai no INVOKE generico (reify+call) e crasha (ILLEGAL_INSTRUCTION).
        // Emite `parallel.map(vec, fn_ptr)` direto — mesmo resultado da
        // reescrita. Callback eh Ident (user fn ou arrow ja' liftada pra
        // top-level `__lifted_arr_method_N`).
        "map" if call.args.len() == 1 && call.args[0].spread.is_none() => {
            let cb = &call.args[0].expr;
            let fn_ptr = if let Expr::Ident(id) = cb.as_ref() {
                if ctx.user_fns.contains_key(id.sym.as_ref())
                    && ctx.var_ty(id.sym.as_ref()).is_none()
                {
                    emit_user_fn_addr(ctx, id.sym.as_ref())?.val
                } else {
                    let tv = lower_expr(ctx, cb)?;
                    ctx.coerce_to_i64(tv).val
                }
            } else {
                let tv = lower_expr(ctx, cb)?;
                ctx.coerce_to_i64(tv).val
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_PARALLEL_MAP",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "push" => {
            if call.args.is_empty() {
                return Ok(None);
            }
            let push_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            // (cross-runtime #150) JS: push aceita N args, todos sao
            // pushed em ordem.
            for arg in &call.args {
                // (cross-runtime #86) Spread `arr.push(...src)`: estende com os
                // elementos de src (Vec OU Buffer) via VEC_EXTEND_FROM. Cobre
                // `parts.push(...chunk.value)` onde chunk.value vem de member/fn.
                if arg.spread.is_some() {
                    let extend_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    let src_tv = lower_expr(ctx, &arg.expr)?;
                    let src = ctx.coerce_to_i64(src_tv).val;
                    ctx.builder.ins().call(extend_fn, &[obj_h, src]);
                    continue;
                }
                // (#1275) Literal float fracionario: armazena bits f64 (igual
                // ao array literal) p/ que `arr.push(1.5); arr[0]` preserve 1.5.
                // Leitura de exibicao reinterpreta via heuristica >2^53.
                let is_frac_lit = super::super::members::expr_is_frac_float_lit(&arg.expr);
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = if matches!(tv.ty, ValTy::F64) && is_frac_lit {
                    ctx.builder.ins().bitcast(
                        cl::I64,
                        cranelift_codegen::ir::MemFlags::new(),
                        tv.val,
                    )
                } else {
                    ctx.coerce_to_i64(tv).val
                };
                ctx.builder.ins().call(push_fn, &[obj_h, v]);
            }
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
        // pop/shift drenados → classe global "Array" (retorno AMBIGUOUS_RET).
        // length/size (method-form) drenado → classe global "Array" (VEC_LEN).
        // at drenado → classe global "Array" (VEC_AT_AUTO: negative-index +
        // OOR→undefined no runtime; AMBIGUOUS_RET).
        // join drenado → classe global "Array" (sep? = "," via VEC_JOIN sep=0).
        // clear drenado → classe global "Array" (VEC_CLEAR, retorno void).
        // (#208 / #476) Array methods sem callback — args concretos só.
        "indexOf" | "lastIndexOf" | "includes" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            // (cross-runtime #135) JS spec: indexOf/lastIndexOf usam strict
            // equality (===), e NaN !== NaN, entao sempre retornam -1.
            // `includes` por outro lado usa SameValueZero e ACHA NaN.
            if matches!(method, "indexOf" | "lastIndexOf") {
                if let swc_ecma_ast::Expr::Ident(id) = call.args[0].expr.as_ref() {
                    if id.sym.as_str() == "NaN" {
                        let neg1 = ctx.builder.ins().iconst(cl::I64, -1);
                        return Ok(Some(TypedVal::new(neg1, ValTy::I64)));
                    }
                }
            }
            let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
            let needle = ctx.coerce_to_i64(needle_tv).val;
            // (#208) indexOf/lastIndexOf/includes(needle, fromIndex) — 2-arg.
            if matches!(method, "indexOf" | "lastIndexOf" | "includes")
                && call.args.len() == 2
            {
                let from_tv = lower_expr(ctx, &call.args[1].expr)?;
                let from = ctx.coerce_to_i64(from_tv).val;
                let sym = match method {
                    "indexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM",
                    "lastIndexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM",
                    _ => "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM",
                };
                let fref = ctx.get_extern(
                    sym,
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[obj_h, needle, from]);
                let v = ctx.builder.inst_results(inst)[0];
                let ty = if method == "includes" { ValTy::Bool } else { ValTy::I64 };
                return Ok(Some(TypedVal::new(v, ty)));
            }
            if call.args.len() != 1 {
                return Ok(None);
            }
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
        // reverse drenado → classe global "Array". flat fica (overload de depth).
        // flat drenado → classe global "Array" (VEC_FLAT_DEPTH, depth? = 1).
        // unshift drenado → classe global "Array" (VEC_UNSHIFT_VARIADIC variádico).
        // (#93) `subarray(start, end)` em TypedArray Vec backing: trata como
        // slice (cópia do range). TypedArray real compartilharia o buffer, mas
        // pra Vec backing a cópia eh suficiente para leitura via Array.from.
        // slice/subarray drenados → classe global "Array" (defaults start=0/end=SENTINEL).
        // (#305) Iterator helpers eager sobre array: take(n)=slice(0,n);
        // drop(n)=slice(n); toArray()=copia (slice(0,fim)).
        "take" | "drop" | "toArray" => {
            let (start, end) = match method {
                "take" => {
                    let n = if let Some(a) = call.args.first() {
                        let tv = lower_expr(ctx, &a.expr)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    };
                    (ctx.builder.ins().iconst(cl::I64, 0), n)
                }
                "drop" => {
                    let n = if let Some(a) = call.args.first() {
                        let tv = lower_expr(ctx, &a.expr)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    };
                    (n, ctx.builder.ins().iconst(cl::I64, i64::MIN))
                }
                _ => (
                    ctx.builder.ins().iconst(cl::I64, 0),
                    ctx.builder.ins().iconst(cl::I64, i64::MIN),
                ),
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
        // concat drenado → classe global "Array" (variádico: VEC_CONCAT_VARIADIC;
        // empacotamento de args + spread via member.variadic no emitter genérico).
        // (#93) `typedArray.set(srcArray, offset?)` — copia elementos de
        // srcArray a partir de offset. Restrito a `set([...], n)` (1o arg
        // array literal) para NAO colidir com Map.set / obj.set(k, v).
        "set" if matches!(call.args.first().map(|a| a.expr.as_ref()), Some(Expr::Array(_)))
            && call.args.iter().all(|a| a.spread.is_none()) => {
            let src_tv = lower_expr(ctx, &call.args[0].expr)?;
            let src_h = ctx.coerce_to_i64(src_tv).val;
            let offset = if let Some(a) = call.args.get(1) {
                let tv = lower_expr(ctx, &a.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let set_from = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SET_FROM",
                &[cl::I64, cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(set_from, &[obj_h, src_h, offset]);
            // set retorna undefined; devolve o proprio handle (no-op p/ chains).
            Ok(Some(TypedVal::new(obj_h, ValTy::Handle)))
        }
        // fill drenado → classe global "Array" (defaults start=0/end=SENTINEL).
        "splice" => {
            // splice(start, deleteCount, ...items)
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            if call.args.len() <= 2 {
                let fref = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[obj_h, start, count]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            // splice com ...items: aloca vec novo e usa VEC_SPLICE_INSERT.
            let new_vec = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                &[],
                Some(cl::I64),
            )?;
            let push = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let new_inst = ctx.builder.ins().call(new_vec, &[]);
            let items_h = ctx.builder.inst_results(new_inst)[0];
            for arg in &call.args[2..] {
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(push, &[items_h, v]);
            }
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_INSERT",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, count, items_h]);
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
        // copyWithin drenado → classe global "Array" (defaults start=0/end=SENTINEL).
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
            // (cross-runtime #260) findLast pode retornar elemento OU
            // sentinela MIN+2 (undefined). Marca I64 ambiguo via
            // var_member_call_values — operadores de concat usam TPL_COERCE
            // pra resolver sentinela/handle/i64. findLastIndex sempre i64.
            if method == "findLast" {
                ctx.var_member_call_values.insert(v);
            }
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "reduce" => {
            // (cross-runtime closures) `arr.reduce(fn [, init])` sobre um array
            // de tipo desconhecido/capturado (ex.: rest array `...fns` capturado
            // numa arrow retornada — `pipe`/`compose`/`reduce` de closures). O
            // array_methods_pass nao alcanca essa posicao aninhada, entao sem
            // isto cai no map_get("reduce")=0 -> trapz -> SIGILL. Reifica o
            // callback como handle COM param_kinds e usa o REDUCE_BOUND runtime
            // (mesmo do caminho parallel), que invoca via FUNCTION_CALL
            // (normaliza args number). So' intercepta o fallback de handle
            // desconhecido; arrays de tipo estatico seguem o pass paralelo.
            if !matches!(call.args.len(), 1 | 2) || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            // Callback precisa ser reificavel como callable (ident de user fn /
            // arrow liftada / var handle). Bail (None) p/ outras formas.
            let fn_handle = match super::lower_callable_target_h(ctx, &call.args[0].expr) {
                Ok(h) => h,
                Err(_) => return Ok(None),
            };
            let v = if call.args.len() == 2 {
                let init_tv = lower_expr(ctx, &call.args[1].expr)?;
                let init = ctx.coerce_to_i64(init_tv).val;
                let f = ctx.get_extern(
                    "__RTS_FN_NS_PARALLEL_REDUCE_BOUND",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, init, fn_handle]);
                ctx.builder.inst_results(inst)[0]
            } else {
                let f = ctx.get_extern(
                    "__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT_BOUND",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, fn_handle]);
                ctx.builder.inst_results(inst)[0]
            };
            ctx.var_member_call_values.insert(v);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "reduceRight" => {
            // (cross-runtime #202) reduceRight aceita (fn) ou (fn, init).
            if !matches!(call.args.len(), 1 | 2) || call.args.iter().any(|a| a.spread.is_some()) {
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
            let v = if call.args.len() == 2 {
                let init_tv = lower_expr(ctx, &call.args[1].expr)?;
                let init = ctx.coerce_to_i64(init_tv).val;
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, init, fn_ptr]);
                ctx.builder.inst_results(inst)[0]
            } else {
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT_NO_INIT",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
                ctx.builder.inst_results(inst)[0]
            };
            // (cross-runtime #808) Resultado pode ser handle de string OU
            // i64 raw — marca como ambiguo pra TPL_COERCE_AUTO em runtime.
            ctx.var_member_call_values.insert(v);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // (#208 ES2023) Immutable variants.
        // toReversed drenado → classe global "Array".
        "toSorted" => {
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
                "__RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "toSpliced" if call.args.len() >= 2
            && call.args.iter().all(|a| a.spread.is_none()) =>
        {
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count_tv = lower_expr(ctx, &call.args[1].expr)?;
            let count = ctx.coerce_to_i64(count_tv).val;
            if call.args.len() == 2 {
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, start, count]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            // toSpliced com inserts.
            let new_vec = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                &[],
                Some(cl::I64),
            )?;
            let push = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let new_inst = ctx.builder.ins().call(new_vec, &[]);
            let items_h = ctx.builder.inst_results(new_inst)[0];
            for arg in &call.args[2..] {
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(push, &[items_h, v]);
            }
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_INSERT",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, start, count, items_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        // with drenado → classe global "Array".
        // (#208) Iterators eager: values()/keys()/entries().
        // values/keys/entries drenados → classe global "Array".
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
        // Fallback genérico: métodos limpos recv-only (pop/shift/reverse/
        // toReversed/values/keys/entries) resolvem pela classe global "Array"
        // no Registry, sem braço hardcoded.
        _ => super::ns_call::try_global_class_instance_method(
            ctx,
            "Array",
            method,
            TypedVal::new(obj_h, ValTy::Handle),
            call,
        ),
    }
}

/// Math object (#760) — trata Math.hypot variádico.
/// JS spec: Math.hypot(...values) calcula sqrt(sum(values[i]²)).
/// Casos especiais: hypot() = 0, hypot(x) = abs(x).
pub(super) fn lower_math_builtin(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let Some(method) = qualified.strip_prefix("Math.") else {
        return Ok(None);
    };

    match method {
        "hypot" => {
            // Implementação variádica de Math.hypot
            if call.args.is_empty() {
                // hypot() = 0
                let zero = ctx.builder.ins().f64const(0.0);
                return Ok(Some(TypedVal::new(zero, ValTy::F64)));
            }

            if call.args.len() == 1 {
                // hypot(x) = abs(x)
                if call.args[0].spread.is_some() {
                    return Err(anyhow!("spread not supported in Math.hypot"));
                }
                let tv = lower_expr(ctx, &call.args[0].expr)?;
                let x = ctx.coerce_to_f64(tv).val;
                let abs_val = ctx.builder.ins().fabs(x);
                return Ok(Some(TypedVal::new(abs_val, ValTy::F64)));
            }

            // hypot(x1, x2, ..., xn) = sqrt(x1² + x2² + ... + xn²)
            // Implementação numericamente estável: encontra o máximo absoluto
            // e normaliza para evitar overflow/underflow.
            let mut values = Vec::new();
            for arg in &call.args {
                if arg.spread.is_some() {
                    return Err(anyhow!("spread not supported in Math.hypot"));
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_f64(tv).val;
                values.push(v);
            }

            // Encontra o máximo absoluto
            let mut max_abs = ctx.builder.ins().fabs(values[0]);
            for &v in &values[1..] {
                let abs_v = ctx.builder.ins().fabs(v);
                max_abs = ctx.builder.ins().fmax(max_abs, abs_v);
            }

            // Se max_abs é 0, retorna 0 (evita divisão por zero)
            let zero = ctx.builder.ins().f64const(0.0);
            let is_zero = ctx.builder.ins().fcmp(
                cranelift_codegen::ir::condcodes::FloatCC::Equal,
                max_abs,
                zero,
            );

            // Normaliza e soma os quadrados
            let mut sum_sq = zero;
            for &v in &values {
                let normalized = ctx.builder.ins().fdiv(v, max_abs);
                let sq = ctx.builder.ins().fmul(normalized, normalized);
                sum_sq = ctx.builder.ins().fadd(sum_sq, sq);
            }

            // result = max_abs * sqrt(sum_sq)
            let sqrt_sum = ctx.builder.ins().sqrt(sum_sq);
            let result = ctx.builder.ins().fmul(max_abs, sqrt_sum);

            // Se max_abs era zero, retorna zero; senão retorna result
            let final_result = ctx.builder.ins().select(is_zero, zero, result);

            Ok(Some(TypedVal::new(final_result, ValTy::F64)))
        }
        _ => Ok(None),
    }
}
