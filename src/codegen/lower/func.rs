//! User-defined function and module-level compilation.
//!
//! `compile_program` declares all user functions first (for forward calls),
//! lowers bodies, then lowers top-level statements into `__RTS_MAIN`.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use cranelift_codegen::Context as ClContext;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use swc_ecma_ast::{Decl, Expr, Lit, Pat, Stmt, TsType, TsTypeRef};

use crate::parser::ast::{
    ClassDecl, ClassMember, ConstructorDecl, FunctionDecl, Item, MethodDecl, Parameter, Program,
    PropertyDecl, Statement,
};

use super::ctx::{ClassField, ClassInfo, ClassMethod, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::stmt::lower_stmt;

const RUNTIME_MAIN_SYMBOL: &str = "__RTS_MAIN";

/// Width of every object-buffer slot. Fields are laid out at `idx * 8`,
/// regardless of the underlying primitive type. Keeps alignment simple and
/// lets the handle point at a single contiguous `Vec<u8>`.
const SLOT_SIZE: i32 = 8;

/// Info about a user-defined function needed by callers.
#[derive(Debug, Clone)]
struct UserFn {
    id: cranelift_module::FuncId,
    params: Vec<ValTy>,
    ret: Option<ValTy>,
}

/// Compiles the full program: user functions + top-level `main`.
pub fn compile_program(
    program: &Program,
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    let globals = collect_module_globals(program, module)?;

    // Collect function and class declarations.
    let fn_decls: Vec<&FunctionDecl> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Function(f) = item {
                Some(f)
            } else {
                None
            }
        })
        .collect();

    let class_decls: Vec<&ClassDecl> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Class(c) = item {
                Some(c)
            } else {
                None
            }
        })
        .collect();

    // Phase 1a: declare all user functions so forward calls resolve.
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let info = declare_user_fn(module, fn_decl)?;
        let mangled: &'static str = Box::leak(format!("__user_{}", fn_decl.name).into_boxed_str());
        extern_cache.insert(mangled, info.id);
        user_fns.insert(fn_decl.name.clone(), info);
    }

    // Phase 1b: compute class layouts and declare ctor/method symbols.
    let mut classes: HashMap<String, ClassInfo> = HashMap::new();
    let mut class_user_fns: HashMap<&'static str, UserFn> = HashMap::new();
    for class_decl in &class_decls {
        let (info, declared) = declare_class(module, class_decl)?;
        for (sym, user_fn) in declared {
            extern_cache.insert(sym, user_fn.id);
            class_user_fns.insert(sym, user_fn);
        }
        classes.insert(class_decl.name.clone(), info);
    }

    let user_fn_abis: HashMap<String, UserFnAbi> = user_fns
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                UserFnAbi {
                    params: info.params.clone(),
                    ret: info.ret,
                },
            )
        })
        .collect();

    // Phase 2a: compile user function bodies.
    for fn_decl in &fn_decls {
        let info = user_fns
            .get(&fn_decl.name)
            .ok_or_else(|| anyhow!("missing user function metadata for `{}`", fn_decl.name))?;
        compile_user_fn(
            module,
            extern_cache,
            data_counter,
            &globals,
            &user_fn_abis,
            &classes,
            fn_decl,
            info,
        )
        .with_context(|| format!("in function `{}`", fn_decl.name))?;
    }

    // Phase 2b: compile ctor and method bodies for each class.
    for class_decl in &class_decls {
        compile_class_bodies(
            module,
            extern_cache,
            data_counter,
            &globals,
            &user_fn_abis,
            &classes,
            &class_user_fns,
            class_decl,
        )
        .with_context(|| format!("in class `{}`", class_decl.name))?;
    }

    // Phase 3: collect top-level statements.
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

    // Phase 4: emit runtime entrypoint + exported C `main` shim.
    compile_main(
        module,
        extern_cache,
        data_counter,
        &globals,
        &user_fn_abis,
        &classes,
        &top_stmts,
        &mut warnings,
    )
    .context("in top-level runtime entry")?;

    Ok(warnings)
}

fn collect_module_globals(
    program: &Program,
    module: &mut ObjectModule,
) -> Result<HashMap<String, GlobalVar>> {
    let mut globals = HashMap::<String, GlobalVar>::new();
    let mut counter = 0usize;

    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else {
            continue;
        };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else {
            continue;
        };

        for decl in &var_decl.decls {
            let name = match &decl.name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => {
                    return Err(anyhow!(
                        "unsupported top-level binding pattern in global decl"
                    ));
                }
            };

            if globals.contains_key(&name) {
                continue;
            }

            let ann_ty = match &decl.name {
                Pat::Ident(id) => id
                    .type_ann
                    .as_ref()
                    .and_then(|ann| ts_type_to_val_ty(&ann.type_ann)),
                _ => None,
            };
            let ty = ann_ty.unwrap_or_else(|| infer_expr_ty(decl.init.as_deref()));

            let symbol = format!("__rts_global_{}_{}", sanitize_symbol(&name), counter);
            counter += 1;
            let data_id = module
                .declare_data(&symbol, Linkage::Local, true, false)
                .with_context(|| format!("failed to declare module global `{name}`"))?;

            let size = match ty {
                ValTy::I32 => 4,
                _ => 8,
            };
            let mut desc = DataDescription::new();
            desc.define(vec![0u8; size].into_boxed_slice());
            module
                .define_data(data_id, &desc)
                .with_context(|| format!("failed to define module global `{name}`"))?;

            globals.insert(name, GlobalVar { data_id, ty });
        }
    }

    Ok(globals)
}

fn sanitize_symbol(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "global".to_string()
    } else {
        out
    }
}

fn infer_expr_ty(expr: Option<&Expr>) -> ValTy {
    let Some(expr) = expr else {
        return ValTy::I64;
    };

    match expr {
        Expr::Lit(Lit::Num(n)) => {
            let v = n.value;
            if v.fract() == 0.0 && v.is_finite() && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                ValTy::I32
            } else if v.fract() == 0.0 && v.is_finite() {
                ValTy::I64
            } else {
                ValTy::F64
            }
        }
        Expr::Lit(Lit::Bool(_)) => ValTy::Bool,
        Expr::Lit(Lit::Str(_)) => ValTy::Handle,
        _ => ValTy::I64,
    }
}

fn ts_type_to_val_ty(ty: &TsType) -> Option<ValTy> {
    use swc_ecma_ast::TsKeywordTypeKind;

    if let TsType::TsKeywordType(kw) = ty {
        return Some(match kw.kind {
            TsKeywordTypeKind::TsNumberKeyword => ValTy::I32,
            TsKeywordTypeKind::TsBooleanKeyword => ValTy::Bool,
            TsKeywordTypeKind::TsStringKeyword => ValTy::Handle,
            TsKeywordTypeKind::TsVoidKeyword => ValTy::I64,
            _ => return None,
        });
    }

    if let TsType::TsTypeRef(TsTypeRef { type_name, .. }) = ty {
        let name = match type_name {
            swc_ecma_ast::TsEntityName::Ident(id) => id.sym.as_str(),
            _ => return None,
        };
        return Some(ValTy::from_annotation(name));
    }

    None
}

fn declare_user_fn(module: &mut ObjectModule, fn_decl: &FunctionDecl) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl);
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &ty in &params {
        sig.params.push(AbiParam::new(ty.cl_type()));
    }
    if let Some(rt) = ret {
        sig.returns.push(AbiParam::new(rt.cl_type()));
    }

    let symbol = user_symbol_name(&fn_decl.name);
    let id = module
        .declare_function(&symbol, Linkage::Local, &sig)
        .with_context(|| format!("failed to declare function `{}`", fn_decl.name))?;

    Ok(UserFn { id, params, ret })
}

fn user_symbol_name(name: &str) -> String {
    format!("__RTS_USER_{}", sanitize_symbol(name))
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

    let ret = fn_decl.return_type.as_deref().and_then(|r| {
        if r == "void" {
            None
        } else {
            Some(ValTy::from_annotation(r))
        }
    });

    (params, ret)
}

fn compile_user_fn(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassInfo>,
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

        let mut fn_ctx = FnCtx::new(
            &mut builder,
            module,
            extern_cache,
            data_counter,
            globals,
            user_fns,
            classes,
            false,
        );
        fn_ctx.current_return_ty = info.ret;

        // Bind parameters as locals.
        for (i, param) in fn_decl.parameters.iter().enumerate() {
            let ty = param
                .type_annotation
                .as_deref()
                .map(ValTy::from_annotation)
                .unwrap_or(ValTy::I64);
            let block_param = fn_ctx.builder.block_params(entry)[i];
            let class_name = param
                .type_annotation
                .as_deref()
                .and_then(|t| classes.get(t).map(|c| c.name.clone()));
            fn_ctx.declare_local_of_class(&param.name, ty, block_param, class_name);
        }

        // Compile body statements.
        let mut terminated = false;
        for stmt_raw in &fn_decl.body {
            if terminated {
                break;
            }
            let Statement::Raw(raw) = stmt_raw;
            if let Some(swc_stmt) = raw.stmt.as_ref() {
                terminated = lower_stmt(&mut fn_ctx, swc_stmt)?;
            }
        }

        // If we did not hit a return, emit one.
        if !terminated && !fn_ctx.builder.is_unreachable() {
            if let Some(ret_ty) = info.ret {
                let zero = match ret_ty {
                    ValTy::F64 => fn_ctx.builder.ins().f64const(0.0),
                    ValTy::I32 => fn_ctx.builder.ins().iconst(cl::I32, 0),
                    _ => fn_ctx.builder.ins().iconst(cl::I64, 0),
                };
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

fn compile_main(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassInfo>,
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
            true,
        );

        for stmt in stmts {
            match lower_stmt(&mut fn_ctx, stmt) {
                Ok(_) => {}
                Err(e) => warnings.push(format!("codegen warning: {e}")),
            }
        }

        let zero = fn_ctx.builder.ins().iconst(cl::I32, 0);
        if !fn_ctx.builder.is_unreachable() {
            fn_ctx.builder.ins().return_(&[zero]);
        }

        builder.finalize();
    }

    module
        .define_function(runtime_main_id, &mut runtime_ctx)
        .context("failed to define runtime entrypoint __RTS_MAIN")?;

    compile_main_entry_shim(module, runtime_main_id, &sig)
        .context("failed to define C entrypoint shim `main`")?;

    Ok(())
}

/// Computes layout and declares Cranelift symbols for a class' constructor
/// and methods. Property types determine field ValTy (all fields share an
/// 8-byte slot for alignment simplicity). Every ctor/method implicitly takes
/// `this: Handle` as its first parameter.
fn declare_class(
    module: &mut ObjectModule,
    class_decl: &ClassDecl,
) -> Result<(ClassInfo, Vec<(&'static str, UserFn)>)> {
    let mut info = ClassInfo {
        name: class_decl.name.clone(),
        ..ClassInfo::default()
    };
    let mut declared: Vec<(&'static str, UserFn)> = Vec::new();

    // Properties declared directly in the class body.
    let mut field_idx = 0i32;
    let record_field = |info: &mut ClassInfo, field_idx: &mut i32, prop: &PropertyDecl| {
        if prop.modifiers.is_static {
            return;
        }
        let ty = prop
            .type_annotation
            .as_deref()
            .map(ValTy::from_annotation)
            .unwrap_or(ValTy::I64);
        info.fields.push(ClassField {
            name: prop.name.clone(),
            ty,
            offset: *field_idx * SLOT_SIZE,
        });
        *field_idx += 1;
    };
    for member in &class_decl.members {
        if let ClassMember::Property(prop) = member {
            record_field(&mut info, &mut field_idx, prop);
        }
    }
    // Ctor parameters marked with a visibility modifier become fields too.
    for member in &class_decl.members {
        if let ClassMember::Constructor(ctor) = member {
            for param in &ctor.parameters {
                if param.modifiers.visibility.is_none() {
                    continue;
                }
                let ty = param
                    .type_annotation
                    .as_deref()
                    .map(ValTy::from_annotation)
                    .unwrap_or(ValTy::I64);
                if info.fields.iter().any(|f| f.name == param.name) {
                    continue;
                }
                info.fields.push(ClassField {
                    name: param.name.clone(),
                    ty,
                    offset: field_idx * SLOT_SIZE,
                });
                field_idx += 1;
            }
        }
    }
    info.size_bytes = (field_idx as i64) * (SLOT_SIZE as i64);

    // Declare ctor + method symbols. Each takes `this: Handle` first.
    for member in &class_decl.members {
        match member {
            ClassMember::Constructor(ctor) => {
                let params = ctor_params(ctor);
                let (user_fn, symbol) =
                    declare_class_fn(module, &class_decl.name, "CTOR", &params, None)?;
                declared.push((symbol, user_fn.clone()));
                info.ctor = Some(ClassMethod {
                    symbol,
                    params: params.into_iter().skip(1).collect(),
                    ret: None,
                });
            }
            ClassMember::Method(m) => {
                if m.modifiers.is_static {
                    continue;
                }
                let (params, ret) = method_signature(m);
                let (user_fn, symbol) =
                    declare_class_fn(module, &class_decl.name, &m.name, &params, ret)?;
                declared.push((symbol, user_fn.clone()));
                info.methods.insert(
                    m.name.clone(),
                    ClassMethod {
                        symbol,
                        params: params.into_iter().skip(1).collect(),
                        ret,
                    },
                );
            }
            ClassMember::Property(_) => {}
        }
    }

    Ok((info, declared))
}

/// Declares a Cranelift function for a class member (ctor or method).
/// `params` already includes the implicit `this: Handle` at index 0.
fn declare_class_fn(
    module: &mut ObjectModule,
    class_name: &str,
    member_name: &str,
    params: &[ValTy],
    ret: Option<ValTy>,
) -> Result<(UserFn, &'static str)> {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &ty in params {
        sig.params.push(AbiParam::new(ty.cl_type()));
    }
    if let Some(rt) = ret {
        sig.returns.push(AbiParam::new(rt.cl_type()));
    }

    let symbol_owned = format!(
        "__RTS_USER_{}_{}",
        sanitize_symbol(class_name),
        sanitize_symbol(member_name)
    );
    let symbol: &'static str = Box::leak(symbol_owned.into_boxed_str());

    let id = module
        .declare_function(symbol, Linkage::Local, &sig)
        .with_context(|| format!("failed to declare class member `{symbol}`"))?;

    Ok((
        UserFn {
            id,
            params: params.to_vec(),
            ret,
        },
        symbol,
    ))
}

fn ctor_params(ctor: &ConstructorDecl) -> Vec<ValTy> {
    let mut params = vec![ValTy::Handle]; // implicit this
    for p in &ctor.parameters {
        params.push(param_val_ty(p));
    }
    params
}

fn method_signature(m: &MethodDecl) -> (Vec<ValTy>, Option<ValTy>) {
    let mut params = vec![ValTy::Handle]; // implicit this
    for p in &m.parameters {
        params.push(param_val_ty(p));
    }
    let ret = m.return_type.as_deref().and_then(|r| {
        if r == "void" {
            None
        } else {
            Some(ValTy::from_annotation(r))
        }
    });
    (params, ret)
}

fn param_val_ty(p: &Parameter) -> ValTy {
    p.type_annotation
        .as_deref()
        .map(ValTy::from_annotation)
        .unwrap_or(ValTy::I64)
}

/// Compiles the bodies of a class' constructor and methods. `this` is always
/// the first Cranelift block parameter; the caller exposes it as a regular
/// local named `this` with the owning class attached.
fn compile_class_bodies(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassInfo>,
    class_user_fns: &HashMap<&'static str, UserFn>,
    class_decl: &ClassDecl,
) -> Result<()> {
    for member in &class_decl.members {
        match member {
            ClassMember::Constructor(ctor) => {
                let symbol = class_member_symbol(&class_decl.name, "CTOR");
                let info = class_user_fns
                    .get(symbol.as_str())
                    .ok_or_else(|| anyhow!("missing declared symbol for `{symbol}`"))?;
                compile_class_member_body(
                    module,
                    extern_cache,
                    data_counter,
                    globals,
                    user_fns,
                    classes,
                    &class_decl.name,
                    info,
                    &ctor.parameters,
                    &ctor.body,
                )
                .with_context(|| format!("in constructor of `{}`", class_decl.name))?;
            }
            ClassMember::Method(m) => {
                if m.modifiers.is_static {
                    continue;
                }
                let symbol = class_member_symbol(&class_decl.name, &m.name);
                let info = class_user_fns
                    .get(symbol.as_str())
                    .ok_or_else(|| anyhow!("missing declared symbol for `{symbol}`"))?;
                compile_class_member_body(
                    module,
                    extern_cache,
                    data_counter,
                    globals,
                    user_fns,
                    classes,
                    &class_decl.name,
                    info,
                    &m.parameters,
                    &m.body,
                )
                .with_context(|| format!("in method `{}.{}`", class_decl.name, m.name))?;
            }
            ClassMember::Property(_) => {}
        }
    }
    Ok(())
}

fn class_member_symbol(class_name: &str, member: &str) -> String {
    format!(
        "__RTS_USER_{}_{}",
        sanitize_symbol(class_name),
        sanitize_symbol(member)
    )
}

#[allow(clippy::too_many_arguments)]
fn compile_class_member_body(
    module: &mut ObjectModule,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    info: &UserFn,
    parameters: &[Parameter],
    body: &[Statement],
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

        let mut fn_ctx = FnCtx::new(
            &mut builder,
            module,
            extern_cache,
            data_counter,
            globals,
            user_fns,
            classes,
            false,
        );
        fn_ctx.current_class = Some(class_name.to_string());
        fn_ctx.current_return_ty = info.ret;

        // Bind `this` (block param #0) as a typed local.
        let this_val = fn_ctx.builder.block_params(entry)[0];
        fn_ctx.declare_local_of_class(
            "this",
            ValTy::Handle,
            this_val,
            Some(class_name.to_string()),
        );

        // Bind user parameters starting at index 1.
        for (i, param) in parameters.iter().enumerate() {
            let ty = param_val_ty(param);
            let block_param = fn_ctx.builder.block_params(entry)[i + 1];
            let class_of = param
                .type_annotation
                .as_deref()
                .and_then(|t| classes.get(t).map(|c| c.name.clone()));
            fn_ctx.declare_local_of_class(&param.name, ty, block_param, class_of);
        }

        let mut terminated = false;
        for stmt_raw in body {
            if terminated {
                break;
            }
            let Statement::Raw(raw) = stmt_raw;
            if let Some(swc_stmt) = raw.stmt.as_ref() {
                terminated = lower_stmt(&mut fn_ctx, swc_stmt)?;
            }
        }

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
        .with_context(|| format!("failed to define class member function"))?;

    Ok(())
}

fn compile_main_entry_shim(
    module: &mut ObjectModule,
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
