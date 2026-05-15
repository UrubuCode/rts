//! Chamadas indiretas via handle ou ident-de-fn:
//! - lower_var_member_call: `obj.method(args)` quando obj eh local sem
//!   tipo de classe definido — caminho generico de dispatch.
//! - lower_indirect_call: callee eh Expr::Ident apontando pra valor
//!   que nao eh user fn estatica (ex: param que recebeu func_addr).
//! - emit_function_handle_indirect_call: invoca via handle Function
//!   reificado (com bound_this/bound_args).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::Module;
use swc_ecma_ast::{CallExpr, Expr};

use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::operators::to_f64;
use super::builtins::{
    lower_array_builtin, lower_map_set_builtin, lower_number_builtin, lower_string_builtin,
};

pub(super) fn lower_var_member_call(
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
        // (cross-runtime #157/#231) arr.toString()/toLocaleString() = join(",").
        if matches!(prop, "toString" | "toLocaleString") && call.args.is_empty() {
            use cranelift_codegen::ir::{InstBuilder, types as cl};
            let sep_h = ctx.emit_str_handle(b",")?.val;
            let join_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(join_fn, &[obj_h, sep_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(crate::codegen::lower::ctx::TypedVal::new(v, ValTy::Handle));
        }
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

    // RegExp.prototype methods (test/exec) — antes de string/map pois
    // regex handle nao eh String nem Map; cair em map_get_chain trap por
    // user1 (SIGILL). Detecta via local_class_ty["RegExp"] OU via match
    // direto dos nomes de metodo (test/exec).
    let is_regexp = ctx
        .local_class_ty
        .get(obj_name)
        .map(|s| s == "RegExp")
        .unwrap_or(false);
    if matches!(obj_tv.ty, ValTy::Handle) && !is_proto_instance
        && (is_regexp || matches!(prop, "test" | "exec"))
    {
        if let Some(tv) = super::builtins::lower_regexp_builtin(ctx, prop, obj_h, call)? {
            return Ok(tv);
        }
    }

    // Builtins de string em receiver Handle: s.indexOf(...), s.startsWith(...), etc.
    // Tem que vir antes do map_get porque uma string handle nao e um map —
    // map_get retornaria lixo, e o call_indirect subsequente saltaria pra
    // endereco invalido. (#235: indexOf travava/SIGSEGV em string com \0)
    // Tambem tenta I64 ambiguo (parâmetro de arrow sem tipo anotado que pode
    // ser uma string handle, ex: callback de replace/map/forEach).
    if (matches!(obj_tv.ty, ValTy::Handle) || matches!(obj_tv.ty, ValTy::I64)) && !is_proto_instance {
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

    // (cross-runtime #772) `obj.isPrototypeOf(other)` — walk __proto__
    // de `other` procurando `obj`. Retorna true se obj aparece na cadeia.
    if prop == "isPrototypeOf" && call.args.len() == 1 {
        let other_tv = lower_expr(ctx, &call.args[0].expr)?;
        let other_h = ctx.coerce_to_i64(other_tv).val;
        let f = ctx.get_extern(
            "__RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[obj_h, other_h]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(v, ValTy::Bool));
    }

    // (#264 PR5) `obj.hasOwnProperty(key)` — verifica own props sem chain.
    // (cross-runtime #788) `obj.propertyIsEnumerable(key)` — own + enumerable.
    if (prop == "hasOwnProperty" || prop == "propertyIsEnumerable") && call.args.len() == 1 {
        let key_tv = lower_expr(ctx, &call.args[0].expr)?;
        // Numero -> string handle (JS converte: `arr.hasOwnProperty(0)`
        // testa key "0"). `coerce_to_handle` ja' faz stringify de I64/F64.
        let key_h = ctx.coerce_to_handle(key_tv)?.val;
        let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let inst_p = ctx.builder.ins().call(str_ptr_fn, &[key_h]);
        let kptr = ctx.builder.inst_results(inst_p)[0];
        let inst_l = ctx.builder.ins().call(str_len_fn, &[key_h]);
        let klen = ctx.builder.inst_results(inst_l)[0];
        let sym = if prop == "hasOwnProperty" {
            "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY"
        } else {
            "__RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE"
        };
        let has_own = ctx.get_extern(sym, &[cl::I64, cl::I64, cl::I64], Some(cl::I64))?;
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
pub(super) fn lower_indirect_call(ctx: &mut FnCtx, callee_expr: &Expr, call: &CallExpr) -> Result<TypedVal> {
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
pub(super) fn emit_function_handle_indirect_call(
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
