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
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{Decl, ForHead, Pat, Stmt};

use crate::parser::ast::{
    ClassDecl, FunctionDecl, Item, Program, Statement,
};

use super::analysis::address_taken::collect_address_taken_fns;
use super::analysis::captures::extract_class_owner;
use super::analysis::module_globals::collect_module_globals;
use super::analysis::types::sanitize_symbol;
use super::passes::args::default_args::expand_default_args;
use super::passes::args::rest_args::expand_rest_args;
use super::passes::args::spread_args::expand_spread_args;
use super::passes::async_expand::{expand_async_functions, expand_await_exprs};
use super::passes::destructuring::expand_destructuring;
use super::passes::hoist_fn::hoist_fn_expressions;
use super::passes::object_methods::desugar_object_methods;
use super::passes::parallelism::{
    array_methods_pass, lift_inline_arrows_in_array_methods, purity_pass, reduce_pass,
};
use super::compile::class::{
    class_init_name, synthesize_class_fns, validate_abstract_method_implementations,
};
use super::passes::static_fields::expand_static_fields;
use super::passes::this_arrow::lift_arrow_callbacks;
use super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::statements::lower_stmt;

// Re-export para callers externos (expressions/*) que ainda referenciam
// `lower::func::class_*_name`.
pub(crate) use super::compile::class::{
    class_getter_name, class_setter_name, class_static_method_name,
};

const RUNTIME_MAIN_SYMBOL: &str = crate::abi::symbols::ENTRY_POINT;

/// Info about a user-defined function needed by callers.
#[derive(Debug, Clone)]
struct UserFn {
    id: cranelift_module::FuncId,
    params: Vec<ValTy>,
    ret: Option<ValTy>,
}

/// Compiles the full program: user functions + top-level `main`.
pub fn compile_program(
    program: &mut Program,
    module: &mut dyn Module,
    extern_cache: &mut HashMap<String, cranelift_module::FuncId>,
    data_counter: &mut u32,
) -> Result<Vec<String>> {
    // (etapa 4.3) Limpa o MIR_CACHE thread-local — fns lowered na rodada
    // anterior nao devem vazar pra esta. Inline so' inlinea o que
    // estiver cached pra esta passada de compile_program.
    clear_mir_cache_for_program();

    expand_static_fields(program);
    lift_inline_arrows_in_array_methods(program);
    array_methods_pass(program);
    let mut par_fn_names = reduce_pass(program);
    par_fn_names.extend(purity_pass(program));
    let lifted_needs_c_callconv = lift_arrow_callbacks(program);
    desugar_object_methods(program);
    hoist_fn_expressions(program);
    expand_destructuring(program);
    expand_default_args(program);
    // Async functions (#413/F2): reescreve `async function f(arg)` em
    // wrapper sincrono que retorna Promise + spawna body em thread.
    expand_async_functions(program);
    // Await expressions (#414/F3): reescreve `await x` em `promise.wait(x)`.
    // Roda DEPOIS de expand_async_functions porque o body original que
    // sai dali pode conter await; e o pass de async pode ter introduzido
    // calls que retornam Promise (nao precisam wait pq o pass ja' retorna
    // o handle, mas user pode ter `await f()` dentro de outra fn).
    expand_await_exprs(program);
    // Spread antes de rest: spread aplaina array literal nos call sites
    // (`f(...[1,2,3])` → `f(1,2,3)`); rest depois empacota argumentos
    // extras conforme o callee é variadic.
    expand_spread_args(program);
    expand_rest_args(program);

    // Single-file AOT path: imports are not stripped before compile_program,
    // so we scan them here to populate node_import_map (JIT multi-file path
    // already populated this in ModuleGraph::flatten_for_jit).
    for item in &program.items {
        if let Item::Import(decl) = item {
            if let Some(prefix) = crate::nodespace::ns_prefix_for(&decl.from) {
                for name in &decl.names {
                    program
                        .node_import_map
                        .entry(name.clone())
                        .or_insert_with(|| format!("{prefix}.{name}"));
                }
                if let Some(default_name) = &decl.default_name {
                    program
                        .node_import_map
                        .entry(default_name.clone())
                        .or_insert_with(|| prefix.to_string());
                }
            }
        }
    }
    let node_import_map = std::mem::take(&mut program.node_import_map);

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

    // Segundo pass: computa layout nativo das classes elegiveis em ordem
    // topologica (pais antes de filhos), de forma que filhos vejam o
    // layout do parent ao herdarem offsets. Aditivo: o codegen ainda
    // nao consome este campo — preserva os 187/187 testes.
    {
        use super::class_layout::compute_layout;
        let mut remaining: Vec<String> = classes.keys().cloned().collect();
        let mut progress = true;
        while progress && !remaining.is_empty() {
            progress = false;
            let mut still: Vec<String> = Vec::new();
            for name in remaining.drain(..) {
                let parent_name = classes.get(&name).and_then(|m| m.super_class.clone());
                let parent_ready = match &parent_name {
                    None => true,
                    Some(p) => classes
                        .get(p)
                        .map(|pm| pm.layout.is_some())
                        .unwrap_or(true), // parent ausente: trata como "pronto"
                                          // — compute_layout vai retornar None
                };
                if !parent_ready {
                    still.push(name);
                    continue;
                }
                let parent_layout = parent_name
                    .as_ref()
                    .and_then(|p| classes.get(p))
                    .and_then(|pm| pm.layout.clone());
                let layout = {
                    let meta = classes.get(&name).expect("present");
                    compute_layout(meta, parent_layout.as_ref())
                };
                if let Some(meta) = classes.get_mut(&name) {
                    meta.layout = layout;
                }
                progress = true;
            }
            remaining = still;
        }
    }

    // Valida que toda classe concreta implementa todos os abstract methods
    // herdados. Coleta os abstract de toda a hierarquia, descontando os
    // que a classe (ou descendentes diretos) implementam.
    validate_abstract_method_implementations(&classes)?;

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

    // Coleta nomes de fns cujo endereço é tomado (`f as unknown as
    // number`, ou ident em posição de valor — ex: arg de `thread.spawn`).
    // Essas fns precisam de C-callconv para serem chamáveis via FFI/
    // thread entrypoint sem corrupção de stack (#206).
    let mut address_taken_fns =
        collect_address_taken_fns(&fn_decls, program, &synthetic_fns);
    // União com fns chamadas de trampolins lifted C-callconv (#206).
    address_taken_fns.extend(lifted_needs_c_callconv.iter().cloned());
    // Funções sintéticas do purity_pass (Level-1 parallel ForOf).
    address_taken_fns.extend(par_fn_names.iter().cloned());

    // Métodos de classe (`__class_<C>_<m>`, exceto `__init`) são marcados
    // como address-taken para permitir reificação via `obj.method` (Function
    // class #359). Custo: perda de TCO em métodos. Benefício: `c.add.bind(c)`
    // funciona como handle Function de primeira classe.
    for fn_decl in &fn_decls {
        let n = &fn_decl.name;
        if n.starts_with("__class_") && !n.ends_with("__init") && !n.contains("_static_") {
            address_taken_fns.insert(n.clone());
        }
    }

    // (refator etapa 2.4 + 3.6) HIR + MIR proof-of-life: lower cada user
    // fn pra HIR tipado e em seguida pra MIR SSA + passes (fold/dce/narrow),
    // descartando o resultado. Hoje os hints ainda nao alimentam o codegen
    // (caminho da AST permanece autoritativo); proxima fase consumira o
    // MirFunc via lower/inst.rs 1:1. Issue #611.
    {
        let dump_mir = std::env::var("RTS_DUMP_MIR")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let mut hir_scope = rts_hir::scope::Scope::new();
        for fn_decl in &fn_decls {
            let hir_fn = rts_hir::lower::lower_func(fn_decl, &mut hir_scope);
            let mut mir_fn = rts_mir::lower::lower_func(&hir_fn);
            rts_mir::passes::optimize(&mut mir_fn);
            rts_mir::passes::narrow(&mut mir_fn);
            // Verify em debug build apenas — produção pula o assert.
            #[cfg(debug_assertions)]
            {
                let _ = rts_mir::passes::verify(&mir_fn);
            }
            if dump_mir {
                eprintln!("--- {} MIR ---", fn_decl.name);
                eprint!("{}", mir_fn);
            }
        }
    }

    // Phase 1: declare all user functions so forward calls resolve.
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let address_taken = address_taken_fns.contains(&fn_decl.name);
        let info = declare_user_fn(module, fn_decl, address_taken)?;
        let mangled: String = format!("__user_{}", fn_decl.name);
        extern_cache.insert(mangled.clone(), info.id);
        user_fns.insert(fn_decl.name.clone(), info);
    }

    // Built after fn_class_returns is populated below; placeholder here.
    let mut user_fn_abis: HashMap<String, UserFnAbi> = user_fns
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                UserFnAbi {
                    params: info.params.clone(),
                    ret: info.ret,
                    ret_class: None,
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
    // Wire class return info into UserFnAbi so lhs_static_class can resolve
    // method chains like `expect(...).toBe(...)`.
    for (name, class_name) in &fn_class_returns {
        if let Some(abi) = user_fn_abis.get_mut(name) {
            abi.ret_class = Some(class_name.clone());
        }
    }

    // Mapeia globais module-scope cuja anotacao bate com classe
    // registrada. Permite funcoes top-level acessarem globais como
    // instancias e participarem de overload.
    let mut global_class_ty: HashMap<String, String> = HashMap::new();
    // (#330) global_obj_field_types — populado a partir de globais que
    // sao object literals (incluindo enum string desugar). Compartilhado
    // entre fns user pra que \`E.Member\` retorne o ValTy correto (Handle
    // pra string enum, Bool, etc) em vez de I64 anonimo.
    let mut global_obj_field_types: HashMap<String, HashMap<String, ValTy>> =
        HashMap::new();
    // (#nested-chain) Tipos nested para globais — analogo ao local.
    let mut global_nested_obj_field_types: HashMap<
        (String, String),
        HashMap<String, ValTy>,
    > = HashMap::new();
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
                                global_class_ty.insert(name.clone(), cn.to_string());
                            }
                        }
                    }
                }
            }
            // (#330) Object literal init -> coleta field types pra compartilhar.
            if let Some(init) = d.init.as_ref() {
                if let swc_ecma_ast::Expr::Object(obj) = init.as_ref() {
                    let mut fts: HashMap<String, ValTy> = HashMap::new();
                    for prop in &obj.props {
                        if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                            if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                                let key = match &kv.key {
                                    swc_ecma_ast::PropName::Ident(i) => i.sym.as_str().to_string(),
                                    swc_ecma_ast::PropName::Str(s) => {
                                        s.value.to_string_lossy().to_string()
                                    }
                                    _ => continue,
                                };
                                match kv.value.as_ref() {
                                    swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_)) => {
                                        fts.insert(key, ValTy::Handle);
                                    }
                                    swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)) => {
                                        fts.insert(key, ValTy::I64);
                                    }
                                    swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => {
                                        fts.insert(key, ValTy::Bool);
                                    }
                                    // (#nested-chain) Object literal aninhado:
                                    // registra sub-fields para `cfg.server.host`
                                    // funcionar de dentro de user fn.
                                    swc_ecma_ast::Expr::Object(sub) => {
                                        fts.insert(key.clone(), ValTy::Handle);
                                        let mut sub_fts: HashMap<String, ValTy> =
                                            HashMap::new();
                                        for sp in &sub.props {
                                            if let swc_ecma_ast::PropOrSpread::Prop(spx) = sp {
                                                if let swc_ecma_ast::Prop::KeyValue(skv) =
                                                    spx.as_ref()
                                                {
                                                    let sk = match &skv.key {
                                                        swc_ecma_ast::PropName::Ident(i) => {
                                                            i.sym.as_str().to_string()
                                                        }
                                                        swc_ecma_ast::PropName::Str(s) => s
                                                            .value
                                                            .to_string_lossy()
                                                            .to_string(),
                                                        _ => continue,
                                                    };
                                                    match skv.value.as_ref() {
                                                        swc_ecma_ast::Expr::Lit(
                                                            swc_ecma_ast::Lit::Str(_),
                                                        ) => {
                                                            sub_fts.insert(sk, ValTy::Handle);
                                                        }
                                                        swc_ecma_ast::Expr::Lit(
                                                            swc_ecma_ast::Lit::Num(_),
                                                        ) => {
                                                            sub_fts.insert(sk, ValTy::I64);
                                                        }
                                                        swc_ecma_ast::Expr::Lit(
                                                            swc_ecma_ast::Lit::Bool(_),
                                                        ) => {
                                                            sub_fts.insert(sk, ValTy::Bool);
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                        if !sub_fts.is_empty() {
                                            global_nested_obj_field_types
                                                .insert((name.clone(), key), sub_fts);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    if !fts.is_empty() {
                        global_obj_field_types.insert(name, fts);
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
        let address_taken = address_taken_fns.contains(&fn_decl.name);
        let fn_warnings = compile_user_fn(
            module,
            extern_cache,
            data_counter,
            &globals,
            &user_fn_abis,
            &classes,
            &global_class_ty,
            &global_obj_field_types,
            &global_nested_obj_field_types,
            &fn_class_returns,
            &node_import_map,
            fn_decl,
            info,
            owner_class,
            address_taken,
        )
        .with_context(|| format!("in function `{}`", fn_decl.name))?;
        warnings.extend(fn_warnings);
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
        &global_obj_field_types,
        &global_nested_obj_field_types,
        &fn_class_returns,
        &node_import_map,
        &top_stmts,
        &mut warnings,
    )
    .context("in top-level runtime entry")?;

    Ok(warnings)
}

/// Lifted callback stubs (`__lifted_arrow_*`) are invoked by native UI
/// toolkits as plain C function pointers (`extern "C" fn()`), so they must
/// use the platform default calling convention.
#[inline]
fn is_lifted_callback(name: &str) -> bool {
    // Trampolins simples (sem captura de `this`): `__lifted_arrow_N`.
    // Trampolins de classe (capturam `this`/`super`): `__class_C_lifted_arrow_N`.
    // Ambos atravessam a fronteira C ABI quando invocados pelo FLTK.
    if name.starts_with("__lifted_arrow_") {
        return true;
    }
    if let Some(rest) = name.strip_prefix("__class_") {
        if rest.contains("_lifted_arrow_") {
            return true;
        }
    }
    false
}

/// User-defined functions generally use the Tail calling convention so codegen
/// can emit `return_call` for tail-position invocations (#93). Lifted UI
/// callbacks are the exception: they cross a native C ABI boundary, e
/// fns cujo endereço é tomado (passadas a APIs nativas como
/// `thread.spawn`, FFI, etc — #206).
fn user_call_conv(module: &dyn Module, fn_name: &str, address_taken: bool) -> CallConv {
    if is_lifted_callback(fn_name) || address_taken {
        module.isa().default_call_conv()
    } else {
        CallConv::Tail
    }
}

fn declare_user_fn(
    module: &mut dyn Module,
    fn_decl: &FunctionDecl,
    address_taken: bool,
) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl);
    let mut sig = Signature::new(user_call_conv(module, &fn_decl.name, address_taken));
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

/// (#301) Coleta os nomes de todos os `var x` declarados no statement,
/// recursivamente, sem atravessar boundaries de function/arrow/class.
/// Usado para var hoisting — todas as `var` em uma fn sao pre-declaradas
/// no topo com valor 0 (proxy de undefined).
fn collect_var_decls(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Decl(Decl::Var(vd)) => {
            if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        out.push(id.id.sym.as_str().to_string());
                    }
                }
            }
        }
        Stmt::Block(b) => {
            for s in &b.stmts {
                collect_var_decls(s, out);
            }
        }
        Stmt::If(i) => {
            collect_var_decls(&i.cons, out);
            if let Some(alt) = &i.alt {
                collect_var_decls(alt, out);
            }
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = &f.init {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::ForIn(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::ForOf(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::While(w) => collect_var_decls(&w.body, out),
        Stmt::DoWhile(d) => collect_var_decls(&d.body, out),
        Stmt::Try(t) => {
            for s in &t.block.stmts {
                collect_var_decls(s, out);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    collect_var_decls(s, out);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    collect_var_decls(s, out);
                }
            }
        }
        Stmt::Switch(sw) => {
            for case in &sw.cases {
                for s in &case.cons {
                    collect_var_decls(s, out);
                }
            }
        }
        Stmt::Labeled(l) => collect_var_decls(&l.body, out),
        Stmt::With(w) => collect_var_decls(&w.body, out),
        // function/arrow/class declarations dentro do body criam novo
        // scope — nao recursa.
        _ => {}
    }
}

/// Thread-local cache de MirFuncs ja lowered nesta passada de
/// `compile_program`. Permite inline aplicar quando o callee foi
/// declarado antes do caller no source. Limpado pelo `compile_program`
/// no inicio de cada nova chamada via `clear_mir_cache_for_program`.
thread_local! {
    static MIR_CACHE: std::cell::RefCell<HashMap<String, rts_mir::ir::MirFunc>>
        = std::cell::RefCell::new(HashMap::new());
}

pub(crate) fn clear_mir_cache_for_program() {
    MIR_CACHE.with(|c| c.borrow_mut().clear());
}

/// Try to compile `fn_decl` through the MIR pipeline (HIR → MIR → optimize
/// → mir_codegen). Returns `Ok(true)` when MIR took over and the function
/// is fully defined in `module`; `Ok(false)` when MIR bailed (unsupported
/// shape) and the AST path should run.
fn try_compile_via_mir(
    module: &mut dyn Module,
    fn_decl: &FunctionDecl,
    info: &UserFn,
    address_taken: bool,
) -> Result<bool> {
    use rts_mir::ir::{Inst, Terminator, TrapHint};

    if std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!("[mir-trace] enter try_compile_via_mir({})", fn_decl.name);
    }

    // Conservative gate: bail on any synthetic name (starts with `__`),
    // any async fn, or any synthetic body. The MIR doesn't yet model the
    // runtime hooks these need (Promise, this binding, closure capture).
    if fn_decl.is_async || fn_decl.name.starts_with("__") {
        debug_bail(fn_decl, "synthetic / async fn");
        return Ok(false);
    }
    let body_synthetic = fn_decl.body.iter().any(|stmt| {
        let crate::parser::ast::Statement::Raw(raw) = stmt;
        raw.text.starts_with('<') && raw.text.ends_with('>')
    });
    if body_synthetic {
        debug_bail(fn_decl, "synthetic body");
        return Ok(false);
    }

    // Whitelist by signature: only fns with explicit primitive numeric
    // ret + params route through MIR. Anything taking/returning string,
    // class, void with side effects, etc. stays on the AST path until
    // those features are modeled in MIR.
    fn ann_is_supported(ann: &str) -> bool {
        matches!(
            ann,
            "number" | "i64" | "i32" | "f64" | "bool" | "boolean" | "void"
                | "i8" | "i16" | "u8" | "u16" | "u32" | "u64" | "f32"
        )
    }
    let ret_ok = fn_decl
        .return_type
        .as_deref()
        .map(ann_is_supported)
        .unwrap_or(false);
    if !ret_ok {
        debug_bail(fn_decl, "ret type not whitelisted");
        return Ok(false);
    }
    for p in &fn_decl.parameters {
        let ok = p
            .type_annotation
            .as_deref()
            .map(ann_is_supported)
            .unwrap_or(false);
        if !ok {
            debug_bail(fn_decl, "param type not whitelisted");
            return Ok(false);
        }
    }

    // 1. Lower TS AST → HIR. O scope local recebe pre-registro das
    //    assinaturas das fns ja cached em MIR_CACHE, para que o lower
    //    HIR resolve `inc(x)` no body de `caller` quando `inc` foi
    //    compilado pelo MIR antes (ordem natural: callees declarados
    //    antes dos callers).
    let mut hir_scope = rts_hir::scope::Scope::new();
    MIR_CACHE.with(|c| {
        let cache = c.borrow();
        for (name, mir) in cache.iter() {
            let param_tys: Vec<rts_hir::ir::HirType> =
                mir.params.iter().map(|(_, t)| t.clone()).collect();
            hir_scope.register_param_types(name.clone(), param_tys);
            if !matches!(mir.ret, rts_hir::ir::HirType::Unknown) {
                hir_scope.register_return_type(name.clone(), mir.ret.clone());
            }
        }
    });
    let hir_fn = rts_hir::lower::lower_func(fn_decl, &mut hir_scope);

    // 2. Lower HIR → MIR with both extern + intrinsic resolvers.
    let extern_resolver = crate::mir_codegen::extern_resolver_default();
    let intrinsic_resolver = crate::mir_codegen::intrinsic_resolver_default();
    let mut mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
        &hir_fn,
        &hir_scope,
        Some(&extern_resolver),
        Some(&intrinsic_resolver),
    );

    // 3a. Bail if any block trapped (unsupported shape) — AST handles it.
    let has_trap = mir_fn.blocks.iter().any(|b| {
        matches!(b.term, Terminator::Trap { code: TrapHint::User(_) })
    });
    if has_trap {
        debug_bail(fn_decl, "has trap");
        return Ok(false);
    }

    // 3b. Bail if the lower silently inserted placeholder zeros — that
    // means an expression (member access, unknown ident, unresolved call)
    // fell through to a default i64 0. Compiling this would silently
    // return wrong values; AST path handles those cases properly.
    if mir_fn.had_placeholders {
        debug_bail(fn_decl, "had placeholders");
        return Ok(false);
    }

    // 4a. Inline em fixed-point: cada passada substitui CallUser por
    //     corpo inline quando elegivel; o optimize subsequente colapsa
    //     fold/cse/dce, possivelmente revelando novas oportunidades de
    //     inline em CallUser que so' agora se tornaram alcançáveis. Max
    //     4 iterações (programs profundos sao raros — past N=2 já
    //     colapsa quase tudo na pratica).
    const MAX_INLINE_ITERS: usize = 4;
    let mut total_inlined = 0;
    for _iter in 0..MAX_INLINE_ITERS {
        let changed = MIR_CACHE.with(|c| {
            let cache = c.borrow();
            rts_mir::passes::inline(&mut mir_fn, &cache)
        });
        if !changed {
            break;
        }
        total_inlined += 1;
        // Roda optimize entre as iteracoes pra simplificar antes do
        // proximo inline ver o IR atualizado.
        rts_mir::passes::optimize(&mut mir_fn);
    }
    if total_inlined > 0 && std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!(
            "[mir-trace] inline applied in {} ({} iters)",
            fn_decl.name, total_inlined
        );
    }

    // 4b. Final optimize + verify. A verify failure is a MIR bug, not
    //     user code — fall back to AST and don't panic.
    rts_mir::passes::optimize(&mut mir_fn);
    if rts_mir::passes::verify(&mir_fn).is_err() {
        return Ok(false);
    }

    // 5. Override conv to match what `compile_program` declared for this
    //    user fn (Tail vs host default depending on address_taken). We
    //    detect from `info.id`'s declared signature via the module — but
    //    that's not directly exposed, so use the conservative rule: if the
    //    fn was declared with Tail conv (most user fns), keep Tail; if
    //    address-taken (host default), match it.
    //
    //    `compile_user_fn` signature uses `info.params/info.ret` derived
    //    from the AST. We compare against the MIR signature; if param
    //    types or arity don't match, the JIT would call with wrong ABI →
    //    bail.
    if mir_fn.params.len() != info.params.len() {
        debug_bail(fn_decl, "param arity mismatch");
        return Ok(false);
    }
    for (i, &ast_ty) in info.params.iter().enumerate() {
        if !mir_param_compatible(&mir_fn.params[i].1, ast_ty) {
            debug_bail(fn_decl, &format!("param[{i}] type mismatch"));
            return Ok(false);
        }
    }
    let ast_ret = info.ret;
    if !mir_ret_compatible(&mir_fn.ret, ast_ret) {
        debug_bail(fn_decl, "ret type mismatch");
        return Ok(false);
    }

    // Match the conv the AST path would have used. Address-taken fns
    // (passed as fn pointers to thread.spawn, promise.then, etc.) precisam
    // de host default extern "C" conv, mas o caminho MIR ainda revela
    // edge cases sutis (ex.: spawned thread crashando) — bail por
    // segurança até que a integração com runtime callers (thread.spawn,
    // promise.then) seja auditada caso a caso.
    if address_taken {
        debug_bail(fn_decl, "address-taken (MIR routing not yet safe)");
        return Ok(false);
    }
    mir_fn.conv = rts_mir::ir::CallConvHint::Tail;

    // CallExtern reativado: o MIR resolveu ABI mismatches via address-taken
    // gating + GC stack maps + StrLit expansion + correct Bool/Float iconst.
    // Se um caso edge surgir, o gate generico (had_placeholders, sig
    // mismatch, ret/param whitelist) ainda bail antes de produzir codigo
    // errado.

    // Auto-recursão tail: o AST emite `return_call` (TCO Cranelift) que
    // permite recursão profunda sem stack overflow. O MIR ainda emite
    // call regular — bail quando a fn chama a si própria pra preservar
    // semantica de tail call.
    let self_recursive = mir_fn.blocks.iter().flat_map(|b| &b.insts).any(|i| {
        matches!(i, rts_mir::ir::Inst::CallUser { name, .. } if name == &fn_decl.name)
    });
    if self_recursive {
        debug_bail(fn_decl, "self-recursive (TCO needed)");
        return Ok(false);
    }

    // 6. Build a `decls` map containing only `info.id` for self (recursion).
    //    Cross-fn calls fall back to AST if `mir_fn` references unknown
    //    fns (CallUser to non-self) — we bail in that case.
    let mut decls = HashMap::new();
    decls.insert(fn_decl.name.clone(), info.id);
    for block in &mir_fn.blocks {
        for inst in &block.insts {
            if let Inst::CallUser { name, .. } = inst {
                if !decls.contains_key(name) {
                    debug_bail(fn_decl, "unknown CallUser target");
                    return Ok(false);
                }
            }
        }
    }

    // 7. Lower MIR → Cranelift IR using the existing FuncId.
    match crate::mir_codegen::lower::lower_mir_func_with_decls(module, &mir_fn, &decls) {
        Ok(_) => {
            if std::env::var("RTS_MIR_DEBUG").is_ok() {
                eprintln!("[mir] compiled `{}` via MIR path", fn_decl.name);
            }
            // Cache este MirFunc para que callers subsequentes possam
            // inline esta fn. Pre-inline (antes da etapa 4a) pra que a
            // versao cached nao tenha CallUser inlinados redundantes —
            // mas o mir_fn aqui ja sofreu inline + optimize. Isso eh OK:
            // se este caller eventualmente vira callee de outro caller,
            // o inline do outro caller vai expandir tudo de uma vez.
            MIR_CACHE.with(|c| {
                c.borrow_mut().insert(fn_decl.name.clone(), mir_fn.clone());
            });
            Ok(true)
        }
        Err(e) => {
            debug_bail(fn_decl, &format!("lower err: {e}"));
            Ok(false)
        }
    }
}

fn debug_bail(fn_decl: &FunctionDecl, reason: &str) {
    if std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!("[mir] {} bail: {}", fn_decl.name, reason);
    }
}

fn mir_param_compatible(mir_ty: &rts_hir::ir::HirType, ast_ty: ValTy) -> bool {
    use rts_hir::ir::HirType;
    // Conservative: only allow numeric primitive types. Handle/Str/anything
    // else routes to the AST path so the MIR doesn't accidentally produce
    // wrong ABI for GC-tracked values.
    match (mir_ty, ast_ty) {
        (HirType::I64, ValTy::I64) => true,
        (HirType::Bool, ValTy::Bool) => true,
        (HirType::F64 | HirType::Number, ValTy::F64) => true,
        (HirType::I32, ValTy::I32) => true,
        // Narrow ints — MIR roteia overflow via pass `narrow`. ABI
        // permanece I64 nos dois lados (callsite AST chama com i64,
        // MIR mascara internamente; sign-extend de signed e' feito
        // no return via narrow_return_signed_extend pass).
        (HirType::I8, ValTy::I8) => true,
        (HirType::I16, ValTy::I16) => true,
        (HirType::U8, ValTy::U8) => true,
        (HirType::U16, ValTy::U16) => true,
        // Mismatch → caller bails to AST path
        _ => false,
    }
}

fn mir_ret_compatible(mir_ret: &rts_hir::ir::HirType, ast_ret: Option<ValTy>) -> bool {
    use rts_hir::ir::HirType;
    match (mir_ret, ast_ret) {
        (HirType::Void, None) => true,
        (HirType::Void, Some(_)) | (_, None) => false,
        (mir_ty, Some(ast_ty)) => mir_param_compatible(mir_ty, ast_ty),
    }
}

fn compile_user_fn(
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

fn compile_main(
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
                    let msg = format!("{e}");
                    let is_hard = msg.contains("abstract")
                        || msg.contains("readonly")
                        || msg.contains("private")
                        || msg.contains("protected");
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

