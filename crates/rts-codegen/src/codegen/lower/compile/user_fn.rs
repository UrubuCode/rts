//! Compilacao individual de uma user fn — orquestra HIR → MIR → Cranelift
//! com fallback automatico pra codegen AST.
//!
//! Extraido de `func.rs`. Helpers `user_call_conv`, `collect_var_decls`,
//! `try_compile_via_mir` etc. moram em `func` ou `compile/mir_route`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use cranelift_codegen::Context as ClContext;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::Module;

use crate::parser::ast::{FunctionDecl, Statement};

use super::super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::super::statements::lower_stmt;
use super::class::class_init_name;
use super::mir_route::try_compile_via_mir;
use super::program::UserFn;
use super::util::{collect_var_decls, user_call_conv};

/// (issue-pai invoke/param_kinds, metade b) Anotacao textual eh fn-type
/// retornando `number`? Ex: `(n: number) => number`. Marca o param p/ que `f(n)`
/// normalize o retorno via INVOKE_AUTO_AS_F64.
fn ann_is_fn_returning_number(ann: &str) -> bool {
    let t = ann.trim();
    if let Some(idx) = t.rfind("=>") {
        let ret = t[idx + 2..].trim().trim_end_matches(|c| c == ';' || c == ' ');
        return ret == "number";
    }
    false
}

pub(crate) fn compile_user_fn(
    module: &mut dyn Module,
    extern_cache: &mut HashMap<String, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassMeta>,
    global_class_ty: &HashMap<String, String>,
    global_obj_field_types: &HashMap<String, HashMap<String, ValTy>>,
    global_nested_obj_field_types: &HashMap<(String, String), HashMap<String, ValTy>>,
    fn_class_returns: &HashMap<String, String>,
    node_import_map: &HashMap<String, String>,
    local_alias_map: &HashMap<String, String>,
    fn_decl: &FunctionDecl,
    info: &UserFn,
    current_class: Option<String>,
    address_taken: bool,
) -> Result<Vec<String>> {
    let warnings: Vec<String> = Vec::new();

    // (etapa 3.19/3.25) Routing híbrido MIR ↔ AST.
    //
    // Caminho MIR (HIR → MIR → optimize → mir_codegen → Cranelift) tenta
    // assumir cada user fn cujo gate aceita (synthetic/async/types
    // whitelisted/etc.); em qualquer falha (Trap, signature mismatch,
    // had_placeholders, lower error) cai automaticamente no AST.
    //
    // RTS_USE_MIR controla o opt-out:
    //   - unset / "1" / "on" / "all" → MIR ON (default, etapa 3.25)
    //   - "0" / "off" / "none"        → MIR OFF (AST only)
    //   - "fn1,fn2,fn3"               → MIR só pras fns listadas
    //
    // Gate testado: zero regressão na suite TS (621/632 com MIR ON ==
    // 621/632 com MIR OFF, etapa 3.24). Reativar address-taken e
    // CallExtern eh trabalho futuro auditado por namespace.
    let mir_allowed = match std::env::var("RTS_USE_MIR") {
        Err(_) => true,
        Ok(spec) => {
            let s = spec.trim();
            if s.is_empty() || s.eq_ignore_ascii_case("on") || s == "1"
                || s.eq_ignore_ascii_case("all")
            {
                true
            } else if s == "0" || s.eq_ignore_ascii_case("off")
                || s.eq_ignore_ascii_case("none")
            {
                false
            } else {
                // Lista por nome — só ativa quando match.
                s.split(',').any(|n| n.trim() == fn_decl.name)
            }
        }
    };
    if mir_allowed && try_compile_via_mir(module, fn_decl, info, address_taken)? {
        return Ok(warnings);
    }

    let mut warnings = warnings;
    let mut ctx = ClContext::new();
    let call_conv = user_call_conv(module, &fn_decl.name, address_taken);
    ctx.func.signature = {
        let mut sig = Signature::new(call_conv);
        for &ty in &info.params {
            sig.params.push(AbiParam::new(ty.cl_type()));
        }
        if let Some(rt) = info.ret {
            sig.returns.push(AbiParam::new(rt.cl_type()));
        }
        sig
    };

    let mut fbx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fbx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        // Force layout insertion para body vazio nao crashar Cranelift.
        // Sem nenhum opcode/terminator, builder.finalize() pode deixar
        // o entry block fora do layout, e remove_constant_phis explode
        // em "entry block unknown".
        builder.func.layout.append_block(entry);

        let mut fn_ctx = FnCtx::new(
            &mut builder,
            module,
            extern_cache,
            data_counter,
            globals,
            user_fns,
            classes,
            global_class_ty,
            global_obj_field_types,
            global_nested_obj_field_types,
            fn_class_returns,
            node_import_map,
            local_alias_map,
            false,
        );
        fn_ctx.return_ty = info.ret;
        fn_ctx.is_tail_conv = call_conv == CallConv::Tail;
        fn_ctx.current_class = current_class.clone();
        fn_ctx.current_fn_name = fn_decl.name.clone();
        fn_ctx.current_file = fn_decl.span.file
            .and_then(rts_diagnostics::source_store::path_of)
            .map(|p| {
                // Remove Windows UNC prefix \\?\ for readability.
                let s = p.display().to_string();
                s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
            })
            .unwrap_or_default();
        // Detecta se a função é um constructor de classe pelo mangled name.
        // Usado pra permitir assign em readonly fields.
        fn_ctx.current_is_ctor = current_class
            .as_ref()
            .map(|c| fn_decl.name == class_init_name(c))
            .unwrap_or(false);
        // Reset por fn — \`super_already_called\` rastreia chamadas dentro
        // do constructor corrente. Sem reset, multiplos constructors no
        // mesmo programa compartilhariam a flag.
        fn_ctx.super_already_called = false;
        // Em metodos/constructors, o param `this` e instancia da classe
        // dona — populamos local_class_ty pra que `this.field`/dispatch
        // tipicos funcionem (e overload em `this.x + ...`).
        if let Some(cls) = current_class.as_deref() {
            fn_ctx
                .local_class_ty
                .insert("this".to_string(), cls.to_string());
        }
        // Parametros tipados como classe registrada → trackear.
        for p in &fn_decl.parameters {
            if let Some(ann) = p.type_annotation.as_deref() {
                let ann = ann.trim();
                if classes.contains_key(ann) {
                    fn_ctx
                        .local_class_ty
                        .insert(p.name.clone(), ann.to_string());
                }
                // (cross-runtime) param `: Map<...>`/`Set<...>`/Weak* marca
                // local_map_vars p/ `.size` rotear ao UNIVERSAL_LENGTH. Sem
                // isso `function f(m: Map<...>){ return m.size }` dava 0.
                let base = ann.split('<').next().unwrap_or(ann).trim();
                if matches!(
                    base,
                    "Map" | "Set" | "WeakMap" | "WeakSet" | "ReadonlyMap" | "ReadonlySet"
                ) {
                    fn_ctx.local_map_vars.insert(p.name.clone());
                }
                // (cross-runtime) param `: string` marca local_string_vars p/
                // `for (const ch of s)` iterar os chars. Sem isso, for-of sobre
                // param string nao iterava (so' var string top-level/local).
                if base == "string" {
                    fn_ctx.local_string_vars.insert(p.name.clone());
                }
                // (issue-pai invoke/param_kinds, metade b) param fn-type
                // retornando number (`f: (n: number) => number`): marca
                // local_fn_ret_f64 p/ que `f(n)` use INVOKE_AUTO_AS_F64 e
                // normalize o retorno como f64 (user fn f64-ret OU function
                // expression i64-ret). Sem isto, HOF retornava bits crus.
                if ann_is_fn_returning_number(ann) {
                    fn_ctx.local_fn_ret_f64.insert(p.name.clone());
                }
            }
        }

        // Bind parameters as locals.
        // Caso especial: param `__rts_spawn_arg_f64` (gerado pelo lifter
        // de thread.spawn quando worker pede `number`) — block_param
        // chega como i64 mas ja contem o bit pattern de um f64. Bind
        // local como F64 via bitcast em vez de fcvt (que perderia o
        // valor por interpretar bits como inteiro).
        for (i, param) in fn_decl.parameters.iter().enumerate() {
            let block_param = fn_ctx.builder.block_params(entry)[i];
            if param.name == "__rts_spawn_arg_f64" {
                let f = fn_ctx.builder.ins().bitcast(
                    cranelift_codegen::ir::types::F64,
                    cranelift_codegen::ir::MemFlags::new(),
                    block_param,
                );
                fn_ctx.declare_local(&param.name, ValTy::F64, f);
                continue;
            }
            let ty = param
                .type_annotation
                .as_deref()
                .map(ValTy::from_annotation)
                .unwrap_or(ValTy::I64);
            fn_ctx.declare_local(&param.name, ty, block_param);

            // Hoisted/lifted arrows (geradas de replace/map/forEach/reduce callbacks)
            // recebem parametros como handles de string ou Vec (u64 cast para i64).
            // Marcar como ambiguos para que string concat use TPL_COERCE_AUTO em vez
            // de STRING_FROM_I64 (que formataria o handle como numero decimal).
            // (#776) Para __lifted_arr_method_*, marca apenas o primeiro param
            // (val) como ambiguo. Params 2/3 (idx, array_handle) tem tipos bem
            // definidos: idx eh i64 puro, array eh handle Vec — coerce auto so'
            // faz sentido pra val. Sem essa restricao, `idx=0` no template literal
            // viraria "null" via TPL_COERCE (que trata 0 como sentinel null).
            // (#776) Para fns liftadas de array methods, marcamos slots
            // de handle como ambiguos para TPL_COERCE_AUTO:
            // - `__lifted_arr_method_<N>` (forEach/map/filter/find/etc):
            //   slot 0 (val) e slot 2 (array handle) ambiguos. Slot 1
            //   (idx) eh i64 puro — NAO marcado (idx=0 nao deve virar
            //   "null").
            // - `__lifted_arr_method_reduce_<N>` (#254): callback
            //   `(acc, val)` — ambos slots ambiguos (string reduce
            //   funcionar).
            let is_reduce_lifted = fn_decl.name.starts_with("__lifted_arr_method_reduce_");
            let is_arr_method_lifted = fn_decl.name.starts_with("__lifted_arr_method_")
                && !is_reduce_lifted;
            // (#195) `__lifted_cap_N`: os primeiros K params sao CAPTURAS (vars
            // do escopo enclosing passadas via bound_args). O K real vem de
            // LIFTED_CAPTURES. Capturas e o `val` (slot K) sao ambiguos; alem
            // disso, capturas que sejam arrays precisam que `.length`/index/
            // array methods despachem por tipo de Entry em runtime -> marca em
            // local_array_vars + ambiguo. Sem isso, `keys.length` (captura
            // array) caia em MAP_GET("length") -> 0.
            let cap_count = if fn_decl.name.starts_with("__lifted_cap_") {
                crate::codegen::lower::passes::parallelism::LIFTED_CAPTURES
                    .with(|c| c.borrow().get(&fn_decl.name).map(|v| v.len()))
                    .unwrap_or(0)
            } else {
                0
            };
            let is_cap_lifted = cap_count > 0;
            let is_cap_reduce = fn_decl.name.starts_with("__lifted_cap_reduce_");
            // Para __lifted_cap_: capturas (i<K) sempre ambiguas. Apos as
            // capturas, marca os slots de payload ambiguos: reduce -> acc(K) +
            // val(K+1); map/filter/forEach -> val(K) e array(K+2), nao idx(K+1).
            let cap_payload_ambiguous = is_cap_lifted && i >= cap_count && {
                let p = i - cap_count;
                if is_cap_reduce { p == 0 || p == 1 } else { p == 0 || p == 2 }
            };
            if ty == ValTy::I64 && (
                fn_decl.name.starts_with("__hoisted_arrow_")
                || fn_decl.name.starts_with("__lifted_arrow_")
                || (is_arr_method_lifted && (i == 0 || i == 2))
                || (is_reduce_lifted && (i == 0 || i == 1))
                || (is_cap_lifted && i < cap_count)
                || cap_payload_ambiguous
            ) {
                fn_ctx.var_member_call_values.insert(block_param);
                // (#862) Tambem marca pelo nome para que reads em blocks
                // sucessores propaguem ambiguity — use_var em block diferente
                // gera SSA Value novo que nao esta em var_member_call_values.
                fn_ctx.local_ambiguous_vars.insert(param.name.clone());
            }
            // (cross-runtime #1130) Param tipado `any`/`unknown`/sem anotacao
            // pode receber handle (Map/Vec/String/Function/instance) ou
            // numero. Marca como ambiguo pra que `typeof v` despache runtime
            // helper que inspeciona Entry em vez de assumir "number".
            // Brand check pattern `typeof v === "object" && #x in v` depende
            // disso.
            if ty == ValTy::I64 {
                let ann = param.type_annotation.as_deref().map(str::trim);
                let is_ambiguous_ann = match ann {
                    None => true,
                    Some(a) => matches!(a, "any" | "unknown" | "object" | "Object"),
                };
                if is_ambiguous_ann {
                    fn_ctx.var_member_call_values.insert(block_param);
                    // (#862) Idem — propagar por nome via local_ambiguous_vars.
                    fn_ctx.local_ambiguous_vars.insert(param.name.clone());
                }
            }

            // (#592) Param tipado `Cls[]` registra local_array_class_ty
            // pra que arr[i].field saiba o tipo do field na classe.
            if let Some(ann) = param.type_annotation.as_deref() {
                // ValTy::from_annotation aceita string; tentamos casar
                // padrao "Cls[]" simples por substring (sem TS AST aqui).
                if let Some(stripped) = ann.strip_suffix("[]") {
                    let cn = stripped.trim();
                    if fn_ctx.classes.contains_key(cn) {
                        fn_ctx.local_array_class_ty.insert(
                            param.name.clone(),
                            cn.to_string(),
                        );
                    }
                }
            }
        }

        // (#450) `arguments` em fn nao-arrow: detecta uso no body e injeta
        // bind `arguments` como handle Vec contendo todos os parametros
        // passados (na aridade declarada). Aridade dinamica (variadic) nao
        // existe em RTS — todos args sao formais. Para fn arrow, nao injeta
        // (arrows herdam arguments do enclosing).
        let body_uses_arguments = fn_decl
            .body
            .iter()
            .any(|s| {
                let Statement::Raw(r) = s;
                r.text.contains("arguments")
            });
        let is_arrow = fn_decl.name.starts_with("__hoisted_arrow_")
            || fn_decl.name.starts_with("__lifted_arrow_");
        if body_uses_arguments && !is_arrow {
            // Aloca Vec<i64> com cada param (coerced a i64) — empacota todos
            // como i64 raw. \`arguments.length\` retorna handle_len, suficiente
            // pra caso comum. \`arguments[i]\` ainda nao tipado (caveat).
            let vec_new = fn_ctx
                .get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
            let inst = fn_ctx.builder.ins().call(vec_new, &[]);
            let args_h = fn_ctx.builder.inst_results(inst)[0];
            let vec_push = fn_ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            for param in fn_decl.parameters.iter() {
                if param.name == "__rts_spawn_arg_f64" {
                    continue;
                }
                if let Some(local) = fn_ctx.read_local(&param.name) {
                    let v = fn_ctx.coerce_to_i64(local).val;
                    fn_ctx.builder.ins().call(vec_push, &[args_h, v]);
                }
            }
            fn_ctx.declare_local("arguments", ValTy::Handle, args_h);
        }

        // Auto-instrumentação de stack trace: emite push_frame no entry.
        // Pop é emitido antes de cada `return_` (via emit_trace_pop em
        // lower_return_stmt) e antes do implicit return abaixo. Funções
        // sintéticas (hoisted arrows, async inner) são excluídas do trace
        // para reduzir ruído — só fns com nome user-visible são rastreadas.
        let fn_line = fn_decl.span.start.line as u32;
        let fn_col = fn_decl.span.start.column as u32;
        let is_synthetic = fn_decl.name.starts_with("__hoisted_arrow_")
            || fn_decl.name.starts_with("__lifted_arrow_")
            || fn_decl.name.starts_with("__async_inner_")
            || fn_decl.name.starts_with("__rts_");
        if !is_synthetic && !fn_ctx.current_file.is_empty() {
            fn_ctx.emit_trace_push(fn_line, fn_col)?;
        }

        // (#301) Var hoisting: coletar todos os nomes `var x` no body
        // (incluindo nested em if/for/while/try mas ignorando function/
        // arrow/class boundaries) e pre-declarar como I64=0. Isso
        // permite `console.log(x); var x = 5;` retornar 0 (proxy de
        // undefined) em vez de "undefined variable" erro.
        {
            let mut hoisted: Vec<String> = Vec::new();
            for stmt_raw in fn_decl.body.iter() {
                let Statement::Raw(raw) = stmt_raw;
                if let Some(stmt) = raw.stmt.as_ref() {
                    collect_var_decls(stmt, &mut hoisted);
                }
            }
            for name in &hoisted {
                if fn_ctx.var_ty(name).is_none() {
                    let zero = fn_ctx.builder.ins().iconst(cl::I64, 0);
                    fn_ctx.declare_local_kind(name, ValTy::I64, zero, false, true);
                }
            }
        }

        // Compile body statements.
        let mut terminated = false;
        let mut iter = fn_decl.body.iter();
        while let Some(stmt_raw) = iter.next() {
            if terminated {
                break;
            }
            let Statement::Raw(raw) = stmt_raw;
            if let Some(swc_stmt) = raw.stmt.as_ref() {
                terminated = lower_stmt(&mut fn_ctx, swc_stmt)?;
                // #205 — emite warning quando ha statements depois de
                // um terminal (return/throw/break/continue) no body
                // top-level da fn. Ignora Statement::Raw sem stmt
                // (placeholders sinteticos do lifter).
                if terminated {
                    if let Some(next) = iter.clone().find(|s| {
                        let Statement::Raw(r) = s;
                        r.stmt.as_ref().map(|st| !matches!(st, swc_ecma_ast::Stmt::Empty(_))).unwrap_or(false)
                    }) {
                        let Statement::Raw(_) = next;
                        let kind = match swc_stmt {
                            swc_ecma_ast::Stmt::Return(_) => "return",
                            swc_ecma_ast::Stmt::Throw(_) => "throw",
                            swc_ecma_ast::Stmt::Break(_) => "break",
                            swc_ecma_ast::Stmt::Continue(_) => "continue",
                            _ => "terminal statement",
                        };
                        fn_ctx.warnings.push(format!(
                            "warning: unreachable code after `{}`",
                            kind
                        ));
                    }
                }
            }
        }

        // If we did not hit a return, emit one. Body vazio: o entry
        // block precisa ter terminator obrigatorio para Cranelift.
        if !terminated && !fn_ctx.builder.is_unreachable() {
            fn_ctx.emit_trace_pop()?;
            if let Some(rt) = info.ret {
                let zero = match rt {
                    ValTy::F64 => fn_ctx.builder.ins().f64const(0.0),
                    ValTy::I32 => fn_ctx.builder.ins().iconst(cl::I32, 0),
                    // JS spec: implicit return = undefined (i64::MIN+2 sentinel).
                    _ => fn_ctx.builder.ins().iconst(cl::I64, i64::MIN + 2),
                };
                fn_ctx.builder.ins().return_(&[zero]);
            } else {
                fn_ctx.builder.ins().return_(&[]);
            }
        }

        // Drena warnings emitidos durante o lower (#205 unreachable code).
        // Prefixa com nome da fn para diagnostico util.
        for w in fn_ctx.warnings.drain(..) {
            warnings.push(format!("in `{}`: {}", fn_decl.name, w));
        }

        builder.finalize();
    }

    if crate::codegen::ir_dump_enabled() {
        let file = crate::codegen::ir_source_file();
        let loc = if file.is_empty() {
            format!("line {}:{}", fn_decl.span.start.line, fn_decl.span.start.column)
        } else {
            format!("{}:{}:{}", file, fn_decl.span.start.line, fn_decl.span.start.column)
        };
        eprintln!("--- {} [{}] IR ---\n{}", fn_decl.name, loc, ctx.func.display());
    }

    // Pre-compile to capture GC stack maps BEFORE define_function clears the context.
    // JITModule::define_function_with_control_plane calls ctx.clear() internally, so
    // ctx.compiled_code() is always None after define_function. We compile once here
    // just to read the stack maps, then define_function recompiles (double compilation).
    {
        use cranelift_codegen::control::ControlPlane;
        let mut ctrl = ControlPlane::default();
        let gc_debug = std::env::var("RTS_GC_DEBUG").is_ok();
        match ctx.compile(module.isa(), &mut ctrl) {
            Ok(compiled) => {
                let raw_maps = compiled.buffer.user_stack_maps();
                if gc_debug {
                    eprintln!("[gc] fn `{}` — {} raw stack map entries", fn_decl.name, raw_maps.len());
                }
                let maps: Vec<(u32, Vec<u32>)> = raw_maps
                    .iter()
                    .filter_map(|(ret_offset, _, map)| {
                        let offsets: Vec<u32> = map.entries().map(|(_, sp_off)| sp_off).collect();
                        if gc_debug {
                            eprintln!("[gc]   safepoint offset={ret_offset} offsets={offsets:?}");
                        }
                        if offsets.is_empty() { None } else { Some((*ret_offset, offsets)) }
                    })
                    .collect();
                if !maps.is_empty() {
                    crate::namespaces::gc::stack_map_registry::push_pending(info.id.as_u32(), maps);
                }
            }
            Err(e) => {
                if gc_debug {
                    eprintln!("[gc] fn `{}` — pre-compile failed: {}", fn_decl.name, e.inner);
                }
            }
        }
    }

    module
        .define_function(info.id, &mut ctx)
        .with_context(|| format!("failed to define function `{}`", fn_decl.name))?;

    Ok(warnings)
}
