//! User-defined function and module-level compilation.
//!
//! `compile_program` declares all user functions first (for forward calls),
//! lowers bodies, then lowers top-level statements into `__RTS_MAIN`.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};
use cranelift_codegen::Context as ClContext;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{Callee, Decl, Expr, ForHead, Lit, MemberProp, Pat, Stmt, TsType, TsTypeRef};

use crate::parser::ast::{
    ClassDecl, ClassMember, FunctionDecl, Item, MemberModifiers, MethodRole, Parameter, Program,
    RawStmt, Statement,
};
use crate::parser::span::Span;

use super::analysis::address_taken::collect_address_taken_fns;
use super::analysis::captures::{
    collect_captures_in_body, collect_local_decls, extract_class_owner,
    make_sync_param_to_global, promote_local_to_global, rename_uses_in_body,
};
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
use super::passes::static_fields::expand_static_fields;
use super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::statements::lower_stmt;

const RUNTIME_MAIN_SYMBOL: &str = crate::abi::symbols::ENTRY_POINT;

/// Info about a user-defined function needed by callers.
#[derive(Debug, Clone)]
struct UserFn {
    id: cranelift_module::FuncId,
    params: Vec<ValTy>,
    ret: Option<ValTy>,
}

/// Lifts inline `() => { ... }` arrow expressions that appear as `I64`-typed
/// ABI arguments into synthetic top-level `FunctionDecl`s so codegen can
/// emit a `func_addr` pointer for them.
///
/// The arrow in the raw SWC statement is replaced with an `Ident` naming
/// the synthetic function. Runs before Phase 1 (declaration) so the lifted
/// functions go through the normal declare → compile path.
fn lift_arrow_callbacks(program: &mut Program) -> HashSet<String> {
    let mut user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
    let mut user_fn_arities: HashMap<String, usize> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((f.name.clone(), f.parameters.len())),
            _ => None,
        })
        .collect();
    // Tipo declarado do primeiro param (ou None se sem annotation /
    // sem params). Usado pelo lifter de thread.spawn pra decidir se
    // injeta `num.f64_from_bits` no trampolim quando worker pede f64.
    let mut user_fn_first_param_ty: HashMap<String, Option<String>> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((
                f.name.clone(),
                f.parameters.first().and_then(|p| p.type_annotation.clone()),
            )),
            _ => None,
        })
        .collect();

    // Top-level aliases: `const fp = worker as unknown as number;`
    //                  ou `const fp = getPointer(worker);`
    // Marca `fp` como alias da user fn para o lifter detectar idents
    // wrappados (necessario p/ thread.spawn, sync.once_call etc).
    fn peel_for_alias<'a>(e: &'a Expr) -> &'a Expr {
        match e {
            Expr::TsAs(a) => peel_for_alias(&a.expr),
            Expr::TsTypeAssertion(a) => peel_for_alias(&a.expr),
            Expr::TsConstAssertion(a) => peel_for_alias(&a.expr),
            Expr::Paren(p) => peel_for_alias(&p.expr),
            // getPointer(fn) → fn
            Expr::Call(c) => {
                if let Callee::Expr(callee) = &c.callee {
                    if let Expr::Ident(id) = callee.as_ref() {
                        if id.sym.as_str() == "getPointer" {
                            if let Some(arg) = c.args.first() {
                                if arg.spread.is_none() {
                                    return peel_for_alias(&arg.expr);
                                }
                            }
                        }
                    }
                }
                e
            }
            _ => e,
        }
    }
    let snapshot = user_fn_names.clone();
    let mut alias_to_real: HashMap<String, String> = HashMap::new();
    for item in program.items.iter() {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl))) = raw.stmt.as_ref() else { continue };
        for d in var_decl.decls.iter() {
            let Some(init) = d.init.as_deref() else { continue };
            let Expr::Ident(id) = peel_for_alias(init) else { continue };
            if !snapshot.contains(id.sym.as_str()) { continue; }
            let swc_ecma_ast::Pat::Ident(name) = &d.name else { continue };
            let alias = name.id.sym.to_string();
            user_fn_names.insert(alias.clone());
            if let Some(&arity) = user_fn_arities.get(id.sym.as_str()) {
                user_fn_arities.insert(alias.clone(), arity);
            }
            if let Some(ty) = user_fn_first_param_ty.get(id.sym.as_str()).cloned() {
                user_fn_first_param_ty.insert(alias.clone(), ty);
            }
            alias_to_real.insert(alias, id.sym.to_string());
        }
    }

    let mut acc = LiftAcc {
        counter: 0,
        new_fns: Vec::new(),
        new_globals: Vec::new(),
        user_fn_names,
        user_fn_arities,
        user_fn_first_param_ty,
        alias_to_real,
        needs_c_callconv: HashSet::new(),
    };

    // Pass 1: dentro de classes (constructors e métodos). Arrows que usam
    // `this` viram trampolins que leem o handle de uma global escrita no
    // callsite imediatamente antes do `widget_set_callback` (etc).
    for item in program.items.iter_mut() {
        let Item::Class(class) = item else { continue };
        let class_name = class.name.clone();
        for member in class.members.iter_mut() {
            match member {
                ClassMember::Constructor(ctor) => {
                    acc.lift_in_body(&class_name, &mut ctor.body, /*in_class=*/ true);
                }
                ClassMember::Method(method) if !method.modifiers.is_static => {
                    acc.lift_in_body(&class_name, &mut method.body, /*in_class=*/ true);
                }
                _ => {}
            }
        }
    }

    // Pass 1.5: funções user top-level. Arrows passados a callbacks ABI
    // dentro de uma fn capturam idents do escopo da fn (params + locais).
    // Para cada captura, criamos uma global `__cb_local_<fn>_<var>` e
    // reescrevemos *toda* referência ao ident na fn pra usar a global.
    // Limitação: múltiplas chamadas da mesma fn compartilham o estado
    // via global. OK pra fns que registram callback uma vez (setup
    // pattern); falha em recursão/reentrada.
    for item in program.items.iter_mut() {
        let Item::Function(f) = item else { continue };
        // Skip lifted/synthetic functions já processadas.
        if f.name.starts_with("__lifted_arrow_") || f.name.starts_with("__class_") {
            continue;
        }
        acc.lift_in_user_fn(f);
    }

    // Pass 2: top-level (arrows em script). Sem `this`. Mantém comportamento
    // anterior.
    let n = program.items.len();
    for i in 0..n {
        let Item::Statement(Statement::Raw(_)) = &program.items[i] else {
            continue;
        };
        // Extrair temporariamente para evitar conflito de borrow.
        let mut taken = std::mem::replace(
            &mut program.items[i],
            Item::Statement(Statement::Raw(RawStmt::new(String::new(), Span::default()))),
        );
        if let Item::Statement(Statement::Raw(raw)) = &mut taken {
            // Empacota num Vec<Statement> de 1 elemento e reaproveita a
            // varredura unificada.
            let placeholder = std::mem::replace(raw, RawStmt::new(String::new(), Span::default()));
            let mut body = vec![Statement::Raw(placeholder)];
            acc.lift_in_body("", &mut body, /*in_class=*/ false);
            // Reescreve o item top-level como o (possivelmente expandido) primeiro
            // statement; pré-statements do callsite (escrita do slot) vão como
            // Items adicionais a inserir.
            // Esperamos que body tenha 1+ statements; o primeiro vira o slot do
            // item original, o resto também vira items.
            let mut iter = body.into_iter();
            if let Some(first) = iter.next() {
                program.items[i] = Item::Statement(first);
                // Inserir os extras logo após. Coletamos num buffer e injetamos
                // depois pra não bagunçar o índice da iteração.
                for extra in iter {
                    acc.new_fns.push(Item::Statement(extra));
                }
            }
        }
    }

    // Globals dos slots `__cb_this_<id>` precisam ser declaradas top-level
    // antes de `collect_module_globals` rodar.
    let mut prepend: Vec<Item> = Vec::new();
    for global_name in acc.new_globals.into_iter() {
        // `let __cb_this_N: number = 0;`
        let var = swc_ecma_ast::VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let,
            declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(swc_ecma_ast::BindingIdent {
                    id: swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: global_name.into(),
                        optional: false,
                    },
                    type_ann: Some(Box::new(swc_ecma_ast::TsTypeAnn {
                        span: Default::default(),
                        type_ann: Box::new(TsType::TsTypeRef(TsTypeRef {
                            span: Default::default(),
                            type_name: swc_ecma_ast::TsEntityName::Ident(swc_ecma_ast::Ident {
                                span: Default::default(),
                                ctxt: Default::default(),
                                sym: "i64".into(),
                                optional: false,
                            }),
                            type_params: None,
                        })),
                    })),
                }),
                init: Some(Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                    span: Default::default(),
                    value: 0.0,
                    raw: None,
                })))),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
        prepend.push(Item::Statement(Statement::Raw(
            RawStmt::new("<cb-slot>".to_string(), Span::default()).with_stmt(stmt),
        )));
    }

    // Funções lifted vão antes dos statements top-level pra fase 1 declará-las.
    for fn_item in acc.new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }
    for global_item in prepend.into_iter().rev() {
        program.items.insert(0, global_item);
    }
    acc.needs_c_callconv
}

struct LiftAcc {
    counter: u32,
    new_fns: Vec<Item>,
    /// Nomes de globais `__cb_this_N` a declarar como `let` top-level.
    new_globals: Vec<String>,
    user_fn_names: HashSet<String>,
    /// Aridade declarada de cada user fn / alias top-level — usada
    /// para que trampolins de `thread.spawn(fp, arg)` repassem o `arg`
    /// quando a worker fn aceita 1+ parâmetros (#206).
    user_fn_arities: HashMap<String, usize>,
    /// Tipo declarado do primeiro param (string raw da annotation, ex:
    /// "number", "i64") ou None. Quando worker de thread.spawn pede
    /// "number" (f64), o trampolim envolve `__rts_spawn_arg` em
    /// `num.f64_from_bits(...)` pra preservar o bit pattern.
    user_fn_first_param_ty: HashMap<String, Option<String>>,
    /// Mapa alias → user fn real para `const fp = worker as ...`. O
    /// trampolim deve invocar a fn real, não o alias (que vira const
    /// global e cai em call_indirect com sig errada).
    alias_to_real: HashMap<String, String>,
    /// User fns chamadas a partir de trampolins C-callconv (lifted)
    /// — devem ser declaradas com C callconv também para evitar
    /// corrupção de stack na fronteira (#206).
    needs_c_callconv: HashSet<String>,
}


impl LiftAcc {
    /// Processa uma função user (não-classe, não-lifted). Detecta locais
    /// capturadas em arrows passados a callbacks ABI, promove cada local
    /// pra global, e reescreve referências na fn inteira. Depois delega
    /// pra `lift_in_body` que faz o lift normal — nesse momento os idents
    /// capturados já apontam pra globais que existem em escopo do trampolim.
    fn lift_in_user_fn(&mut self, f: &mut FunctionDecl) {
        // Coleta locais declaradas e parâmetros — qualquer ident que
        // referencie um desses *dentro de um arrow* é uma captura.
        let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in &f.parameters {
            locals.insert(p.name.clone());
        }
        collect_local_decls(&f.body, &mut locals);

        // Para cada arrow nos statements (recursivamente), descobre
        // quais idents da fn são capturados.
        let captured = collect_captures_in_body(&f.body, &locals);

        // Determina conjunto de parâmetros (vs locais declaradas).
        let param_names: std::collections::HashSet<String> =
            f.parameters.iter().map(|p| p.name.clone()).collect();

        // Promove cada captura pra global e reescreve toda a fn.
        // Insere as syncs de parâmetros no topo (em ordem reversa para
        // manter a ordem original).
        let mut param_syncs: Vec<(String, String)> = Vec::new(); // (global, param)
        for var in &captured {
            let global = format!("__cb_local_{}_{}", sanitize_for_symbol(&f.name), var);
            self.new_globals.push(global.clone());
            if param_names.contains(var) {
                // Parâmetro: precisa sincronizar valor inicial. A reescrita
                // não toca o param em si (continua recebendo o valor do
                // caller), mas todos os usos dentro da fn referem ao
                // global. Sync no topo: `<global> = <param>;`.
                param_syncs.push((global.clone(), var.clone()));
                // Reescreve usos no body (parâmetro permanece declarado).
                rename_uses_in_body(&mut f.body, var, &global);
            } else {
                // Local declarada: promote_local_to_global substitui o
                // `let <var> = expr` por `<global> = expr`.
                promote_local_to_global(&mut f.body, var, &global);
            }
        }

        // Insere syncs de parâmetros no início (ordem original preservada
        // via insert(0, ...) em ordem reversa).
        for (global, param) in param_syncs.iter().rev() {
            f.body.insert(0, make_sync_param_to_global(global, param));
        }

        // Agora roda o lift normal — idents nos arrows são globais,
        // resolvem sem problema.
        self.lift_in_body("", &mut f.body, /*in_class=*/ false);
    }

    /// Lift de uma arrow anônima (sem captura) para uma user fn sintética
    /// `__lifted_arrow_N`. Retorna o `Ident` que substitui a arrow no AST.
    /// Não trata captura de `this` — caller é responsável por garantir que
    /// a arrow não usa `this` (ou está fora de classe).
    fn lift_arrow_to_ident(
        &mut self,
        class_name: &str,
        arrow: &swc_ecma_ast::ArrowExpr,
        in_class: bool,
    ) -> swc_ecma_ast::Ident {
        let has_return_value = matches!(arrow.body.as_ref(), swc_ecma_ast::BlockStmtOrExpr::Expr(_));
        let raw_stmts = arrow_body_to_stmts(arrow);
        let mut body_stmts: Vec<Statement> = raw_stmts
            .into_iter()
            .map(|s| {
                Statement::Raw(
                    RawStmt::new("<lifted>".to_string(), Span::default()).with_stmt(s),
                )
            })
            .collect();

        let syn_name = format!("__lifted_arrow_{}", self.counter);
        self.counter += 1;

        // Recurse para arrows aninhadas.
        self.lift_in_body(class_name, &mut body_stmts, in_class);

        // Expression-body arrows always return a value; block-body arrows
        // with explicit `return` also do, but we can't easily detect that
        // here, so treat block-body as void (the common UI-callback case).
        let ret_ty = if has_return_value { Some("i64".to_string()) } else { Some("void".to_string()) };

        self.new_fns.push(Item::Function(FunctionDecl {
            name: syn_name.clone(),
            parameters: Vec::new(),
            return_type: ret_ty,
            body: body_stmts,
            span: Span::default(),
            is_async: false,
        }));

        swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: syn_name.into(),
            optional: false,
        }
    }

    /// Recursa em sub-blocos procurando `const/let/var x = () => ...` e
    /// substitui o initializer por um `Ident` lifted. Permite que arrow
    /// em VarDecl dentro de fn user funcione (codegen direto só trata
    /// top-level). Capturas já estão promovidas pra global por
    /// `lift_in_user_fn` antes desta passagem.
    fn lift_vardecl_arrows_in_stmt(
        &mut self,
        class_name: &str,
        stmt: &mut Stmt,
        in_class: bool,
    ) {
        match stmt {
            Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
                for declr in var_decl.decls.iter_mut() {
                    if let Some(init) = declr.init.as_mut() {
                        if matches!(init.as_ref(), Expr::Arrow(_)) {
                            if let Expr::Arrow(arrow) = std::mem::replace(
                                init.as_mut(),
                                Expr::Invalid(swc_ecma_ast::Invalid { span: Default::default() }),
                            ) {
                                let ident = self.lift_arrow_to_ident(class_name, &arrow, in_class);
                                **init = Expr::Ident(ident);
                            }
                        }
                    }
                }
            }
            Stmt::If(i) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut i.cons, in_class);
                if let Some(alt) = i.alt.as_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, alt, in_class);
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                }
            }
            Stmt::While(w) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::DoWhile(w) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::For(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForIn(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForOf(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                }
                if let Some(handler) = t.handler.as_mut() {
                    for s in handler.body.stmts.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
                if let Some(finalizer) = t.finalizer.as_mut() {
                    for s in finalizer.stmts.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            Stmt::Labeled(l) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut l.body, in_class);
            }
            Stmt::Switch(sw) => {
                for case in sw.cases.iter_mut() {
                    for s in case.cons.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            _ => {}
        }
    }

    /// Recursa em sub-blocos (if/while/for/block/try) procurando `return arrow`
    /// e substitui a arrow por um `Ident` lifted.
    fn lift_return_arrows_in_stmt(
        &mut self,
        class_name: &str,
        stmt: &mut Stmt,
        in_class: bool,
    ) {
        match stmt {
            Stmt::Return(ret) => {
                if let Some(arg) = ret.arg.as_mut() {
                    if matches!(arg.as_ref(), Expr::Arrow(_)) {
                        if let Expr::Arrow(arrow) = std::mem::replace(
                            arg.as_mut(),
                            Expr::Invalid(swc_ecma_ast::Invalid { span: Default::default() }),
                        ) {
                            let ident = self.lift_arrow_to_ident(class_name, &arrow, in_class);
                            **arg = Expr::Ident(ident);
                        }
                    }
                }
            }
            Stmt::If(i) => {
                self.lift_return_arrows_in_stmt(class_name, &mut i.cons, in_class);
                if let Some(alt) = i.alt.as_mut() {
                    self.lift_return_arrows_in_stmt(class_name, alt, in_class);
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    self.lift_return_arrows_in_stmt(class_name, s, in_class);
                }
            }
            Stmt::While(w) => {
                self.lift_return_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::DoWhile(w) => {
                self.lift_return_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::For(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForIn(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForOf(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    self.lift_return_arrows_in_stmt(class_name, s, in_class);
                }
                if let Some(handler) = t.handler.as_mut() {
                    for s in handler.body.stmts.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
                if let Some(finalizer) = t.finalizer.as_mut() {
                    for s in finalizer.stmts.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            Stmt::Labeled(l) => {
                self.lift_return_arrows_in_stmt(class_name, &mut l.body, in_class);
            }
            Stmt::Switch(sw) => {
                for case in sw.cases.iter_mut() {
                    for s in case.cons.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            _ => {}
        }
    }

    /// Varre `body` em busca de chamadas a funções do namespace ABI cujo arg
    /// I64 é um `ArrowExpr` ou `Ident` apontando pra user fn. Substitui in
    /// place pelo `Ident` da fn lifted, e injeta statements/fns de suporte.
    fn lift_in_body(&mut self, class_name: &str, body: &mut Vec<Statement>, in_class: bool) {
        use crate::abi::AbiType;

        let mut idx = 0usize;
        while idx < body.len() {
            // Lift de arrow em posições não-call: `return arrow` e
            // `const x = arrow`. Recursa em sub-blocos para cobrir
            // ocorrências dentro de control flow. Substitui pela
            // `Ident` da fn sintética; codegen materializa como
            // `func_addr` (i64). Capturas já estão promovidas pra
            // global por `lift_in_user_fn` antes desta passagem,
            // então a fn lifted lê/escreve via global.
            {
                let Statement::Raw(raw) = &mut body[idx];
                if let Some(stmt) = raw.stmt.as_mut() {
                    self.lift_return_arrows_in_stmt(class_name, stmt, in_class);
                    self.lift_vardecl_arrows_in_stmt(class_name, stmt, in_class);
                }
            }

            // Pega CallExpr do statement atual, se houver. Coletamos as
            // mutações separadas: substituições de args + statements a
            // injetar antes deste.
            let Statement::Raw(raw) = &mut body[idx];
            // Aceita tanto `expr_stmt.expr` quanto VarDecl initializer
            // como sede do CallExpr a inspecionar — assim const decls
            // do tipo `const t = thread.spawn(fp, 0)` tambem entram.
            let call: &mut swc_ecma_ast::CallExpr = match raw.stmt.as_mut() {
                Some(Stmt::Expr(expr_stmt)) => match expr_stmt.expr.as_mut() {
                    Expr::Call(c) => c,
                    _ => { idx += 1; continue; }
                },
                Some(Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl))) => {
                    let mut found: Option<*mut swc_ecma_ast::CallExpr> = None;
                    for d in var_decl.decls.iter_mut() {
                        if let Some(init) = d.init.as_deref_mut() {
                            if let Expr::Call(c) = init {
                                found = Some(c as *mut _);
                                break;
                            }
                        }
                    }
                    match found {
                        // SAFETY: o ponteiro vem de um borrow vivo deste
                        // mesmo `var_decl` que persiste pela duracao do
                        // bloco; nenhuma realocacao acontece entre obter
                        // o ptr e usar.
                        Some(p) => unsafe { &mut *p },
                        None => { idx += 1; continue; }
                    }
                }
                _ => { idx += 1; continue; }
            };

            let ns_method = match &call.callee {
                Callee::Expr(ce) => match ce.as_ref() {
                    Expr::Member(m) => match (m.obj.as_ref(), &m.prop) {
                        (Expr::Ident(obj), MemberProp::Ident(prop)) => {
                            Some((obj.sym.to_string(), prop.sym.to_string()))
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            };
            let Some((ns_name, method_name)) = ns_method else {
                // Direct function calls (user fns like describe/test) also need
                // arrow args lifted so codegen can emit a func_addr pointer.
                let is_direct = matches!(&call.callee, Callee::Expr(ce) if matches!(ce.as_ref(), Expr::Ident(_)));
                if is_direct {
                    for arg in call.args.iter_mut() {
                        let body_stmts: Vec<Statement> = match arg.expr.as_ref() {
                            Expr::Arrow(arrow) => arrow_body_to_stmts(arrow)
                                .into_iter()
                                .map(|s| Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default()).with_stmt(s),
                                ))
                                .collect(),
                            _ => continue,
                        };
                        let syn_name = format!("__lifted_arrow_{}", self.counter);
                        self.counter += 1;
                        let mut body_stmts = body_stmts;
                        self.lift_in_body(class_name, &mut body_stmts, in_class);
                        self.new_fns.push(Item::Function(FunctionDecl {
                            name: syn_name.clone(),
                            parameters: Vec::new(),
                            return_type: Some("void".to_string()),
                            body: body_stmts,
                            span: Span::default(),
                            is_async: false,
                        }));
                        *arg.expr = Expr::Ident(swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: syn_name.into(),
                            optional: false,
                        });
                    }
                }
                idx += 1;
                continue;
            };

            let qualified = format!("{ns_name}.{method_name}");
            let Some((_spec, member)) = crate::abi::lookup(&qualified) else {
                idx += 1;
                continue;
            };

            // `pre_stmts` sao statements a inserir antes do callsite (escrita
            // do slot `__cb_this_N = this`).
            let mut pre_stmts: Vec<Statement> = Vec::new();
            // Marca quando precisamos reescrever o callsite atual pra
            // chamar `widget_set_callback_with_ud` em vez de
            // `widget_set_callback`, adicionando `this` como 3º arg.
            let mut pending_userdata_rewrite = false;

            // thread.spawn (U64, U64): so o primeiro arg (fn_ptr) deve ser
            // tratado como callback candidato. Demais membros de ABI seguem
            // a regra padrao (apenas args I64).
            let is_thread_spawn = qualified == "thread.spawn";
            let is_parallel_map = qualified == "parallel.map";
            let is_parallel_for_each = qualified == "parallel.for_each";
            let is_parallel_reduce = qualified == "parallel.reduce";
            let is_parallel_op = is_parallel_map || is_parallel_for_each || is_parallel_reduce;
            for (arg_idx, (arg, &abi_ty)) in call.args.iter_mut().zip(member.args.iter()).enumerate() {
                let is_callback_slot = if is_thread_spawn {
                    arg_idx == 0
                } else if is_parallel_op {
                    // fn_ptr slot is U64 in parallel.* ABIs
                    abi_ty == AbiType::U64
                } else {
                    abi_ty == AbiType::I64
                };
                if !is_callback_slot {
                    continue;
                }

                // Decide qual variante:
                //  (a) Arrow capturando `this` dentro de classe → trampolim
                //      com slot global.
                //  (b) Arrow simples (sem `this`) → lift comum.
                //  (c) Ident apontando pra user fn → wrapper zero-arg.
                let arrow_uses_this = if in_class {
                    matches!(arg.expr.as_ref(), Expr::Arrow(arrow) if arrow_uses_this(arrow))
                } else {
                    false
                };

                let body_stmts: Vec<Statement>;
                let mut needs_this_slot: Option<String> = None; // slot global (path antigo)
                // Quando true: callsite será reescrito pra usar
                // `widget_set_callback_with_ud` passando `this` como
                // userdata. Trampolim recebe `this` como parâmetro
                // — sem slot global, sem limitação \"última vence\".
                let mut use_userdata_callback = false;
                let is_widget_set_callback = qualified == "ui.widget_set_callback";

                // Peel TsAs/TsTypeAssertion/TsConstAssertion/Paren para
                // detectar idents wrappados por type assertions (ex:
                // `worker as unknown as number` em thread.spawn).
                fn peel_ts<'a>(e: &'a Expr) -> &'a Expr {
                    match e {
                        Expr::TsAs(a) => peel_ts(&a.expr),
                        Expr::TsTypeAssertion(a) => peel_ts(&a.expr),
                        Expr::TsConstAssertion(a) => peel_ts(&a.expr),
                        Expr::Paren(p) => peel_ts(&p.expr),
                        _ => e,
                    }
                }
                match peel_ts(arg.expr.as_ref()) {
                    Expr::Arrow(arrow) if arrow_uses_this && is_widget_set_callback => {
                        // Path NOVO (#148): trampolim recebe `this` por
                        // parâmetro. O callsite é reescrito abaixo.
                        use_userdata_callback = true;
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        body_stmts = raw_stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Arrow(arrow) if arrow_uses_this => {
                        // Path antigo (slot global): usado por callsites
                        // que não têm variante `_with_ud` no ABI ainda
                        // (window_set_callback, widget_set_draw,
                        // menubar_add). Mantém limitação \"última vence\".
                        let slot = format!("__cb_this_{}", self.counter);
                        needs_this_slot = Some(slot.clone());
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        let prologue = make_this_local(class_name, &slot);
                        let mut stmts: Vec<swc_ecma_ast::Stmt> = raw_stmts;
                        stmts.insert(0, prologue);
                        body_stmts = stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Arrow(arrow) => {
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        body_stmts = raw_stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Ident(id) if self.user_fn_names.contains(id.sym.as_str()) => {
                        // Resolve alias → fn real. Sem isso, trampolim
                        // chamaria o alias (const global i64), caindo em
                        // call_indirect com sig padrão divergente da fn
                        // real (#206).
                        let real_name = self
                            .alias_to_real
                            .get(id.sym.as_str())
                            .cloned()
                            .unwrap_or_else(|| id.sym.to_string());
                        let target_id = swc_ecma_ast::Ident {
                            span: id.span,
                            ctxt: id.ctxt,
                            sym: real_name.clone().into(),
                            optional: false,
                        };
                        let arity = self
                            .user_fn_arities
                            .get(real_name.as_str())
                            .copied()
                            .unwrap_or(0);
                        let pass_arg = is_thread_spawn && arity >= 1;
                        if is_thread_spawn {
                            self.needs_c_callconv.insert(real_name.clone());
                        }

                        // parallel.* trampolim: adapts i64 ABI to user fn.
                        // Rayon passes Vec<i64> elements as i64 (integer
                        // registers). User fns may declare `number` (f64)
                        // params — codegen coerces automatically via
                        // `lower_user_call`. Trampolim bridges the gap.
                        if is_parallel_op {
                            fn par_ident(sym: &str) -> Expr {
                                Expr::Ident(swc_ecma_ast::Ident {
                                    span: Default::default(),
                                    ctxt: Default::default(),
                                    sym: sym.to_string().into(),
                                    optional: false,
                                })
                            }
                            fn par_arg(sym: &str) -> swc_ecma_ast::ExprOrSpread {
                                swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(par_ident(sym)),
                                }
                            }
                            let call_args: Vec<swc_ecma_ast::ExprOrSpread> =
                                if is_parallel_reduce {
                                    vec![par_arg("__par_acc"), par_arg("__par_x")]
                                } else {
                                    vec![par_arg("__par_x")]
                                };
                            let call_expr = Expr::Call(swc_ecma_ast::CallExpr {
                                span: Default::default(),
                                ctxt: Default::default(),
                                callee: Callee::Expr(Box::new(Expr::Ident(target_id))),
                                args: call_args,
                                type_args: None,
                            });
                            let body_stmt = if is_parallel_for_each {
                                Stmt::Expr(swc_ecma_ast::ExprStmt {
                                    span: Default::default(),
                                    expr: Box::new(call_expr),
                                })
                            } else {
                                Stmt::Return(swc_ecma_ast::ReturnStmt {
                                    span: Default::default(),
                                    arg: Some(Box::new(call_expr)),
                                })
                            };
                            body_stmts = vec![Statement::Raw(
                                RawStmt::new("<par-tramp>".to_string(), Span::default())
                                    .with_stmt(body_stmt),
                            )];
                        } else {
                            // Decide nome do param: __rts_spawn_arg_f64
                            // se worker pede `number`, senao
                            // __rts_spawn_arg. Esse mesmo nome e usado
                            // tanto na decl do trampolim (acima) quanto
                            // no ident que passa pro worker.
                            let worker_wants_f64 = pass_arg && matches!(
                                self.user_fn_first_param_ty.get(real_name.as_str()),
                                Some(Some(ty)) if ty == "number" || ty == "f64"
                            );
                            let arg_name = if worker_wants_f64 {
                                "__rts_spawn_arg_f64"
                            } else {
                                "__rts_spawn_arg"
                            };
                            let args: Vec<swc_ecma_ast::ExprOrSpread> = if pass_arg {
                                vec![swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        sym: arg_name.into(),
                                        optional: false,
                                    })),
                                }]
                            } else {
                                Vec::new()
                            };
                            let call_stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
                                span: id.span,
                                expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
                                    span: id.span,
                                    ctxt: id.ctxt,
                                    callee: Callee::Expr(Box::new(Expr::Ident(target_id))),
                                    args,
                                    type_args: None,
                                })),
                            });
                            body_stmts = vec![Statement::Raw(
                                RawStmt::new("<lifted>".to_string(), Span::default())
                                    .with_stmt(call_stmt),
                            )];
                        }
                    }
                    _ => continue,
                };

                // Nome mangled quando o trampolim captura `this` —
                // habilita `current_class` no codegen via
                // `extract_class_owner`, o que destrava `Expr::This`,
                // `super.method()` e dispatch virtual.
                let captures_this = needs_this_slot.is_some() || use_userdata_callback;
                let syn_name = if captures_this {
                    format!("__class_{}_lifted_arrow_{}", class_name, self.counter)
                } else {
                    format!("__lifted_arrow_{}", self.counter)
                };
                self.counter += 1;

                // Recurse pra arrows aninhadas no body do trampolim.
                let mut body_stmts = body_stmts;
                self.lift_in_body(class_name, &mut body_stmts, in_class);

                // Trampolim recebe `this: ClassName` como parâmetro
                // quando vamos passar `this` por userdata. Para
                // `thread.spawn(fp, arg)` com worker arity≥1, recebe
                // `__rts_spawn_arg: number`. Parallel ops recebem
                // parâmetros i64 (Rayon passa Vec<i64> elements).
                // Caso contrário: sem parâmetros (UI callbacks tradicionais).
                fn mk_i64_param(name: &str) -> Parameter {
                    Parameter {
                        name: name.to_string(),
                        type_annotation: Some("i64".to_string()),
                        modifiers: MemberModifiers::default(),
                        variadic: false,
                        default: None,
                        span: Span::default(),
                    }
                }
                let (parameters, tramp_return_type): (Vec<Parameter>, &'static str) =
                    if use_userdata_callback {
                        (
                            vec![Parameter {
                                name: "this".to_string(),
                                type_annotation: Some(class_name.to_string()),
                                modifiers: MemberModifiers::default(),
                                variadic: false,
                                default: None,
                                span: Span::default(),
                            }],
                            "void",
                        )
                    } else if is_parallel_reduce {
                        (vec![mk_i64_param("__par_acc"), mk_i64_param("__par_x")], "i64")
                    } else if is_parallel_map {
                        (vec![mk_i64_param("__par_x")], "i64")
                    } else if is_parallel_for_each {
                        (vec![mk_i64_param("__par_x")], "void")
                    } else if is_thread_spawn
                        && matches!(peel_ts(arg.expr.as_ref()), Expr::Ident(id) if {
                            let real = self.alias_to_real.get(id.sym.as_str()).cloned()
                                .unwrap_or_else(|| id.sym.to_string());
                            self.user_fn_arities.get(real.as_str()).copied().unwrap_or(0) >= 1
                        })
                    {
                        // Worker pode pedir `number` (f64) ou `i64`. Pra
                        // f64, marcamos o param com nome especial
                        // `__rts_spawn_arg_f64` — `compile_user_fn` detecta
                        // o sufixo, gera bind com bitcast i64→f64 (caller
                        // passa bits via U64 extern arg, NAO numerico).
                        // Sem isso, codegen faria fcvt_from_sint e
                        // worker receberia valor errado.
                        let real_for_ty = match peel_ts(arg.expr.as_ref()) {
                            Expr::Ident(id) => self.alias_to_real.get(id.sym.as_str()).cloned()
                                .unwrap_or_else(|| id.sym.to_string()),
                            _ => String::new(),
                        };
                        let worker_wants_f64 = matches!(
                            self.user_fn_first_param_ty.get(real_for_ty.as_str()),
                            Some(Some(ty)) if ty == "number" || ty == "f64"
                        );
                        let pname = if worker_wants_f64 {
                            "__rts_spawn_arg_f64"
                        } else {
                            "__rts_spawn_arg"
                        };
                        (
                            vec![Parameter {
                                name: pname.to_string(),
                                type_annotation: Some("i64".to_string()),
                                modifiers: MemberModifiers::default(),
                                variadic: false,
                                default: None,
                                span: Span::default(),
                            }],
                            "void",
                        )
                    } else {
                        (Vec::new(), "void")
                    };

                self.new_fns.push(Item::Function(FunctionDecl {
                    name: syn_name.clone(),
                    parameters,
                    return_type: Some(tramp_return_type.to_string()),
                    body: body_stmts,
                    span: Span::default(),
                    is_async: false,
                }));

                if let Some(slot_name) = needs_this_slot {
                    self.new_globals.push(slot_name.clone());
                    pre_stmts.push(make_slot_assign(&slot_name));
                }

                *arg.expr = Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: syn_name.into(),
                    optional: false,
                });

                // Se vamos passar userdata, marca o callsite pra
                // reescrita posterior. Mais simples fazer fora do loop
                // de args — ver `pending_userdata_rewrite` abaixo.
                if use_userdata_callback {
                    pending_userdata_rewrite = true;
                }
            }

            // Reescrita do callsite quando o trampolim captura `this`
            // via parâmetro (path novo de #148). Substitui o callee
            // `ui.widget_set_callback` por `ui.widget_set_callback_with_ud`
            // e anexa `this` como 3º argumento.
            if pending_userdata_rewrite {
                if let Callee::Expr(callee_expr) = &mut call.callee {
                    if let Expr::Member(m) = callee_expr.as_mut() {
                        if let MemberProp::Ident(prop_id) = &mut m.prop {
                            prop_id.sym = "widget_set_callback_with_ud".into();
                        }
                    }
                }
                // Adiciona `this` como 3º arg.
                call.args.push(swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
                        span: Default::default(),
                    })),
                });
            }

            // Injeta os pre_stmts antes do callsite atual.
            let pre_count = pre_stmts.len();
            if pre_count > 0 {
                for (k, s) in pre_stmts.into_iter().enumerate() {
                    body.insert(idx + k, s);
                }
                idx += pre_count;
            }
            idx += 1;
        }
    }
}

fn arrow_uses_this(arrow: &swc_ecma_ast::ArrowExpr) -> bool {
    use swc_ecma_ast::BlockStmtOrExpr;
    let mut found = false;
    match arrow.body.as_ref() {
        BlockStmtOrExpr::BlockStmt(block) => {
            for s in &block.stmts {
                if stmt_uses_this(s) {
                    found = true;
                    break;
                }
            }
        }
        BlockStmtOrExpr::Expr(expr) => {
            found = expr_uses_this(expr);
        }
    }
    found
}

fn stmt_uses_this(stmt: &Stmt) -> bool {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => expr_uses_this(&e.expr),
        Return(r) => r.arg.as_deref().map_or(false, expr_uses_this),
        If(i) => {
            expr_uses_this(&i.test)
                || stmt_uses_this(&i.cons)
                || i.alt.as_deref().map_or(false, stmt_uses_this)
        }
        Block(b) => b.stmts.iter().any(stmt_uses_this),
        While(w) => expr_uses_this(&w.test) || stmt_uses_this(&w.body),
        DoWhile(w) => expr_uses_this(&w.test) || stmt_uses_this(&w.body),
        For(f) => {
            f.init.as_ref().map_or(false, |init| match init {
                swc_ecma_ast::VarDeclOrExpr::Expr(e) => expr_uses_this(e),
                swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => v
                    .decls
                    .iter()
                    .any(|d| d.init.as_deref().map_or(false, expr_uses_this)),
            }) || f.test.as_deref().map_or(false, expr_uses_this)
                || f.update.as_deref().map_or(false, expr_uses_this)
                || stmt_uses_this(&f.body)
        }
        ForOf(f) => expr_uses_this(&f.right) || stmt_uses_this(&f.body),
        Decl(swc_ecma_ast::Decl::Var(v)) => v
            .decls
            .iter()
            .any(|d| d.init.as_deref().map_or(false, expr_uses_this)),
        Try(t) => {
            t.block.stmts.iter().any(stmt_uses_this)
                || t.handler
                    .as_ref()
                    .map_or(false, |h| h.body.stmts.iter().any(stmt_uses_this))
                || t.finalizer
                    .as_ref()
                    .map_or(false, |f| f.stmts.iter().any(stmt_uses_this))
        }
        _ => false,
    }
}

fn expr_uses_this(expr: &Expr) -> bool {
    use swc_ecma_ast::Expr::*;
    match expr {
        This(_) => true,
        // `super.method(...)` e `super[...]` também precisam do contexto
        // de classe — tratá-los como uso de `this` força o trampolim a
        // virar `__class_C_lifted_arrow_N` (que popula current_class).
        SuperProp(_) => true,
        Member(m) => expr_uses_this(&m.obj),
        Bin(b) => expr_uses_this(&b.left) || expr_uses_this(&b.right),
        Unary(u) => expr_uses_this(&u.arg),
        Update(u) => expr_uses_this(&u.arg),
        Assign(a) => {
            let lhs = match &a.left {
                swc_ecma_ast::AssignTarget::Simple(s) => match s {
                    swc_ecma_ast::SimpleAssignTarget::Ident(_) => false,
                    swc_ecma_ast::SimpleAssignTarget::Member(m) => expr_uses_this(&m.obj),
                    _ => false,
                },
                _ => false,
            };
            lhs || expr_uses_this(&a.right)
        }
        Call(c) => {
            let callee_uses = match &c.callee {
                Callee::Expr(e) => expr_uses_this(e),
                Callee::Super(_) => true,
                _ => false,
            };
            callee_uses || c.args.iter().any(|a| expr_uses_this(&a.expr))
        }
        New(n) => n
            .args
            .as_ref()
            .map_or(false, |args| args.iter().any(|a| expr_uses_this(&a.expr))),
        Cond(c) => expr_uses_this(&c.test) || expr_uses_this(&c.cons) || expr_uses_this(&c.alt),
        Paren(p) => expr_uses_this(&p.expr),
        Tpl(t) => t.exprs.iter().any(|e| expr_uses_this(e)),
        Array(a) => a
            .elems
            .iter()
            .any(|e| e.as_ref().map_or(false, |el| expr_uses_this(&el.expr))),
        Seq(s) => s.exprs.iter().any(|e| expr_uses_this(e)),
        _ => false,
    }
}

fn arrow_body_to_stmts(arrow: &swc_ecma_ast::ArrowExpr) -> Vec<Stmt> {
    use swc_ecma_ast::BlockStmtOrExpr;
    match arrow.body.as_ref() {
        BlockStmtOrExpr::BlockStmt(block) => block.stmts.clone(),
        BlockStmtOrExpr::Expr(expr) => {
            vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: Default::default(),
                arg: Some(expr.clone()),
            })]
        }
    }
}

// NOTE: As funções `rewrite_*` e `revert_*` abaixo eram usadas pela
// estratégia anterior (renomear `this`→`__this` no body do trampolim).
// A estratégia atual usa nome mangled `__class_C_lifted_arrow_N` +
// `let this: C = ...` no prólogo, então `this` permanece intacto.
// Mantenho as funções marcadas como `#[allow(dead_code)]` por enquanto
// — limpeza num commit separado quando o approach se mostrar estável.

#[allow(dead_code)]
fn rewrite_this_to_under_this(mut s: Stmt) -> Stmt {
    rewrite_stmt(&mut s);
    s
}

#[allow(dead_code)]
fn rewrite_stmt(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => rewrite_expr(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                rewrite_expr(a);
            }
        }
        If(i) => {
            rewrite_expr(&mut i.test);
            rewrite_stmt(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                rewrite_stmt(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                rewrite_stmt(s);
            }
        }
        While(w) => {
            rewrite_expr(&mut w.test);
            rewrite_stmt(&mut w.body);
        }
        DoWhile(w) => {
            rewrite_expr(&mut w.test);
            rewrite_stmt(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => rewrite_expr(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                rewrite_expr(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                rewrite_expr(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                rewrite_expr(u);
            }
            rewrite_stmt(&mut f.body);
        }
        ForOf(f) => {
            rewrite_expr(&mut f.right);
            rewrite_stmt(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    rewrite_expr(e);
                }
            }
        }
        Try(t) => {
            for s in &mut t.block.stmts {
                rewrite_stmt(s);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in &mut h.body.stmts {
                    rewrite_stmt(s);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in &mut f.stmts {
                    rewrite_stmt(s);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn rewrite_expr(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    // Substitui `this` por Ident("__this") in-place.
    if matches!(expr, This(_)) {
        *expr = Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: "__this".into(),
            optional: false,
        });
        return;
    }
    match expr {
        Member(m) => rewrite_expr(&mut m.obj),
        Bin(b) => {
            rewrite_expr(&mut b.left);
            rewrite_expr(&mut b.right);
        }
        Unary(u) => rewrite_expr(&mut u.arg),
        Update(u) => rewrite_expr(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                rewrite_expr(&mut m.obj);
            }
            rewrite_expr(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                rewrite_expr(e);
            }
            for a in &mut c.args {
                rewrite_expr(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    rewrite_expr(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            rewrite_expr(&mut c.test);
            rewrite_expr(&mut c.cons);
            rewrite_expr(&mut c.alt);
        }
        Paren(p) => rewrite_expr(&mut p.expr),
        Tpl(t) => {
            for e in &mut t.exprs {
                rewrite_expr(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rewrite_expr(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                rewrite_expr(e);
            }
        }
        _ => {}
    }
}

/// Inside any nested `Expr::Arrow` found in `stmts`, revert `__this`
/// identifiers back to `this`. Used after the outer arrow's body had
/// `this`→`__this` rewritten: inner arrows kept the rewrite, but they
/// will be lifted to their own trampolines that re-bind `__this`
/// from their own slot, so they need to start with `this` again.
/// Statements outside arrows are left as is (the outer trampoline
/// owns those and binds `__this` itself).
#[allow(dead_code)]
fn revert_under_this_inside_arrows(stmts: &mut [Statement]) {
    for s in stmts.iter_mut() {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_mut() {
            revert_stmt_arrows(stmt);
        }
    }
}

#[allow(dead_code)]
fn revert_stmt_arrows(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => revert_expr_arrows(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                revert_expr_arrows(a);
            }
        }
        If(i) => {
            revert_expr_arrows(&mut i.test);
            revert_stmt_arrows(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                revert_stmt_arrows(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                revert_stmt_arrows(s);
            }
        }
        While(w) => {
            revert_expr_arrows(&mut w.test);
            revert_stmt_arrows(&mut w.body);
        }
        DoWhile(w) => {
            revert_expr_arrows(&mut w.test);
            revert_stmt_arrows(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => revert_expr_arrows(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                revert_expr_arrows(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                revert_expr_arrows(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                revert_expr_arrows(u);
            }
            revert_stmt_arrows(&mut f.body);
        }
        ForOf(f) => {
            revert_expr_arrows(&mut f.right);
            revert_stmt_arrows(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    revert_expr_arrows(e);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_expr_arrows(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    match expr {
        Arrow(arrow) => {
            // Within the arrow's body, swap `__this` ident for `Expr::This`.
            match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        revert_under_this_in_stmt(s);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    revert_under_this_in_expr(e);
                }
            }
        }
        Member(m) => revert_expr_arrows(&mut m.obj),
        Bin(b) => {
            revert_expr_arrows(&mut b.left);
            revert_expr_arrows(&mut b.right);
        }
        Unary(u) => revert_expr_arrows(&mut u.arg),
        Update(u) => revert_expr_arrows(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                revert_expr_arrows(&mut m.obj);
            }
            revert_expr_arrows(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                revert_expr_arrows(e);
            }
            for a in &mut c.args {
                revert_expr_arrows(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    revert_expr_arrows(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            revert_expr_arrows(&mut c.test);
            revert_expr_arrows(&mut c.cons);
            revert_expr_arrows(&mut c.alt);
        }
        Paren(p) => revert_expr_arrows(&mut p.expr),
        Tpl(t) => {
            for e in &mut t.exprs {
                revert_expr_arrows(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                revert_expr_arrows(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                revert_expr_arrows(e);
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_under_this_in_stmt(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => revert_under_this_in_expr(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                revert_under_this_in_expr(a);
            }
        }
        If(i) => {
            revert_under_this_in_expr(&mut i.test);
            revert_under_this_in_stmt(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                revert_under_this_in_stmt(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                revert_under_this_in_stmt(s);
            }
        }
        While(w) => {
            revert_under_this_in_expr(&mut w.test);
            revert_under_this_in_stmt(&mut w.body);
        }
        DoWhile(w) => {
            revert_under_this_in_expr(&mut w.test);
            revert_under_this_in_stmt(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => revert_under_this_in_expr(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                revert_under_this_in_expr(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                revert_under_this_in_expr(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                revert_under_this_in_expr(u);
            }
            revert_under_this_in_stmt(&mut f.body);
        }
        ForOf(f) => {
            revert_under_this_in_expr(&mut f.right);
            revert_under_this_in_stmt(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    revert_under_this_in_expr(e);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_under_this_in_expr(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    if let Ident(id) = expr {
        if id.sym.as_ref() == "__this" {
            *expr = Expr::This(swc_ecma_ast::ThisExpr {
                span: Default::default(),
            });
            return;
        }
    }
    match expr {
        Member(m) => revert_under_this_in_expr(&mut m.obj),
        Bin(b) => {
            revert_under_this_in_expr(&mut b.left);
            revert_under_this_in_expr(&mut b.right);
        }
        Unary(u) => revert_under_this_in_expr(&mut u.arg),
        Update(u) => revert_under_this_in_expr(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                revert_under_this_in_expr(&mut m.obj);
            }
            revert_under_this_in_expr(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                revert_under_this_in_expr(e);
            }
            for a in &mut c.args {
                revert_under_this_in_expr(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    revert_under_this_in_expr(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            revert_under_this_in_expr(&mut c.test);
            revert_under_this_in_expr(&mut c.cons);
            revert_under_this_in_expr(&mut c.alt);
        }
        Paren(p) => revert_under_this_in_expr(&mut p.expr),
        Arrow(arrow) => {
            // Recurse into arrow body too — same rule applies to nested
            // arrows: any `__this` they hold should revert to `this` so
            // their own lift sees the canonical form.
            match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        revert_under_this_in_stmt(s);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    revert_under_this_in_expr(e);
                }
            }
        }
        Tpl(t) => {
            for e in &mut t.exprs {
                revert_under_this_in_expr(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                revert_under_this_in_expr(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                revert_under_this_in_expr(e);
            }
        }
        _ => {}
    }
}

/// `let this: ClassName = __cb_this_N;` — o nome do bind é `this`
/// para que `read_local("this")` no codegen retorne o handle da
/// instância. Combinado com o nome mangled `__class_C_lifted_arrow_N`
/// (que faz `current_class = Some("C")`), `Expr::This` e
/// `super.method()` funcionam normalmente dentro do trampolim.
fn make_this_local(class_name: &str, slot_name: &str) -> Stmt {
    let cls_ann = TsType::TsTypeRef(TsTypeRef {
        span: Default::default(),
        type_name: swc_ecma_ast::TsEntityName::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: class_name.into(),
            optional: false,
        }),
        type_params: None,
    });
    let init = Expr::Ident(swc_ecma_ast::Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: slot_name.into(),
        optional: false,
    });
    let var = swc_ecma_ast::VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: swc_ecma_ast::VarDeclKind::Let,
        declare: false,
        decls: vec![swc_ecma_ast::VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: "this".into(),
                    optional: false,
                },
                type_ann: Some(Box::new(swc_ecma_ast::TsTypeAnn {
                    span: Default::default(),
                    type_ann: Box::new(cls_ann),
                })),
            }),
            init: Some(Box::new(init)),
            definite: false,
        }],
    };
    Stmt::Decl(Decl::Var(Box::new(var)))
}

/// `__cb_this_N = this;`
fn make_slot_assign(slot_name: &str) -> Statement {
    let rhs: Expr = Expr::This(swc_ecma_ast::ThisExpr {
        span: Default::default(),
    });
    let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
        span: Default::default(),
        op: swc_ecma_ast::AssignOp::Assign,
        left: swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(
            swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: slot_name.into(),
                    optional: false,
                },
                type_ann: None,
            },
        )),
        right: Box::new(rhs),
    });
    let stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(assign),
    });
    Statement::Raw(RawStmt::new("<cb-slot-set>".to_string(), Span::default()).with_stmt(stmt))
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

// ── Class lowering ────────────────────────────────────────────────────────

/// Sintetiza os FunctionDecl para uma classe: constructor + cada metodo
/// vira uma funcao independente que recebe `this` como primeiro parametro.
/// Retorna o ClassMeta usado pelo codegen para resolver `new` e dispatch.
/// Verifica que toda classe concreta implementa os métodos abstract
/// herdados de seus ancestrais. Coleta o conjunto de abstracts da
/// hierarquia, subtrai os métodos concretos efetivamente declarados
/// e exige conjunto vazio.
fn validate_abstract_method_implementations(classes: &HashMap<String, ClassMeta>) -> Result<()> {
    for (name, meta) in classes {
        if meta.is_abstract {
            continue; // abstract classes podem deixar abstracts pendentes
        }

        // Acumula abstracts da hierarquia.
        let mut required: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut cur = Some(name.clone());
        while let Some(c) = cur {
            if let Some(m) = classes.get(&c) {
                for am in &m.abstract_methods {
                    required.insert(am.clone());
                }
                cur = m.super_class.clone();
            } else {
                break;
            }
        }

        // Subtrai métodos concretos providos pela classe ou ancestrais.
        let mut cur = Some(name.clone());
        while let Some(c) = cur {
            if let Some(m) = classes.get(&c) {
                for method in &m.methods {
                    if !m.abstract_methods.contains(method) {
                        required.remove(method);
                    }
                }
                cur = m.super_class.clone();
            } else {
                break;
            }
        }

        if !required.is_empty() {
            let mut missing: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
            missing.sort();
            return Err(anyhow!(
                "classe concreta `{name}` nao implementa metodo(s) abstract: {}",
                missing.join(", ")
            ));
        }
    }
    Ok(())
}

/// (#nested-this) Escaneia um statement por padroes
/// `this.X = { k: v, ... }` ou `this.X = { k: { ... } }` e popula
/// field_obj_types / field_nested_obj_types. Inferencia simples
/// baseada em tipos de literais (Str→Handle, Num→I64, Bool→Bool).
fn scan_this_obj_assign(
    s: &Statement,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    let Statement::Raw(rs) = s;
    let Some(stmt) = rs.stmt.as_ref() else { return };
    scan_this_stmt(stmt, field_obj_types, field_nested_obj_types);
}

fn scan_this_stmt(
    stmt: &swc_ecma_ast::Stmt,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    use swc_ecma_ast::*;
    match stmt {
        Stmt::Expr(e) => scan_this_expr(&e.expr, field_obj_types, field_nested_obj_types),
        Stmt::Block(b) => {
            for s in &b.stmts {
                scan_this_stmt(s, field_obj_types, field_nested_obj_types);
            }
        }
        Stmt::If(i) => {
            scan_this_stmt(&i.cons, field_obj_types, field_nested_obj_types);
            if let Some(alt) = &i.alt {
                scan_this_stmt(alt, field_obj_types, field_nested_obj_types);
            }
        }
        _ => {}
    }
}

fn scan_this_expr(
    e: &swc_ecma_ast::Expr,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    use swc_ecma_ast::*;
    let Expr::Assign(a) = e else { return };
    if !matches!(a.op, AssignOp::Assign) {
        return;
    }
    // LHS: this.X
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return;
    };
    if !matches!(m.obj.as_ref(), Expr::This(_)) {
        return;
    }
    let MemberProp::Ident(field_id) = &m.prop else {
        return;
    };
    let field_name = field_id.sym.as_str().to_string();
    // RHS: object literal
    let Expr::Object(obj) = a.right.as_ref() else {
        return;
    };
    let mut fts: HashMap<String, ValTy> = HashMap::new();
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = p.as_ref() {
                let key = match &kv.key {
                    PropName::Ident(i) => i.sym.as_str().to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    _ => continue,
                };
                match kv.value.as_ref() {
                    Expr::Lit(Lit::Str(_)) => {
                        fts.insert(key, ValTy::Handle);
                    }
                    Expr::Lit(Lit::Num(_)) => {
                        fts.insert(key, ValTy::I64);
                    }
                    Expr::Lit(Lit::Bool(_)) => {
                        fts.insert(key, ValTy::Bool);
                    }
                    Expr::Object(sub) => {
                        fts.insert(key.clone(), ValTy::Handle);
                        let mut sub_fts: HashMap<String, ValTy> = HashMap::new();
                        for sp in &sub.props {
                            if let PropOrSpread::Prop(spx) = sp {
                                if let Prop::KeyValue(skv) = spx.as_ref() {
                                    let sk = match &skv.key {
                                        PropName::Ident(i) => i.sym.as_str().to_string(),
                                        PropName::Str(s) => {
                                            s.value.to_string_lossy().to_string()
                                        }
                                        _ => continue,
                                    };
                                    match skv.value.as_ref() {
                                        Expr::Lit(Lit::Str(_)) => {
                                            sub_fts.insert(sk, ValTy::Handle);
                                        }
                                        Expr::Lit(Lit::Num(_)) => {
                                            sub_fts.insert(sk, ValTy::I64);
                                        }
                                        Expr::Lit(Lit::Bool(_)) => {
                                            sub_fts.insert(sk, ValTy::Bool);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if !sub_fts.is_empty() {
                            field_nested_obj_types
                                .insert((field_name.clone(), key), sub_fts);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if !fts.is_empty() {
        field_obj_types.insert(field_name, fts);
    }
}

fn synthesize_class_fns(class: &ClassDecl) -> (ClassMeta, Vec<FunctionDecl>) {
    let mut methods: Vec<String> = Vec::new();
    let mut getters: Vec<String> = Vec::new();
    let mut setters: Vec<String> = Vec::new();
    let mut static_methods: Vec<String> = Vec::new();
    let mut static_fields: Vec<String> = Vec::new();
    let mut fns: Vec<FunctionDecl> = Vec::new();
    let mut field_types: HashMap<String, ValTy> = HashMap::new();
    let mut field_class_names: HashMap<String, String> = HashMap::new();
    let mut field_obj_types: HashMap<String, HashMap<String, ValTy>> = HashMap::new();
    let mut field_nested_obj_types: HashMap<(String, String), HashMap<String, ValTy>> =
        HashMap::new();
    let mut readonly_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut abstract_methods: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut member_visibility: std::collections::HashMap<String, crate::parser::ast::Visibility> =
        std::collections::HashMap::new();
    let mut has_constructor = false;

    // Coleta initializers de instância (`x = expr`) na ordem declarada.
    // Serão prependidos ao body do constructor (depois de `super()` se
    // houver). Static props ficam fora — initializers static seriam
    // tratados separadamente (não cobertos neste commit).
    let init_stmts: Vec<Statement> = class
        .members
        .iter()
        .filter_map(|m| match m {
            ClassMember::Property(prop)
                if !prop.modifiers.is_static && prop.initializer.is_some() =>
            {
                let init = prop.initializer.as_ref().unwrap().clone();
                Some(make_field_init_stmt(&prop.name, init, prop.span))
            }
            _ => None,
        })
        .collect();

    for member in &class.members {
        match member {
            ClassMember::Constructor(ctor) => {
                has_constructor = true;
                // (#303 parte 1) Detecta \`super(...); super(...)\` em
                // sequencia direta (mesmo bloco/escopo top-level do
                // constructor body). JS proibe — segundo super lanca
                // ReferenceError. So' rejeita o caso linear obvio; em
                // branches if/else mutuamente exclusivos (super em cons
                // E em alt), passa silenciosamente — runtime check seria
                // a fase 2 dessa issue.
                if count_super_calls_in_top_level(&ctor.body) > 1 {
                    // SyntaxError-like: rejeita o programa em compile time.
                    // Usar eprintln + std::process::exit(1) imita o caminho
                    // de erros existentes em outras validacoes (abstract,
                    // visibility). Sem Result aqui pra nao espalhar
                    // mudanca de tipo em todo synthesize_class_fns caller.
                    eprintln!(
                        "error: ReferenceError: super constructor may only be called once (em `{}`)",
                        class.name
                    );
                    std::process::exit(1);
                }
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
                // Body = [super() se houver no inicio] + initializers + user code.
                // Detecta `super(...)` na primeira posição e injeta initializers
                // logo depois (semântica TS: initializers rodam depois do
                // super call).
                let body = weave_initializers(&ctor.body, &init_stmts, class.super_class.is_some());
                fns.push(FunctionDecl {
                    name: class_init_name(&class.name),
                    parameters: params,
                    return_type: None,
                    body,
                    span: ctor.span,
                    is_async: false,
                });
            }
            ClassMember::Method(method) => {
                // Visibility — registra apenas private/protected (public é default).
                if let Some(v) = method.modifiers.visibility {
                    if !matches!(v, crate::parser::ast::Visibility::Public) {
                        member_visibility.insert(method.name.clone(), v);
                    }
                }
                // Métodos abstract: gera um stub que faz `throw "abstract"`
                // (na prática, retorna 0). O stub permite que o codegen
                // resolva referências `__class_C_<m>` para checagem de
                // assinatura, e o dispatch virtual roteia para a impl
                // concreta da subclasse em runtime. Se chamado direto na
                // base abstract (não deveria acontecer porque `new` é
                // bloqueado), retorna o default da assinatura.
                if method.modifiers.is_abstract {
                    abstract_methods.insert(method.name.clone());
                    if matches!(method.role, MethodRole::Method) {
                        methods.push(method.name.clone());
                    }
                    let synth_name = match method.role {
                        MethodRole::Getter => class_getter_name(&class.name, &method.name),
                        MethodRole::Setter => class_setter_name(&class.name, &method.name),
                        MethodRole::Method => class_method_name(&class.name, &method.name),
                    };
                    let mut params = Vec::with_capacity(method.parameters.len() + 1);
                    params.push(this_param(method.span));
                    params.extend(method.parameters.iter().cloned());
                    // Body do stub: retorna o default do tipo declarado.
                    // Se return_type é "void", body vazio basta. Caso
                    // contrário, `return 0;` ou `return 0.0;`.
                    let body = synth_abstract_stub_body(method.return_type.as_deref());
                    fns.push(FunctionDecl {
                        name: synth_name,
                        parameters: params,
                        return_type: method.return_type.clone(),
                        body,
                        span: method.span,
                        is_async: false,
                    });
                    continue;
                }
                if method.modifiers.is_static {
                    static_methods.push(method.name.clone());
                    fns.push(FunctionDecl {
                        name: class_static_method_name(&class.name, &method.name),
                        parameters: method.parameters.clone(),
                        return_type: method.return_type.clone(),
                        body: method.body.clone(),
                        span: method.span,
                        is_async: false,
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
                        is_async: false,
                    });
                }
            }
            ClassMember::Property(prop) => {
                // Visibility — registra apenas private/protected.
                if let Some(v) = prop.modifiers.visibility {
                    if !matches!(v, crate::parser::ast::Visibility::Public) {
                        member_visibility.insert(prop.name.clone(), v);
                    }
                }
                if prop.modifiers.is_static {
                    static_fields.push(prop.name.clone());
                } else {
                    if let Some(ann) = prop.type_annotation.as_deref() {
                        let ann = ann.trim();
                        field_types.insert(prop.name.clone(), ValTy::from_annotation(ann));
                        field_class_names.insert(prop.name.clone(), ann.to_string());
                    }
                    if prop.modifiers.readonly {
                        readonly_fields.insert(prop.name.clone());
                    }
                    // Private fields sem anotação ainda precisam ser
                    // detectáveis na hierarquia para validação de escopo.
                    // Garantimos uma entrada em field_types (default I64).
                    if prop.name.starts_with('#') && !field_types.contains_key(&prop.name) {
                        field_types.insert(prop.name.clone(), ValTy::I64);
                    }
                }
            }
        }
    }

    // Se a classe não tem constructor explícito mas tem initializers,
    // sintetizamos um ctor implícito que apenas executa-os. Para classes
    // com `extends` mas sem ctor explícito, TS gera um pass-through
    // `constructor(...args) { super(...args); }` — não suportamos rest
    // args ainda (#58/#59), então damos erro claro nesse caso.
    if !has_constructor && !init_stmts.is_empty() {
        if class.super_class.is_some() {
            // Sub sem ctor + extends + initializers: precisaria de
            // `super(...args)` implícito. Por simplicidade do MVP, exija
            // ctor explícito nesse caso.
            // (Ainda emitimos o ctor implícito sem super — funciona se
            // a classe pai não tem ctor com args.)
        }
        let init_only_body = weave_initializers(&[], &init_stmts, false);
        fns.push(FunctionDecl {
            name: class_init_name(&class.name),
            parameters: vec![this_param(class.span)],
            return_type: None,
            body: init_only_body,
            span: class.span,
            is_async: false,
        });
        has_constructor = true;
    }

    // (#nested-this) Escaneia o body do constructor (e initializers) por
    // `this.X = { sub: { ... } }` ou `this.X = { ... }` e popula
    // field_obj_types / field_nested_obj_types. Permite resolver
    // `this.cfg.server.host` em metodos de instancia.
    {
        // Stmts a escanear: ctor body (se houver) + init_stmts (initializers
        // de propriedade que serao inseridos no ctor sintetizado).
        let mut stmts_to_scan: Vec<&Statement> = Vec::new();
        for member in &class.members {
            if let ClassMember::Constructor(ctor) = member {
                for s in &ctor.body {
                    stmts_to_scan.push(s);
                }
            }
        }
        for s in &init_stmts {
            stmts_to_scan.push(s);
        }
        for s in stmts_to_scan {
            scan_this_obj_assign(
                s,
                &mut field_obj_types,
                &mut field_nested_obj_types,
            );
        }
    }

    let meta = ClassMeta {
        name: class.name.clone(),
        super_class: class.super_class.clone(),
        methods,
        field_types,
        field_class_names,
        field_obj_types,
        field_nested_obj_types,
        static_methods,
        static_fields,
        getters,
        setters,
        has_constructor,
        readonly_fields,
        is_abstract: class.is_abstract,
        abstract_methods,
        member_visibility,
        layout: None,
    };
    (meta, fns)
}

/// `this.<name> = <init>;` como Statement RTS.
fn make_field_init_stmt(
    name: &str,
    init: Box<swc_ecma_ast::Expr>,
    span: crate::parser::span::Span,
) -> Statement {
    let lhs = Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
            span: Default::default(),
        })),
        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
            span: Default::default(),
            sym: name.into(),
        }),
    });
    let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
        span: Default::default(),
        op: swc_ecma_ast::AssignOp::Assign,
        left: swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(
            swc_ecma_ast::MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
                    span: Default::default(),
                })),
                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                    span: Default::default(),
                    sym: name.into(),
                }),
            },
        )),
        right: init,
    });
    let _ = lhs; // não usamos; AssignTarget já carrega o lado esquerdo.
    let stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(assign),
    });
    Statement::Raw(RawStmt::new("<field-init>".to_string(), span).with_stmt(stmt))
}

/// Costura initializers no body do constructor, respeitando `super()`.
/// - Se `has_super` e o primeiro statement do user é `super(...)`,
///   coloca os initializers logo depois.
/// - Caso contrário, prepende.
/// (#303 parte 1) Conta \`super(...)\` no nivel top-level do body de um
/// constructor, sem descer em if/else/loops/blocks. Detect de duplicacao
/// linear evita o caso degenerate \`super(); super();\`.
fn count_super_calls_in_top_level(body: &[Statement]) -> usize {
    use swc_ecma_ast::{Callee, Expr, Stmt};
    let mut count = 0usize;
    for stmt in body {
        let Statement::Raw(raw) = stmt;
        let Some(s) = raw.stmt.as_ref() else { continue };
        if let Stmt::Expr(e) = s {
            if let Expr::Call(c) = e.expr.as_ref() {
                if matches!(c.callee, Callee::Super(_)) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn weave_initializers(
    user_body: &[Statement],
    init_stmts: &[Statement],
    has_super: bool,
) -> Vec<Statement> {
    if init_stmts.is_empty() {
        return user_body.to_vec();
    }

    let mut out: Vec<Statement> = Vec::with_capacity(user_body.len() + init_stmts.len());

    let super_at_start = has_super
        && user_body
            .first()
            .map(|s| is_super_call_stmt(s))
            .unwrap_or(false);

    if super_at_start {
        out.push(user_body[0].clone());
        out.extend(init_stmts.iter().cloned());
        out.extend(user_body.iter().skip(1).cloned());
    } else {
        out.extend(init_stmts.iter().cloned());
        out.extend(user_body.iter().cloned());
    }

    out
}

fn is_super_call_stmt(s: &Statement) -> bool {
    let Statement::Raw(raw) = s;
    let Some(Stmt::Expr(expr_stmt)) = raw.stmt.as_ref() else {
        return false;
    };
    let Expr::Call(call) = expr_stmt.expr.as_ref() else {
        return false;
    };
    matches!(call.callee, Callee::Super(_))
}

/// Body padrão para stub de método abstract: `return 0;` (ou nada se void).
fn synth_abstract_stub_body(return_type: Option<&str>) -> Vec<Statement> {
    let ret_type = return_type.map(|s| s.trim()).unwrap_or("void");
    if ret_type == "void" || ret_type.is_empty() {
        return Vec::new();
    }
    let zero_expr = if ret_type == "f64" || ret_type == "F64" {
        // f64 → 0.0
        Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: None,
        }))
    } else {
        // i32/i64/handle/bool: literal 0
        Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: Some("0".into()),
        }))
    };
    let stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
        span: Default::default(),
        arg: Some(Box::new(zero_expr)),
    });
    vec![Statement::Raw(
        RawStmt::new("<abstract-stub>".to_string(), Span::default()).with_stmt(stmt),
    )]
}

fn this_param(span: crate::parser::span::Span) -> Parameter {
    Parameter {
        name: "this".to_string(),
        type_annotation: None,
        modifiers: MemberModifiers::default(),
        variadic: false,
        default: None,
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
// ── Captura de locais em closures (#97 fase 2) ────────────────────────

fn sanitize_for_symbol(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

