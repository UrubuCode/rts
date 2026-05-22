//! Entry point: `compile_program` orquestra o pipeline completo
//! (passes AST → declare → compile user fns → compile main).
//!
//! Helpers de declaracao (`declare_user_fn`, `fn_signature`,
//! `user_symbol_name`) ficam aqui porque so' sao usados pelo
//! pipeline de declare-then-compile.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{Decl, Pat, Stmt};

use crate::parser::ast::{ClassDecl, FunctionDecl, Item, Program, Statement};

use super::super::analysis::address_taken::collect_address_taken_fns;
use super::super::analysis::captures::extract_class_owner;
use super::super::analysis::module_globals::collect_module_globals;
use super::super::analysis::types::sanitize_symbol;
use super::super::ctx::{ClassMeta, UserFnAbi, ValTy};
use super::super::passes::args::default_args::expand_default_args;
use super::super::passes::args::rest_args::expand_rest_args;
use super::super::passes::args::spread_args::expand_spread_args;
use super::super::passes::async_expand::{expand_async_functions, expand_await_exprs};
use super::super::passes::destructuring::expand_destructuring;
use super::super::passes::hoist_fn::hoist_fn_expressions;
use super::super::passes::object_methods::desugar_object_methods;
use super::super::passes::parallelism::{
    array_methods_pass, lift_inline_arrows_in_array_methods, purity_pass, reduce_pass,
};
use super::super::passes::static_fields::expand_static_fields;
use super::super::passes::this_arrow::lift_arrow_callbacks;
use super::class::{synthesize_class_fns, validate_abstract_method_implementations};
use super::main_fn::compile_main;
use super::mir_route::clear_mir_cache_for_program;
use super::user_fn::compile_user_fn;
use super::util::user_call_conv;

/// Info about a user-defined function needed by callers.
#[derive(Debug, Clone)]
pub(crate) struct UserFn {
    pub(crate) id: cranelift_module::FuncId,
    pub(crate) params: Vec<ValTy>,
    pub(crate) ret: Option<ValTy>,
}

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
                for spec in &decl.names {
                    // Mapeia binding local -> simbolo do nodespace (uso `orig`
                    // pra resolver no source; `local` eh o que o user digita).
                    program
                        .node_import_map
                        .entry(spec.local.clone())
                        .or_insert_with(|| format!("{prefix}.{}", spec.orig));
                }
                if let Some(default_name) = &decl.default_name {
                    program
                        .node_import_map
                        .entry(default_name.clone())
                        .or_insert_with(|| prefix.to_string());
                }
            } else {
                // Aliases de imports user-module: registra apenas local != orig.
                for spec in &decl.names {
                    if spec.local != spec.orig {
                        program
                            .local_alias_map
                            .entry(spec.local.clone())
                            .or_insert_with(|| spec.orig.clone());
                    }
                }
            }
        }
    }
    let node_import_map = std::mem::take(&mut program.node_import_map);
    let local_alias_map = std::mem::take(&mut program.local_alias_map);

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

    // (cross-runtime #267) Pre-computa set de classes que tem field
    // initializers (NAO ctor). Usado pelo synthesize para decidir se a
    // subclasse sem ctor explicito deve chamar super.__init. Ctors com
    // args ficam de fora — chamada implicita super() requer match de
    // assinatura que MVP nao cobre.
    let mut classes_with_init: std::collections::HashSet<String> = std::collections::HashSet::new();
    for class in &class_decls {
        let has_field_init = class.members.iter().any(|m| matches!(m,
            crate::parser::ast::ClassMember::Property(p)
                if !p.modifiers.is_static && p.initializer.is_some()
        ));
        if has_field_init {
            classes_with_init.insert(class.name.clone());
        }
    }
    // Propaga: se classe X tem init, e Y extends X, Y eh marcada como
    // has_init (precisa chamar X.__init via chain). Iteramos ate fixed point.
    let mut changed = true;
    while changed {
        changed = false;
        for class in &class_decls {
            if classes_with_init.contains(&class.name) { continue; }
            if let Some(parent) = class.super_class.as_deref() {
                if classes_with_init.contains(parent) {
                    classes_with_init.insert(class.name.clone());
                    changed = true;
                }
            }
        }
    }

    let mut classes: HashMap<String, ClassMeta> = HashMap::new();
    let mut synthetic_fns: Vec<FunctionDecl> = Vec::new();
    for class in &class_decls {
        let (meta, fns) = synthesize_class_fns(class, &classes_with_init);
        classes.insert(class.name.clone(), meta);
        synthetic_fns.extend(fns);
    }

    // Segundo pass: computa layout nativo das classes elegiveis em ordem
    // topologica (pais antes de filhos), de forma que filhos vejam o
    // layout do parent ao herdarem offsets. Aditivo: o codegen ainda
    // nao consome este campo — preserva os 187/187 testes.
    {
        use super::super::class_layout::compute_layout;
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

    // (cross-runtime #799) Pre-coleta has_this_param do AST: fn declarada
    // como `function f(this: any, ...)` precisa receber thisArg como
    // primeiro arg em invocacoes via Reflect/Function.call.
    let has_this_map: HashMap<String, bool> = fn_decls
        .iter()
        .map(|fd| {
            let has_this = fd
                .parameters
                .first()
                .map(|p| p.name == "this")
                .unwrap_or(false);
            (fd.name.clone(), has_this)
        })
        .collect();
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
                    has_this_param: has_this_map.get(name).copied().unwrap_or(false),
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
            &local_alias_map,
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
        &local_alias_map,
        &top_stmts,
        &mut warnings,
    )
    .context("in top-level runtime entry")?;

    Ok(warnings)
}

/// User-defined functions generally use the Tail calling convention so codegen
/// can emit `return_call` for tail-position invocations (#93). Lifted UI
/// callbacks are the exception: they cross a native C ABI boundary, e
/// fns cujo endereço é tomado (passadas a APIs nativas como
/// `thread.spawn`, FFI, etc — #206).

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

    let ret = match fn_decl.return_type.as_deref() {
        Some("void") => None,
        Some(r) => Some(ValTy::from_annotation(r)),
        None => {
            // (#mul/#294) Sem anotacao explicita: inferir baseado no que
            // o body retorna. Heuristica simples:
            // - se algum return contem string-yielding expr (template,
            //   str lit, concat com str, .toString(), .join()) -> Handle.
            // - se algum return eh bool (literal true/false, comparison,
            //   logical) -> Bool. (cross-runtime #300)
            // - se algum return existe -> F64 (number, caso default).
            // - sem return -> None (void).
            let inferred = inspect_return_kind(&fn_decl.body);
            match inferred {
                ReturnKind::String | ReturnKind::Handle => Some(ValTy::Handle),
                ReturnKind::Bool => Some(ValTy::Bool),
                ReturnKind::Number => Some(ValTy::F64),
                ReturnKind::Void => None,
            }
        }
    };

    (params, ret)
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum ReturnKind {
    Void,
    Number,
    String,
    Bool,
    Handle,
}

/// Heuristica de inferencia de return type baseada no shape do return expr.
/// Conservador: retorna String quando QUALQUER ramo retorna expressao
/// string-yielding; Number quando ha return mas nenhum visivelmente string;
/// Void quando nao ha return value.
fn inspect_return_kind(body: &[Statement]) -> ReturnKind {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::Expr;
    fn expr_yields_string(e: &Expr) -> bool {
        match e {
            Expr::Tpl(_) => true,
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) => true,
            Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
                expr_yields_string(&b.left) || expr_yields_string(&b.right)
            }
            Expr::Call(c) => {
                // .toString(), .join(), .slice() (em string), .concat() em string,
                // .replace(), etc. Heuristica: prop name comum de string-returning.
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    if let Expr::Member(m) = callee.as_ref() {
                        if let swc_ecma_ast::MemberProp::Ident(id) = &m.prop {
                            return matches!(
                                id.sym.as_str(),
                                "toString" | "join" | "concat" | "replace"
                                | "replaceAll" | "trim" | "trimStart" | "trimEnd"
                                | "toUpperCase" | "toLowerCase" | "padStart"
                                | "padEnd" | "repeat" | "substring" | "substr"
                                | "slice" | "charAt" | "normalize"
                            );
                        }
                    }
                    // String(x) coerce
                    if let Expr::Ident(id) = callee.as_ref() {
                        if id.sym.as_str() == "String" {
                            return true;
                        }
                    }
                }
                false
            }
            Expr::Paren(p) => expr_yields_string(&p.expr),
            Expr::Cond(c) => expr_yields_string(&c.cons) || expr_yields_string(&c.alt),
            _ => false,
        }
    }
    // (cross-runtime #300) Detecta returns que produzem bool: literal
    // true/false, comparisons (==/===/!=/!==/</>/...), logical (&&/||/!)
    // unless contains string concat.
    fn expr_yields_bool(e: &Expr) -> bool {
        use swc_ecma_ast::BinaryOp;
        match e {
            Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => true,
            Expr::Bin(b) => matches!(
                b.op,
                BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
                | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
                | BinaryOp::In | BinaryOp::InstanceOf
            ),
            Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Bang) => true,
            Expr::Paren(p) => expr_yields_bool(&p.expr),
            _ => false,
        }
    }
    // (cross-runtime #292) Object/Array literal e new-expr de classe
    // retornam Handle. Sem isso, codegen tipa ret como F64 e callers
    // (e.g. JSON.stringify) interpretam handle como f64 bits.
    fn expr_yields_handle(e: &Expr) -> bool {
        match e {
            Expr::Object(_) | Expr::Array(_) | Expr::New(_) => true,
            Expr::Paren(p) => expr_yields_handle(&p.expr),
            Expr::Cond(c) => expr_yields_handle(&c.cons) || expr_yields_handle(&c.alt),
            _ => false,
        }
    }
    fn check_stmt(stmt: &swc_ecma_ast::Stmt, found: &mut ReturnKind) {
        use swc_ecma_ast::Stmt;
        match stmt {
            Stmt::Return(r) => {
                if let Some(arg) = r.arg.as_deref() {
                    let new_kind = if expr_yields_string(arg) {
                        ReturnKind::String
                    } else if expr_yields_bool(arg) {
                        ReturnKind::Bool
                    } else if expr_yields_handle(arg) {
                        ReturnKind::Handle
                    } else {
                        ReturnKind::Number
                    };
                    // Precedencia: String > Handle > Bool > Number.
                    if new_kind == ReturnKind::String {
                        *found = ReturnKind::String;
                    } else if new_kind == ReturnKind::Handle && *found != ReturnKind::String {
                        *found = ReturnKind::Handle;
                    } else if new_kind == ReturnKind::Bool
                        && *found != ReturnKind::String
                        && *found != ReturnKind::Handle
                    {
                        *found = ReturnKind::Bool;
                    } else if *found == ReturnKind::Void {
                        *found = ReturnKind::Number;
                    }
                }
            }
            Stmt::Block(b) => {
                for s in &b.stmts {
                    check_stmt(s, found);
                }
            }
            Stmt::If(i) => {
                check_stmt(&i.cons, found);
                if let Some(alt) = i.alt.as_deref() {
                    check_stmt(alt, found);
                }
            }
            Stmt::Try(t) => {
                for s in &t.block.stmts {
                    check_stmt(s, found);
                }
                if let Some(h) = &t.handler {
                    for s in &h.body.stmts {
                        check_stmt(s, found);
                    }
                }
                if let Some(f) = &t.finalizer {
                    for s in &f.stmts {
                        check_stmt(s, found);
                    }
                }
            }
            _ => {}
        }
    }
    let mut kind = ReturnKind::Void;
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            check_stmt(stmt, &mut kind);
        }
    }
    kind
}

/// Inspeciona body para detectar `return <expr>` (qualquer valor) em
/// qualquer ramo top-level. Conservador: nao recursa em sub-blocks de
/// if/while/etc — heuristica para o caso comum de fn aritmetica simples.
#[allow(dead_code)]
fn has_return_value(body: &[Statement]) -> bool {
    use crate::parser::ast::Statement;
    fn check_stmt(stmt: &swc_ecma_ast::Stmt) -> bool {
        use swc_ecma_ast::Stmt;
        match stmt {
            Stmt::Return(r) => r.arg.is_some(),
            Stmt::Block(b) => b.stmts.iter().any(check_stmt),
            Stmt::If(i) => {
                check_stmt(&i.cons)
                    || i.alt.as_deref().map(check_stmt).unwrap_or(false)
            }
            Stmt::Try(t) => {
                t.block.stmts.iter().any(check_stmt)
                    || t.handler
                        .as_ref()
                        .map(|h| h.body.stmts.iter().any(check_stmt))
                        .unwrap_or(false)
                    || t.finalizer
                        .as_ref()
                        .map(|f| f.stmts.iter().any(check_stmt))
                        .unwrap_or(false)
            }
            _ => false,
        }
    }
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            if check_stmt(stmt) {
                return true;
            }
        }
    }
    false
}
