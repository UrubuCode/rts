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
use swc_ecma_ast::{Callee, Decl, Expr, ForHead, Lit, MemberProp, Pat, Stmt, TsType, TsTypeRef};

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

/// Builds the set of (namespace, member) pairs marked `pure: true` in SPECS.
fn build_pure_ns_set() -> HashSet<(&'static str, &'static str)> {
    let mut s = HashSet::new();
    for spec in crate::abi::SPECS {
        for member in spec.members {
            if member.pure {
                s.insert((spec.name, member.name));
            }
        }
    }
    s
}

/// Returns true if `e` is a pure expression in the context of a ForOf body.
/// Pure: literals, the loop variable, inner-declared idents, arithmetic on
/// pure sub-expressions, and calls to pure namespace members.
fn is_pure_expr_for_parallel(
    e: &Expr,
    loop_var: &str,
    inner: &HashSet<String>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
) -> bool {
    match e {
        Expr::Lit(_) => true,
        Expr::Ident(id) => {
            let n = id.sym.as_str();
            n == loop_var || inner.contains(n)
        }
        Expr::Bin(b) => {
            is_pure_expr_for_parallel(&b.left, loop_var, inner, pure_ns)
                && is_pure_expr_for_parallel(&b.right, loop_var, inner, pure_ns)
        }
        Expr::Unary(u) => is_pure_expr_for_parallel(&u.arg, loop_var, inner, pure_ns),
        Expr::Paren(p) => is_pure_expr_for_parallel(&p.expr, loop_var, inner, pure_ns),
        Expr::TsAs(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsTypeAssertion(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsNonNull(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsConstAssertion(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::Call(call) => {
            let Callee::Expr(ce) = &call.callee else { return false };
            let Expr::Member(m) = ce.as_ref() else { return false };
            let Expr::Ident(ns_id) = m.obj.as_ref() else { return false };
            let MemberProp::Ident(prop_id) = &m.prop else { return false };
            if !pure_ns.contains(&(ns_id.sym.as_str(), prop_id.sym.as_str())) {
                return false;
            }
            call.args.iter().all(|a| {
                a.spread.is_none()
                    && is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns)
            })
        }
        _ => false,
    }
}

/// Returns true if the ForOf body is parallelisable: no assignments, no
/// control flow escapes, only pure namespace calls, all idents are either
/// the loop variable or declared within the body.
fn analyze_for_of_body_pure(
    body: &Stmt,
    loop_var: &str,
    pure_ns: &HashSet<(&'static str, &'static str)>,
) -> bool {
    let stmts: &[Stmt] = match body {
        Stmt::Block(b) => &b.stmts,
        Stmt::Expr(e) => {
            return is_pure_expr_for_parallel(&e.expr, loop_var, &HashSet::new(), pure_ns);
        }
        _ => return false,
    };
    let mut inner: HashSet<String> = HashSet::new();
    for stmt in stmts {
        match stmt {
            Stmt::Decl(Decl::Var(vd)) => {
                for d in &vd.decls {
                    let Pat::Ident(id) = &d.name else { return false };
                    if let Some(init) = &d.init {
                        if !is_pure_expr_for_parallel(init, loop_var, &inner, pure_ns) {
                            return false;
                        }
                    }
                    inner.insert(id.sym.as_str().to_string());
                }
            }
            Stmt::Expr(e) => {
                if !is_pure_expr_for_parallel(&e.expr, loop_var, &inner, pure_ns) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Builds a `parallel.for_each(arr_expr, fn_ident)` expression statement.
fn make_par_foreach_stmt(arr_expr: &Expr, fn_name: &str) -> Stmt {
    Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
            span: Default::default(),
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: "parallel".into(),
                    optional: false,
                })),
                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                    span: Default::default(),
                    sym: "for_each".into(),
                }),
            }))),
            args: vec![
                swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(arr_expr.clone()),
                },
                swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: fn_name.to_string().into(),
                        optional: false,
                    })),
                },
            ],
            type_args: None,
        })),
    })
}

/// Level-1 silent array methods: reescreve `arr.map(fn)`,
/// `arr.forEach(fn)`, `arr.reduce(fn, init)` para `parallel.map(arr, fn)`,
/// `parallel.for_each(arr, fn)`, `parallel.reduce(arr, init, fn)` quando
/// `fn` e um Ident apontando pra uma user fn top-level.
///
/// Visita todos os statements do programa (top-level e bodies de fns)
/// e reescreve os MemberExpr.Call qualificados.
///
/// Nao requer purity check no codegen — o user esta usando a sintaxe
/// JS standard. Se a fn tiver side effects, o resultado paralelo
/// pode ser nao-deterministico (ex: console.log). E responsabilidade
/// do user passar fn pure quando quer behavior consistente.
fn array_methods_pass(program: &mut Program) {
    // Coleta nomes de user fns top-level pra validar que o arg e ident
    // de user fn (caso contrario fica serial — pode ser arrow inline
    // que ja e lifted por outros passes).
    let user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    // Visita top-level statements.
    let n_items = program.items.len();
    for i in 0..n_items {
        let Item::Statement(Statement::Raw(raw)) = &mut program.items[i] else { continue };
        if let Some(stmt) = raw.stmt.as_mut() {
            rewrite_array_methods_in_stmt(stmt, &user_fn_names);
        }
    }

    // Visita body de cada user fn.
    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            for stmt_raw in &mut f.body {
                let Statement::Raw(raw) = stmt_raw;
                if let Some(stmt) = raw.stmt.as_mut() {
                    rewrite_array_methods_in_stmt(stmt, &user_fn_names);
                }
            }
        }
    }
}

fn rewrite_array_methods_in_stmt(stmt: &mut Stmt, user_fn_names: &HashSet<String>) {
    match stmt {
        Stmt::Expr(e) => rewrite_array_methods_in_expr(&mut e.expr, user_fn_names),
        Stmt::Decl(Decl::Var(vd)) => {
            for d in &mut vd.decls {
                if let Some(init) = d.init.as_deref_mut() {
                    rewrite_array_methods_in_expr(init, user_fn_names);
                }
            }
        }
        Stmt::Return(r) => {
            if let Some(arg) = r.arg.as_deref_mut() {
                rewrite_array_methods_in_expr(arg, user_fn_names);
            }
        }
        _ => {}
    }
}

fn rewrite_array_methods_in_expr(expr: &mut Expr, user_fn_names: &HashSet<String>) {
    // Recursa em sub-expressoes simples primeiro.
    if let Expr::Call(call) = expr {
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(m) = callee.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    let method = prop.sym.as_str();
                    let arg0_is_user_fn = call.args.first()
                        .and_then(|a| match a.expr.as_ref() {
                            Expr::Ident(i) => Some(i.sym.to_string()),
                            _ => None,
                        })
                        .map(|n| user_fn_names.contains(&n))
                        .unwrap_or(false);

                    let target_method: Option<&str> = match method {
                        "map" if call.args.len() == 1 && arg0_is_user_fn => Some("map"),
                        "forEach" if call.args.len() == 1 && arg0_is_user_fn => Some("for_each"),
                        // arr.reduce(fn, init) — 2 args: fn primeiro,
                        // init segundo. parallel.reduce(arr, init, fn).
                        "reduce" if call.args.len() == 2 && arg0_is_user_fn => Some("reduce"),
                        _ => None,
                    };

                    if let Some(par_method) = target_method {
                        let arr_expr = (*m.obj).clone();
                        let fn_arg = call.args[0].expr.clone();
                        let new_args: Vec<swc_ecma_ast::ExprOrSpread> = if par_method == "reduce" {
                            // parallel.reduce(arr, init, fn)
                            let init_arg = call.args[1].expr.clone();
                            vec![
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: Box::new(arr_expr) },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: init_arg },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: fn_arg },
                            ]
                        } else {
                            // parallel.map / for_each (arr, fn)
                            vec![
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: Box::new(arr_expr) },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: fn_arg },
                            ]
                        };
                        // Reescreve o call: callee = parallel.<par_method>, args = new_args
                        *call = swc_ecma_ast::CallExpr {
                            span: call.span,
                            ctxt: call.ctxt,
                            callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
                                span: Default::default(),
                                obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                    span: Default::default(), ctxt: Default::default(),
                                    sym: "parallel".into(), optional: false,
                                })),
                                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                                    span: Default::default(),
                                    sym: par_method.to_string().into(),
                                }),
                            }))),
                            args: new_args,
                            type_args: None,
                        };
                        return;
                    }
                }
            }
        }
        // Se nao reescreveu, recursa nos args + callee.
        for a in &mut call.args {
            rewrite_array_methods_in_expr(&mut a.expr, user_fn_names);
        }
    }
}

/// Level-1 silent reduce: detecta padrao
///
/// ```text
/// let acc = INIT;
/// for (const x of arr) {
///     acc = acc + EXPR;     // ou acc += EXPR
/// }
/// ```
///
/// e reescreve para:
///
/// ```text
/// let acc = parallel.reduce(arr, INIT, __par_reduce_N);
/// ```
///
/// Onde `__par_reduce_N(a: i64, x: i64) -> i64` retorna `a + EXPR` com
/// `x` substituido pelo loop var. So aceita operacoes associativas
/// (+, *, max via Math.max) — chunks paralelos podem ser combinados em
/// qualquer ordem. EXPR precisa ser puro (so usa loop var, lits, ou
/// fns pure de namespaces).
///
/// Nota: roda antes do purity_pass. Quando este passa nao casa, o
/// purity_pass tenta a versao for_each (sem reduce), e se nao casar
/// nem isso, o for...of fica serial.
fn reduce_pass(program: &mut Program) -> HashSet<String> {
    let pure_ns = build_pure_ns_set();
    let mut counter = 0u32;
    let mut par_fn_names: HashSet<String> = HashSet::new();
    let mut new_fns: Vec<Item> = Vec::new();

    // Top-level: itera program.items[i] e items[i+1].
    apply_reduce_pass_to_top_level(
        &mut program.items, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
    );

    // Bodies de cada user fn: apply em fn.body (Vec<Statement>).
    // Coletamos os indices primeiro pra evitar borrow conflict.
    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            apply_reduce_pass_to_body(
                &mut f.body, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
            );
        }
    }

    // Prepend new fns
    for fn_item in new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }

    par_fn_names
}

/// Aplica reduce_pass no top-level (lista de Items).
fn apply_reduce_pass_to_top_level(
    items: &mut Vec<Item>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Match {
        decl_idx: usize,
        for_idx: usize,
        acc_name: String,
        init_expr: Expr,
        arr_expr: Expr,
        loop_var: String,
        rhs_expr: Expr,
        fn_name: String,
        op: AssocOp,
    }
    let mut matches: Vec<Match> = Vec::new();
    let n_items = items.len();
    for i in 0..n_items.saturating_sub(1) {
        let Item::Statement(Statement::Raw(decl_raw)) = &items[i] else { continue };
        let Some(Stmt::Decl(Decl::Var(vd))) = decl_raw.stmt.as_ref() else { continue };
        if vd.decls.len() != 1 {
            continue;
        }
        let Pat::Ident(acc_pat) = &vd.decls[0].name else { continue };
        let acc_name = acc_pat.id.sym.as_str().to_string();
        let Some(init) = vd.decls[0].init.as_deref() else { continue };
        // Init precisa ser literal (0, 1, etc)
        if !matches!(init, Expr::Lit(Lit::Num(_))) {
            continue;
        }

        // Item[i+1] precisa ser `for (const x of arr) { ... }`
        let Item::Statement(Statement::Raw(for_raw)) = &items[i + 1] else { continue };
        let Some(Stmt::ForOf(for_of)) = for_raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }

        let loop_var = match &for_of.left {
            ForHead::VarDecl(lvd) if lvd.decls.len() == 1 => match &lvd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => continue,
            },
            _ => continue,
        };

        // Body precisa ter UMA stmt: `acc = acc OP EXPR` ou `acc OP= EXPR`.
        let stmts: &[Stmt] = match for_of.body.as_ref() {
            Stmt::Block(b) => &b.stmts,
            other => std::slice::from_ref(other),
        };
        if stmts.len() != 1 {
            continue;
        }
        let Stmt::Expr(expr_stmt) = &stmts[0] else { continue };
        let Expr::Assign(assign) = expr_stmt.expr.as_ref() else { continue };

        // LHS deve ser o ident `acc`.
        let lhs_ok = matches!(
            &assign.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id))
                if id.id.sym.as_str() == acc_name
        );
        if !lhs_ok { continue; }

        // Detecta op + extrai rhs_expr.
        let (op, rhs_expr): (AssocOp, Expr) = match assign.op {
            swc_ecma_ast::AssignOp::AddAssign => {
                // acc += EXPR
                (AssocOp::Add, (*assign.right).clone())
            }
            swc_ecma_ast::AssignOp::MulAssign => {
                (AssocOp::Mul, (*assign.right).clone())
            }
            swc_ecma_ast::AssignOp::Assign => {
                // acc = acc OP EXPR (procurar bin com acc do lado esquerdo)
                let Expr::Bin(bin) = assign.right.as_ref() else { continue };
                let acc_lhs_ok = matches!(
                    bin.left.as_ref(),
                    Expr::Ident(i) if i.sym.as_str() == acc_name
                );
                if !acc_lhs_ok { continue; }
                let op = match bin.op {
                    swc_ecma_ast::BinaryOp::Add => AssocOp::Add,
                    swc_ecma_ast::BinaryOp::Mul => AssocOp::Mul,
                    _ => continue,
                };
                (op, (*bin.right).clone())
            }
            _ => continue,
        };

        // EXPR deve ser puro (so usa loop_var, lits, fns pure).
        if !is_pure_expr_for_parallel(&rhs_expr, &loop_var, &HashSet::new(), pure_ns) {
            continue;
        }

        let fn_name = format!("__par_reduce_{counter}");
        *counter += 1;
        matches.push(Match {
            decl_idx: i,
            for_idx: i + 1,
            acc_name,
            init_expr: init.clone(),
            arr_expr: for_of.right.as_ref().clone(),
            loop_var,
            rhs_expr,
            fn_name,
            op,
        });
    }

    if matches.is_empty() {
        return;
    }

    // Para cada match, substitui o for...of por NOOP (um stmt vazio
    // expr `0`) e o decl pelo `let acc = parallel.reduce(...)`.
    for m in &matches {
        // Sintetiza fn `(acc: i64, x: i64) -> i64 { return acc OP rhs_expr; }`
        let bin_op = match m.op {
            AssocOp::Add => swc_ecma_ast::BinaryOp::Add,
            AssocOp::Mul => swc_ecma_ast::BinaryOp::Mul,
        };
        let body_expr = Expr::Bin(swc_ecma_ast::BinExpr {
            span: Default::default(),
            op: bin_op,
            left: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: m.acc_name.clone().into(),
                optional: false,
            })),
            right: Box::new(m.rhs_expr.clone()),
        });
        let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(Box::new(body_expr)),
        });
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-reduce>".to_string(), Span::default()).with_stmt(return_stmt),
        )];

        // Params: (acc, x) — usamos o nome `acc_name` pra que o body
        // possa referenciar diretamente, e `loop_var` igualmente.
        new_fns.push(Item::Function(FunctionDecl {
            name: m.fn_name.clone(),
            parameters: vec![
                Parameter {
                    name: m.acc_name.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                },
                Parameter {
                    name: m.loop_var.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                },
            ],
            return_type: Some("i64".to_string()),
            body: body_stmts,
            span: Span::default(),
        }));
        par_fn_names.insert(m.fn_name.clone());
    }

    // Aplica replacements: decl vira `let acc = parallel.reduce(arr, init, fn)`,
    // for...of vira no-op (`0;`).
    for m in &matches {
        // Substitui o for_idx por no-op (Expr `0`).
        if let Item::Statement(Statement::Raw(raw)) = &mut items[m.for_idx] {
            raw.stmt = Some(Stmt::Expr(swc_ecma_ast::ExprStmt {
                span: Default::default(),
                expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                    span: Default::default(),
                    value: 0.0,
                    raw: None,
                }))),
            }));
        }

        // Substitui o decl pelo `let acc = parallel.reduce(...)`.
        if let Item::Statement(Statement::Raw(raw)) = &mut items[m.decl_idx] {
            let reduce_call = make_par_reduce_expr(&m.arr_expr, &m.init_expr, &m.fn_name);
            raw.stmt = Some(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
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
                            sym: m.acc_name.clone().into(),
                            optional: false,
                        },
                        type_ann: None,
                    }),
                    init: Some(Box::new(reduce_call)),
                    definite: false,
                }],
            }))));
        }
    }
}

/// Aplica reduce_pass num body de fn (lista de Statements).
/// Usa stmt-level matching: procura `let acc = ...; for (...)` adjacentes.
fn apply_reduce_pass_to_body(
    body: &mut Vec<Statement>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Match {
        decl_idx: usize,
        for_idx: usize,
        acc_name: String,
        init_expr: Expr,
        arr_expr: Expr,
        loop_var: String,
        rhs_expr: Expr,
        fn_name: String,
        op: AssocOp,
    }
    let mut matches: Vec<Match> = Vec::new();
    let n = body.len();
    for i in 0..n.saturating_sub(1) {
        let Statement::Raw(decl_raw) = &body[i];
        let Some(Stmt::Decl(Decl::Var(vd))) = decl_raw.stmt.as_ref() else { continue };
        if vd.decls.len() != 1 { continue; }
        let Pat::Ident(acc_pat) = &vd.decls[0].name else { continue };
        let acc_name = acc_pat.id.sym.as_str().to_string();
        let Some(init) = vd.decls[0].init.as_deref() else { continue };
        if !matches!(init, Expr::Lit(Lit::Num(_))) { continue; }
        let Statement::Raw(for_raw) = &body[i + 1];
        let Some(Stmt::ForOf(for_of)) = for_raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(lvd) if lvd.decls.len() == 1 => match &lvd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => continue,
            },
            _ => continue,
        };
        let stmts: &[Stmt] = match for_of.body.as_ref() {
            Stmt::Block(b) => &b.stmts,
            other => std::slice::from_ref(other),
        };
        if stmts.len() != 1 { continue; }
        let Stmt::Expr(expr_stmt) = &stmts[0] else { continue };
        let Expr::Assign(assign) = expr_stmt.expr.as_ref() else { continue };
        let lhs_ok = matches!(
            &assign.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id))
                if id.id.sym.as_str() == acc_name
        );
        if !lhs_ok { continue; }
        let (op, rhs_expr): (AssocOp, Expr) = match assign.op {
            swc_ecma_ast::AssignOp::AddAssign => (AssocOp::Add, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::MulAssign => (AssocOp::Mul, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::Assign => {
                let Expr::Bin(bin) = assign.right.as_ref() else { continue };
                let acc_lhs_ok = matches!(
                    bin.left.as_ref(),
                    Expr::Ident(i) if i.sym.as_str() == acc_name
                );
                if !acc_lhs_ok { continue; }
                let op = match bin.op {
                    swc_ecma_ast::BinaryOp::Add => AssocOp::Add,
                    swc_ecma_ast::BinaryOp::Mul => AssocOp::Mul,
                    _ => continue,
                };
                (op, (*bin.right).clone())
            }
            _ => continue,
        };
        if !is_pure_expr_for_parallel(&rhs_expr, &loop_var, &HashSet::new(), pure_ns) {
            continue;
        }
        let fn_name = format!("__par_reduce_{counter}");
        *counter += 1;
        matches.push(Match {
            decl_idx: i, for_idx: i + 1, acc_name, init_expr: init.clone(),
            arr_expr: for_of.right.as_ref().clone(),
            loop_var, rhs_expr, fn_name, op,
        });
    }
    if matches.is_empty() { return; }
    for m in &matches {
        let bin_op = match m.op {
            AssocOp::Add => swc_ecma_ast::BinaryOp::Add,
            AssocOp::Mul => swc_ecma_ast::BinaryOp::Mul,
        };
        let body_expr = Expr::Bin(swc_ecma_ast::BinExpr {
            span: Default::default(),
            op: bin_op,
            left: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(), ctxt: Default::default(),
                sym: m.acc_name.clone().into(), optional: false,
            })),
            right: Box::new(m.rhs_expr.clone()),
        });
        let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(), arg: Some(Box::new(body_expr)),
        });
        let fn_body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-reduce>".to_string(), Span::default()).with_stmt(return_stmt),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: m.fn_name.clone(),
            parameters: vec![
                Parameter {
                    name: m.acc_name.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false, default: None, span: Span::default(),
                },
                Parameter {
                    name: m.loop_var.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false, default: None, span: Span::default(),
                },
            ],
            return_type: Some("i64".to_string()),
            body: fn_body_stmts,
            span: Span::default(),
        }));
        par_fn_names.insert(m.fn_name.clone());
    }
    for m in &matches {
        let Statement::Raw(raw) = &mut body[m.for_idx];
        raw.stmt = Some(Stmt::Expr(swc_ecma_ast::ExprStmt {
            span: Default::default(),
            expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                span: Default::default(), value: 0.0, raw: None,
            }))),
        }));
        let reduce_call = make_par_reduce_expr(&m.arr_expr, &m.init_expr, &m.fn_name);
        let Statement::Raw(raw2) = &mut body[m.decl_idx];
        raw2.stmt = Some(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
            span: Default::default(), ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let, declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(swc_ecma_ast::BindingIdent {
                    id: swc_ecma_ast::Ident {
                        span: Default::default(), ctxt: Default::default(),
                        sym: m.acc_name.clone().into(), optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(reduce_call)), definite: false,
            }],
        }))));
    }
}

#[derive(Clone, Copy)]
enum AssocOp {
    Add,
    Mul,
}

fn make_par_reduce_expr(arr_expr: &Expr, init_expr: &Expr, fn_name: &str) -> Expr {
    Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
            span: Default::default(),
            obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: "parallel".into(),
                optional: false,
            })),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                span: Default::default(),
                sym: "reduce".into(),
            }),
        }))),
        args: vec![
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(arr_expr.clone()),
            },
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(init_expr.clone()),
            },
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: fn_name.to_string().into(),
                    optional: false,
                })),
            },
        ],
        type_args: None,
    })
}

/// Level-1 silent parallelism: rewrites pure top-level `for...of` loops into
/// `parallel.for_each(arr, __par_forof_N)` calls backed by a Rayon thread
/// pool. A ForOf is eligible when:
///   - no assignments in the body
///   - all function calls are to pure namespace members
///   - all idents in the body are either the loop variable or inner decls
///   - no break / continue / return / throw
///
/// For each eligible loop a synthetic `FunctionDecl` is prepended to the
/// program, and the loop statement is replaced with the `for_each` call.
/// Returns the set of synthetic function names so `compile_program` can give
/// them C calling convention (required for Rayon worker invocations).
fn purity_pass(program: &mut Program) -> HashSet<String> {
    let pure_ns = build_pure_ns_set();
    let mut counter = 0u32;
    let mut par_fn_names: HashSet<String> = HashSet::new();
    let mut new_fns: Vec<Item> = Vec::new();

    apply_purity_pass_to_top_level(
        &mut program.items, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
    );

    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            apply_purity_pass_to_body(
                &mut f.body, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
            );
        }
    }

    for fn_item in new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }

    par_fn_names
}

fn apply_purity_pass_to_top_level(
    items: &mut Vec<Item>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Transform {
        idx: usize,
        arr_expr: Expr,
        body_stmt: Stmt,
        loop_var: String,
        fn_name: String,
    }
    let mut transforms: Vec<Transform> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::ForOf(for_of)) = raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(vd) => {
                if vd.decls.len() != 1 { continue; }
                match &vd.decls[0].name {
                    Pat::Ident(id) => id.sym.as_str().to_string(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !analyze_for_of_body_pure(&for_of.body, &loop_var, pure_ns) { continue; }
        let fn_name = format!("__par_forof_{counter}");
        *counter += 1;
        transforms.push(Transform {
            idx, arr_expr: for_of.right.as_ref().clone(),
            body_stmt: for_of.body.as_ref().clone(),
            loop_var, fn_name,
        });
    }
    if transforms.is_empty() { return; }
    for t in &transforms {
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-forof>".to_string(), Span::default())
                .with_stmt(t.body_stmt.clone()),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: t.fn_name.clone(),
            parameters: vec![Parameter {
                name: t.loop_var.clone(),
                type_annotation: Some("i64".to_string()),
                modifiers: MemberModifiers::default(),
                variadic: false, default: None, span: Span::default(),
            }],
            return_type: Some("void".to_string()),
            body: body_stmts, span: Span::default(),
        }));
        par_fn_names.insert(t.fn_name.clone());
    }
    for t in &transforms {
        if let Item::Statement(Statement::Raw(raw)) = &mut items[t.idx] {
            raw.stmt = Some(make_par_foreach_stmt(&t.arr_expr, &t.fn_name));
        }
    }
}

fn apply_purity_pass_to_body(
    body: &mut Vec<Statement>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Transform {
        idx: usize,
        arr_expr: Expr,
        body_stmt: Stmt,
        loop_var: String,
        fn_name: String,
    }
    let mut transforms: Vec<Transform> = Vec::new();
    for (idx, stmt) in body.iter().enumerate() {
        let Statement::Raw(raw) = stmt;
        let Some(Stmt::ForOf(for_of)) = raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(vd) => {
                if vd.decls.len() != 1 { continue; }
                match &vd.decls[0].name {
                    Pat::Ident(id) => id.sym.as_str().to_string(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !analyze_for_of_body_pure(&for_of.body, &loop_var, pure_ns) { continue; }
        let fn_name = format!("__par_forof_{counter}");
        *counter += 1;
        transforms.push(Transform {
            idx, arr_expr: for_of.right.as_ref().clone(),
            body_stmt: for_of.body.as_ref().clone(),
            loop_var, fn_name,
        });
    }
    if transforms.is_empty() { return; }
    for t in &transforms {
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-forof>".to_string(), Span::default())
                .with_stmt(t.body_stmt.clone()),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: t.fn_name.clone(),
            parameters: vec![Parameter {
                name: t.loop_var.clone(),
                type_annotation: Some("i64".to_string()),
                modifiers: MemberModifiers::default(),
                variadic: false, default: None, span: Span::default(),
            }],
            return_type: Some("void".to_string()),
            body: body_stmts, span: Span::default(),
        }));
        par_fn_names.insert(t.fn_name.clone());
    }
    for t in &transforms {
        let Statement::Raw(raw) = &mut body[t.idx];
        raw.stmt = Some(make_par_foreach_stmt(&t.arr_expr, &t.fn_name));
    }
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
    // Marca `fp` como alias da user fn para o lifter detectar idents
    // wrappados (necessario p/ thread.spawn, sync.once_call etc).
    fn peel_for_alias<'a>(e: &'a Expr) -> &'a Expr {
        match e {
            Expr::TsAs(a) => peel_for_alias(&a.expr),
            Expr::TsTypeAssertion(a) => peel_for_alias(&a.expr),
            Expr::TsConstAssertion(a) => peel_for_alias(&a.expr),
            Expr::Paren(p) => peel_for_alias(&p.expr),
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
/// Expande static fields de classe: cada `static x: T = init` vira
/// uma `let __class_static_<C>_<x> = init` no top-level, e todos os
/// usos `C.x` (read/write/update) sao reescritos para o ident global.
///
/// Precondicao: roda antes de expand_destructuring/default_args/etc,
/// mas depois do parser. As classes ainda estao em Item::Class.
fn expand_static_fields(program: &mut Program) {
    use crate::parser::ast::ClassMember;

    // Coleta (class, field, type_ann, init) e remove os Property static.
    struct StaticField {
        class: String,
        field: String,
        type_ann: Option<String>,
        init: Option<Box<Expr>>,
    }
    let mut fields: Vec<StaticField> = Vec::new();
    // Mapa: (class_name, field) -> nome global gerado. Usado pra
    // reescrita de C.field e pra detectar conflitos.
    let mut static_map: HashMap<(String, String), String> = HashMap::new();
    // Stmts dos `static { ... }` blocks, drenados das ClassDecls,
    // re-emitidos como top-level apos as let de static fields.
    let mut static_init_stmts: Vec<Statement> = Vec::new();

    for item in program.items.iter_mut() {
        let Item::Class(class) = item else { continue };
        let class_name = class.name.clone();
        class.members.retain(|m| {
            if let ClassMember::Property(p) = m {
                if p.modifiers.is_static {
                    let global_name = format!("__class_static_{}_{}", class_name, p.name);
                    static_map.insert((class_name.clone(), p.name.clone()), global_name);
                    fields.push(StaticField {
                        class: class_name.clone(),
                        field: p.name.clone(),
                        type_ann: p.type_annotation.clone(),
                        init: p.initializer.clone(),
                    });
                    return false; // remove
                }
            }
            true
        });
        // Drena `static { ... }` blocks. Mantem ordem source — multiplos
        // blocks na mesma classe sao concatenados, e classes anteriores
        // executam antes (program order).
        if !class.static_init_body.is_empty() {
            static_init_stmts.extend(std::mem::take(&mut class.static_init_body));
        }
    }

    if fields.is_empty() && static_init_stmts.is_empty() {
        return;
    }

    // Reescreve C.F em todas as expressões. Suporta:
    //   - read:  Class.field           -> Ident(global)
    //   - write: Class.field = v       -> Ident(global) = v
    //   - update: Class.field++        -> via Ident
    //   - compound: Class.field += v   -> via Ident
    let static_keys: HashMap<String, HashMap<String, String>> = {
        let mut m: HashMap<String, HashMap<String, String>> = HashMap::new();
        for ((c, f), g) in &static_map {
            m.entry(c.clone()).or_default().insert(f.clone(), g.clone());
        }
        m
    };

    rewrite_static_in_program(program, &static_keys);

    // Insere `let __class_static_<C>_<F> = init;` antes de qualquer
    // statement no programa. Tem que rodar antes do uso, e o codegen
    // promove `let` top-level a global automaticamente.
    let mut decls: Vec<Item> = Vec::new();
    for sf in &fields {
        let global_name = format!("__class_static_{}_{}", sf.class, sf.field);
        let init_expr = match &sf.init {
            Some(e) => (**e).clone(),
            None => default_expr_for_ann(sf.type_ann.as_deref()),
        };
        let mut binding = swc_ecma_ast::BindingIdent {
            id: swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: global_name.into(),
                optional: false,
            },
            type_ann: None,
        };
        if let Some(ann) = sf.type_ann.as_deref() {
            // Anota `let x: T = ...` para preservar tipo.
            binding.type_ann = build_type_ann(ann);
        }
        let var = swc_ecma_ast::VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let,
            declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: swc_ecma_ast::Pat::Ident(binding),
                init: Some(Box::new(init_expr)),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
        let raw = RawStmt::new("<static-field>".to_string(), Span::default()).with_stmt(stmt);
        decls.push(Item::Statement(Statement::Raw(raw)));
    }

    // Anexa os stmts dos `static { }` blocks depois das declaracoes
    // das let. Eles vao por `rewrite_static_in_program` na proxima fase
    // — mas como ja' rodamos rewrite acima, precisamos rodar so' neles.
    for stmt in static_init_stmts.iter_mut() {
        let Statement::Raw(r) = stmt;
        if let Some(s) = r.stmt.as_mut() {
            rewrite_static_in_swc_stmt(s, &static_keys);
        }
    }
    for stmt in static_init_stmts {
        decls.push(Item::Statement(stmt));
    }

    // Insere antes da primeira Class declaration (pra que ja existam
    // quando o codegen das classes processar referências).
    let insert_at = program
        .items
        .iter()
        .position(|i| matches!(i, Item::Class(_)))
        .unwrap_or(0);
    for (i, decl) in decls.into_iter().enumerate() {
        program.items.insert(insert_at + i, decl);
    }
}

/// Default value compativel com a anotacao do field. Static fields sem
/// initializer (ex: `static readonly X: string;`) ainda precisam de um
/// init na let global pra Cranelift nao reclamar de tipo. O valor real
/// chega via static {} block.
fn default_expr_for_ann(ann: Option<&str>) -> Expr {
    match ann.map(str::trim) {
        Some("string") => Expr::Lit(Lit::Str(swc_ecma_ast::Str {
            span: Default::default(),
            value: "".into(),
            raw: None,
        })),
        Some("boolean") | Some("bool") => Expr::Lit(Lit::Bool(swc_ecma_ast::Bool {
            span: Default::default(),
            value: false,
        })),
        _ => Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: None,
        })),
    }
}

/// Constroi um TsTypeAnn minimo a partir de uma anotacao como "number"
/// ou "string". Tipos compostos sao deixados como undefined-ann; o
/// codegen ainda usara o tipo do initializer.
fn build_type_ann(ann: &str) -> Option<Box<swc_ecma_ast::TsTypeAnn>> {
    use swc_ecma_ast::{TsKeywordType, TsKeywordTypeKind, TsType, TsTypeAnn};
    let kind = match ann.trim() {
        "number" | "i64" | "f64" | "i32" => TsKeywordTypeKind::TsNumberKeyword,
        "string" => TsKeywordTypeKind::TsStringKeyword,
        "boolean" | "bool" => TsKeywordTypeKind::TsBooleanKeyword,
        _ => return None,
    };
    Some(Box::new(TsTypeAnn {
        span: Default::default(),
        type_ann: Box::new(TsType::TsKeywordType(TsKeywordType {
            span: Default::default(),
            kind,
        })),
    }))
}

/// Reescreve `C.F` -> `Ident(__class_static_C_F)` em todo o programa.
fn rewrite_static_in_program(
    program: &mut Program,
    map: &HashMap<String, HashMap<String, String>>,
) {
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for stmt in f.body.iter_mut() {
                    rewrite_static_in_statement(stmt, map);
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                rewrite_static_in_statement(s, map);
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                rewrite_static_in_statement(s, map);
                            }
                        }
                        ClassMember::Property(p) => {
                            if let Some(init) = p.initializer.as_mut() {
                                rewrite_static_in_expr(init, map);
                            }
                        }
                    }
                }
            }
            Item::Statement(Statement::Raw(r)) => {
                if let Some(s) = r.stmt.as_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_static_in_statement(
    stmt: &mut Statement,
    map: &HashMap<String, HashMap<String, String>>,
) {
    let Statement::Raw(r) = stmt;
    if let Some(s) = r.stmt.as_mut() {
        rewrite_static_in_swc_stmt(s, map);
    }
}

fn rewrite_static_in_swc_stmt(s: &mut Stmt, map: &HashMap<String, HashMap<String, String>>) {
    match s {
        Stmt::Block(b) => {
            for s in b.stmts.iter_mut() {
                rewrite_static_in_swc_stmt(s, map);
            }
        }
        Stmt::Expr(e) => rewrite_static_in_expr(&mut e.expr, map),
        Stmt::Return(r) => {
            if let Some(e) = r.arg.as_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Stmt::If(i) => {
            rewrite_static_in_expr(&mut i.test, map);
            rewrite_static_in_swc_stmt(&mut i.cons, map);
            if let Some(alt) = i.alt.as_mut() {
                rewrite_static_in_swc_stmt(alt, map);
            }
        }
        Stmt::While(w) => {
            rewrite_static_in_expr(&mut w.test, map);
            rewrite_static_in_swc_stmt(&mut w.body, map);
        }
        Stmt::DoWhile(d) => {
            rewrite_static_in_expr(&mut d.test, map);
            rewrite_static_in_swc_stmt(&mut d.body, map);
        }
        Stmt::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => rewrite_static_in_expr(e, map),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                        for d in vd.decls.iter_mut() {
                            if let Some(e) = d.init.as_mut() {
                                rewrite_static_in_expr(e, map);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_mut() {
                rewrite_static_in_expr(t, map);
            }
            if let Some(u) = f.update.as_mut() {
                rewrite_static_in_expr(u, map);
            }
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::ForOf(f) => {
            rewrite_static_in_expr(&mut f.right, map);
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::ForIn(f) => {
            rewrite_static_in_expr(&mut f.right, map);
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::Switch(sw) => {
            rewrite_static_in_expr(&mut sw.discriminant, map);
            for c in sw.cases.iter_mut() {
                if let Some(t) = c.test.as_mut() {
                    rewrite_static_in_expr(t, map);
                }
                for s in c.cons.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        Stmt::Throw(t) => rewrite_static_in_expr(&mut t.arg, map),
        Stmt::Try(t) => {
            for s in t.block.stmts.iter_mut() {
                rewrite_static_in_swc_stmt(s, map);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in h.body.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in f.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(e) = d.init.as_mut() {
                    rewrite_static_in_expr(e, map);
                }
            }
        }
        Stmt::Labeled(l) => rewrite_static_in_swc_stmt(&mut l.body, map),
        _ => {}
    }
}

fn rewrite_static_in_expr(e: &mut Expr, map: &HashMap<String, HashMap<String, String>>) {
    // Caso 1: Member { obj: Ident(C), prop: Ident(F) } onde (C,F) sao
    // static field — substitui o Member inteiro por Ident(global).
    if let Expr::Member(m) = e {
        if let (Expr::Ident(obj), MemberProp::Ident(prop)) = (m.obj.as_ref(), &m.prop) {
            if let Some(fields) = map.get(obj.sym.as_ref()) {
                if let Some(global) = fields.get(prop.sym.as_ref()) {
                    *e = Expr::Ident(swc_ecma_ast::Ident {
                        span: m.span,
                        ctxt: Default::default(),
                        sym: global.clone().into(),
                        optional: false,
                    });
                    return;
                }
            }
        }
    }
    // Recursa em sub-expressões.
    match e {
        Expr::Bin(b) => {
            rewrite_static_in_expr(&mut b.left, map);
            rewrite_static_in_expr(&mut b.right, map);
        }
        Expr::Unary(u) => rewrite_static_in_expr(&mut u.arg, map),
        Expr::Update(u) => rewrite_static_in_expr(&mut u.arg, map),
        Expr::Assign(a) => {
            // LHS pode ser Class.field. swc tem AssignTarget — quando
            // for SimpleAssignTarget::Member com Class.field, substitui
            // por SimpleAssignTarget::Ident(global).
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &mut a.left
            {
                if let (Expr::Ident(obj), MemberProp::Ident(prop)) =
                    (m.obj.as_ref(), &m.prop)
                {
                    if let Some(fields) = map.get(obj.sym.as_ref()) {
                        if let Some(global) = fields.get(prop.sym.as_ref()) {
                            a.left = swc_ecma_ast::AssignTarget::Simple(
                                swc_ecma_ast::SimpleAssignTarget::Ident(
                                    swc_ecma_ast::BindingIdent {
                                        id: swc_ecma_ast::Ident {
                                            span: m.span,
                                            ctxt: Default::default(),
                                            sym: global.clone().into(),
                                            optional: false,
                                        },
                                        type_ann: None,
                                    },
                                ),
                            );
                        } else {
                            rewrite_static_in_expr(&mut m.obj, map);
                        }
                    } else {
                        rewrite_static_in_expr(&mut m.obj, map);
                    }
                } else {
                    rewrite_static_in_expr(&mut m.obj, map);
                }
            }
            rewrite_static_in_expr(&mut a.right, map);
        }
        Expr::Cond(c) => {
            rewrite_static_in_expr(&mut c.test, map);
            rewrite_static_in_expr(&mut c.cons, map);
            rewrite_static_in_expr(&mut c.alt, map);
        }
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &mut c.callee {
                rewrite_static_in_expr(callee, map);
            }
            for a in c.args.iter_mut() {
                rewrite_static_in_expr(&mut a.expr, map);
            }
        }
        Expr::New(n) => {
            rewrite_static_in_expr(&mut n.callee, map);
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    rewrite_static_in_expr(&mut a.expr, map);
                }
            }
        }
        Expr::Member(m) => {
            rewrite_static_in_expr(&mut m.obj, map);
            if let MemberProp::Computed(c) = &mut m.prop {
                rewrite_static_in_expr(&mut c.expr, map);
            }
        }
        Expr::Paren(p) => rewrite_static_in_expr(&mut p.expr, map),
        Expr::Seq(s) => {
            for e in s.exprs.iter_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rewrite_static_in_expr(&mut el.expr, map);
            }
        }
        Expr::Object(o) => {
            for p in o.props.iter_mut() {
                if let swc_ecma_ast::PropOrSpread::Prop(p) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_mut() {
                        rewrite_static_in_expr(&mut kv.value, map);
                    }
                }
            }
        }
        Expr::Tpl(t) => {
            for e in t.exprs.iter_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Expr::TsAs(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::TsTypeAssertion(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::TsNonNull(n) => rewrite_static_in_expr(&mut n.expr, map),
        Expr::TsConstAssertion(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::Arrow(ar) => match ar.body.as_mut() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                for s in b.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rewrite_static_in_expr(e, map),
        },
        Expr::Fn(f) => {
            if let Some(body) = f.function.body.as_mut() {
                for s in body.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        _ => {}
    }
}

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

        self.new_fns.push(Item::Function(FunctionDecl {
            name: syn_name.clone(),
            parameters: Vec::new(),
            return_type: Some("void".to_string()),
            body: body_stmts,
            span: Span::default(),
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
    expand_static_fields(program);
    array_methods_pass(program);
    let mut par_fn_names = reduce_pass(program);
    par_fn_names.extend(purity_pass(program));
    let lifted_needs_c_callconv = lift_arrow_callbacks(program);
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

    // Phase 1: declare all user functions so forward calls resolve.
    let mut user_fns: HashMap<String, UserFn> = HashMap::new();
    for fn_decl in &fn_decls {
        let address_taken = address_taken_fns.contains(&fn_decl.name);
        let info = declare_user_fn(module, fn_decl, address_taken)?;
        let mangled: &'static str = Box::leak(format!("__user_{}", fn_decl.name).into_boxed_str());
        extern_cache.insert(mangled, info.id);
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
        let address_taken = address_taken_fns.contains(&fn_decl.name);
        let fn_warnings = compile_user_fn(
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

    // Identifica vars top-level cujo identificador eh referenciado por
    // alguma user fn (ou class method). Essas precisam de storage global
    // pra que outras fns possam ler/escrever atraves do mesmo data_id.
    // Vars NAO referenciadas viram Cranelift Variables locais a __RTS_MAIN
    // (sem load/store em hot loops top-level — 5x speedup mensurado).
    let referenced = collect_idents_referenced_in_user_fns(program);

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

            // Promote-to-local: se nenhuma user fn referencia o nome,
            // pula a alocacao do data global. lower_var_decl em
            // module_scope cai em declare_local_kind quando has_global
            // retorna false.
            if !referenced.contains(&name) {
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

/// Coleta nomes de identifiers usados em qualquer body de user fn ou
/// class method (incl. arrows lifted, callbacks). Vars top-level cujo
/// nome esta nesse set precisam de storage global; as demais sao
/// seguras pra promover a Variables locais a __RTS_MAIN.
///
/// Conservador: nao distingue read de write, nao olha shadow scoping
/// (params/locais com mesmo nome contam como referencia). Falsos
/// positivos so pioram em deixar var como global (caminho ja-padrao).
/// Falsos negativos seriam graves — varredura completa de Expr.
fn collect_idents_referenced_in_user_fns(program: &Program) -> HashSet<String> {
    use swc_ecma_ast::{Expr, Pat, Stmt};

    let mut out: HashSet<String> = HashSet::new();

    fn walk_expr(e: &Expr, out: &mut HashSet<String>) {
        match e {
            Expr::Ident(id) => {
                out.insert(id.sym.as_str().to_string());
            }
            Expr::Array(a) => {
                for elem in a.elems.iter().flatten() {
                    walk_expr(&elem.expr, out);
                }
            }
            Expr::Object(o) => {
                for prop in &o.props {
                    if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                            walk_expr(&kv.value, out);
                        }
                    } else if let swc_ecma_ast::PropOrSpread::Spread(sp) = prop {
                        walk_expr(&sp.expr, out);
                    }
                }
            }
            Expr::Unary(u) => walk_expr(&u.arg, out),
            Expr::Update(u) => walk_expr(&u.arg, out),
            Expr::Bin(b) => {
                walk_expr(&b.left, out);
                walk_expr(&b.right, out);
            }
            Expr::Assign(a) => {
                if let swc_ecma_ast::AssignTarget::Simple(s) = &a.left {
                    if let swc_ecma_ast::SimpleAssignTarget::Ident(id) = s {
                        out.insert(id.id.sym.as_str().to_string());
                    } else if let swc_ecma_ast::SimpleAssignTarget::Member(m) = s {
                        walk_expr(&m.obj, out);
                    }
                }
                walk_expr(&a.right, out);
            }
            Expr::Member(m) => {
                walk_expr(&m.obj, out);
                if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                    walk_expr(&c.expr, out);
                }
            }
            Expr::Cond(c) => {
                walk_expr(&c.test, out);
                walk_expr(&c.cons, out);
                walk_expr(&c.alt, out);
            }
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    walk_expr(callee, out);
                }
                for a in &c.args {
                    walk_expr(&a.expr, out);
                }
            }
            Expr::New(n) => {
                walk_expr(&n.callee, out);
                if let Some(args) = n.args.as_ref() {
                    for a in args {
                        walk_expr(&a.expr, out);
                    }
                }
            }
            Expr::Seq(s) => {
                for e in &s.exprs {
                    walk_expr(e, out);
                }
            }
            Expr::Tpl(t) => {
                for e in &t.exprs {
                    walk_expr(e, out);
                }
            }
            Expr::Paren(p) => walk_expr(&p.expr, out),
            Expr::TsAs(a) => walk_expr(&a.expr, out),
            Expr::TsTypeAssertion(a) => walk_expr(&a.expr, out),
            Expr::TsConstAssertion(a) => walk_expr(&a.expr, out),
            Expr::TsNonNull(n) => walk_expr(&n.expr, out),
            Expr::Arrow(a) => {
                use swc_ecma_ast::BlockStmtOrExpr;
                match a.body.as_ref() {
                    BlockStmtOrExpr::BlockStmt(b) => {
                        for s in &b.stmts {
                            walk_stmt(s, out);
                        }
                    }
                    BlockStmtOrExpr::Expr(e) => walk_expr(e, out),
                }
            }
            Expr::Fn(f) => {
                if let Some(body) = f.function.body.as_ref() {
                    for s in &body.stmts {
                        walk_stmt(s, out);
                    }
                }
            }
            _ => {}
        }
    }

    fn walk_stmt(s: &Stmt, out: &mut HashSet<String>) {
        match s {
            Stmt::Expr(e) => walk_expr(&e.expr, out),
            Stmt::Block(b) => {
                for s in &b.stmts {
                    walk_stmt(s, out);
                }
            }
            Stmt::Return(r) => {
                if let Some(e) = r.arg.as_ref() {
                    walk_expr(e, out);
                }
            }
            Stmt::If(i) => {
                walk_expr(&i.test, out);
                walk_stmt(&i.cons, out);
                if let Some(alt) = i.alt.as_ref() {
                    walk_stmt(alt, out);
                }
            }
            Stmt::While(w) => {
                walk_expr(&w.test, out);
                walk_stmt(&w.body, out);
            }
            Stmt::DoWhile(d) => {
                walk_expr(&d.test, out);
                walk_stmt(&d.body, out);
            }
            Stmt::For(f) => {
                if let Some(init) = f.init.as_ref() {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::Expr(e) => walk_expr(e, out),
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                            for d in &vd.decls {
                                if let Some(e) = d.init.as_ref() {
                                    walk_expr(e, out);
                                }
                            }
                        }
                    }
                }
                if let Some(t) = f.test.as_ref() {
                    walk_expr(t, out);
                }
                if let Some(u) = f.update.as_ref() {
                    walk_expr(u, out);
                }
                walk_stmt(&f.body, out);
            }
            Stmt::ForOf(f) => {
                walk_expr(&f.right, out);
                walk_stmt(&f.body, out);
            }
            Stmt::ForIn(f) => {
                walk_expr(&f.right, out);
                walk_stmt(&f.body, out);
            }
            Stmt::Switch(s) => {
                walk_expr(&s.discriminant, out);
                for c in &s.cases {
                    if let Some(t) = c.test.as_ref() {
                        walk_expr(t, out);
                    }
                    for s in &c.cons {
                        walk_stmt(s, out);
                    }
                }
            }
            Stmt::Throw(t) => walk_expr(&t.arg, out),
            Stmt::Try(t) => {
                for s in &t.block.stmts {
                    walk_stmt(s, out);
                }
                if let Some(h) = t.handler.as_ref() {
                    if let Some(Pat::Ident(id)) = h.param.as_ref() {
                        out.insert(id.id.sym.as_str().to_string());
                    }
                    for s in &h.body.stmts {
                        walk_stmt(s, out);
                    }
                }
                if let Some(f) = t.finalizer.as_ref() {
                    for s in &f.stmts {
                        walk_stmt(s, out);
                    }
                }
            }
            Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) => {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_ref() {
                        walk_expr(e, out);
                    }
                }
            }
            Stmt::Labeled(l) => walk_stmt(&l.body, out),
            _ => {}
        }
    }

    for item in &program.items {
        match item {
            Item::Function(f) => {
                for stmt_raw in &f.body {
                    let Statement::Raw(raw) = stmt_raw;
                    if let Some(s) = raw.stmt.as_ref() {
                        walk_stmt(s, &mut out);
                    }
                }
            }
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(method) => {
                            for stmt_raw in &method.body {
                                let Statement::Raw(raw) = stmt_raw;
                                if let Some(s) = raw.stmt.as_ref() {
                                    walk_stmt(s, &mut out);
                                }
                            }
                        }
                        ClassMember::Constructor(ctor) => {
                            for stmt_raw in &ctor.body {
                                let Statement::Raw(raw) = stmt_raw;
                                if let Some(s) = raw.stmt.as_ref() {
                                    walk_stmt(s, &mut out);
                                }
                            }
                        }
                        ClassMember::Property(p) => {
                            if let Some(init) = p.initializer.as_ref() {
                                walk_expr(init, &mut out);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    out
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
        // `new X(...)` sempre produz uma instancia (handle GC). Cobre
        // classes do usuario E Map/Set v0 (#222) em top-level — sem
        // isso o global eh declarado como I64 e member calls em
        // receiver Handle nao disparam.
        Expr::New(_) => ValTy::Handle,
        // Array literal `[...]` aloca Vec via collections.vec_new e armazena
        // o handle. Sem isso `let arr: T[] = []` em top-level vira I64 e
        // `.length`/`.push` no codegen nao reconhecem como Handle (caem em
        // map_get nominal — o que segfauta ou retorna lixo).
        Expr::Array(_) => ValTy::Handle,
        // Object literal `{ ... }` idem — aloca Map via collections.map_new.
        Expr::Object(_) => ValTy::Handle,
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
    if let Some((_, member)) = crate::abi::lookup(&qualified) {
        return Some(ValTy::from_abi(member.returns));
    }
    // Redirect implicito JSON.* → json.* (#215). Sem isso o tipo
    // do global declarado em top-level via `const x = JSON.parse(...)`
    // cai no fallback I64 e a leitura subsequente perde o tipo Handle.
    if let Some(method) = qualified.strip_prefix("JSON.") {
        let target = format!("json.{method}");
        if let Some((_, member)) = crate::abi::lookup(&target) {
            return Some(ValTy::from_abi(member.returns));
        }
    }
    if let Some(method) = qualified.strip_prefix("Date.") {
        let target = match method {
            "now" => "date.now_ms",
            "parse" => "date.from_iso",
            _ => "",
        };
        if !target.is_empty() {
            if let Some((_, member)) = crate::abi::lookup(target) {
                return Some(ValTy::from_abi(member.returns));
            }
        }
    }
    None
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

/// Coleta nomes de user fns cujo endereço é potencialmente tomado: idents
/// usados em qualquer posição que não seja callee de `CallExpr` nem `obj`/
/// `prop` de `MemberExpr`. Conservador: pode marcar fns que jamais cruzam
/// fronteira FFI, mas o custo é só perder TCO nessas (usabilidade > pure
/// optimization). Sem isso, `thread.spawn(f, ...)` segfaulta (#206).
fn collect_address_taken_fns(
    fn_decls: &[&FunctionDecl],
    program: &Program,
    synthetic_fns: &[FunctionDecl],
) -> HashSet<String> {
    let known: HashSet<String> = fn_decls.iter().map(|f| f.name.clone()).collect();
    let mut taken: HashSet<String> = HashSet::new();

    for item in &program.items {
        match item {
            Item::Function(f) => {
                for stmt in &f.body {
                    let Statement::Raw(raw) = stmt;
                    if let Some(s) = raw.stmt.as_ref() {
                        scan_stmt(s, &known, &mut taken);
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(s) = raw.stmt.as_ref() {
                    scan_stmt(s, &known, &mut taken);
                }
            }
            _ => {}
        }
    }
    for f in synthetic_fns {
        for stmt in &f.body {
            let Statement::Raw(raw) = stmt;
            if let Some(s) = raw.stmt.as_ref() {
                scan_stmt(s, &known, &mut taken);
            }
        }
    }
    taken
}

fn scan_stmt(stmt: &Stmt, known: &HashSet<String>, taken: &mut HashSet<String>) {
    use swc_ecma_ast::*;
    match stmt {
        Stmt::Block(b) => b.stmts.iter().for_each(|s| scan_stmt(s, known, taken)),
        Stmt::Expr(e) => scan_expr(&e.expr, known, taken),
        Stmt::Return(r) => {
            if let Some(arg) = &r.arg {
                scan_expr(arg, known, taken);
            }
        }
        Stmt::If(i) => {
            scan_expr(&i.test, known, taken);
            scan_stmt(&i.cons, known, taken);
            if let Some(alt) = &i.alt {
                scan_stmt(alt, known, taken);
            }
        }
        Stmt::While(w) => {
            scan_expr(&w.test, known, taken);
            scan_stmt(&w.body, known, taken);
        }
        Stmt::DoWhile(w) => {
            scan_expr(&w.test, known, taken);
            scan_stmt(&w.body, known, taken);
        }
        Stmt::For(f) => {
            if let Some(init) = &f.init {
                match init {
                    VarDeclOrExpr::VarDecl(v) => {
                        for d in &v.decls {
                            if let Some(init) = &d.init {
                                scan_expr(init, known, taken);
                            }
                        }
                    }
                    VarDeclOrExpr::Expr(e) => scan_expr(e, known, taken),
                }
            }
            if let Some(t) = &f.test {
                scan_expr(t, known, taken);
            }
            if let Some(u) = &f.update {
                scan_expr(u, known, taken);
            }
            scan_stmt(&f.body, known, taken);
        }
        Stmt::ForIn(f) => {
            scan_expr(&f.right, known, taken);
            scan_stmt(&f.body, known, taken);
        }
        Stmt::ForOf(f) => {
            scan_expr(&f.right, known, taken);
            scan_stmt(&f.body, known, taken);
        }
        Stmt::Decl(Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(init) = &d.init {
                    scan_expr(init, known, taken);
                }
            }
        }
        Stmt::Try(t) => {
            for s in &t.block.stmts {
                scan_stmt(s, known, taken);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    scan_stmt(s, known, taken);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    scan_stmt(s, known, taken);
                }
            }
        }
        Stmt::Switch(sw) => {
            scan_expr(&sw.discriminant, known, taken);
            for case in &sw.cases {
                if let Some(t) = &case.test {
                    scan_expr(t, known, taken);
                }
                for s in &case.cons {
                    scan_stmt(s, known, taken);
                }
            }
        }
        Stmt::Throw(t) => scan_expr(&t.arg, known, taken),
        Stmt::Labeled(l) => scan_stmt(&l.body, known, taken),
        _ => {}
    }
}

fn scan_expr(expr: &Expr, known: &HashSet<String>, taken: &mut HashSet<String>) {
    use swc_ecma_ast::*;
    match expr {
        Expr::Ident(id) => {
            let name = id.sym.as_ref();
            if known.contains(name) {
                taken.insert(name.to_string());
            }
        }
        Expr::Call(c) => {
            // O callee NAO marca address-taken (chamada normal).
            // Mas precisamos descer no callee se for member.fn(...) etc.
            match &c.callee {
                Callee::Expr(e) => match e.as_ref() {
                    Expr::Ident(_) => { /* chamada direta — não marca */ }
                    Expr::Member(m) => {
                        scan_expr(&m.obj, known, taken);
                    }
                    other => scan_expr(other, known, taken),
                },
                _ => {}
            }
            for arg in &c.args {
                scan_expr(&arg.expr, known, taken);
            }
        }
        Expr::New(n) => {
            scan_expr(&n.callee, known, taken);
            if let Some(args) = &n.args {
                for a in args {
                    scan_expr(&a.expr, known, taken);
                }
            }
        }
        Expr::Bin(b) => {
            scan_expr(&b.left, known, taken);
            scan_expr(&b.right, known, taken);
        }
        Expr::Unary(u) => scan_expr(&u.arg, known, taken),
        Expr::Update(u) => scan_expr(&u.arg, known, taken),
        Expr::Assign(a) => {
            scan_expr(&a.right, known, taken);
        }
        Expr::Cond(c) => {
            scan_expr(&c.test, known, taken);
            scan_expr(&c.cons, known, taken);
            scan_expr(&c.alt, known, taken);
        }
        Expr::Member(m) => {
            scan_expr(&m.obj, known, taken);
        }
        Expr::Paren(p) => scan_expr(&p.expr, known, taken),
        Expr::TsAs(a) => scan_expr(&a.expr, known, taken),
        Expr::TsConstAssertion(a) => scan_expr(&a.expr, known, taken),
        Expr::TsTypeAssertion(a) => scan_expr(&a.expr, known, taken),
        Expr::TsSatisfies(a) => scan_expr(&a.expr, known, taken),
        Expr::TsNonNull(n) => scan_expr(&n.expr, known, taken),
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                scan_expr(&el.expr, known, taken);
            }
        }
        Expr::Object(o) => {
            for p in &o.props {
                if let PropOrSpread::Prop(prop) = p {
                    if let Prop::KeyValue(kv) = prop.as_ref() {
                        scan_expr(&kv.value, known, taken);
                    }
                }
            }
        }
        Expr::Seq(s) => {
            for e in &s.exprs {
                scan_expr(e, known, taken);
            }
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                scan_expr(e, known, taken);
            }
        }
        Expr::Await(a) => scan_expr(&a.arg, known, taken),
        Expr::Yield(y) => {
            if let Some(arg) = &y.arg {
                scan_expr(arg, known, taken);
            }
        }
        _ => {}
    }
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
    address_taken: bool,
) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();
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

    module
        .define_function(info.id, &mut ctx)
        .with_context(|| format!("failed to define function `{}`", fn_decl.name))?;

    Ok(warnings)
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

    if crate::codegen::ir_dump_enabled() {
        let file = crate::codegen::ir_source_file();
        let loc = if file.is_empty() {
            "top-level".to_string()
        } else {
            format!("{} top-level", file)
        };
        eprintln!("--- __RTS_MAIN [{}] IR ---\n{}", loc, runtime_ctx.func.display());
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
