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
use cranelift_module::{DataDescription, Linkage, Module};
use swc_ecma_ast::{Callee, Decl, Expr, Lit, MemberProp, Pat, Stmt, TsType, TsTypeRef};

use crate::parser::ast::{
    ClassDecl, ClassMember, FunctionDecl, Item, MemberModifiers, MethodRole, Parameter, Program,
    RawStmt, Statement,
};
use crate::parser::span::Span;

use super::ctx::{ClassMeta, FnCtx, GlobalVar, UserFnAbi, ValTy};
use super::statements::lower_stmt;

const RUNTIME_MAIN_SYMBOL: &str = "__RTS_MAIN";

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
fn lift_arrow_callbacks(program: &mut Program) {
    let user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    let mut acc = LiftAcc {
        counter: 0,
        new_fns: Vec::new(),
        new_globals: Vec::new(),
        user_fn_names,
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
}

struct LiftAcc {
    counter: u32,
    new_fns: Vec<Item>,
    /// Nomes de globais `__cb_this_N` a declarar como `let` top-level.
    new_globals: Vec<String>,
    user_fn_names: HashSet<String>,
}

/// Expande callsites \`f(args)\` que omitem parâmetros com default,
/// preenchendo com a expressão default declarada na fn.
///
/// Coleta todos os defaults por nome (fns user top-level e métodos de
/// classe) e percorre toda a árvore (top-level statements + bodies de
/// fns + bodies de métodos) reescrevendo callsites em `Expr::Call` que
/// passem menos args que o esperado.
/// Expande destructuring patterns em decls separadas.
///
/// Cobertura nesta fase:
/// - Array: \`const [a, b] = arr\` → \`const _t = arr; const a = _t[0]; const b = _t[1]\`
/// - Object: \`const {x, y} = obj\` → \`const _t = obj; const x = _t["x"]; const y = _t["y"]\`
/// - Aliasing: \`const {x: a} = obj\` → \`const _t = obj; const a = _t["x"]\`
/// - Default: \`const {x = 0} = obj\` → ... \`const x = _t["x"]\` (default precisa
///   de expansão futura via runtime check; sem null-coalesce ainda).
///
/// Não cobertos (follow-up):
/// - Nested patterns (\`const {a: {b}} = obj\`)
/// - Rest em destructuring (\`const {a, ...rest}\`)
/// - Destructuring em parâmetros de função e for-of
fn expand_destructuring(program: &mut Program) {
    // Counter global para gerar nomes de temporários únicos.
    let mut counter: u32 = 0;

    // Helper: processa um Vec<Statement>, expandindo decls de destructuring.
    // Cria novos statements e replace in place.
    fn process_body(body: &mut Vec<Statement>, counter: &mut u32) {
        let mut i = 0;
        while i < body.len() {
            let Statement::Raw(raw) = &body[i];
            let Some(stmt) = raw.stmt.as_ref() else {
                i += 1;
                continue;
            };
            // Apenas \`Stmt::Decl(Decl::Var)\` pode ter patterns.
            let Stmt::Decl(Decl::Var(var_decl)) = stmt else {
                i += 1;
                continue;
            };

            // Verifica se algum decl tem pattern.
            let has_pattern = var_decl
                .decls
                .iter()
                .any(|d| !matches!(d.name, Pat::Ident(_)));
            if !has_pattern {
                i += 1;
                continue;
            }

            // Expande: gera múltiplos statements.
            let kind = var_decl.kind;
            let mut new_stmts: Vec<Statement> = Vec::new();
            for decl in &var_decl.decls {
                expand_destruct_decl(
                    &decl.name,
                    decl.init.as_deref(),
                    kind,
                    counter,
                    &mut new_stmts,
                );
            }

            // Substitui o stmt atual pelos novos.
            body.remove(i);
            for (k, s) in new_stmts.into_iter().enumerate() {
                body.insert(i + k, s);
            }
            // Skip pelos stmts inseridos (eles já são "Ident" decls,
            // não recursam aqui).
            // i fica no próximo após o último inserido — mas como
            // recursamos via process_body em fns aninhadas só, ok.
            // Avança pelo número de inserts.
            // (Já incrementei body.remove + insert; i agora aponta pro
            // primeiro inserido. Avanço pra depois deles.)
            // Não faço skip extra pq são todos Ident decls.
            i += 1;
        }
    }

    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => process_body(&mut f.body, &mut counter),
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            process_body(&mut ctor.body, &mut counter);
                        }
                        ClassMember::Method(method) => {
                            process_body(&mut method.body, &mut counter);
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(_) => {
                // Top-level: empacota num Vec único e processa.
                // A reorganização é mais delicada porque os items são
                // Item::Statement, não Statement. Simplifico: detecta
                // patterns em items individuais.
            }
            _ => {}
        }
    }

    // Top-level: itera com índice mutável.
    let mut i = 0;
    while i < program.items.len() {
        let needs_expansion = matches!(
            &program.items[i],
            Item::Statement(Statement::Raw(raw))
                if matches!(
                    raw.stmt.as_ref(),
                    Some(Stmt::Decl(Decl::Var(v)))
                    if v.decls.iter().any(|d| !matches!(d.name, Pat::Ident(_)))
                )
        );
        if !needs_expansion {
            i += 1;
            continue;
        }
        let Item::Statement(Statement::Raw(raw)) = &program.items[i] else {
            i += 1;
            continue;
        };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else {
            i += 1;
            continue;
        };
        let kind = var_decl.kind;
        let mut new_stmts: Vec<Statement> = Vec::new();
        for decl in &var_decl.decls {
            expand_destruct_decl(
                &decl.name,
                decl.init.as_deref(),
                kind,
                &mut counter,
                &mut new_stmts,
            );
        }
        program.items.remove(i);
        for (k, s) in new_stmts.into_iter().enumerate() {
            program.items.insert(i + k, Item::Statement(s));
        }
        // Avança pelos inseridos.
        // (não faço i += new_stmts.len() porque já consumi via into_iter,
        // mas acima o número era inserido sequencialmente; após insert,
        // i deve avançar pelo número de elementos inseridos. Loop simples:
        // body[i..i+N] são novos Ident decls, não precisam re-expansão.)
        // Conta quantos foram inseridos.
        // Como não temos esse número aqui (consumiu o iter), recomputo
        // varredando até achar não-Statement ou seja arbitrário.
        // Mais simples: continuar do índice atual; os novos decls são
        // Ident (não disparam needs_expansion).
        // Sem `i +=`, pq remove + insert deixa i no primeiro inserido.
        i += 1;
    }
}

/// Expande um decl com pattern para Vec<Statement>. Recurse em nested.
fn expand_destruct_decl(
    pat: &Pat,
    init: Option<&Expr>,
    kind: swc_ecma_ast::VarDeclKind,
    counter: &mut u32,
    out: &mut Vec<Statement>,
) {
    match pat {
        Pat::Ident(_) => {
            // Sem destructuring — apenas regenera o decl simples.
            let var_decl = swc_ecma_ast::VarDecl {
                span: Default::default(),
                ctxt: Default::default(),
                kind,
                declare: false,
                decls: vec![swc_ecma_ast::VarDeclarator {
                    span: Default::default(),
                    name: pat.clone(),
                    init: init.map(|e| Box::new(e.clone())),
                    definite: false,
                }],
            };
            let stmt = Stmt::Decl(Decl::Var(Box::new(var_decl)));
            out.push(Statement::Raw(
                RawStmt::new("<destruct>".to_string(), Span::default()).with_stmt(stmt),
            ));
        }
        Pat::Array(arr) => {
            let tmp_name = format!("__destruct_{}", *counter);
            *counter += 1;
            // Gera const __destruct_N = init;
            if let Some(init) = init {
                out.push(make_const_decl(&tmp_name, init.clone(), kind));
            }
            for (idx, elem) in arr.elems.iter().enumerate() {
                let Some(e) = elem else { continue }; // hole — pula
                // const elem_name = __destruct_N[idx];
                let access = make_index_access(&tmp_name, idx as f64);
                expand_destruct_decl(&e, Some(&access), kind, counter, out);
            }
        }
        Pat::Object(obj) => {
            let tmp_name = format!("__destruct_{}", *counter);
            *counter += 1;
            if let Some(init) = init {
                out.push(make_const_decl(&tmp_name, init.clone(), kind));
            }
            for prop in &obj.props {
                match prop {
                    swc_ecma_ast::ObjectPatProp::Assign(ap) => {
                        // \`{ x }\` ou \`{ x = default }\`
                        let key = ap.key.id.sym.as_str();
                        let access = make_member_access(&tmp_name, key);
                        let inner_pat = Pat::Ident(swc_ecma_ast::BindingIdent {
                            id: ap.key.id.clone(),
                            type_ann: None,
                        });
                        // Default em destructuring ainda não suportado;
                        // o init do AssignPatProp é ignorado neste MVP.
                        expand_destruct_decl(&inner_pat, Some(&access), kind, counter, out);
                    }
                    swc_ecma_ast::ObjectPatProp::KeyValue(kvp) => {
                        // \`{ x: a }\` — alias
                        let key = match &kvp.key {
                            swc_ecma_ast::PropName::Ident(id) => id.sym.to_string(),
                            swc_ecma_ast::PropName::Str(s) => s.value.to_string_lossy().to_string(),
                            _ => continue,
                        };
                        let access = make_member_access(&tmp_name, &key);
                        expand_destruct_decl(&kvp.value, Some(&access), kind, counter, out);
                    }
                    swc_ecma_ast::ObjectPatProp::Rest(_) => {
                        // Rest em object destructuring — follow-up.
                    }
                }
            }
        }
        _ => {
            // Outros patterns (Rest, Assign solo, etc) — emite decl direto
            // se for Ident interno; senão silencia.
        }
    }
}

/// `const <name> = <expr>;` (kind preservado).
fn make_const_decl(name: &str, expr: Expr, kind: swc_ecma_ast::VarDeclKind) -> Statement {
    let var = swc_ecma_ast::VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind,
        declare: false,
        decls: vec![swc_ecma_ast::VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: name.into(),
                    optional: false,
                },
                type_ann: None,
            }),
            init: Some(Box::new(expr)),
            definite: false,
        }],
    };
    let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
    Statement::Raw(RawStmt::new("<destruct-tmp>".to_string(), Span::default()).with_stmt(stmt))
}

/// `<obj>[<idx>]` como Expr.
fn make_index_access(obj_name: &str, idx: f64) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: obj_name.into(),
            optional: false,
        })),
        prop: MemberProp::Computed(swc_ecma_ast::ComputedPropName {
            span: Default::default(),
            expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                span: Default::default(),
                value: idx,
                raw: Some(format!("{}", idx as i64).into()),
            }))),
        }),
    })
}

/// `<obj>.<key>` como Expr.
fn make_member_access(obj_name: &str, key: &str) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: obj_name.into(),
            optional: false,
        })),
        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
            span: Default::default(),
            sym: key.into(),
        }),
    })
}

fn expand_default_args(program: &mut Program) {
    use std::collections::HashMap;

    // Mapa: nome → params (defaults inclusos). Para métodos: mesmo nome
    // pode aparecer em múltiplas classes — guardamos em outro mapa
    // indexado por (class, method).
    let mut fn_defaults: HashMap<String, Vec<Option<Box<Expr>>>> = HashMap::new();
    let mut method_defaults: HashMap<(String, String), Vec<Option<Box<Expr>>>> = HashMap::new();

    for item in &program.items {
        match item {
            Item::Function(f) => {
                if f.parameters.iter().any(|p| p.default.is_some()) {
                    let defaults: Vec<Option<Box<Expr>>> =
                        f.parameters.iter().map(|p| p.default.clone()).collect();
                    fn_defaults.insert(f.name.clone(), defaults);
                }
            }
            Item::Class(c) => {
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        if method.parameters.iter().any(|p| p.default.is_some()) {
                            let defaults: Vec<Option<Box<Expr>>> = method
                                .parameters
                                .iter()
                                .map(|p| p.default.clone())
                                .collect();
                            method_defaults.insert((c.name.clone(), method.name.clone()), defaults);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if fn_defaults.is_empty() && method_defaults.is_empty() {
        return;
    }

    // Reescreve callsites.
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                    }
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                                }
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                }
            }
            _ => {}
        }
    }
}

fn expand_in_stmt(
    stmt: &mut Stmt,
    fn_defaults: &std::collections::HashMap<String, Vec<Option<Box<Expr>>>>,
    method_defaults: &std::collections::HashMap<(String, String), Vec<Option<Box<Expr>>>>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => expand_in_expr(&mut e.expr, fn_defaults, method_defaults),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                expand_in_expr(a, fn_defaults, method_defaults);
            }
        }
        If(i) => {
            expand_in_expr(&mut i.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut i.cons, fn_defaults, method_defaults);
            if let Some(alt) = i.alt.as_deref_mut() {
                expand_in_stmt(alt, fn_defaults, method_defaults);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                expand_in_stmt(s, fn_defaults, method_defaults);
            }
        }
        While(w) => {
            expand_in_expr(&mut w.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut w.body, fn_defaults, method_defaults);
        }
        DoWhile(w) => {
            expand_in_expr(&mut w.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut w.body, fn_defaults, method_defaults);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            expand_in_expr(e, fn_defaults, method_defaults);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                expand_in_expr(t, fn_defaults, method_defaults);
            }
            if let Some(u) = f.update.as_deref_mut() {
                expand_in_expr(u, fn_defaults, method_defaults);
            }
            expand_in_stmt(&mut f.body, fn_defaults, method_defaults);
        }
        ForOf(f) => {
            expand_in_expr(&mut f.right, fn_defaults, method_defaults);
            expand_in_stmt(&mut f.body, fn_defaults, method_defaults);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    expand_in_expr(e, fn_defaults, method_defaults);
                }
            }
        }
        Try(t) => {
            for s in &mut t.block.stmts {
                expand_in_stmt(s, fn_defaults, method_defaults);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in &mut h.body.stmts {
                    expand_in_stmt(s, fn_defaults, method_defaults);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in &mut f.stmts {
                    expand_in_stmt(s, fn_defaults, method_defaults);
                }
            }
        }
        _ => {}
    }
}

fn expand_in_expr(
    expr: &mut Expr,
    fn_defaults: &std::collections::HashMap<String, Vec<Option<Box<Expr>>>>,
    method_defaults: &std::collections::HashMap<(String, String), Vec<Option<Box<Expr>>>>,
) {
    // Recurse primeiro para que callsites internos também sejam expandidos.
    match expr {
        Expr::Call(call) => {
            // Recurse em args primeiro (call aninhado).
            for a in call.args.iter_mut() {
                expand_in_expr(&mut a.expr, fn_defaults, method_defaults);
            }
            if let Callee::Expr(callee_expr) = &mut call.callee {
                expand_in_expr(callee_expr, fn_defaults, method_defaults);
            }
            // Detecta callee:
            //   - Ident("f") → fn_defaults["f"]
            //   - Member(this, "m") em método de classe → não temos
            //     contexto da classe aqui, então skip; será tratado em
            //     dispatch virtual posterior.
            //   - Member(obj_local, "m") onde obj_local é Ident — skip por
            //     mesmo motivo.
            //
            // Cobertura: defaults de fns top-level. Defaults em métodos
            // ficam parcialmente cobertos no codegen futuro (não nesse
            // pass).
            let fn_name = if let Callee::Expr(ce) = &call.callee {
                if let Expr::Ident(id) = ce.as_ref() {
                    Some(id.sym.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(name) = fn_name {
                if let Some(defaults) = fn_defaults.get(&name) {
                    let provided = call.args.len();
                    let total = defaults.len();
                    if provided < total {
                        for i in provided..total {
                            if let Some(def) = &defaults[i] {
                                let mut def_clone = (**def).clone();
                                expand_in_expr(&mut def_clone, fn_defaults, method_defaults);
                                call.args.push(swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(def_clone),
                                });
                            } else {
                                // Param sem default — TS exige que callsite
                                // proveja, codegen vai dar erro mais claro.
                                break;
                            }
                        }
                    }
                }
            }
        }
        Expr::Member(m) => expand_in_expr(&mut m.obj, fn_defaults, method_defaults),
        Expr::Bin(b) => {
            expand_in_expr(&mut b.left, fn_defaults, method_defaults);
            expand_in_expr(&mut b.right, fn_defaults, method_defaults);
        }
        Expr::Unary(u) => expand_in_expr(&mut u.arg, fn_defaults, method_defaults),
        Expr::Update(u) => expand_in_expr(&mut u.arg, fn_defaults, method_defaults),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                expand_in_expr(&mut m.obj, fn_defaults, method_defaults);
            }
            expand_in_expr(&mut a.right, fn_defaults, method_defaults);
        }
        Expr::New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    expand_in_expr(&mut a.expr, fn_defaults, method_defaults);
                }
            }
        }
        Expr::Cond(c) => {
            expand_in_expr(&mut c.test, fn_defaults, method_defaults);
            expand_in_expr(&mut c.cons, fn_defaults, method_defaults);
            expand_in_expr(&mut c.alt, fn_defaults, method_defaults);
        }
        Expr::Paren(p) => expand_in_expr(&mut p.expr, fn_defaults, method_defaults),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                expand_in_expr(e, fn_defaults, method_defaults);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                expand_in_expr(&mut el.expr, fn_defaults, method_defaults);
            }
        }
        _ => {}
    }
}

/// Empacota argumentos variádicos `...rest` num array literal no callsite.
///
/// Para uma fn user com último param marcado \`variadic\` (sintaxe
/// \`...rest\`), todos os args do callsite a partir da posição desse param
/// são coletados num \`Expr::Array\` e passados como único arg na posição
/// do rest. Codegen do callee vê \`rest\` como Handle de array normal —
/// pode iterar via \`for...of\`.
fn expand_rest_args(program: &mut Program) {
    use std::collections::HashMap;

    // Mapa: nome → índice do parâmetro variadic (último). Apenas
    // funções top-level e métodos.
    let mut fn_rest: HashMap<String, usize> = HashMap::new();
    let mut method_rest: HashMap<(String, String), usize> = HashMap::new();

    for item in &program.items {
        match item {
            Item::Function(f) => {
                if let Some(idx) = f.parameters.iter().position(|p| p.variadic) {
                    fn_rest.insert(f.name.clone(), idx);
                }
            }
            Item::Class(c) => {
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        if let Some(idx) = method.parameters.iter().position(|p| p.variadic) {
                            method_rest.insert((c.name.clone(), method.name.clone()), idx);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if fn_rest.is_empty() && method_rest.is_empty() {
        return;
    }

    // Reescrita.
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        rest_in_stmt(stmt, &fn_rest, &method_rest);
                    }
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    rest_in_stmt(stmt, &fn_rest, &method_rest);
                                }
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    rest_in_stmt(stmt, &fn_rest, &method_rest);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    rest_in_stmt(stmt, &fn_rest, &method_rest);
                }
            }
            _ => {}
        }
    }
}

fn rest_in_stmt(
    stmt: &mut Stmt,
    fn_rest: &std::collections::HashMap<String, usize>,
    method_rest: &std::collections::HashMap<(String, String), usize>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => rest_in_expr(&mut e.expr, fn_rest, method_rest),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                rest_in_expr(a, fn_rest, method_rest);
            }
        }
        If(i) => {
            rest_in_expr(&mut i.test, fn_rest, method_rest);
            rest_in_stmt(&mut i.cons, fn_rest, method_rest);
            if let Some(alt) = i.alt.as_deref_mut() {
                rest_in_stmt(alt, fn_rest, method_rest);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                rest_in_stmt(s, fn_rest, method_rest);
            }
        }
        While(w) => {
            rest_in_expr(&mut w.test, fn_rest, method_rest);
            rest_in_stmt(&mut w.body, fn_rest, method_rest);
        }
        DoWhile(w) => {
            rest_in_expr(&mut w.test, fn_rest, method_rest);
            rest_in_stmt(&mut w.body, fn_rest, method_rest);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            rest_in_expr(e, fn_rest, method_rest);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                rest_in_expr(t, fn_rest, method_rest);
            }
            if let Some(u) = f.update.as_deref_mut() {
                rest_in_expr(u, fn_rest, method_rest);
            }
            rest_in_stmt(&mut f.body, fn_rest, method_rest);
        }
        ForOf(f) => {
            rest_in_expr(&mut f.right, fn_rest, method_rest);
            rest_in_stmt(&mut f.body, fn_rest, method_rest);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    rest_in_expr(e, fn_rest, method_rest);
                }
            }
        }
        _ => {}
    }
}

fn rest_in_expr(
    expr: &mut Expr,
    fn_rest: &std::collections::HashMap<String, usize>,
    method_rest: &std::collections::HashMap<(String, String), usize>,
) {
    match expr {
        Expr::Call(call) => {
            // Recurse args/callee primeiro.
            for a in call.args.iter_mut() {
                rest_in_expr(&mut a.expr, fn_rest, method_rest);
            }
            if let Callee::Expr(callee_expr) = &mut call.callee {
                rest_in_expr(callee_expr, fn_rest, method_rest);
            }
            // Detecta callee Ident → fn_rest.
            let fn_name = if let Callee::Expr(ce) = &call.callee {
                if let Expr::Ident(id) = ce.as_ref() {
                    Some(id.sym.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(name) = fn_name {
                if let Some(&rest_idx) = fn_rest.get(&name) {
                    pack_rest_args(&mut call.args, rest_idx);
                }
            }
        }
        Expr::Member(m) => rest_in_expr(&mut m.obj, fn_rest, method_rest),
        Expr::Bin(b) => {
            rest_in_expr(&mut b.left, fn_rest, method_rest);
            rest_in_expr(&mut b.right, fn_rest, method_rest);
        }
        Expr::Unary(u) => rest_in_expr(&mut u.arg, fn_rest, method_rest),
        Expr::Update(u) => rest_in_expr(&mut u.arg, fn_rest, method_rest),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                rest_in_expr(&mut m.obj, fn_rest, method_rest);
            }
            rest_in_expr(&mut a.right, fn_rest, method_rest);
        }
        Expr::New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    rest_in_expr(&mut a.expr, fn_rest, method_rest);
                }
            }
        }
        Expr::Cond(c) => {
            rest_in_expr(&mut c.test, fn_rest, method_rest);
            rest_in_expr(&mut c.cons, fn_rest, method_rest);
            rest_in_expr(&mut c.alt, fn_rest, method_rest);
        }
        Expr::Paren(p) => rest_in_expr(&mut p.expr, fn_rest, method_rest),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                rest_in_expr(e, fn_rest, method_rest);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rest_in_expr(&mut el.expr, fn_rest, method_rest);
            }
        }
        _ => {}
    }
}

/// Substitui args[rest_idx..] por um Expr::Array contendo esses elementos.
fn pack_rest_args(args: &mut Vec<swc_ecma_ast::ExprOrSpread>, rest_idx: usize) {
    if args.len() <= rest_idx {
        // Caller não passou nenhum arg variadic → empacota array vazio.
        let empty = Expr::Array(swc_ecma_ast::ArrayLit {
            span: Default::default(),
            elems: Vec::new(),
        });
        args.push(swc_ecma_ast::ExprOrSpread {
            spread: None,
            expr: Box::new(empty),
        });
        return;
    }
    let extra: Vec<Option<swc_ecma_ast::ExprOrSpread>> = args.drain(rest_idx..).map(Some).collect();
    let arr = Expr::Array(swc_ecma_ast::ArrayLit {
        span: Default::default(),
        elems: extra,
    });
    args.push(swc_ecma_ast::ExprOrSpread {
        spread: None,
        expr: Box::new(arr),
    });
}

/// Expande argumentos com spread literal em callsites: \`fn(...[1,2,3])\`
/// vira \`fn(1, 2, 3)\` em compile-time. Trabalha sobre toda a árvore.
///
/// Cobertura nesta fase: spread de \`Expr::Array\` literal inline.
/// Spread de variável (\`fn(...arr)\`) ainda é rejeitado pelo codegen
/// — exige geração de loop ou copy dinâmico que fica como follow-up.
fn expand_spread_args(program: &mut Program) {
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        spread_in_stmt(stmt);
                    }
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    spread_in_stmt(stmt);
                                }
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    spread_in_stmt(stmt);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    spread_in_stmt(stmt);
                }
            }
            _ => {}
        }
    }
}

fn spread_in_stmt(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => spread_in_expr(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                spread_in_expr(a);
            }
        }
        If(i) => {
            spread_in_expr(&mut i.test);
            spread_in_stmt(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                spread_in_stmt(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                spread_in_stmt(s);
            }
        }
        While(w) => {
            spread_in_expr(&mut w.test);
            spread_in_stmt(&mut w.body);
        }
        DoWhile(w) => {
            spread_in_expr(&mut w.test);
            spread_in_stmt(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            spread_in_expr(e);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                spread_in_expr(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                spread_in_expr(u);
            }
            spread_in_stmt(&mut f.body);
        }
        ForOf(f) => {
            spread_in_expr(&mut f.right);
            spread_in_stmt(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    spread_in_expr(e);
                }
            }
        }
        _ => {}
    }
}

fn spread_in_expr(expr: &mut Expr) {
    match expr {
        Expr::Call(call) => {
            // Recurse primeiro nos args.
            for a in call.args.iter_mut() {
                spread_in_expr(&mut a.expr);
            }
            if let Callee::Expr(e) = &mut call.callee {
                spread_in_expr(e);
            }
            expand_spread_in_args(&mut call.args);
        }
        Expr::New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    spread_in_expr(&mut a.expr);
                }
                expand_spread_in_args(args);
            }
        }
        Expr::Member(m) => spread_in_expr(&mut m.obj),
        Expr::Bin(b) => {
            spread_in_expr(&mut b.left);
            spread_in_expr(&mut b.right);
        }
        Expr::Unary(u) => spread_in_expr(&mut u.arg),
        Expr::Update(u) => spread_in_expr(&mut u.arg),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                spread_in_expr(&mut m.obj);
            }
            spread_in_expr(&mut a.right);
        }
        Expr::Cond(c) => {
            spread_in_expr(&mut c.test);
            spread_in_expr(&mut c.cons);
            spread_in_expr(&mut c.alt);
        }
        Expr::Paren(p) => spread_in_expr(&mut p.expr),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                spread_in_expr(e);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                spread_in_expr(&mut el.expr);
            }
        }
        _ => {}
    }
}

/// Substitui args com spread de array literal pelos elementos do array.
/// `f(a, ...[b, c], d)` → `f(a, b, c, d)`.
fn expand_spread_in_args(args: &mut Vec<swc_ecma_ast::ExprOrSpread>) {
    if !args.iter().any(|a| a.spread.is_some()) {
        return;
    }
    let original = std::mem::take(args);
    for arg in original {
        if arg.spread.is_some() {
            // Spread de literal array: expande inline.
            if let Expr::Array(arr) = arg.expr.as_ref() {
                for elem in &arr.elems {
                    if let Some(el) = elem {
                        // Spread de array com hole (sparse) → push literal 0.
                        // Mas mantemos ExprOrSpread interno (suporta nested
                        // spread? não, simplificação: aplaina um nível).
                        args.push(swc_ecma_ast::ExprOrSpread {
                            spread: el.spread,
                            expr: el.expr.clone(),
                        });
                    } else {
                        // Hole — vira literal 0 (matching JS undefined → 0 here).
                        args.push(swc_ecma_ast::ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                                span: Default::default(),
                                value: 0.0,
                                raw: Some("0".into()),
                            }))),
                        });
                    }
                }
                continue;
            }
            // Spread de não-literal: deixamos como está. Codegen vai
            // rejeitar com erro claro em runtime.
            args.push(arg);
        } else {
            args.push(arg);
        }
    }
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

    /// Varre `body` em busca de chamadas a funções do namespace ABI cujo arg
    /// I64 é um `ArrowExpr` ou `Ident` apontando pra user fn. Substitui in
    /// place pelo `Ident` da fn lifted, e injeta statements/fns de suporte.
    fn lift_in_body(&mut self, class_name: &str, body: &mut Vec<Statement>, in_class: bool) {
        use crate::abi::AbiType;

        let mut idx = 0usize;
        while idx < body.len() {
            // Pega CallExpr do statement atual, se houver. Coletamos as
            // mutações separadas: substituições de args + statements a
            // injetar antes deste.
            let Statement::Raw(raw) = &mut body[idx];
            let Some(Stmt::Expr(expr_stmt)) = raw.stmt.as_mut() else {
                idx += 1;
                continue;
            };
            let Expr::Call(call) = expr_stmt.expr.as_mut() else {
                idx += 1;
                continue;
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

            for (arg, &abi_ty) in call.args.iter_mut().zip(member.args.iter()) {
                if abi_ty != AbiType::I64 {
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

                match arg.expr.as_ref() {
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
                        let call_stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
                            span: id.span,
                            expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
                                span: id.span,
                                ctxt: id.ctxt,
                                callee: Callee::Expr(Box::new(Expr::Ident(id.clone()))),
                                args: Vec::new(),
                                type_args: None,
                            })),
                        });
                        body_stmts = vec![Statement::Raw(
                            RawStmt::new("<lifted>".to_string(), Span::default())
                                .with_stmt(call_stmt),
                        )];
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
                // quando vamos passar `this` por userdata. Caso contrário,
                // sem parâmetros (path antigo).
                let parameters: Vec<Parameter> = if use_userdata_callback {
                    vec![Parameter {
                        name: "this".to_string(),
                        type_annotation: Some(class_name.to_string()),
                        modifiers: MemberModifiers::default(),
                        variadic: false,
                        default: None,
                        span: Span::default(),
                    }]
                } else {
                    Vec::new()
                };

                self.new_fns.push(Item::Function(FunctionDecl {
                    name: syn_name.clone(),
                    parameters,
                    return_type: Some("void".to_string()),
                    body: body_stmts,
                    span: Span::default(),
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
    extern_cache: &mut HashMap<&'static str, cranelift_module::FuncId>,
    data_counter: &mut u32,
) -> Result<Vec<String>> {
    lift_arrow_callbacks(program);
    expand_destructuring(program);
    expand_default_args(program);
    // Spread antes de rest: spread aplaina array literal nos call sites
    // (`f(...[1,2,3])` → `f(1,2,3)`); rest depois empacota argumentos
    // extras conforme o callee é variadic.
    expand_spread_args(program);
    expand_rest_args(program);

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
    let Expr::Ident(ns) = m.obj.as_ref() else {
        return None;
    };
    let name = match &m.prop {
        swc_ecma_ast::MemberProp::Ident(id) => id.sym.as_str(),
        _ => return None,
    };
    let qualified = format!("{}.{}", ns.sym.as_str(), name);
    let (_, member) = crate::abi::lookup(&qualified)?;
    Some(ValTy::from_abi(member.returns))
}

fn ts_type_to_val_ty(ty: &TsType) -> Option<ValTy> {
    use swc_ecma_ast::{TsKeywordTypeKind, TsLit, TsLitType, TsUnionOrIntersectionType};

    if let TsType::TsKeywordType(kw) = ty {
        return Some(match kw.kind {
            TsKeywordTypeKind::TsNumberKeyword => ValTy::I32,
            TsKeywordTypeKind::TsBooleanKeyword => ValTy::Bool,
            TsKeywordTypeKind::TsStringKeyword => ValTy::Handle,
            TsKeywordTypeKind::TsVoidKeyword => ValTy::I64,
            _ => return None,
        });
    }

    if let TsType::TsLitType(TsLitType { lit, .. }) = ty {
        return Some(match lit {
            TsLit::Str(_) | TsLit::Tpl(_) => ValTy::Handle,
            TsLit::Number(_) => ValTy::I64,
            TsLit::Bool(_) => ValTy::Bool,
            TsLit::BigInt(_) => ValTy::I64,
        });
    }

    if let TsType::TsTypeRef(TsTypeRef { type_name, .. }) = ty {
        let name = match type_name {
            swc_ecma_ast::TsEntityName::Ident(id) => id.sym.as_str(),
            _ => return None,
        };
        return Some(ValTy::from_annotation(name));
    }

    if let TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(u)) = ty {
        let mut acc: Option<ValTy> = None;
        for member in &u.types {
            if let TsType::TsKeywordType(k) = member.as_ref() {
                if matches!(
                    k.kind,
                    TsKeywordTypeKind::TsNullKeyword
                        | TsKeywordTypeKind::TsUndefinedKeyword
                ) {
                    continue;
                }
            }
            let mt = ts_type_to_val_ty(member)?;
            match acc {
                None => acc = Some(mt),
                Some(prev) if prev == mt => {}
                _ => return None,
            }
        }
        return acc;
    }

    if let TsType::TsParenthesizedType(p) = ty {
        return ts_type_to_val_ty(&p.type_ann);
    }

    None
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
/// callbacks are the exception: they cross a native C ABI boundary.
fn user_call_conv(module: &dyn Module, fn_name: &str) -> CallConv {
    if is_lifted_callback(fn_name) {
        module.isa().default_call_conv()
    } else {
        CallConv::Tail
    }
}

fn declare_user_fn(module: &mut dyn Module, fn_decl: &FunctionDecl) -> Result<UserFn> {
    let (params, ret) = fn_signature(fn_decl);
    let mut sig = Signature::new(user_call_conv(module, &fn_decl.name));
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
    let call_conv = user_call_conv(module, &fn_decl.name);
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
            fn_class_returns,
            false,
        );
        fn_ctx.return_ty = info.ret;
        fn_ctx.is_tail_conv = call_conv == CallConv::Tail;
        fn_ctx.current_class = current_class.clone();
        // Detecta se a função é um constructor de classe pelo mangled name.
        // Usado pra permitir assign em readonly fields.
        fn_ctx.current_is_ctor = current_class
            .as_ref()
            .map(|c| fn_decl.name == class_init_name(c))
            .unwrap_or(false);
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

fn synthesize_class_fns(class: &ClassDecl) -> (ClassMeta, Vec<FunctionDecl>) {
    let mut methods: Vec<String> = Vec::new();
    let mut getters: Vec<String> = Vec::new();
    let mut setters: Vec<String> = Vec::new();
    let mut static_methods: Vec<String> = Vec::new();
    let mut static_fields: Vec<String> = Vec::new();
    let mut fns: Vec<FunctionDecl> = Vec::new();
    let mut field_types: HashMap<String, ValTy> = HashMap::new();
    let mut field_class_names: HashMap<String, String> = HashMap::new();
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
        });
        has_constructor = true;
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
        readonly_fields,
        is_abstract: class.is_abstract,
        abstract_methods,
        member_visibility,
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

/// Colete nomes declarados via `let`/`const`/`var` em todos os statements
/// do body. Não desce em arrows (escopos próprios) — apenas o escopo da
/// fn. Adiciona ao set existente.
fn collect_local_decls(body: &[Statement], locals: &mut std::collections::HashSet<String>) {
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            collect_decls_in_stmt(stmt, locals);
        }
    }
}

fn collect_decls_in_stmt(stmt: &Stmt, locals: &mut std::collections::HashSet<String>) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Pat::Ident(id) = &d.name {
                    locals.insert(id.id.sym.to_string());
                }
            }
        }
        If(i) => {
            collect_decls_in_stmt(&i.cons, locals);
            if let Some(alt) = i.alt.as_deref() {
                collect_decls_in_stmt(alt, locals);
            }
        }
        Block(b) => {
            for s in &b.stmts {
                collect_decls_in_stmt(s, locals);
            }
        }
        While(w) => collect_decls_in_stmt(&w.body, locals),
        DoWhile(w) => collect_decls_in_stmt(&w.body, locals),
        For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        locals.insert(id.id.sym.to_string());
                    }
                }
            }
            collect_decls_in_stmt(&f.body, locals);
        }
        ForOf(f) => {
            if let swc_ecma_ast::ForHead::VarDecl(vd) = &f.left {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        locals.insert(id.id.sym.to_string());
                    }
                }
            }
            collect_decls_in_stmt(&f.body, locals);
        }
        _ => {}
    }
}

/// Coleta o conjunto de idents que ocorrem dentro de arrows aninhados
/// no body e que pertencem ao set `locals` da fn enclosing. Esses são
/// os capturados.
fn collect_captures_in_body(
    body: &[Statement],
    locals: &std::collections::HashSet<String>,
) -> std::collections::BTreeSet<String> {
    let mut captured = std::collections::BTreeSet::new();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            scan_stmt_for_arrows(stmt, locals, &mut captured);
        }
    }
    captured
}

fn scan_stmt_for_arrows(
    stmt: &Stmt,
    locals: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => scan_expr_for_arrows(&e.expr, locals, captured),
        Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                scan_expr_for_arrows(a, locals, captured);
            }
        }
        If(i) => {
            scan_expr_for_arrows(&i.test, locals, captured);
            scan_stmt_for_arrows(&i.cons, locals, captured);
            if let Some(alt) = i.alt.as_deref() {
                scan_stmt_for_arrows(alt, locals, captured);
            }
        }
        Block(b) => {
            for s in &b.stmts {
                scan_stmt_for_arrows(s, locals, captured);
            }
        }
        While(w) => {
            scan_expr_for_arrows(&w.test, locals, captured);
            scan_stmt_for_arrows(&w.body, locals, captured);
        }
        DoWhile(w) => {
            scan_expr_for_arrows(&w.test, locals, captured);
            scan_stmt_for_arrows(&w.body, locals, captured);
        }
        For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_deref() {
                        scan_expr_for_arrows(e, locals, captured);
                    }
                }
            }
            if let Some(t) = f.test.as_deref() {
                scan_expr_for_arrows(t, locals, captured);
            }
            scan_stmt_for_arrows(&f.body, locals, captured);
        }
        ForOf(f) => {
            scan_expr_for_arrows(&f.right, locals, captured);
            scan_stmt_for_arrows(&f.body, locals, captured);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    scan_expr_for_arrows(e, locals, captured);
                }
            }
        }
        _ => {}
    }
}

fn scan_expr_for_arrows(
    expr: &Expr,
    locals: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    match expr {
        Expr::Arrow(arrow) => {
            // Coleta idents do body do arrow que existam em `locals`.
            collect_captured_from_arrow(arrow, locals, captured);
        }
        Expr::Call(c) => {
            if let Callee::Expr(e) = &c.callee {
                scan_expr_for_arrows(e, locals, captured);
            }
            for a in &c.args {
                scan_expr_for_arrows(&a.expr, locals, captured);
            }
        }
        Expr::Member(m) => scan_expr_for_arrows(&m.obj, locals, captured),
        Expr::Bin(b) => {
            scan_expr_for_arrows(&b.left, locals, captured);
            scan_expr_for_arrows(&b.right, locals, captured);
        }
        Expr::Unary(u) => scan_expr_for_arrows(&u.arg, locals, captured),
        Expr::Update(u) => scan_expr_for_arrows(&u.arg, locals, captured),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &a.left
            {
                scan_expr_for_arrows(&m.obj, locals, captured);
            }
            scan_expr_for_arrows(&a.right, locals, captured);
        }
        Expr::Paren(p) => scan_expr_for_arrows(&p.expr, locals, captured),
        Expr::Cond(c) => {
            scan_expr_for_arrows(&c.test, locals, captured);
            scan_expr_for_arrows(&c.cons, locals, captured);
            scan_expr_for_arrows(&c.alt, locals, captured);
        }
        _ => {}
    }
}

fn collect_captured_from_arrow(
    arrow: &swc_ecma_ast::ArrowExpr,
    enclosing_locals: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    // Locais do arrow (parâmetros + decls dentro do body) — não são capturas.
    let mut arrow_locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &arrow.params {
        if let Pat::Ident(id) = p {
            arrow_locals.insert(id.id.sym.to_string());
        }
    }
    let stmts: Vec<Stmt> = match arrow.body.as_ref() {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b.stmts.clone(),
        swc_ecma_ast::BlockStmtOrExpr::Expr(e) => vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(e.clone()),
        })],
    };
    for s in &stmts {
        collect_decls_in_stmt(s, &mut arrow_locals);
    }

    for s in &stmts {
        collect_idents_used_in_stmt(s, enclosing_locals, &arrow_locals, captured);
    }
}

fn collect_idents_used_in_stmt(
    stmt: &Stmt,
    enclosing: &std::collections::HashSet<String>,
    shadowed: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => collect_idents_used_in_expr(&e.expr, enclosing, shadowed, captured),
        Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                collect_idents_used_in_expr(a, enclosing, shadowed, captured);
            }
        }
        If(i) => {
            collect_idents_used_in_expr(&i.test, enclosing, shadowed, captured);
            collect_idents_used_in_stmt(&i.cons, enclosing, shadowed, captured);
            if let Some(alt) = i.alt.as_deref() {
                collect_idents_used_in_stmt(alt, enclosing, shadowed, captured);
            }
        }
        Block(b) => {
            for s in &b.stmts {
                collect_idents_used_in_stmt(s, enclosing, shadowed, captured);
            }
        }
        While(w) => {
            collect_idents_used_in_expr(&w.test, enclosing, shadowed, captured);
            collect_idents_used_in_stmt(&w.body, enclosing, shadowed, captured);
        }
        DoWhile(w) => {
            collect_idents_used_in_expr(&w.test, enclosing, shadowed, captured);
            collect_idents_used_in_stmt(&w.body, enclosing, shadowed, captured);
        }
        For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_deref() {
                        collect_idents_used_in_expr(e, enclosing, shadowed, captured);
                    }
                }
            }
            if let Some(t) = f.test.as_deref() {
                collect_idents_used_in_expr(t, enclosing, shadowed, captured);
            }
            if let Some(u) = f.update.as_deref() {
                collect_idents_used_in_expr(u, enclosing, shadowed, captured);
            }
            collect_idents_used_in_stmt(&f.body, enclosing, shadowed, captured);
        }
        ForOf(f) => {
            collect_idents_used_in_expr(&f.right, enclosing, shadowed, captured);
            collect_idents_used_in_stmt(&f.body, enclosing, shadowed, captured);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    collect_idents_used_in_expr(e, enclosing, shadowed, captured);
                }
            }
        }
        _ => {}
    }
}

fn collect_idents_used_in_expr(
    expr: &Expr,
    enclosing: &std::collections::HashSet<String>,
    shadowed: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    match expr {
        Expr::Ident(id) => {
            let name = id.sym.as_str();
            if enclosing.contains(name) && !shadowed.contains(name) {
                captured.insert(name.to_string());
            }
        }
        Expr::Member(m) => collect_idents_used_in_expr(&m.obj, enclosing, shadowed, captured),
        Expr::Bin(b) => {
            collect_idents_used_in_expr(&b.left, enclosing, shadowed, captured);
            collect_idents_used_in_expr(&b.right, enclosing, shadowed, captured);
        }
        Expr::Unary(u) => collect_idents_used_in_expr(&u.arg, enclosing, shadowed, captured),
        Expr::Update(u) => collect_idents_used_in_expr(&u.arg, enclosing, shadowed, captured),
        Expr::Assign(a) => {
            // LHS Ident também conta como uso (vamos reescrever).
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) =
                &a.left
            {
                let name = id.id.sym.as_str();
                if enclosing.contains(name) && !shadowed.contains(name) {
                    captured.insert(name.to_string());
                }
            }
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &a.left
            {
                collect_idents_used_in_expr(&m.obj, enclosing, shadowed, captured);
            }
            collect_idents_used_in_expr(&a.right, enclosing, shadowed, captured);
        }
        Expr::Call(c) => {
            if let Callee::Expr(e) = &c.callee {
                collect_idents_used_in_expr(e, enclosing, shadowed, captured);
            }
            for a in &c.args {
                collect_idents_used_in_expr(&a.expr, enclosing, shadowed, captured);
            }
        }
        Expr::Cond(c) => {
            collect_idents_used_in_expr(&c.test, enclosing, shadowed, captured);
            collect_idents_used_in_expr(&c.cons, enclosing, shadowed, captured);
            collect_idents_used_in_expr(&c.alt, enclosing, shadowed, captured);
        }
        Expr::Paren(p) => collect_idents_used_in_expr(&p.expr, enclosing, shadowed, captured),
        Expr::Tpl(t) => {
            for e in &t.exprs {
                collect_idents_used_in_expr(e, enclosing, shadowed, captured);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                collect_idents_used_in_expr(&el.expr, enclosing, shadowed, captured);
            }
        }
        Expr::Arrow(arrow) => {
            // Recursão em arrows aninhados: arrow_locals aninhado adiciona-se
            // ao shadowed temporariamente.
            let mut nested_shadowed = shadowed.clone();
            for p in &arrow.params {
                if let Pat::Ident(id) = p {
                    nested_shadowed.insert(id.id.sym.to_string());
                }
            }
            let stmts: Vec<Stmt> = match arrow.body.as_ref() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b.stmts.clone(),
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                        span: Default::default(),
                        arg: Some(e.clone()),
                    })]
                }
            };
            for s in &stmts {
                collect_decls_in_stmt(s, &mut nested_shadowed);
            }
            for s in &stmts {
                collect_idents_used_in_stmt(s, enclosing, &nested_shadowed, captured);
            }
        }
        _ => {}
    }
}

/// Reescreve `Ident(old)` → `Ident(new)` em um statement (recursivo).
fn rename_ident_in_stmt(stmt: &mut Stmt, old: &str, new: &str) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => rename_ident_in_expr(&mut e.expr, old, new),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                rename_ident_in_expr(a, old, new);
            }
        }
        If(i) => {
            rename_ident_in_expr(&mut i.test, old, new);
            rename_ident_in_stmt(&mut i.cons, old, new);
            if let Some(alt) = i.alt.as_deref_mut() {
                rename_ident_in_stmt(alt, old, new);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                rename_ident_in_stmt(s, old, new);
            }
        }
        While(w) => {
            rename_ident_in_expr(&mut w.test, old, new);
            rename_ident_in_stmt(&mut w.body, old, new);
        }
        DoWhile(w) => {
            rename_ident_in_expr(&mut w.test, old, new);
            rename_ident_in_stmt(&mut w.body, old, new);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            rename_ident_in_expr(e, old, new);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                rename_ident_in_expr(t, old, new);
            }
            if let Some(u) = f.update.as_deref_mut() {
                rename_ident_in_expr(u, old, new);
            }
            rename_ident_in_stmt(&mut f.body, old, new);
        }
        ForOf(f) => {
            rename_ident_in_expr(&mut f.right, old, new);
            rename_ident_in_stmt(&mut f.body, old, new);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            // ATENÇÃO: se a var promovida está sendo declarada aqui
            // (ex: `let count = 0`), removemos a declaração? Não — a
            // global é zero-init. Mas precisamos que o init original
            // ainda rode escrevendo no global. Estratégia: mantemos
            // a declaração local (declara `count` como local), mas o
            // global inicial recebe sync no prólogo via
            // `make_global_assign_from_local`. Reescrita só toca usos
            // **após** a decl.
            // Caso simples: deixa init reescrito. Usos posteriores
            // referem ao global.
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    rename_ident_in_expr(e, old, new);
                }
            }
        }
        _ => {}
    }
}

fn rename_ident_in_expr(expr: &mut Expr, old: &str, new: &str) {
    if let Expr::Ident(id) = expr {
        if id.sym.as_str() == old {
            *expr = Expr::Ident(swc_ecma_ast::Ident {
                span: id.span,
                ctxt: id.ctxt,
                sym: new.into(),
                optional: false,
            });
            return;
        }
    }
    match expr {
        Expr::Member(m) => rename_ident_in_expr(&mut m.obj, old, new),
        Expr::Bin(b) => {
            rename_ident_in_expr(&mut b.left, old, new);
            rename_ident_in_expr(&mut b.right, old, new);
        }
        Expr::Unary(u) => rename_ident_in_expr(&mut u.arg, old, new),
        Expr::Update(u) => rename_ident_in_expr(&mut u.arg, old, new),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) =
                &mut a.left
            {
                if id.id.sym.as_str() == old {
                    id.id.sym = new.into();
                }
            }
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                rename_ident_in_expr(&mut m.obj, old, new);
            }
            rename_ident_in_expr(&mut a.right, old, new);
        }
        Expr::Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                rename_ident_in_expr(e, old, new);
            }
            for a in &mut c.args {
                rename_ident_in_expr(&mut a.expr, old, new);
            }
        }
        Expr::Cond(c) => {
            rename_ident_in_expr(&mut c.test, old, new);
            rename_ident_in_expr(&mut c.cons, old, new);
            rename_ident_in_expr(&mut c.alt, old, new);
        }
        Expr::Paren(p) => rename_ident_in_expr(&mut p.expr, old, new),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                rename_ident_in_expr(e, old, new);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rename_ident_in_expr(&mut el.expr, old, new);
            }
        }
        Expr::Arrow(arrow) => {
            // Não renomeia dentro de arrow se ele declara o ident como
            // parâmetro/local (shadow). Para simplicidade do MVP, sempre
            // descemos — assume que captura é exatamente o que queremos.
            match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        rename_ident_in_stmt(s, old, new);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rename_ident_in_expr(e, old, new),
            }
        }
        _ => {}
    }
}

/// Apenas reescreve usos de `old` para `new` em todos os statements
/// (sem tocar declarações). Usado para parâmetros — o param continua
/// recebendo o valor original, mas todos os usos referem à global.
fn rename_uses_in_body(body: &mut Vec<Statement>, old: &str, new: &str) {
    for s in body.iter_mut() {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_mut() {
            rename_ident_in_stmt(stmt, old, new);
        }
    }
}

/// `<global> = <param>;` injetado no topo da fn pra sincronizar valor
/// inicial do parâmetro com a global promovida.
fn make_sync_param_to_global(global: &str, param: &str) -> Statement {
    let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
        span: Default::default(),
        op: swc_ecma_ast::AssignOp::Assign,
        left: swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(
            swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: global.into(),
                    optional: false,
                },
                type_ann: None,
            },
        )),
        right: Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: param.into(),
            optional: false,
        })),
    });
    let stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(assign),
    });
    Statement::Raw(RawStmt::new("<cb-param-sync>".to_string(), Span::default()).with_stmt(stmt))
}

/// Promove uma local da fn pra global. Substitui `let <var> = expr` por
/// `<var-renomeado> = expr` (assignment ao global) e reescreve todas as
/// outras referências.
fn promote_local_to_global(body: &mut Vec<Statement>, old: &str, new: &str) {
    for s in body.iter_mut() {
        let Statement::Raw(raw) = s;
        let Some(stmt) = raw.stmt.as_mut() else {
            continue;
        };
        // Caso especial: `let <var> = expr` no topo do body.
        if let Stmt::Decl(swc_ecma_ast::Decl::Var(v)) = stmt {
            // Se a única decl é o `var` que estamos promovendo, substitui
            // o stmt inteiro por `new = expr`.
            if v.decls.len() == 1 {
                if let Pat::Ident(id) = &v.decls[0].name {
                    if id.id.sym.as_str() == old {
                        let init = v.decls[0].init.clone().unwrap_or_else(|| {
                            // sem init: default 0
                            Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                                span: Default::default(),
                                value: 0.0,
                                raw: Some("0".into()),
                            })))
                        });
                        // Reescreve init recursivamente também (caso o init
                        // referencie outras capturas).
                        let mut init_expr = *init;
                        rename_ident_in_expr(&mut init_expr, old, new);
                        let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
                            span: Default::default(),
                            op: swc_ecma_ast::AssignOp::Assign,
                            left: swc_ecma_ast::AssignTarget::Simple(
                                swc_ecma_ast::SimpleAssignTarget::Ident(
                                    swc_ecma_ast::BindingIdent {
                                        id: swc_ecma_ast::Ident {
                                            span: Default::default(),
                                            ctxt: Default::default(),
                                            sym: new.into(),
                                            optional: false,
                                        },
                                        type_ann: None,
                                    },
                                ),
                            ),
                            right: Box::new(init_expr),
                        });
                        *stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
                            span: Default::default(),
                            expr: Box::new(assign),
                        });
                        continue;
                    }
                }
            }
        }
        // Caso geral: reescreve referências a `old` no statement.
        rename_ident_in_stmt(stmt, old, new);
    }
}

fn extract_class_owner(fn_name: &str) -> Option<String> {
    let rest = fn_name.strip_prefix("__class_")?;
    // Variante: `<C>__init`
    if let Some(idx) = rest.find("__init") {
        return Some(rest[..idx].to_string());
    }
    // Variantes especiais com prefixo de papel: `<C>_get_<x>`,
    // `<C>_set_<x>`, `<C>_static_<x>`. Detecta o prefixo no resto e
    // pega tudo antes dele.
    // `_lifted_arrow_<n>` cobre os trampolins gerados pelo lifter
    // para arrows que capturam `this` ou usam `super`.
    for marker in ["_get_", "_set_", "_static_", "_lifted_arrow_"] {
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
