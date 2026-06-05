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
use super::super::passes::args::arguments_object::expand_arguments_object;
use super::super::passes::args::default_args::expand_default_args;
use super::super::passes::args::new_collection_arg::desugar_new_collection_call_arg;
use super::super::passes::args::rest_args::expand_rest_args;
use super::super::passes::args::spread_args::expand_spread_args;
use super::super::passes::async_expand::{expand_async_functions, expand_await_exprs};
use super::super::passes::custom_iterator::desugar_custom_iterators;
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
    // (#195 mutable closures) Caixa (boxing) de locais capturados-E-mutados em
    // celulas heap ANTES de qualquer lift de arrow — assim a closure captura o
    // HANDLE da celula por valor e as escritas sao compartilhadas (env-record).
    crate::codegen::lower::passes::box_captures::box_mutable_captures(program);
    // (#374) `new Map(arr.map(...))` -> extrai o call p/ var temporaria ANTES
    // do lift de array methods, p/ que o .map seja liftado como statement
    // normal e o Map popule via MAP_FROM_ENTRIES (caminho via-var, que funciona).
    desugar_new_collection_call_arg(program);
    lift_inline_arrows_in_array_methods(program);
    array_methods_pass(program);
    let mut par_fn_names = reduce_pass(program);
    par_fn_names.extend(purity_pass(program));
    let lifted_needs_c_callconv = lift_arrow_callbacks(program);
    // (#272) Iterator protocol custom (`[Symbol.iterator]`): renomeia o metodo,
    // promove capturas do object iterator a `this`-fields, e reescreve
    // `for-of` sobre instancias em loop `.next()`. Roda ANTES de object_methods
    // (que desugara o `next()` shorthand) e hoist_fn.
    desugar_custom_iterators(program);
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
    // (#299) `arguments` vira rest param sintetico ANTES de expand_rest_args,
    // que entao empacota todos os args do callsite no slot `arguments`.
    expand_arguments_object(program);
    expand_rest_args(program);

    // (generators) Detecta user fns que sao generators: o generator_desugar
    // prepend `const __gen_buf = []` no body. Registra os nomes num
    // thread-local pra que o decl marque `const it = g()` como generator_var
    // e `it.next()` roteie para GENERATOR_NEXT (cursor lateral).
    {
        let mut gen_fns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in &program.items {
            if let Item::Function(f) = item {
                if fn_body_declares_gen_buf(&f.body) {
                    gen_fns.insert(f.name.clone());
                }
            }
        }
        set_generator_fns(gen_fns);
    }

    // (#1078/#341) Metodos de prototype que retornam f64 INEQUIVOCO.
    // `Fn.prototype.m = namedFn` onde namedFn retorna evidencia forte de
    // float (Math.sqrt/pow/.../ divisao `/` / float lit nao-inteiro). O call
    // site `obj.m()` usa INVOKE_AUTO_TYPED(rk=1) para nao truncar. So' f64
    // (nao Number generico) — metodos que retornam int ficam de fora.
    register_proto_method_f64(program);

    // (#270) Coleta nomes de fns (incl. arrows liftadas) cujo corpo retorna
    // string inequivoca. `inspect_return_kind` consulta isso para inferir que
    // `return someFn()` (call de fn string-yielding) retorna string — sem isso
    // o handle de string voltava como bits crus.
    register_string_ret_fns(program);
    // (cross-runtime) registra tipos de campo do objeto literal retornado
    // por cada user fn, pra que `const r = fn()` propague em decls.rs.
    register_fn_ret_obj_field_types(program);

    // (#1071) Getters bool instalados via `Object.defineProperty(C.prototype,
    // "x", { get(){ return this._campo } })` onde `_campo` eh bool. Registra
    // `x` para que a leitura `obj.x` tipe o resultado como Bool (true/false,
    // nao 1/0).
    register_bool_getters(program);

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

    // (cross-runtime #1057) Pre-coleta params do constructor de cada classe.
    // Permite que subclasse sem ctor explicito propague params na chamada
    // sintetica de super.__init. Sem isso, `class Dog extends Animal {}`
    // gerava __class_Dog__init(this) chamando __class_Animal__init(this)
    // sem args, e `super(name)` em sub-sub-class falhava com "espera 0 args".
    let mut class_ctor_params: HashMap<String, Vec<crate::parser::ast::Parameter>> = HashMap::new();
    for class in &class_decls {
        for m in &class.members {
            if let crate::parser::ast::ClassMember::Constructor(c) = m {
                class_ctor_params.insert(class.name.clone(), c.parameters.clone());
                break;
            }
        }
    }
    let mut classes: HashMap<String, ClassMeta> = HashMap::new();
    let mut synthetic_fns: Vec<FunctionDecl> = Vec::new();
    for class in &class_decls {
        let (meta, fns) = synthesize_class_fns(class, &classes_with_init, &class_ctor_params);
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

    // (#1052) Pre-coleta globais top-level inicializados como array literal
    // de strings: `const X = ["a", "b", ...]`. Usado em inspect_return_kind
    // para inferir que `return X[idx]` produz String.
    let string_array_globals = collect_string_array_globals(program);
    // (cross-runtime #1052) Popula thread_local com valores das string-arrays
    // top-level pra que codegen possa propagar `k[N]` em compile time.
    let sav = collect_string_array_values(program);
    crate::codegen::lower::passes::parallelism::STRING_ARRAY_VALUES
        .with(|c| *c.borrow_mut() = sav);

    // Phase 1: declare all user functions so forward calls resolve.
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let address_taken = address_taken_fns.contains(&fn_decl.name);
        let info = declare_user_fn(module, fn_decl, address_taken, &string_array_globals)?;
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
            } else if ret_trim == "this" {
                // (cross-runtime builder) Metodo `inc(): this { return this }`:
                // o return eh a propria classe. Extrai o owner do nome mangled
                // `__class_<C>_<method>` e usa como ret_class. Permite o chain
                // builder (`c.inc().inc()`) com `: this` (nao so' `: C`).
                if let Some(owner) = extract_class_owner(&fn_decl.name) {
                    if classes.contains_key(&owner) {
                        fn_class_returns.insert(fn_decl.name.clone(), owner);
                    }
                }
            }
        } else {
            // (#376) Sem return_type anotado: infere a classe do corpo quando
            // TODOS os `return` retornam `new C(...)` da MESMA classe C. Permite
            // chain `v1.add(v2).toString()` quando `add` nao anota `: Vec2`.
            if let Some(c) = infer_return_class_from_body(&fn_decl.body, &classes) {
                fn_class_returns.insert(fn_decl.name.clone(), c);
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
                            // (#1059) Globais inicializadas com `new WeakMap/
                            // WeakSet()` precisam ser detectadas pelo codegen
                            // para que `wm.set/get/has/delete` rotem para o
                            // dispatch WEAKMAP_*/WEAKSET_*, em vez do
                            // MAP_SET genérico que coerce key como string.
                            if matches!(cn, "WeakMap" | "WeakSet" | "Map" | "Set") {
                                global_class_ty.insert(name.clone(), cn.to_string());
                            }
                            // (Web Streams) Globais `const s = new ReadableStream/
                            // TransformStream(...)` referenciadas de dentro de uma
                            // async fn precisam carregar a classe para que
                            // `s.getReader()/getWriter()` rotem ao instance method
                            // global (e propaguem a classe do reader/writer).
                            if !global_class_ty.contains_key(&name)
                                && crate::abi::global_class_lookup(cn).is_some()
                            {
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
    string_array_globals: &std::collections::HashSet<String>,
) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl, string_array_globals);
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

/// (#1052) Coleta nomes de globais top-level inicializados como literal
/// de array de strings (ex: `const _0x = ["a", "b", "c"]`). Usado pela
/// inferencia de tipo de retorno para detectar `return arr[idx]` como
/// String em fns sem anotacao explicita.
fn collect_string_array_globals(program: &crate::parser::ast::Program) -> std::collections::HashSet<String> {
    use crate::parser::ast::{Item, Statement};
    use swc_ecma_ast::{Decl, Expr, Lit, Pat, Stmt};
    let mut out = std::collections::HashSet::new();
    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else { continue };
        for d in &var_decl.decls {
            let Pat::Ident(id) = &d.name else { continue };
            let Some(init) = d.init.as_deref() else { continue };
            let Expr::Array(arr) = init else { continue };
            // Aceita arrays nao-vazios cujos elementos visiveis sao todos
            // string literals (ou string lit em paren). Holes ignoradas.
            let mut all_str = true;
            let mut any = false;
            for elem in &arr.elems {
                let Some(e) = elem else { continue };
                any = true;
                let mut cur: &Expr = &e.expr;
                while let Expr::Paren(p) = cur { cur = &p.expr; }
                match cur {
                    Expr::Lit(Lit::Str(_)) => {}
                    _ => { all_str = false; break; }
                }
            }
            if any && all_str {
                out.insert(id.sym.as_str().to_string());
            }
        }
    }
    out
}

/// (cross-runtime #1052) Coleta valores de strings literais top-level
/// `const k = ["push", "length", ...]`. Permite codegen propagar
/// `k[N]` para string literal em compile time, despachando
/// `arr[k[N]](args)` como `arr.<method>(args)`.
pub(crate) fn collect_string_array_values(
    program: &crate::parser::ast::Program,
) -> std::collections::HashMap<String, Vec<String>> {
    use crate::parser::ast::{Item, Statement};
    use swc_ecma_ast::{Decl, Expr, Lit, Pat, Stmt};
    let mut out = std::collections::HashMap::new();
    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else { continue };
        for d in &var_decl.decls {
            let Pat::Ident(id) = &d.name else { continue };
            let Some(init) = d.init.as_deref() else { continue };
            let Expr::Array(arr) = init else { continue };
            fn try_eval_str(e: &Expr) -> Option<String> {
                let mut cur = e;
                while let Expr::Paren(p) = cur { cur = &p.expr; }
                match cur {
                    Expr::Lit(Lit::Str(s)) => Some(s.value.to_string_lossy().to_string()),
                    Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
                        let l = try_eval_str(&b.left)?;
                        let r = try_eval_str(&b.right)?;
                        Some(format!("{l}{r}"))
                    }
                    _ => None,
                }
            }
            let mut vals: Vec<String> = Vec::new();
            let mut ok = true;
            for elem in &arr.elems {
                let Some(e) = elem else { ok = false; break; };
                match try_eval_str(&e.expr) {
                    Some(s) => vals.push(s),
                    None => { ok = false; break; }
                }
            }
            if ok && !vals.is_empty() {
                out.insert(id.sym.as_str().to_string(), vals);
            }
        }
    }
    out
}

fn user_symbol_name(name: &str) -> String {
    format!("__RTS_USER_{}", sanitize_symbol(name))
}

fn fn_signature(
    fn_decl: &FunctionDecl,
    string_array_globals: &std::collections::HashSet<String>,
) -> (Vec<ValTy>, Option<ValTy>) {
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

    // (#345) Tag fn de tagged template recebe TemplateStringsArray
    // como primeiro param. Quando body retorna `param[i]`, sabemos que
    // eh string handle — adicionamos o nome ao set de "string array
    // globals" virtual para reuso da heurística existente.
    let mut sag_extended: std::collections::HashSet<String> =
        string_array_globals.clone();
    for p in &fn_decl.parameters {
        if let Some(ann) = p.type_annotation.as_deref() {
            let trimmed = ann.trim();
            if trimmed == "TemplateStringsArray" || trimmed.starts_with("string[")
                || trimmed.ends_with("string[]")
            {
                sag_extended.insert(p.name.clone());
            }
            // (cross-runtime) param escalar `string`: `(a: string, b: string)
            // => a + b` — sem isto a heuristica nao sabe que `a`/`b` sao
            // string e o concat infere i64 (handle cru). Registrar o param
            // como string-yielding faz `a + b` virar Handle.
            if trimmed == "string" {
                sag_extended.insert(p.name.clone());
            }
        }
    }
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
            let inferred = inspect_return_kind(&fn_decl.body, &sag_extended);
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

/// (#270) True se algum `return` no bloco SWC produz string inequivoca
/// (template, str literal, String(...), concat com string). Usado para
/// detectar arrows/fns locais string-yielding (`const a = () => "x"`).
fn block_returns_string(
    stmts: &[swc_ecma_ast::Stmt],
    sag: &std::collections::HashSet<String>,
) -> bool {
    use swc_ecma_ast::{Expr, Lit, Stmt};
    fn yields(e: &Expr, sag: &std::collections::HashSet<String>) -> bool {
        match e {
            Expr::Tpl(_) => true,
            Expr::Lit(Lit::Str(_)) => true,
            Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) =>
                yields(&b.left, sag) || yields(&b.right, sag),
            Expr::Paren(p) => yields(&p.expr, sag),
            Expr::Cond(c) => yields(&c.cons, sag) || yields(&c.alt, sag),
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(ce) = &c.callee {
                    if let Expr::Ident(id) = ce.as_ref() {
                        return id.sym.as_str() == "String" || sag.contains(id.sym.as_str());
                    }
                    if let Expr::Member(m) = ce.as_ref() {
                        if let swc_ecma_ast::MemberProp::Ident(p) = &m.prop {
                            return matches!(p.sym.as_str(),
                                "toString" | "join" | "concat" | "replace" | "replaceAll"
                                | "trim" | "toUpperCase" | "toLowerCase" | "slice" | "padStart"
                                | "padEnd" | "repeat" | "substring" | "charAt");
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
    fn check(s: &Stmt, sag: &std::collections::HashSet<String>) -> bool {
        match s {
            Stmt::Return(r) => r.arg.as_deref().map(|e| yields(e, sag)).unwrap_or(false),
            Stmt::Block(b) => b.stmts.iter().any(|s| check(s, sag)),
            Stmt::If(i) => check(&i.cons, sag) || i.alt.as_deref().map(|a| check(a, sag)).unwrap_or(false),
            _ => false,
        }
    }
    stmts.iter().any(|s| check(s, sag))
}

/// Heuristica de inferencia de return type baseada no shape do return expr.
/// Conservador: retorna String quando QUALQUER ramo retorna expressao
/// string-yielding; Number quando ha return mas nenhum visivelmente string;
/// Void quando nao ha return value.
fn inspect_return_kind(
    body: &[Statement],
    string_array_globals: &std::collections::HashSet<String>,
) -> ReturnKind {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::Expr;
    fn expr_yields_string(
        e: &Expr,
        sag: &std::collections::HashSet<String>,
    ) -> bool {
        match e {
            Expr::Tpl(_) => true,
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) => true,
            Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
                expr_yields_string(&b.left, sag) || expr_yields_string(&b.right, sag)
            }
            // (#374/LookupTable) `a ?? "fallback"` / `a || "fallback"`:
            // se o lado esquerdo eh string-yielding OU o fallback eh string,
            // o resultado eh string. Cobre `map.get(k) ?? "default"` (T=string).
            Expr::Bin(b) if matches!(
                b.op,
                swc_ecma_ast::BinaryOp::NullishCoalescing | swc_ecma_ast::BinaryOp::LogicalOr
            ) => {
                expr_yields_string(&b.left, sag) || expr_yields_string(&b.right, sag)
            }
            // (#1052) arr[i] onde arr e' global declarado como string[] literal.
            // Heuristica conservadora: o ident base esta em sag.
            Expr::Member(m) if m.computed_string_access().is_some() => {
                if let Expr::Ident(id) = m.obj.as_ref() {
                    return sag.contains(id.sym.as_str());
                }
                false
            }
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    if let Expr::Member(m) = callee.as_ref() {
                        if let swc_ecma_ast::MemberProp::Ident(id) = &m.prop {
                            let method = id.sym.as_str();
                            if matches!(
                                method,
                                "toString" | "join" | "concat" | "replace"
                                | "replaceAll" | "trim" | "trimStart" | "trimEnd"
                                | "toUpperCase" | "toLowerCase" | "padStart"
                                | "padEnd" | "repeat" | "substring" | "substr"
                                | "slice" | "charAt" | "normalize"
                            ) {
                                return true;
                            }
                            // (#345) `arr.reduce(fn, "")` — initial value
                            // string implica retorno string. Detecta via 2o arg.
                            // Inclui `reduce_bound`/`reduce_right_bound`: quando
                            // o callback inline captura vars (ex: tagged template
                            // tag com `...values`), lift_inline_arrows reescreve
                            // o metodo para a variante _bound mantendo o init
                            // string em args[1]. Sem aceitar _bound aqui, a fn
                            // wrapper era inferida como Number (-> f64) e o handle
                            // de string virava lixo no console.log.
                            if matches!(
                                method,
                                "reduce" | "reduceRight"
                                | "reduce_bound" | "reduce_right_bound"
                            )
                                && c.args.len() >= 2
                                && matches!(
                                    c.args[1].expr.as_ref(),
                                    Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
                                )
                            {
                                return true;
                            }
                        }
                    }
                    if let Expr::Ident(id) = callee.as_ref() {
                        // `String(...)` coercao; `arrow()`/`fn()` de var local
                        // string-yielding (sag); ou fn top-level/liftada que
                        // retorna string (STRING_RET_FNS — #270).
                        if id.sym.as_str() == "String"
                            || sag.contains(id.sym.as_str())
                            || fn_returns_string(id.sym.as_str())
                        {
                            return true;
                        }
                    }
                }
                false
            }
            Expr::Paren(p) => expr_yields_string(&p.expr, sag),
            Expr::Cond(c) => expr_yields_string(&c.cons, sag) || expr_yields_string(&c.alt, sag),
            // Var local conhecida como string-yielding (coletada acima) ou
            // global string[]/string conhecido.
            Expr::Ident(id) => sag.contains(id.sym.as_str()),
            _ => false,
        }
    }
    // Adapter trait p/ deteccao de arr[i] (computed access).
    trait MemberComputedExt {
        fn computed_string_access(&self) -> Option<()>;
    }
    impl MemberComputedExt for swc_ecma_ast::MemberExpr {
        fn computed_string_access(&self) -> Option<()> {
            match &self.prop {
                swc_ecma_ast::MemberProp::Computed(_) => Some(()),
                _ => None,
            }
        }
    }
    // (cross-runtime #300) Detecta returns que produzem bool: literal
    // true/false, comparisons (==/===/!=/!==/</>/...), logical (&&/||/!)
    // unless contains string concat.
    fn expr_yields_bool(e: &Expr) -> bool {
        use swc_ecma_ast::BinaryOp;
        match e {
            Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => true,
            Expr::Bin(b) => {
                if matches!(
                    b.op,
                    BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
                    | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
                    | BinaryOp::In | BinaryOp::InstanceOf
                ) {
                    return true;
                }
                // (cross-runtime followup) `a && b` / `a || b` retornam bool
                // quando AMBOS os lados sao bool. Cobre brand-check pattern:
                // `v !== null && typeof v === "object" && #x in v`.
                if matches!(b.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
                    return expr_yields_bool(&b.left) && expr_yields_bool(&b.right);
                }
                false
            }
            Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Bang) => true,
            Expr::Paren(p) => expr_yields_bool(&p.expr),
            Expr::Cond(c) => expr_yields_bool(&c.cons) && expr_yields_bool(&c.alt),
            _ => false,
        }
    }
    // (cross-runtime #292) Object/Array literal e new-expr de classe
    // retornam Handle. Sem isso, codegen tipa ret como F64 e callers
    // (e.g. JSON.stringify) interpretam handle como f64 bits.
    fn expr_yields_handle(e: &Expr) -> bool {
        // (cross-runtime #1125/#1079) `(globalThis as any)[key]` — computed
        // member access em globalThis sempre retorna any/handle. Sem isso,
        // `function ensureGlobal(...) { return (globalThis as any)[key]; }`
        // infere F64 e o map handle volta como f64-bits corrompido.
        fn peel<'a>(e: &'a Expr) -> &'a Expr {
            match e {
                Expr::TsAs(a) => peel(&a.expr),
                Expr::TsTypeAssertion(a) => peel(&a.expr),
                Expr::TsConstAssertion(a) => peel(&a.expr),
                Expr::TsNonNull(a) => peel(&a.expr),
                Expr::Paren(p) => peel(&p.expr),
                _ => e,
            }
        }
        match e {
            Expr::Object(_) | Expr::Array(_) | Expr::New(_) => true,
            // (cross-runtime #41) `return function() {...}` / `return () => ...`
            // produz fn handle (ponteiro de user fn liftada via hoist).
            // Sem isso, body inferia Number/F64 e o ponteiro era convertido
            // via fcvt_from_sint perdendo bits — chamada subsequente
            // `f(...)` falhava.
            Expr::Fn(_) | Expr::Arrow(_) => true,
            // (#1281) Quando o pass de lift roda ANTES desta inferencia, o
            // `return () => ...` ja' virou `return __lifted_arrow_N` (Ident).
            // Sem reconhecer esses prefixos sinteticos de funcao, a fn que
            // RETORNA uma arrow (curry/partial: `partial(f,a){ return (b)=>... }`)
            // inferia Number/F64 e o handle da arrow voltava como f64-bits
            // corrompido -> chamada subsequente dava 0.
            Expr::Ident(id) => {
                let n = id.sym.as_str();
                n.starts_with("__lifted_arrow_")
                    || n.starts_with("__hoisted_arrow_")
                    || n.starts_with("__hoisted_fn_")
            }
            Expr::Member(m) => {
                if let swc_ecma_ast::MemberProp::Computed(_) = &m.prop {
                    if let Expr::Ident(id) = peel(m.obj.as_ref()) {
                        if id.sym.as_str() == "globalThis" {
                            return true;
                        }
                    }
                }
                false
            }
            Expr::Paren(p) => expr_yields_handle(&p.expr),
            Expr::TsAs(a) => expr_yields_handle(&a.expr),
            Expr::TsTypeAssertion(a) => expr_yields_handle(&a.expr),
            Expr::TsConstAssertion(a) => expr_yields_handle(&a.expr),
            Expr::TsNonNull(a) => expr_yields_handle(&a.expr),
            Expr::Cond(c) => expr_yields_handle(&c.cons) || expr_yields_handle(&c.alt),
            _ => false,
        }
    }
    fn check_stmt(
        stmt: &swc_ecma_ast::Stmt,
        found: &mut ReturnKind,
        sag: &std::collections::HashSet<String>,
    ) {
        use swc_ecma_ast::Stmt;
        match stmt {
            Stmt::Return(r) => {
                if let Some(arg) = r.arg.as_deref() {
                    let new_kind = if expr_yields_string(arg, sag) {
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
                    check_stmt(s, found, sag);
                }
            }
            Stmt::If(i) => {
                check_stmt(&i.cons, found, sag);
                if let Some(alt) = i.alt.as_deref() {
                    check_stmt(alt, found, sag);
                }
            }
            Stmt::Try(t) => {
                for s in &t.block.stmts {
                    check_stmt(s, found, sag);
                }
                if let Some(h) = &t.handler {
                    for s in &h.body.stmts {
                        check_stmt(s, found, sag);
                    }
                }
                if let Some(f) = &t.finalizer {
                    for s in &f.stmts {
                        check_stmt(s, found, sag);
                    }
                }
            }
            _ => {}
        }
    }
    // (#270 new_target / String-via-var) Coleta vars locais inicializadas
    // com expr string-yielding (`const s = String(x)`, `let t = a.join(",")`)
    // no topo do body. `return s` referenciando uma delas conta como String.
    // Sem isso, fn que retorna uma string guardada em var inferia Number e o
    // handle voltava como bits crus.
    let mut string_vars: std::collections::HashSet<String> =
        string_array_globals.clone();
    {
        use swc_ecma_ast::{Decl, Expr, Pat, Stmt};
        for s in body {
            let Statement::Raw(raw) = s;
            let Some(Stmt::Decl(Decl::Var(v))) = raw.stmt.as_ref() else { continue };
            for d in &v.decls {
                let Pat::Ident(id) = &d.name else { continue };
                let Some(init) = d.init.as_deref() else { continue };
                if expr_yields_string(init, string_array_globals) {
                    string_vars.insert(id.id.sym.as_str().to_string());
                }
                // (#270) `const arrow = () => <string-expr>` / `function() {
                // return <string-expr> }` em var local: registra o NOME para
                // que `return arrow()` (call dessa var) seja inferido como
                // string. Sem isso, fn que retorna `arrow()` (call de arrow
                // local string-yielding) inferia Number e o handle voltava cru.
                let arrow_body_str = match init {
                    Expr::Arrow(a) => match a.body.as_ref() {
                        swc_ecma_ast::BlockStmtOrExpr::Expr(e) =>
                            expr_yields_string(e, string_array_globals),
                        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) =>
                            block_returns_string(&b.stmts, string_array_globals),
                    },
                    Expr::Fn(f) => f.function.body.as_ref()
                        .map(|b| block_returns_string(&b.stmts, string_array_globals))
                        .unwrap_or(false),
                    _ => false,
                };
                if arrow_body_str {
                    string_vars.insert(id.id.sym.as_str().to_string());
                }
                // (#270) `const arrow = <Ident de fn liftada string-yielding>`
                // — o lift troca a arrow inline por um Ident pra fn liftada.
                if let Expr::Ident(fid) = init {
                    if fn_returns_string(fid.sym.as_str()) {
                        string_vars.insert(id.id.sym.as_str().to_string());
                    }
                }
            }
        }
    }

    let mut kind = ReturnKind::Void;
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            check_stmt(stmt, &mut kind, &string_vars);
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

thread_local! {
    static GENERATOR_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// (generators) Registra nomes de generator fns (detectadas via
/// `const __gen_buf = []` no body — marcador do generator_desugar).
pub(crate) fn set_generator_fns(fns: std::collections::HashSet<String>) {
    GENERATOR_FNS.with(|c| *c.borrow_mut() = fns);
}

/// (generators) True se `name` eh uma generator fn registrada.
pub(crate) fn is_generator_fn(name: &str) -> bool {
    GENERATOR_FNS.with(|c| c.borrow().contains(name))
}

thread_local! {
    // (#1071) nomes de getters (via defineProperty) que retornam bool.
    static BOOL_GETTERS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// (#1071) True se `obj.<name>` eh um getter (defineProperty) que retorna bool.
pub(crate) fn is_bool_getter(name: &str) -> bool {
    BOOL_GETTERS.with(|c| c.borrow().contains(name))
}

/// Pre-scan: `Object.defineProperty(C.prototype, "x", { get(){ return
/// this._campo } })` onde `_campo` eh campo bool de C → registra `x`.
fn register_bool_getters(program: &Program) {
    use crate::parser::ast::{ClassMember, Item, Statement};
    use swc_ecma_ast::{Callee, Expr, MemberProp, Prop, PropName, PropOrSpread, Stmt};

    // 1. Mapa: classe -> conjunto de campos bool.
    let mut class_bool_fields: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for item in &program.items {
        if let Item::Class(c) = item {
            let mut fields = std::collections::HashSet::new();
            for m in &c.members {
                if let ClassMember::Property(p) = m {
                    let is_bool = p.type_annotation.as_deref() == Some("boolean")
                        || matches!(
                            p.initializer.as_deref(),
                            Some(Expr::Lit(swc_ecma_ast::Lit::Bool(_)))
                        );
                    if is_bool {
                        fields.insert(p.name.clone());
                    }
                }
            }
            if !fields.is_empty() {
                class_bool_fields.insert(c.name.clone(), fields);
            }
        }
    }
    if class_bool_fields.is_empty() {
        return;
    }

    // Bodies de fns top-level (o getter inline pode ter virado Ident de fn
    // hoisted apos desugar_object_methods/hoist_fn_expressions).
    let mut fn_bodies: HashMap<String, Vec<Stmt>> = HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            let stmts: Vec<Stmt> = f.body.iter().filter_map(|s| {
                let Statement::Raw(raw) = s;
                raw.stmt.clone()
            }).collect();
            fn_bodies.insert(f.name.clone(), stmts);
        }
    }
    // `return this.<campo>` com campo em fields?
    fn stmts_ret_bool_field(stmts: &[Stmt], fields: &std::collections::HashSet<String>) -> bool {
        use swc_ecma_ast::{Expr, MemberProp, Stmt};
        for s in stmts {
            if let Stmt::Return(r) = s {
                if let Some(Expr::Member(m)) = r.arg.as_deref() {
                    if matches!(m.obj.as_ref(), Expr::This(_)) {
                        if let MemberProp::Ident(p) = &m.prop {
                            if fields.contains(p.sym.as_str()) { return true; }
                        }
                    }
                }
            }
        }
        false
    }

    // 2. Varre `Object.defineProperty(C.prototype, "x", { get(){...} })`.
    // O getter retorna `this._campo`? Se `_campo` eh bool de C, registra x.
    let getter_returns_bool_field = |getter: &Expr, fields: &std::collections::HashSet<String>| -> bool {
        // getter inline (fn-expr/arrow) OU Ident de fn hoisted.
        if let Expr::Ident(id) = getter {
            if let Some(stmts) = fn_bodies.get(id.sym.as_str()) {
                return stmts_ret_bool_field(stmts, fields);
            }
        }
        let body = match getter {
            Expr::Fn(fe) => fe.function.body.as_ref().map(|b| b.stmts.clone()),
            Expr::Arrow(a) => match a.body.as_ref() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => Some(b.stmts.clone()),
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    // arrow expr-body: `() => this._f`
                    if let Expr::Member(m) = e.as_ref() {
                        if matches!(m.obj.as_ref(), Expr::This(_)) {
                            if let MemberProp::Ident(p) = &m.prop {
                                return fields.contains(p.sym.as_str());
                            }
                        }
                    }
                    return false;
                }
            },
            _ => None,
        };
        let Some(stmts) = body else { return false };
        for s in &stmts {
            if let Stmt::Return(r) = s {
                if let Some(Expr::Member(m)) = r.arg.as_deref() {
                    if matches!(m.obj.as_ref(), Expr::This(_)) {
                        if let MemberProp::Ident(p) = &m.prop {
                            if fields.contains(p.sym.as_str()) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    };

    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Expr(e)) = raw.stmt.as_ref() else { continue };
        let Expr::Call(c) = e.expr.as_ref() else { continue };
        let Callee::Expr(callee) = &c.callee else { continue };
        let Expr::Member(m) = callee.as_ref() else { continue };
        let is_object = matches!(m.obj.as_ref(), Expr::Ident(id) if id.sym.as_str() == "Object");
        let is_define = matches!(&m.prop, MemberProp::Ident(p) if p.sym.as_str() == "defineProperty");
        if !is_object || !is_define { continue; }
        // arg0 = C.prototype; extrai C.
        let Some(arg0) = c.args.first() else { continue };
        let Expr::Member(proto_m) = arg0.expr.as_ref() else { continue };
        let is_proto = matches!(&proto_m.prop, MemberProp::Ident(p) if p.sym.as_str() == "prototype");
        if !is_proto { continue; }
        let Expr::Ident(cls_id) = proto_m.obj.as_ref() else { continue };
        let Some(fields) = class_bool_fields.get(cls_id.sym.as_str()) else { continue };
        // arg1 = "x" (nome); arg2 = { get(){...} }.
        let prop_name = c.args.get(1).and_then(|a| match a.expr.as_ref() {
            Expr::Lit(swc_ecma_ast::Lit::Str(s)) => s.value.as_str().map(|v| v.to_string()),
            _ => None,
        });
        let Some(prop_name) = prop_name else { continue };
        let Some(desc_arg) = c.args.get(2) else { continue };
        let Expr::Object(desc) = desc_arg.expr.as_ref() else { continue };
        for dp in &desc.props {
            let PropOrSpread::Prop(dprop) = dp else { continue };
            // get pode vir como KeyValue("get", fn) OU Method com PropName "get".
            let getter_expr: Option<&Expr> = match dprop.as_ref() {
                Prop::KeyValue(kv) => {
                    let is_get = matches!(&kv.key, PropName::Ident(i) if i.sym.as_str() == "get");
                    if is_get { Some(kv.value.as_ref()) } else { None }
                }
                _ => None,
            };
            if let Some(getter) = getter_expr {
                if getter_returns_bool_field(getter, fields) {
                    BOOL_GETTERS.with(|s| { s.borrow_mut().insert(prop_name.clone()); });
                }
            }
            // Method-shorthand `get() {...}` no descriptor.
            if let Prop::Method(meth) = dprop.as_ref() {
                let is_get = matches!(&meth.key, PropName::Ident(i) if i.sym.as_str() == "get");
                if is_get {
                    let stmts = meth.function.body.as_ref().map(|b| b.stmts.clone()).unwrap_or_default();
                    for s in &stmts {
                        if let Stmt::Return(r) = s {
                            if let Some(Expr::Member(mm)) = r.arg.as_deref() {
                                if matches!(mm.obj.as_ref(), Expr::This(_)) {
                                    if let MemberProp::Ident(p) = &mm.prop {
                                        if fields.contains(p.sym.as_str()) {
                                            BOOL_GETTERS.with(|s| { s.borrow_mut().insert(prop_name.clone()); });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

thread_local! {
    // (#270) nomes de fns (incl. arrows liftadas) cujo corpo retorna string.
    static STRING_RET_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// (#270) True se a user fn `name` retorna string inequivoca.
fn fn_returns_string(name: &str) -> bool {
    STRING_RET_FNS.with(|c| c.borrow().contains(name))
}

thread_local! {
    // (cross-runtime) tipos de campo do objeto literal retornado por uma
    // user fn: `function mk(): {label: string} { return {label: "x"}; }`.
    // Consumido por decls.rs em `const r = mk()` para que `r.label`
    // resolva ValTy::Handle e `r.label + r.label` faca concat (nao soma).
    static FN_RET_OBJ_FIELD_TYPES: std::cell::RefCell<
        std::collections::HashMap<String, std::collections::HashMap<String, crate::codegen::lower::ctx::ValTy>>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// (cross-runtime) Retorna os tipos de campo do objeto literal devolvido
/// pela user fn `name`, se houver um return de object literal detectado.
pub(crate) fn fn_ret_obj_field_types(
    name: &str,
) -> Option<std::collections::HashMap<String, crate::codegen::lower::ctx::ValTy>> {
    FN_RET_OBJ_FIELD_TYPES.with(|c| c.borrow().get(name).cloned())
}

/// Pre-pass: para cada fn top-level cujo `return` eh um object literal,
/// coleta os tipos de campo (Str→Handle, Num→I64, Bool, Array/Object→Handle).
fn register_fn_ret_obj_field_types(program: &Program) {
    use crate::codegen::lower::ctx::ValTy;
    use crate::parser::ast::Statement;
    use swc_ecma_ast::{Expr, Stmt};
    fn field_types_of_obj(
        obj: &swc_ecma_ast::ObjectLit,
    ) -> std::collections::HashMap<String, ValTy> {
        let mut ft = std::collections::HashMap::new();
        for prop in &obj.props {
            if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                    let key = match &kv.key {
                        swc_ecma_ast::PropName::Ident(id) => id.sym.as_str().to_string(),
                        swc_ecma_ast::PropName::Str(s) => s.value.to_string_lossy().to_string(),
                        _ => continue,
                    };
                    let ty = match kv.value.as_ref() {
                        Expr::Lit(swc_ecma_ast::Lit::Str(_)) => Some(ValTy::Handle),
                        Expr::Lit(swc_ecma_ast::Lit::Num(_)) => Some(ValTy::I64),
                        Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => Some(ValTy::Bool),
                        Expr::Array(_) | Expr::Object(_) => Some(ValTy::Handle),
                        _ => None,
                    };
                    if let Some(t) = ty {
                        ft.insert(key, t);
                    }
                }
            }
        }
        ft
    }
    // Procura o primeiro `return {obj literal}` num swc Stmt (recursa em Block/If).
    fn scan_stmt(s: &Stmt) -> Option<std::collections::HashMap<String, ValTy>> {
        match s {
            Stmt::Return(r) => {
                let arg = r.arg.as_deref()?;
                let peeled = match arg {
                    Expr::Paren(p) => p.expr.as_ref(),
                    other => other,
                };
                if let Expr::Object(obj) = peeled {
                    let ft = field_types_of_obj(obj);
                    if !ft.is_empty() {
                        return Some(ft);
                    }
                }
                None
            }
            Stmt::Block(b) => b.stmts.iter().find_map(scan_stmt),
            Stmt::If(i) => scan_stmt(&i.cons)
                .or_else(|| i.alt.as_deref().and_then(scan_stmt)),
            _ => None,
        }
    }
    let mut out = std::collections::HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            let found = f.body.iter().find_map(|st| {
                let Statement::Raw(raw) = st;
                raw.stmt.as_ref().and_then(scan_stmt)
            });
            if let Some(ft) = found {
                out.insert(f.name.clone(), ft);
            }
        }
    }
    FN_RET_OBJ_FIELD_TYPES.with(|c| *c.borrow_mut() = out);
}

/// Pre-pass: registra fns top-level cujo corpo retorna string inequivoca.
fn register_string_ret_fns(program: &Program) {
    let empty: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = std::collections::HashSet::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            // Reusa inspect_return_kind: String => retorna string.
            if matches!(inspect_return_kind(&f.body, &empty), ReturnKind::String) {
                out.insert(f.name.clone());
            }
        }
    }
    STRING_RET_FNS.with(|c| *c.borrow_mut() = out);
}

thread_local! {
    // (#1078/#341) nomes de metodos de prototype que retornam f64 inequivoco.
    static PROTO_METHOD_F64: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// (#1078/#341) True se `obj.<method>()` (metodo de prototype dinamico) retorna
/// f64 inequivoco. Lido pelo call site para usar INVOKE_AUTO_TYPED(rk=1).
pub(crate) fn proto_method_is_f64(method: &str) -> bool {
    PROTO_METHOD_F64.with(|c| c.borrow().contains(method))
}

/// Evidencia FORTE de retorno f64 num corpo de fn: algum `return` cuja expr eh
/// divisao `/`, Math.<float-fn>(...), Math.PI/E/..., ou float literal
/// nao-inteiro. Conservador — int puro (`a+b`, literal inteiro) NAO dispara.
fn fn_body_returns_f64(body: &[Statement]) -> bool {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::{Expr, Lit, Stmt};

    fn expr_is_f64(e: &Expr) -> bool {
        match e {
            Expr::Lit(Lit::Num(n)) => n.value.fract() != 0.0,
            Expr::Bin(b) => {
                if matches!(b.op, swc_ecma_ast::BinaryOp::Div) {
                    return true; // `/` sempre f64 (JS spec)
                }
                // +,-,*,** propagam f64 se qualquer lado for f64.
                if matches!(b.op,
                    swc_ecma_ast::BinaryOp::Add | swc_ecma_ast::BinaryOp::Sub
                    | swc_ecma_ast::BinaryOp::Mul | swc_ecma_ast::BinaryOp::Exp) {
                    return expr_is_f64(&b.left) || expr_is_f64(&b.right);
                }
                false
            }
            Expr::Paren(p) => expr_is_f64(&p.expr),
            Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus) => expr_is_f64(&u.arg),
            Expr::Call(c) => {
                // Math.<float-fn>(...) — sempre f64.
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    if let Expr::Member(m) = callee.as_ref() {
                        let is_math = matches!(m.obj.as_ref(),
                            Expr::Ident(id) if id.sym.as_str() == "Math");
                        if is_math {
                            if let swc_ecma_ast::MemberProp::Ident(p) = &m.prop {
                                return matches!(p.sym.as_str(),
                                    "sqrt" | "cbrt" | "pow" | "exp" | "expm1" | "log"
                                    | "log2" | "log10" | "log1p" | "sin" | "cos" | "tan"
                                    | "asin" | "acos" | "atan" | "atan2" | "sinh" | "cosh"
                                    | "tanh" | "hypot" | "random" | "fround");
                            }
                        }
                    }
                }
                false
            }
            Expr::Member(m) => {
                // Math.PI / Math.E / Math.SQRT2 / ... — constantes f64.
                let is_math = matches!(m.obj.as_ref(),
                    Expr::Ident(id) if id.sym.as_str() == "Math");
                if is_math {
                    if let swc_ecma_ast::MemberProp::Ident(p) = &m.prop {
                        return matches!(p.sym.as_str(),
                            "PI" | "E" | "SQRT2" | "SQRT1_2" | "LN2" | "LN10"
                            | "LOG2E" | "LOG10E");
                    }
                }
                false
            }
            Expr::Cond(c) => expr_is_f64(&c.cons) || expr_is_f64(&c.alt),
            _ => false,
        }
    }
    fn check(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return(r) => r.arg.as_deref().map(expr_is_f64).unwrap_or(false),
            Stmt::Block(b) => b.stmts.iter().any(check),
            Stmt::If(i) => check(&i.cons) || i.alt.as_deref().map(check).unwrap_or(false),
            _ => false,
        }
    }
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            if check(stmt) {
                return true;
            }
        }
    }
    false
}

/// Pre-scan: `Fn.prototype.<m> = <namedFn>` onde namedFn retorna f64
/// inequivoco → registra `m` em PROTO_METHOD_F64.
fn register_proto_method_f64(program: &Program) {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::{AssignTarget, Expr, MemberProp, SimpleAssignTarget, Stmt};

    let mut fn_bodies: HashMap<String, &[Statement]> = HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            fn_bodies.insert(f.name.clone(), f.body.as_slice());
        }
    }
    fn peel(mut e: &Expr) -> &Expr {
        loop {
            match e {
                Expr::TsAs(x) => e = &x.expr,
                Expr::Paren(x) => e = &x.expr,
                Expr::TsNonNull(x) => e = &x.expr,
                _ => break,
            }
        }
        e
    }
    // Registra `method` se a fn-expr/ident RHS retorna f64 inequivoco.
    let register_if_f64 = |method: &str, rhs: &Expr, bodies: &HashMap<String, &[Statement]>| {
        let is_f64 = match peel(rhs) {
            Expr::Fn(fe) => fe
                .function
                .body
                .as_ref()
                .map(|b| fn_body_returns_f64_swc(&b.stmts))
                .unwrap_or(false),
            Expr::Ident(id) => bodies
                .get(id.sym.as_str())
                .map(|b| fn_body_returns_f64(b))
                .unwrap_or(false),
            _ => false,
        };
        if is_f64 {
            PROTO_METHOD_F64.with(|c| {
                c.borrow_mut().insert(method.to_string());
            });
        }
    };

    // Caso 1: `Fn.prototype.<m> = <fn>` (heranca classica).
    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Expr(e)) = raw.stmt.as_ref() else { continue };
        let Expr::Assign(a) = e.expr.as_ref() else { continue };
        let AssignTarget::Simple(SimpleAssignTarget::Member(lhs_m)) = &a.left else { continue };
        let MemberProp::Ident(method_id) = &lhs_m.prop else { continue };
        let Expr::Member(inner) = lhs_m.obj.as_ref() else { continue };
        let is_proto = matches!(&inner.prop, MemberProp::Ident(p) if p.sym.as_str() == "prototype");
        if !is_proto { continue; }
        register_if_f64(method_id.sym.as_str(), a.right.as_ref(), &fn_bodies);
    }

    // Caso 2: descriptors em `Object.create(proto, { m: { value: fn } })` e
    // `Object.defineProperty(obj, "m", { value: fn })`. Varre recursivamente
    // todas as expressoes (top-level + corpos de fn) procurando esses calls.
    fn visit_expr(
        e: &Expr,
        bodies: &HashMap<String, &[Statement]>,
        reg: &dyn Fn(&str, &Expr, &HashMap<String, &[Statement]>),
    ) {
        use swc_ecma_ast::{Callee, Prop, PropName, PropOrSpread};
        // Desce em wrappers comuns para alcancar o Call interno.
        match e {
            Expr::Assign(a) => { visit_expr(a.right.as_ref(), bodies, reg); }
            Expr::Paren(p) => { visit_expr(&p.expr, bodies, reg); }
            Expr::TsAs(x) => { visit_expr(&x.expr, bodies, reg); }
            Expr::TsNonNull(x) => { visit_expr(&x.expr, bodies, reg); }
            _ => {}
        }
        if let Expr::Call(c) = e {
            if let Callee::Expr(callee) = &c.callee {
                if let Expr::Member(m) = callee.as_ref() {
                    let is_object = matches!(m.obj.as_ref(),
                        Expr::Ident(id) if id.sym.as_str() == "Object");
                    let prop = match &m.prop {
                        MemberProp::Ident(p) => Some(p.sym.as_str()),
                        _ => None,
                    };
                    // Object.create(proto, descriptors): descriptors eh arg[1].
                    // Object.defineProperties(obj, descriptors): arg[1].
                    if is_object && matches!(prop, Some("create") | Some("defineProperties")) {
                        if let Some(arg) = c.args.get(1) {
                            if let Expr::Object(obj) = arg.expr.as_ref() {
                                for p in &obj.props {
                                    let PropOrSpread::Prop(prop) = p else { continue };
                                    let Prop::KeyValue(kv) = prop.as_ref() else { continue };
                                    let name = match &kv.key {
                                        PropName::Ident(i) => Some(i.sym.as_str().to_string()),
                                        PropName::Str(s) => s.value.as_str().map(|v| v.to_string()),
                                        _ => None,
                                    };
                                    // value eh um descriptor `{ value: fn, ... }`.
                                    if let (Some(name), Expr::Object(desc)) = (name, kv.value.as_ref()) {
                                        for dp in &desc.props {
                                            let PropOrSpread::Prop(dprop) = dp else { continue };
                                            let Prop::KeyValue(dkv) = dprop.as_ref() else { continue };
                                            let is_value = matches!(&dkv.key,
                                                PropName::Ident(i) if i.sym.as_str() == "value");
                                            if is_value {
                                                reg(&name, dkv.value.as_ref(), bodies);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Object.defineProperty(obj, "m", { value: fn }): arg[1]=name, arg[2]=descriptor.
                    if is_object && prop == Some("defineProperty") {
                        let name = c.args.first().and_then(|_| c.args.get(1)).and_then(|a| {
                            match a.expr.as_ref() {
                                Expr::Lit(swc_ecma_ast::Lit::Str(s)) => s.value.as_str().map(|v| v.to_string()),
                                _ => None,
                            }
                        });
                        if let (Some(name), Some(desc_arg)) = (name, c.args.get(2)) {
                            if let Expr::Object(desc) = desc_arg.expr.as_ref() {
                                for dp in &desc.props {
                                    let PropOrSpread::Prop(dprop) = dp else { continue };
                                    let Prop::KeyValue(dkv) = dprop.as_ref() else { continue };
                                    let is_value = matches!(&dkv.key,
                                        PropName::Ident(i) if i.sym.as_str() == "value");
                                    if is_value {
                                        reg(&name, dkv.value.as_ref(), bodies);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Recursa nos args.
            for a in &c.args {
                visit_expr(&a.expr, bodies, reg);
            }
        }
    }
    // Varre statements top-level (basta para o padrao comum; descriptors em
    // corpos de fn sao raros e cobertos pelo Caso 1 quando usam prototype.m=).
    for item in &program.items {
        if let Item::Statement(Statement::Raw(raw)) = item {
            if let Some(Stmt::Expr(e)) = raw.stmt.as_ref() {
                visit_expr(e.expr.as_ref(), &fn_bodies, &register_if_f64);
            }
            // Tambem em var decls (`const proto = Object.create(...)`).
            if let Some(Stmt::Decl(swc_ecma_ast::Decl::Var(v))) = raw.stmt.as_ref() {
                for d in &v.decls {
                    if let Some(init) = d.init.as_deref() {
                        visit_expr(init, &fn_bodies, &register_if_f64);
                    }
                }
            }
        }
    }
}

/// fn_body_returns_f64 sobre stmts SWC crus (corpo de fn-expr inline).
fn fn_body_returns_f64_swc(stmts: &[swc_ecma_ast::Stmt]) -> bool {
    use crate::parser::ast::{RawStmt, Statement};
    let wrapped: Vec<Statement> = stmts
        .iter()
        .map(|s| Statement::Raw(RawStmt::new(String::new(), Default::default()).with_stmt(s.clone())))
        .collect();
    fn_body_returns_f64(&wrapped)
}

/// (generators) True se o body declara `const __gen_buf = ...` no topo —
/// marcador do generator_desugar.
fn fn_body_declares_gen_buf(body: &[Statement]) -> bool {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::{Decl, Expr, Pat, Stmt};
    for s in body {
        let Statement::Raw(raw) = s;
        let Some(Stmt::Decl(Decl::Var(v))) = raw.stmt.as_ref() else { continue };
        for d in &v.decls {
            if let Pat::Ident(id) = &d.name {
                // eager-buffer: const __gen_buf = []
                if id.id.sym.as_str() == "__gen_buf" {
                    return true;
                }
                // (#477) state-machine ctor: const __g = __RTS_GEN_SM_NEW(...)
                if id.id.sym.as_str() == "__g" {
                    if let Some(Expr::Call(c)) = d.init.as_deref() {
                        if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                            if let Expr::Ident(fid) = callee.as_ref() {
                                if fid.sym.as_str() == "__RTS_GEN_SM_NEW" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// (#376) Infere a classe de retorno de uma fn/metodo SEM return_type anotado:
/// retorna `Some(C)` quando TODOS os `return` do corpo sao `return new C(...)`
/// da MESMA classe registrada C (e ha' ao menos um). Permite chain
/// `v1.add(v2).toString()` quando `add` retorna `new Vec2(...)` sem anotacao.
/// Conservador: se qualquer return diverge (outra classe, expr nao-new, ou
/// return sem arg), retorna None.
fn infer_return_class_from_body(
    body: &[Statement],
    classes: &std::collections::HashMap<String, ClassMeta>,
) -> Option<String> {
    use crate::parser::ast::Statement;
    use swc_ecma_ast::{Expr, Stmt};

    fn new_class_name(e: &Expr) -> Option<String> {
        match e {
            Expr::New(n) => {
                if let Expr::Ident(id) = n.callee.as_ref() {
                    Some(id.sym.to_string())
                } else {
                    None
                }
            }
            Expr::Paren(p) => new_class_name(&p.expr),
            Expr::TsAs(a) => new_class_name(&a.expr),
            Expr::TsNonNull(a) => new_class_name(&a.expr),
            _ => None,
        }
    }

    // Coleta o nome de classe de cada `return` (None se algum diverge).
    fn scan(stmt: &Stmt, found: &mut Vec<Option<String>>) {
        match stmt {
            Stmt::Return(r) => {
                let cls = r.arg.as_deref().and_then(new_class_name);
                found.push(cls);
            }
            Stmt::Block(b) => for s in &b.stmts { scan(s, found); },
            Stmt::If(i) => {
                scan(&i.cons, found);
                if let Some(a) = i.alt.as_deref() { scan(a, found); }
            }
            Stmt::While(w) => scan(&w.body, found),
            Stmt::DoWhile(w) => scan(&w.body, found),
            Stmt::For(f) => scan(&f.body, found),
            Stmt::ForOf(f) => scan(&f.body, found),
            Stmt::ForIn(f) => scan(&f.body, found),
            Stmt::Try(t) => {
                for s in &t.block.stmts { scan(s, found); }
                if let Some(h) = &t.handler { for s in &h.body.stmts { scan(s, found); } }
                if let Some(f) = &t.finalizer { for s in &f.stmts { scan(s, found); } }
            }
            Stmt::Switch(sw) => for c in &sw.cases { for s in &c.cons { scan(s, found); } },
            _ => {}
        }
    }

    let mut found: Vec<Option<String>> = Vec::new();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            scan(stmt, &mut found);
        }
    }
    if found.is_empty() {
        return None;
    }
    // Todos os returns devem ser `new C` da mesma classe registrada.
    let first = found[0].clone()?;
    if !classes.contains_key(&first) {
        return None;
    }
    if found.iter().all(|c| c.as_deref() == Some(first.as_str())) {
        Some(first)
    } else {
        None
    }
}
