//! Compilacao do entry point top-level (`__RTS_MAIN`) e do shim
//! `main` C-style que chama o entrypoint do runtime.
//!
//! Extraido de `func.rs`. Helpers de FnCtx / lower_stmt vivem em
//! seus modulos originais.

use std::collections::HashMap;

use anyhow::{Context, Result};
use cranelift_codegen::Context as ClContext;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::Stmt;

use super::super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::super::statements::lower_stmt;
use super::util::collect_var_decls;

const RUNTIME_MAIN_SYMBOL: &str = crate::abi::symbols::ENTRY_POINT;

pub(crate) fn compile_main(
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
    stmts: &[&Stmt],
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut sig = Signature::new(module.isa().default_call_conv());
    sig.returns.push(AbiParam::new(cl::I32));
    let runtime_main_id = module
        .declare_function(RUNTIME_MAIN_SYMBOL, Linkage::Local, &sig)
        .context("failed to declare runtime entrypoint __RTS_MAIN")?;

    let mut runtime_ctx = ClContext::new();
    runtime_ctx.func.signature = sig.clone();

    let mut fbx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut runtime_ctx.func, &mut fbx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

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
            true,
        );

        // (cross-runtime #47) Registra class hierarchy no runtime registry
        // pra suporte de `instanceof` cross-classes (ex: CustomTypeError
        // extends TypeError → err instanceof TypeError = true).
        {
            for (child_name, meta) in classes.iter() {
                if let Some(parent_name) = &meta.super_class {
                    let (cp, cl_) = fn_ctx.emit_str_literal(child_name.as_bytes())?;
                    let (pp, pl) = fn_ctx.emit_str_literal(parent_name.as_bytes())?;
                    let reg_fn = fn_ctx.get_extern(
                        "__RTS_FN_NS_GC_CLASS_REGISTER_PARENT",
                        &[cl::I64, cl::I64, cl::I64, cl::I64],
                        None,
                    )?;
                    fn_ctx.builder.ins().call(reg_fn, &[cp, cl_, pp, pl]);
                }
            }
        }

        // (#1114) Registra methods de classe (instance + getters/setters)
        // no class_method_registry para que `\"method\" in instance` retorne
        // true via OBJ_HAS.
        {
            for (cls_name, meta) in classes.iter() {
                let mut all_methods: Vec<String> = meta.methods.clone();
                for g in &meta.getters {
                    all_methods.push(g.clone());
                }
                for s in &meta.setters {
                    all_methods.push(s.clone());
                }
                if all_methods.is_empty() { continue; }
                let (cp, cl_len) = fn_ctx.emit_str_literal(cls_name.as_bytes())?;
                let reg_fn = fn_ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD",
                    &[cl::I64, cl::I64, cl::I64, cl::I64],
                    None,
                )?;
                for method_name in &all_methods {
                    let (mp, ml) = fn_ctx.emit_str_literal(method_name.as_bytes())?;
                    fn_ctx.builder.ins().call(reg_fn, &[cp, cl_len, mp, ml]);
                }
            }
        }

        // (cross-runtime closures) Registra a ABI (param_kinds + return_kind) de
        // cada user fn pelo seu endereço, p/ que INVOKE_AUTO/array-callback,
        // quando recebem o func_addr CRU de uma user fn capturada/passada como
        // valor, invoquem com a ABI correta (args number como bits f64) em vez
        // do fallback i64 que perdia args f64 (`next(25)`→`next(0)` em closures).
        {
            let reg_fn = fn_ctx.get_extern(
                "__RTS_FN_RT_REGISTER_FN_KINDS",
                &[cl::I64, cl::I64, cl::I64, cl::I32],
                None,
            )?;
            let zero = fn_ctx.builder.ins().iconst(cl::I64, 0);
            let mut names: Vec<&String> = user_fns.keys().collect();
            names.sort();
            for name in names {
                let mangled = format!("__user_{name}");
                if !fn_ctx.extern_cache.contains_key(mangled.as_str()) {
                    continue;
                }
                let abi = &user_fns[name];
                // So' registra fns com pelo menos um PARAM f64 (kind 1) — é o
                // caso que precisa de normalização de arg cru no fallback raw.
                // NÃO usa `rk==1` como gatilho: o ret ValTy é menos confiável
                // (uma fn `: string` pode aparecer como F64 por inferência do
                // corpo `"x" + s`), e registrá-la faria o fallback ler o retorno
                // string como f64 (xmm0) e devolver "" — regressão em
                // `Reflect.apply(greet, [str])`. Fns sem param f64 funcionam no
                // invoke_n e não precisam de entrada.
                let pks: Vec<u8> = abi
                    .params
                    .iter()
                    .map(|p| crate::codegen::lower::expressions::val_ty_to_kind(*p))
                    .collect();
                if !pks.iter().any(|&k| k == 1) {
                    continue;
                }
                let rk: u8 = abi
                    .ret
                    .map(crate::codegen::lower::expressions::val_ty_to_kind)
                    .unwrap_or(0);
                let addr =
                    crate::codegen::lower::expressions::emit_user_fn_addr(&mut fn_ctx, name)?
                        .val;
                // Blob de kinds num DATA SEGMENT estático (não GC) — evita
                // churn de strings GC no startup que deslocava o tick do
                // coletor e tornava outros testes flaky.
                let (kp, kl) = if pks.is_empty() {
                    (zero, zero)
                } else {
                    fn_ctx.emit_str_literal(&pks)?
                };
                let rk_v = fn_ctx.builder.ins().iconst(cl::I32, rk as i64);
                fn_ctx.builder.ins().call(reg_fn, &[addr, kp, kl, rk_v]);

                // (cross-runtime closures) Registra defaults LITERAIS (já no
                // encoding do kind) p/ aplicar em chamadas indiretas (`fn(...a)`
                // de trampoline/curry omite args com default). Marker i64::MIN
                // = sem default. So' literais numéricos/bool (default complexo
                // fica i64::MIN → pad-0 como antes).
                if let Some(lits) =
                    crate::codegen::lower::passes::args::default_args::fn_default_lits(name)
                {
                    let mut blob: Vec<u8> = Vec::with_capacity(pks.len() * 8);
                    for i in 0..pks.len() {
                        let enc: i64 = match lits.get(i).and_then(|o| *o) {
                            Some(v) => {
                                if pks[i] == 1 {
                                    f64::to_bits(v) as i64
                                } else {
                                    v as i64
                                }
                            }
                            None => i64::MIN, // NO_DEFAULT
                        };
                        blob.extend_from_slice(&enc.to_le_bytes());
                    }
                    let reg_def_fn = fn_ctx.get_extern(
                        "__RTS_FN_RT_REGISTER_FN_DEFAULTS",
                        &[cl::I64, cl::I64, cl::I64],
                        None,
                    )?;
                    let (dp, _dbytes) = fn_ctx.emit_str_literal(&blob)?;
                    let dlen = fn_ctx.builder.ins().iconst(cl::I64, pks.len() as i64);
                    fn_ctx.builder.ins().call(reg_def_fn, &[addr, dp, dlen]);
                }
            }
        }

        // (#301) Var hoisting top-level: declarar vars `var x` antes de
        // executar body, com valor 0 (proxy undefined). Globals existentes
        // ja' tem registro em `globals` map — pulamos.
        {
            let mut hoisted: Vec<String> = Vec::new();
            for stmt in stmts {
                collect_var_decls(stmt, &mut hoisted);
            }
            for name in &hoisted {
                if !fn_ctx.has_global(name) && fn_ctx.var_ty(name).is_none() {
                    let zero = fn_ctx.builder.ins().iconst(cl::I64, 0);
                    fn_ctx.declare_local_kind(name, ValTy::I64, zero, false, true);
                }
            }
        }

        for stmt in stmts {
            match lower_stmt(&mut fn_ctx, stmt) {
                Ok(_) => {}
                Err(e) => {
                    // Erros que sinalizam violação de contrato (abstract,
                    // readonly, private de outra classe) devem ser hard-fail
                    // — não fazem sentido como warning.
                    // (#383) `undefined variable`, `unknown namespace member`,
                    // `undeclared user function` tambem sao hard-fail: o
                    // codigo nao compilaria em qualquer outro typed compiler,
                    // e deixa-los como warning leva a segfault em runtime
                    // quando o slot e' lido como fn ptr.
                    let msg = format!("{e}");
                    let is_hard = msg.contains("abstract")
                        || msg.contains("readonly")
                        || msg.contains("private")
                        || msg.contains("protected")
                        || msg.contains("undefined variable")
                        || msg.contains("unknown namespace member")
                        || msg.contains("undeclared user function");
                    if is_hard {
                        return Err(e);
                    }
                    warnings.push(format!("codegen warning: {e}"));
                }
            }
        }

        let zero = fn_ctx.builder.ins().iconst(cl::I32, 0);
        if !fn_ctx.builder.is_unreachable() {
            fn_ctx.builder.ins().return_(&[zero]);
        }

        builder.finalize();
    }

    if crate::codegen::ir_dump_enabled() {
        let file = crate::codegen::ir_source_file();
        let loc = if file.is_empty() {
            "top-level".to_string()
        } else {
            format!("{} top-level", file)
        };
        eprintln!("--- __RTS_MAIN [{}] IR ---\n{}", loc, runtime_ctx.func.display());
    }

    {
        use cranelift_codegen::control::ControlPlane;
        let mut ctrl = ControlPlane::default();
        let gc_debug = std::env::var("RTS_GC_DEBUG").is_ok();
        match runtime_ctx.compile(module.isa(), &mut ctrl) {
            Ok(compiled) => {
                let raw_maps = compiled.buffer.user_stack_maps();
                if gc_debug {
                    eprintln!("[gc] fn `__RTS_MAIN` — {} raw stack map entries", raw_maps.len());
                }
                let maps: Vec<(u32, Vec<u32>)> = raw_maps
                    .iter()
                    .filter_map(|(ret_offset, _, map)| {
                        let offsets: Vec<u32> = map.entries().map(|(_, sp_off)| sp_off).collect();
                        if offsets.is_empty() { None } else { Some((*ret_offset, offsets)) }
                    })
                    .collect();
                if !maps.is_empty() {
                    crate::namespaces::gc::stack_map_registry::push_pending(runtime_main_id.as_u32(), maps);
                }
            }
            Err(e) => {
                if gc_debug {
                    eprintln!("[gc] fn `__RTS_MAIN` — pre-compile failed: {}", e.inner);
                }
            }
        }
    }

    module
        .define_function(runtime_main_id, &mut runtime_ctx)
        .context("failed to define runtime entrypoint __RTS_MAIN")?;

    compile_main_entry_shim(module, runtime_main_id, &sig)
        .context("failed to define C entrypoint shim `main`")?;

    Ok(())
}

fn compile_main_entry_shim(
    module: &mut dyn Module,
    runtime_main_id: cranelift_module::FuncId,
    sig: &Signature,
) -> Result<()> {
    let entry_main_id = module
        .declare_function("main", Linkage::Export, sig)
        .context("failed to declare exported entrypoint `main`")?;

    let mut ctx = ClContext::new();
    ctx.func.signature = sig.clone();

    let mut fbx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fbx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let runtime_ref = module.declare_func_in_func(runtime_main_id, builder.func);
        let call = builder.ins().call(runtime_ref, &[]);
        let result = builder
            .inst_results(call)
            .first()
            .copied()
            .unwrap_or_else(|| builder.ins().iconst(cl::I32, 0));
        // (AOT event loop) Drena microtasks/timers/promises pendentes após
        // __RTS_MAIN — espelha o que o pipeline JIT faz host-side. Sem isto, o
        // binário AOT saía sem rodar await/.then/queueMicrotask/setTimeout.
        // `run_event_loop` (rts-std) é `extern "C" fn()`; resolve por link.
        let drain_sig = module.make_signature();
        if let Ok(drain_id) =
            module.declare_function("__RTS_FN_RT_RUN_EVENT_LOOP", Linkage::Import, &drain_sig)
        {
            let drain_ref = module.declare_func_in_func(drain_id, builder.func);
            builder.ins().call(drain_ref, &[]);
        }
        // (AOT uncaught) Após o event loop, reporta erro pendente (throw sync
        // não-capturado no top-level OU rejection async) no stderr e força exit
        // != 0. Espelha o que o pipeline JIT faz host-side. Sem isto o binário
        // AOT saía 0 e silencioso num uncaught.
        let mut rep_sig = module.make_signature();
        rep_sig.returns.push(AbiParam::new(cl::I32));
        let exit_code = if let Ok(rep_id) =
            module.declare_function("__RTS_FN_RT_REPORT_UNCAUGHT", Linkage::Import, &rep_sig)
        {
            let rep_ref = module.declare_func_in_func(rep_id, builder.func);
            let rc = builder.ins().call(rep_ref, &[]);
            let rep = builder.inst_results(rc)[0];
            let zero = builder.ins().iconst(cl::I32, 0);
            let had_err = builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, rep, zero);
            // erro uncaught → exit code do report (1); senão o code do main.
            builder.ins().select(had_err, rep, result)
        } else {
            result
        };
        builder.ins().return_(&[exit_code]);
        builder.finalize();
    }

    module
        .define_function(entry_main_id, &mut ctx)
        .context("failed to define exported entrypoint `main`")?;

    Ok(())
}
