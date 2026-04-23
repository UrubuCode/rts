//! User-defined function and module-level compilation.
//!
//! `compile_program` is the main entry point: it declares all user functions
//! first (so forward calls resolve), then emits each function body, then
//! emits `main` with module-level statements.

use std::collections::HashMap;

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_codegen::Context as ClContext;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;
use swc_ecma_ast::Stmt;

use crate::parser::ast::{FunctionDecl, Item, Program, Statement};

use super::ctx::{FnCtx, ValTy};
use super::stmt::lower_stmt;

/// Info about a user-defined function needed by callers.
#[derive(Debug, Clone)]
struct UserFn {
    id: cranelift_module::FuncId,
    params: Vec<ValTy>,
    ret: Option<ValTy>,
}

/// Compiles the full program: user functions + module-level statements as `main`.
pub fn compile_program(
    program: &Program,
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    // Collect function declarations
    let fn_decls: Vec<&FunctionDecl> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Function(f) = item { Some(f) } else { None }
        })
        .collect();

    // Phase 1: declare all user functions so forward calls resolve
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let info = declare_user_fn(module, fn_decl)?;
        // Register in extern cache under mangled key
        let mangled: &'static str =
            Box::leak(format!("__user_{}", fn_decl.name).into_boxed_str());
        extern_cache.insert(mangled, info.id);
        user_fns.insert(fn_decl.name.clone(), info);
    }

    // Phase 2: compile user function bodies
    for fn_decl in &fn_decls {
        let info = user_fns.get(&fn_decl.name).unwrap();
        compile_user_fn(module, extern_cache, data_counter, fn_decl, info)
            .with_context(|| format!("in function `{}`", fn_decl.name))?;
    }

    // Phase 3: collect module-level statements + globals
    // Globals are `let`/`const` at module scope; we handle them as locals
    // of `main` initialised in order. Full data-symbol globals come in a
    // later phase (A6); for now they live as `main`-local variables.
    let top_stmts: Vec<&Stmt> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Statement(Statement::Raw(raw)) = item {
                raw.stmt.as_ref()
            } else {
                None
            }
        })
        .collect();

    for item in &program.items {
        if let Item::Statement(Statement::Raw(raw)) = item {
            if raw.stmt.is_none() {
                warnings.push(format!(
                    "statement without parsed SWC node: `{}`",
                    raw.text.trim()
                ));
            }
        }
    }

    // Emit main
    compile_main(module, extern_cache, data_counter, &top_stmts, &mut warnings)
        .context("in top-level main")?;

    Ok(warnings)
}

// ── Declare user function ─────────────────────────────────────────────────

fn declare_user_fn(
    module: &mut ObjectModule,
    fn_decl: &FunctionDecl,
) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl);
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &ty in &params {
        sig.params.push(AbiParam::new(ty.cl_type()));
    }
    if let Some(rt) = ret {
        sig.returns.push(AbiParam::new(rt.cl_type()));
    }
    let id = module
        .declare_function(&fn_decl.name, Linkage::Local, &sig)
        .with_context(|| format!("failed to declare function `{}`", fn_decl.name))?;
    Ok(UserFn { id, params, ret })
}

fn fn_signature(fn_decl: &FunctionDecl) -> (Vec<ValTy>, Option<ValTy>) {
    let params: Vec<ValTy> = fn_decl
        .parameters
        .iter()
        .map(|p| {
            p.type_annotation
                .as_deref()
                .map(ValTy::from_annotation)
                .unwrap_or(ValTy::I64)
        })
        .collect();
    let ret = fn_decl
        .return_type
        .as_deref()
        .and_then(|r| {
            if r == "void" { None } else { Some(ValTy::from_annotation(r)) }
        });
    (params, ret)
}

// ── Compile user function body ────────────────────────────────────────────

fn compile_user_fn(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    fn_decl: &FunctionDecl,
    info: &UserFn,
) -> Result<()> {
    let mut ctx = ClContext::new();
    ctx.func.signature = {
        let mut sig = Signature::new(module.isa().default_call_conv());
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

        let mut fn_ctx = FnCtx::new(&mut builder, module, extern_cache, data_counter);

        // Bind parameters as locals
        for (i, param) in fn_decl.parameters.iter().enumerate() {
            let ty = param
                .type_annotation
                .as_deref()
                .map(ValTy::from_annotation)
                .unwrap_or(ValTy::I64);
            let block_param = fn_ctx.builder.block_params(entry)[i];
            fn_ctx.declare_local(&param.name, ty, block_param);
        }

        // Compile body statements
        let mut terminated = false;
        for stmt_raw in &fn_decl.body {
            if terminated { break; }
            let Statement::Raw(raw) = stmt_raw;
            if let Some(swc_stmt) = raw.stmt.as_ref() {
                terminated = lower_stmt(&mut fn_ctx, swc_stmt)?;
            }
        }

        // If we didn't hit a return, emit one
        if !terminated && !fn_ctx.builder.is_unreachable() {
            if info.ret.is_some() {
                let zero = fn_ctx.builder.ins().iconst(cl::I64, 0);
                fn_ctx.builder.ins().return_(&[zero]);
            } else {
                fn_ctx.builder.ins().return_(&[]);
            }
        }
        builder.finalize();
    }

    module
        .define_function(info.id, &mut ctx)
        .with_context(|| format!("failed to define function `{}`", fn_decl.name))?;
    Ok(())
}

// ── Compile main ──────────────────────────────────────────────────────────

fn compile_main(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    stmts: &[&Stmt],
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut sig = Signature::new(module.isa().default_call_conv());
    sig.returns.push(AbiParam::new(cl::I32));
    let main_id = module
        .declare_function("main", Linkage::Export, &sig)
        .context("failed to declare main")?;

    let mut ctx = ClContext::new();
    ctx.func.signature = sig;

    let mut fbx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fbx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut fn_ctx = FnCtx::new(&mut builder, module, extern_cache, data_counter);

        for stmt in stmts {
            match lower_stmt(&mut fn_ctx, stmt) {
                Ok(_) => {}
                Err(e) => {
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

    module
        .define_function(main_id, &mut ctx)
        .context("failed to define main")?;
    Ok(())
}
