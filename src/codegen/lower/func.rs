//! User-defined function and module-level compilation.
//!
//! `compile_program` declares all user functions first (for forward calls),
//! lowers bodies, then lowers top-level statements into `__RTS_MAIN`.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use cranelift_codegen::Context as ClContext;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};
use swc_ecma_ast::{Decl, Expr, Lit, Pat, Stmt, TsType, TsTypeRef};

use crate::parser::ast::{
    ClassDecl, ClassMember, FunctionDecl, Item, MemberModifiers, MethodRole, Parameter, Program,
    Statement,
};

use super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::stmt::lower_stmt;

const RUNTIME_MAIN_SYMBOL: &str = "__RTS_MAIN";

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
    module: &mut dyn Module,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    let globals = collect_module_globals(program, module)?;

    // Collect class declarations e expande em FunctionDecl sinteticos.
    // Cada classe `C` gera:
    //   - `__class_C__init(this, ...args)` para o constructor
    //   - `__class_C_<method>(this, ...args)` para cada metodo
    // O nome mangled e usado como `FunctionDecl.name`. Nao colide com
    // identifier TS valido (sem `__` no inicio em codigo de usuario).
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

    let mut classes: HashMap<String, ClassMeta> = HashMap::new();
    let mut synthetic_fns: Vec<FunctionDecl> = Vec::new();
    for class in &class_decls {
        let (meta, fns) = synthesize_class_fns(class);
        classes.insert(class.name.clone(), meta);
        synthetic_fns.extend(fns);
    }

    // Collect function declarations (originais + sinteticos das classes).
    let mut fn_decls: Vec<&FunctionDecl> = program
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
    for f in &synthetic_fns {
        fn_decls.push(f);
    }

    // Phase 1: declare all user functions so forward calls resolve.
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let info = declare_user_fn(module, fn_decl)?;
        let mangled: &'static str = Box::leak(format!("__user_{}", fn_decl.name).into_boxed_str());
        extern_cache.insert(mangled, info.id);
        user_fns.insert(fn_decl.name.clone(), info);
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

    // Mapeia funcoes que retornam classe registrada — usado para
    // dispatch de overload em `const x: V = makeV()` e
    // `obj.m() + obj.m()`. Le `return_type` textual do FunctionDecl.
    let mut fn_class_returns: HashMap<String, String> = HashMap::new();
    for fn_decl in &fn_decls {
        if let Some(ret) = fn_decl.return_type.as_deref() {
            let ret_trim = ret.trim();
            if classes.contains_key(ret_trim) {
                fn_class_returns.insert(fn_decl.name.clone(), ret_trim.to_string());
            }
        }
    }

    // Mapeia globais module-scope cuja anotacao bate com classe
    // registrada. Permite funcoes top-level acessarem globais como
    // instancias e participarem de overload.
    let mut global_class_ty: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else {
            continue;
        };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else {
            continue;
        };
        for d in &var_decl.decls {
            let Pat::Ident(id) = &d.name else { continue };
            let name = id.sym.as_str().to_string();
            // Anotacao explicita
            if let Some(ann) = id.type_ann.as_ref() {
                if let swc_ecma_ast::TsType::TsTypeRef(r) = ann.type_ann.as_ref() {
                    if let swc_ecma_ast::TsEntityName::Ident(t) = &r.type_name {
                        let t_name = t.sym.as_str();
                        if classes.contains_key(t_name) {
                            global_class_ty.insert(name.clone(), t_name.to_string());
                        }
                    }
                }
            }
            // Heuristica: init = new C(...)
            if !global_class_ty.contains_key(&name) {
                if let Some(init) = d.init.as_ref() {
                    if let swc_ecma_ast::Expr::New(ne) = init.as_ref() {
                        if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                            let cn = cid.sym.as_str();
                            if classes.contains_key(cn) {
                                global_class_ty.insert(name, cn.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 2: compile user function bodies.
    for fn_decl in &fn_decls {
        let info = user_fns
            .get(&fn_decl.name)
            .ok_or_else(|| anyhow!("missing user function metadata for `{}`", fn_decl.name))?;
        // Determina se a function pertence a uma classe (mangled name
        // `__class_<C>_*` ou `__class_<C>__init`) — usado para resolver
        // `super` no body do metodo.
        let owner_class = extract_class_owner(&fn_decl.name);
        compile_user_fn(
            module,
            extern_cache,
            data_counter,
            &globals,
            &user_fn_abis,
            &classes,
            &global_class_ty,
            &fn_class_returns,
            fn_decl,
            info,
            owner_class,
        )
        .with_context(|| format!("in function `{}`", fn_decl.name))?;
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
        &global_class_ty,
        &fn_class_returns,
        &top_stmts,
        &mut warnings,
    )
    .context("in top-level runtime entry")?;

    Ok(warnings)
}

fn collect_module_globals(
    program: &Program,
    module: &mut dyn Module,
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
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);
            if wrote_as_float || !v.is_finite() || v.fract() != 0.0 {
                ValTy::F64
            } else if v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                ValTy::I32
            } else {
                ValTy::I64
            }
        }
        Expr::Lit(Lit::Bool(_)) => ValTy::Bool,
        Expr::Lit(Lit::Str(_)) => ValTy::Handle,
        Expr::Tpl(_) => ValTy::Handle,
        Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
            let l = infer_expr_ty(Some(&b.left));
            let r = infer_expr_ty(Some(&b.right));
            if l == ValTy::Handle || r == ValTy::Handle {
                ValTy::Handle
            } else if l == ValTy::F64 || r == ValTy::F64 {
                ValTy::F64
            } else {
                ValTy::I64
            }
        }
        // `**` sempre retorna F64 (roteado via libc pow).
        Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Exp) => ValTy::F64,
        Expr::Bin(b) => {
            // Numeric ops other than + (string concat handled above).
            // Propagate F64 so globals holding `math.PI * x` get the right
            // storage; otherwise default to I64.
            let l = infer_expr_ty(Some(&b.left));
            let r = infer_expr_ty(Some(&b.right));
            if l == ValTy::F64 || r == ValTy::F64 {
                ValTy::F64
            } else {
                ValTy::I64
            }
        }
        Expr::Cond(c) => {
            let l = infer_expr_ty(Some(&c.cons));
            let r = infer_expr_ty(Some(&c.alt));
            if l == r { l } else { ValTy::I64 }
        }
        Expr::Paren(p) => infer_expr_ty(Some(&p.expr)),
        Expr::Unary(u) => infer_expr_ty(Some(&u.arg)),
        Expr::Member(_) => infer_abi_member_ty(expr).unwrap_or(ValTy::I64),
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                if let Some(ty) = infer_abi_member_ty(callee) {
                    return ty;
                }
            }
            ValTy::I64
        }
        _ => ValTy::I64,
    }
}

/// If `expr` is an ABI member reference (`ns.name`), returns the ValTy of
/// its return/value type.
fn infer_abi_member_ty(expr: &Expr) -> Option<ValTy> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(ns) = m.obj.as_ref() else { return None };
    let name = match &m.prop {
        swc_ecma_ast::MemberProp::Ident(id) => id.sym.as_str(),
        _ => return None,
    };
    let qualified = format!("{}.{}", ns.sym.as_str(), name);
    let (_, member) = crate::abi::lookup(&qualified)?;
    Some(ValTy::from_abi(member.returns))
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

/// User-defined functions use the Tail calling convention so codegen can
/// emit `return_call` for tail-position invocations (#93). Extern namespace
/// functions (fs, io, fmod, etc) remain on the platform default — they're
/// C-ABI imports that need SystemV/Fastcall, and we only ever do regular
/// calls to them, never tail calls.
fn user_call_conv() -> CallConv {
    CallConv::Tail
}

fn declare_user_fn(module: &mut dyn Module, fn_decl: &FunctionDecl) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl);
    let mut sig = Signature::new(user_call_conv());
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
    module: &mut dyn Module,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassMeta>,
    global_class_ty: &HashMap<String, String>,
    fn_class_returns: &HashMap<String, String>,
    fn_decl: &FunctionDecl,
    info: &UserFn,
    current_class: Option<String>,
) -> Result<()> {
    let mut ctx = ClContext::new();
    ctx.func.signature = {
        let mut sig = Signature::new(user_call_conv());
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
            fn_class_returns,
            false,
        );
        fn_ctx.return_ty = info.ret;
        fn_ctx.is_tail_conv = true;
        fn_ctx.current_class = current_class.clone();
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
            }
        }

        // Bind parameters as locals.
        for (i, param) in fn_decl.parameters.iter().enumerate() {
            let ty = param
                .type_annotation
                .as_deref()
                .map(ValTy::from_annotation)
                .unwrap_or(ValTy::I64);
            let block_param = fn_ctx.builder.block_params(entry)[i];
            fn_ctx.declare_local(&param.name, ty, block_param);
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

        // If we did not hit a return, emit one. Body vazio: o entry
        // block precisa ter terminator obrigatorio para Cranelift.
        if !terminated && !fn_ctx.builder.is_unreachable() {
            if let Some(rt) = info.ret {
                let zero = match rt {
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
    module: &mut dyn Module,
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
    globals: &HashMap<String, GlobalVar>,
    user_fns: &HashMap<String, UserFnAbi>,
    classes: &HashMap<String, ClassMeta>,
    global_class_ty: &HashMap<String, String>,
    fn_class_returns: &HashMap<String, String>,
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
            fn_class_returns,
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


// ── Class lowering ────────────────────────────────────────────────────────

/// Sintetiza os FunctionDecl para uma classe: constructor + cada metodo
/// vira uma funcao independente que recebe `this` como primeiro parametro.
/// Retorna o ClassMeta usado pelo codegen para resolver `new` e dispatch.
fn synthesize_class_fns(class: &ClassDecl) -> (ClassMeta, Vec<FunctionDecl>) {
    let mut methods: Vec<String> = Vec::new();
    let mut getters: Vec<String> = Vec::new();
    let mut setters: Vec<String> = Vec::new();
    let mut static_methods: Vec<String> = Vec::new();
    let mut static_fields: Vec<String> = Vec::new();
    let mut fns: Vec<FunctionDecl> = Vec::new();
    let mut field_types: HashMap<String, ValTy> = HashMap::new();
    let mut field_class_names: HashMap<String, String> = HashMap::new();
    let mut has_constructor = false;

    for member in &class.members {
        match member {
            ClassMember::Constructor(ctor) => {
                has_constructor = true;
                for p in &ctor.parameters {
                    if let Some(ann) = p.type_annotation.as_deref() {
                        field_types
                            .entry(p.name.clone())
                            .or_insert(ValTy::from_annotation(ann));
                    }
                }
                let mut params = Vec::with_capacity(ctor.parameters.len() + 1);
                params.push(this_param(ctor.span));
                params.extend(ctor.parameters.iter().cloned());
                fns.push(FunctionDecl {
                    name: class_init_name(&class.name),
                    parameters: params,
                    return_type: None,
                    body: ctor.body.clone(),
                    span: ctor.span,
                });
            }
            ClassMember::Method(method) => {
                if method.modifiers.is_static {
                    static_methods.push(method.name.clone());
                    fns.push(FunctionDecl {
                        name: class_static_method_name(&class.name, &method.name),
                        parameters: method.parameters.clone(),
                        return_type: method.return_type.clone(),
                        body: method.body.clone(),
                        span: method.span,
                    });
                } else {
                    let synth_name = match method.role {
                        MethodRole::Getter => {
                            getters.push(method.name.clone());
                            class_getter_name(&class.name, &method.name)
                        }
                        MethodRole::Setter => {
                            setters.push(method.name.clone());
                            class_setter_name(&class.name, &method.name)
                        }
                        MethodRole::Method => {
                            methods.push(method.name.clone());
                            class_method_name(&class.name, &method.name)
                        }
                    };
                    let mut params = Vec::with_capacity(method.parameters.len() + 1);
                    params.push(this_param(method.span));
                    params.extend(method.parameters.iter().cloned());
                    fns.push(FunctionDecl {
                        name: synth_name,
                        parameters: params,
                        return_type: method.return_type.clone(),
                        body: method.body.clone(),
                        span: method.span,
                    });
                }
            }
            ClassMember::Property(prop) => {
                if prop.modifiers.is_static {
                    static_fields.push(prop.name.clone());
                } else if let Some(ann) = prop.type_annotation.as_deref() {
                    let ann = ann.trim();
                    field_types.insert(prop.name.clone(), ValTy::from_annotation(ann));
                    field_class_names.insert(prop.name.clone(), ann.to_string());
                }
            }
        }
    }

    let meta = ClassMeta {
        name: class.name.clone(),
        super_class: class.super_class.clone(),
        methods,
        field_types,
        field_class_names,
        static_methods,
        static_fields,
        getters,
        setters,
        has_constructor,
    };
    (meta, fns)
}

fn this_param(span: crate::parser::span::Span) -> Parameter {
    Parameter {
        name: "this".to_string(),
        type_annotation: None,
        modifiers: MemberModifiers::default(),
        variadic: false,
        span,
    }
}

pub(super) fn class_init_name(class: &str) -> String {
    format!("__class_{class}__init")
}

pub(super) fn class_method_name(class: &str, method: &str) -> String {
    format!("__class_{class}_{method}")
}

pub(super) fn class_static_method_name(class: &str, method: &str) -> String {
    format!("__class_{class}_static_{method}")
}

pub(super) fn class_getter_name(class: &str, prop: &str) -> String {
    format!("__class_{class}_get_{prop}")
}

pub(super) fn class_setter_name(class: &str, prop: &str) -> String {
    format!("__class_{class}_set_{prop}")
}

/// Inverso de `class_init_name`/`class_method_name`: extrai o nome da
/// classe quando o function name segue a convencao de mangle.
fn extract_class_owner(fn_name: &str) -> Option<String> {
    let rest = fn_name.strip_prefix("__class_")?;
    // Variante: `<C>__init`
    if let Some(idx) = rest.find("__init") {
        return Some(rest[..idx].to_string());
    }
    // Variantes especiais com prefixo de papel: `<C>_get_<x>`,
    // `<C>_set_<x>`, `<C>_static_<x>`. Detecta o prefixo no resto e
    // pega tudo antes dele.
    for marker in ["_get_", "_set_", "_static_"] {
        if let Some(idx) = rest.find(marker) {
            return Some(rest[..idx].to_string());
        }
    }
    // Variante: `<C>_<method>` — ultimo `_` separa classe de metodo.
    if let Some(idx) = rest.rfind('_') {
        return Some(rest[..idx].to_string());
    }
    None
}
