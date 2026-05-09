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
        builder.ins().return_(&[result]);
        builder.finalize();
    }

    module
        .define_function(entry_main_id, &mut ctx)
        .context("failed to define exported entrypoint `main`")?;

    Ok(())
}
