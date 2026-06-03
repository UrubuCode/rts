//! Chamadas indiretas via handle ou ident-de-fn:
//! - lower_var_member_call: `obj.method(args)` quando obj eh local sem
//!   tipo de classe definido — caminho generico de dispatch.
//! - lower_indirect_call: callee eh Expr::Ident apontando pra valor
//!   que nao eh user fn estatica (ex: param que recebeu func_addr).
//! - emit_function_handle_indirect_call: invoca via handle Function
//!   reificado (com bound_this/bound_args).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
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
    // (cross-runtime #808) `recv.concat(other)` em receiver I64 ambiguo
    // (param de arrow lifted) — despacha em runtime via CONCAT_AUTO porque
    // recv pode ser Vec ou String. Sem esta branch, lower_string_builtin
    // sempre rotearia pra STRING_CONCAT e arrays viravam string concat.
    // (cross-runtime #285) Captures de Vec em arrow body chegam como
    // Handle/I64 — runtime dispatch para distinguir Vec vs String.
    if matches!(obj_tv.ty, ValTy::I64 | ValTy::Handle)
        && !is_proto_instance
        && prop == "slice"
        && (call.args.len() == 1 || call.args.len() == 2)
        && call.args.iter().all(|a| a.spread.is_none())
    {
        use cranelift_codegen::ir::{InstBuilder, types as cl};
        let start_tv = lower_expr(ctx, &call.args[0].expr)?;
        let start = ctx.coerce_to_i64(start_tv).val;
        let end = if let Some(a) = call.args.get(1) {
            let tv = lower_expr(ctx, &a.expr)?;
            ctx.coerce_to_i64(tv).val
        } else {
            ctx.builder.ins().iconst(cl::I64, i64::MIN)
        };
        let f = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_SLICE_AUTO",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[obj_h, start, end]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(crate::codegen::lower::ctx::TypedVal::new(v, ValTy::Handle));
    }
    if matches!(obj_tv.ty, ValTy::I64 | ValTy::Handle)
        && !is_proto_instance
        && prop == "includes"
        && call.args.len() == 1
        && call.args[0].spread.is_none()
    {
        use cranelift_codegen::ir::{InstBuilder, types as cl};
        let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
        let needle = ctx.coerce_to_i64(needle_tv).val;
        let f = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_INCLUDES_AUTO",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[obj_h, needle]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(crate::codegen::lower::ctx::TypedVal::new(v, ValTy::Bool));
    }
    if matches!(obj_tv.ty, ValTy::I64 | ValTy::Handle)
        && !is_proto_instance
        && matches!(prop, "indexOf" | "lastIndexOf")
        && call.args.len() == 1
        && call.args[0].spread.is_none()
    {
        use cranelift_codegen::ir::{InstBuilder, types as cl};
        let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
        let needle = ctx.coerce_to_i64(needle_tv).val;
        let sym = if prop == "indexOf" {
            "__RTS_FN_NS_COLLECTIONS_INDEX_OF_AUTO"
        } else {
            "__RTS_FN_NS_COLLECTIONS_LAST_INDEX_OF_AUTO"
        };
        let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(f, &[obj_h, needle]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(crate::codegen::lower::ctx::TypedVal::new(v, ValTy::I64));
    }
    if matches!(obj_tv.ty, ValTy::I64 | ValTy::Handle)
        && !is_proto_instance
        && prop == "concat"
        && call.args.len() == 1
        && call.args[0].spread.is_none()
    {
        use cranelift_codegen::ir::{InstBuilder, types as cl};
        let other_tv = lower_expr(ctx, &call.args[0].expr)?;
        let other_h = ctx.coerce_to_i64(other_tv).val;
        let f = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_CONCAT_AUTO",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[obj_h, other_h]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(crate::codegen::lower::ctx::TypedVal::new(v, ValTy::Handle));
    }
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

    // (#1078/#341) Metodo de prototype que retorna f64 inequivoco:
    // INVOKE_AUTO_TYPED(rk=1) invoca como `-> f64` (preserva o valor) e
    // reinterpreta os bits via bitcast. Sem isso o trampolim trunca o f64.
    if crate::codegen::lower::compile::program::proto_method_is_f64(prop) {
        let invoke_typed = ctx.get_extern(
            "__RTS_FN_RT_INVOKE_AUTO_TYPED",
            &[cl::I64, cl::I64, cl::I64, cl::I32],
            Some(cl::I64),
        )?;
        let rk_v = ctx.builder.ins().iconst(cl::I32, 1);
        let inst = ctx.builder.ins().call(invoke_typed, &[callee_val, obj_h, args_h, rk_v]);
        let v = ctx.builder.inst_results(inst)[0];
        let f = ctx.builder.ins().bitcast(
            cl::F64, cranelift_codegen::ir::MemFlags::new(), v,
        );
        return Ok(TypedVal::new(f, ValTy::F64));
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

    // (372) Callee eh var `f: () => number`? O invoke retorna i64 carregando
    // os BITS de um f64 (arrow `() => this.campoF64`). Reinterpreta o
    // resultado via bitcast pra que o valor seja o f64 correto, nao o inteiro
    // dos bits.
    let ret_is_f64 = matches!(callee_expr, Expr::Ident(id)
        if ctx.local_fn_ret_f64.contains(id.sym.as_str()));

    // Quando callee é Handle (var que recebeu fn handle de bind/REIFY/
    // new Function), despacha via __RTS_FN_GL_FUNCTION_CALL — esse path
    // entende bound_args, has_this_param, is_arrow, etc. Caso contrário
    // trata como fn pointer raw (call_indirect direto).
    if matches!(callee.ty, ValTy::Handle) {
        let tv = emit_function_handle_indirect_call(ctx, callee.val, call)?;
        if ret_is_f64 && matches!(tv.ty, ValTy::I64) {
            let f = ctx.builder.ins().bitcast(
                cl::F64,
                cranelift_codegen::ir::MemFlags::new(),
                tv.val,
            );
            return Ok(TypedVal::new(f, ValTy::F64));
        }
        return Ok(tv);
    }

    let callee_val = ctx.coerce_to_i64(callee).val;

    // (cross-runtime #84/#92 — promise resolvers via destructuring)
    // Quando callee eh ident de tipo desconhecido (ValTy::I64), pode ser
    // tanto fn_ptr raw quanto handle de Entry::Function (resolve/reject de
    // Promise.withResolvers, callback recebido como arg, etc). Usar
    // call_indirect direto num handle Function pula pra endereco invalido
    // (segfault).
    //
    // INVOKE_AUTO detecta em runtime: se callee eh handle Function valido,
    // extrai fn_ptr + bound_args + invoca via invoke_typed; senao trata
    // como fn_ptr raw. Custo: um lookup HandleTable + branch — irrelevante
    // vs call_indirect quando o engine eh thread-based.
    // Empacota args num Vec handle
    let vec_new = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
        &[],
        Some(cl::I64),
    )?;
    let vec_push = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        &[cl::I64, cl::I64],
        None,
    )?;
    let new_inst = ctx.builder.ins().call(vec_new, &[]);
    let args_vec = ctx.builder.inst_results(new_inst)[0];
    ctx.declare_gc_handle(args_vec);
    let vec_extend = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
        &[cl::I64, cl::I64],
        None,
    )?;
    for arg in &call.args {
        let tv = lower_expr(ctx, &arg.expr)?;
        if arg.spread.is_some() {
            // (cross-runtime #1067) Spread em indirect call: extend o args
            // vec com elementos do source vec/array.
            let src = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(vec_extend, &[args_vec, src]);
        } else {
            // (issue-pai invoke/param_kinds) F64 viaja como to_bits; invoke_typed
            // (via handle com param_kinds) le from_bits. Sem isto `apply(inc,2.5)`
            // truncava. Args int/handle como i64 cru (param_kind reflete o tipo).
            let v = match tv.ty {
                ValTy::F64 => ctx.builder.ins().bitcast(
                    cl::I64,
                    cranelift_codegen::ir::MemFlags::new(),
                    tv.val,
                ),
                _ => ctx.coerce_to_i64(tv).val,
            };
            ctx.builder.ins().call(vec_push, &[args_vec, v]);
        }
    }
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    // (issue-pai invoke/param_kinds, metade b) Callee declara retorno number
    // (`f: (n)=>number`): usa INVOKE_AUTO_AS_F64 que NORMALIZA o retorno p/
    // f64-bits qualquer que seja o callee (user fn f64-ret OU function
    // expression i64-ret). Resultado eh F64 — sem bits crus no template.
    if ret_is_f64 {
        let invoke_f = ctx.get_extern(
            "__RTS_FN_RT_INVOKE_AUTO_AS_F64",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(invoke_f, &[callee_val, zero, args_vec]);
        let raw = ctx.builder.inst_results(inst)[0];
        let f = ctx.builder.ins().bitcast(
            cl::F64,
            cranelift_codegen::ir::MemFlags::new(),
            raw,
        );
        return Ok(TypedVal::new(f, ValTy::F64));
    }
    let invoke = ctx.get_extern(
        "__RTS_FN_RT_INVOKE_AUTO",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(invoke, &[callee_val, zero, args_vec]);
    let v = ctx.builder.inst_results(inst)[0];
    // (cross-runtime followup #1067) INVOKE_AUTO retorna i64 ambiguo
    // (handle de string/object/Vec ou numero direto). Marca como
    // var_member_call_values para que console.log/template use
    // TPL_COERCE_AUTO/INSPECT (detecta handle em runtime).
    ctx.var_member_call_values.insert(v);
    Ok(TypedVal::new(v, ValTy::I64))
}

/// (#1281 curry N-nivel) Callee eh ele proprio uma chamada (`add3(1)(2)(3)`):
/// a subchamada retorna um handle de ARROW LIFTADA (i64-ABI), que le seus
/// params number via `fcvt_from_sint` (espera INTEIRO, nao bits-f64). As
/// capturas (bound_args) ja' chegam como inteiro (REIFY coerce_to_i64), entao
/// os args proprios DEVEM seguir a mesma convencao — senao o nivel final
/// recebe bits-f64 e o resultado corrompe (era `3.0000…013` em add3(1)(2)(3)).
/// Diferente de lower_indirect_call (que empaca F64 como bits p/ callees com
/// param_kinds=1), aqui empacotamos number como INTEIRO (coerce_to_i64). NB:
/// fracao trunca — mesma limitacao que as capturas ja' tem (issue-pai f64).
pub(super) fn lower_curry_call(ctx: &mut FnCtx, callee_expr: &Expr, call: &CallExpr) -> Result<TypedVal> {
    let callee = lower_expr(ctx, callee_expr)?;
    let callee_val = ctx.coerce_to_i64(callee).val;

    // (A+D — #1281) Decide a convencao de empacotamento dos args proprios do
    // curry: bits-f64 (callee e' arrow liftada TYPED com param number) vs i64
    // raw (callee e' fn_ptr i64 cru — function expression hoisted como `nested`,
    // que captura via global e retorna func_addr). Resolve a raiz da cadeia de
    // calls (`add3(1)(2)` -> `add3`) e consulta o registro do pass de lift.
    let root_is_typed = {
        let mut cur = callee_expr;
        loop {
            match cur {
                Expr::Call(c) => {
                    if let swc_ecma_ast::Callee::Expr(inner) = &c.callee {
                        cur = inner.as_ref();
                    } else {
                        break false;
                    }
                }
                Expr::Paren(p) => cur = p.expr.as_ref(),
                Expr::Ident(id) => {
                    break crate::codegen::lower::passes::this_arrow::fn_returns_typed_arrow(
                        id.sym.as_str(),
                    );
                }
                _ => break false,
            }
        }
    };

    let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let vec_push = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[cl::I64, cl::I64], None)?;
    let vec_extend = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM", &[cl::I64, cl::I64], None)?;
    let new_inst = ctx.builder.ins().call(vec_new, &[]);
    let args_vec = ctx.builder.inst_results(new_inst)[0];
    ctx.declare_gc_handle(args_vec);
    for arg in &call.args {
        let tv = lower_expr(ctx, &arg.expr)?;
        if arg.spread.is_some() {
            let src = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(vec_extend, &[args_vec, src]);
            continue;
        }
        // (A+C+D — #1281) Args NUMERICOS viajam como bits-f64 (bitcast); o
        // runtime faz from_bits quando o param_kind do callee e' 1 (arrow
        // liftada agora reificada TYPED com params number=f64). Como o callee
        // dinamico nao expoe kinds aqui, usamos o tipo do ARGUMENTO: F64 OU
        // literal numerico (incl. inteiro `2`, que o param number le como f64).
        // Handles/strings/idents-ambiguos seguem coerce_to_i64 (callee i64,
        // le raw — curry-i64/add3(1)(2)(3) byte-identico, pois nesse caso o
        // param e' i64/kind=0 e o arg inteiro casa).
        //
        // NB: add3(1)(2)(3) — os params (a,b,c) sao number, logo kind=1, e o
        // arg inteiro literal vai como bits-f64 -> from_bits casa. curry-i64
        // (adder com number) idem. Quando o param fosse i64 puro (sem number),
        // o arg inteiro literal cairia em coerce_to_i64; ver match abaixo.
        let arg_is_num_lit = matches!(
            &*arg.expr,
            Expr::Lit(swc_ecma_ast::Lit::Num(_))
        ) || matches!(
            &*arg.expr,
            Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus)
                && matches!(&*u.arg, Expr::Lit(swc_ecma_ast::Lit::Num(_)))
        );
        let v = if root_is_typed && (matches!(tv.ty, ValTy::F64) || arg_is_num_lit) {
            let f = ctx.coerce_to_f64(tv).val;
            ctx.builder.ins().bitcast(
                cl::I64,
                cranelift_codegen::ir::MemFlags::new(),
                f,
            )
        } else {
            ctx.coerce_to_i64(tv).val
        };
        ctx.builder.ins().call(vec_push, &[args_vec, v]);
    }
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let invoke = ctx.get_extern(
        "__RTS_FN_RT_INVOKE_AUTO",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(invoke, &[callee_val, zero, args_vec]);
    let v = ctx.builder.inst_results(inst)[0];
    // I64 ambiguo (handle do proximo nivel OU number final). var_member_call
    // p/ TPL_COERCE_AUTO no consumo. coerce_to_i64 (no-op) round-tripa handle.
    ctx.var_member_call_values.insert(v);
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
    let vec_extend_h = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
        &[cl::I64, cl::I64],
        None,
    )?;
    for a in &call.args {
        if a.spread.is_some() {
            // (cross-runtime #1067) Spread em call de handle Function: extend.
            let tv = lower_expr(ctx, &a.expr)?;
            let src = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(vec_extend_h, &[args_handle, src]);
            continue;
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
    // para return_kind=1 (F64) seria bits f64 — TPL_COERCE_AUTO em concat
    // detecta magnitude/sentinela. Marca como var_member_call_value
    // (cross-runtime #787).
    ctx.var_member_call_values.insert(v_i64);
    Ok(TypedVal::new(v_i64, ValTy::I64))
}
