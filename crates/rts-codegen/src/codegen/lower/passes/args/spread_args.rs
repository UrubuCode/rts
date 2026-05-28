//! Pass que expande spread literal em callsites.
//!
//! `fn(...[1,2,3])` vira `fn(1, 2, 3)` em compile-time. Tambem resolve
//! `fn(...x)` quando `x` eh ident referenciando const top-level
//! inicializado com array literal estatico.

use swc_ecma_ast::{Callee, Expr, Lit, Pat, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, Statement};

pub(crate) fn expand_spread_args(program: &mut Program) {
    // Coleta bindings `const X = [...]` em top-level. Map name -> elements.
    // Replicamos para escopo de fn quando possivel (mantemos apenas scope
    // top-level por enquanto — bindings locais dentro de fn nao sao
    // resolvidos aqui, mas podem ser estendidos).
    let mut const_arrays: std::collections::HashMap<String, swc_ecma_ast::ArrayLit> =
        std::collections::HashMap::new();
    for item in &program.items {
        if let Item::Statement(Statement::Raw(raw)) = item {
            if let Some(Stmt::Decl(swc_ecma_ast::Decl::Var(vd))) = raw.stmt.as_ref() {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Const) {
                    for d in &vd.decls {
                        if let (Pat::Ident(id), Some(init)) = (&d.name, d.init.as_deref()) {
                            if let Expr::Array(arr) = init {
                                // Apenas arrays sem spread interno (1 nivel raso)
                                // pra evitar reentrancy complexa.
                                let safe = arr
                                    .elems
                                    .iter()
                                    .all(|e| e.as_ref().map(|x| x.spread.is_none()).unwrap_or(true));
                                if safe {
                                    const_arrays
                                        .insert(id.id.sym.to_string(), arr.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                // (cross-runtime) Params da fn fazem SHADOW de consts top-level
                // homonimas. `const arr=[...]; function f(arr){ g(...arr) }` —
                // o `...arr` refere ao PARAM, nao a const. Sem remover, o pass
                // expandia com os elementos da const errada.
                let mut scoped = const_arrays.clone();
                for p in &f.parameters {
                    scoped.remove(&p.name);
                }
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        spread_in_stmt(stmt, &scoped);
                    }
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            let mut scoped = const_arrays.clone();
                            for p in &ctor.parameters {
                                scoped.remove(&p.name);
                            }
                            for s in ctor.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    spread_in_stmt(stmt, &scoped);
                                }
                            }
                        }
                        ClassMember::Method(method) => {
                            let mut scoped = const_arrays.clone();
                            for p in &method.parameters {
                                scoped.remove(&p.name);
                            }
                            for s in method.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    spread_in_stmt(stmt, &scoped);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    spread_in_stmt(stmt, &const_arrays);
                }
            }
            _ => {}
        }
    }
}

fn spread_in_stmt(
    stmt: &mut Stmt,
    consts: &std::collections::HashMap<String, swc_ecma_ast::ArrayLit>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => spread_in_expr(&mut e.expr, consts),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                spread_in_expr(a, consts);
            }
        }
        If(i) => {
            spread_in_expr(&mut i.test, consts);
            spread_in_stmt(&mut i.cons, consts);
            if let Some(alt) = i.alt.as_deref_mut() {
                spread_in_stmt(alt, consts);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                spread_in_stmt(s, consts);
            }
        }
        While(w) => {
            spread_in_expr(&mut w.test, consts);
            spread_in_stmt(&mut w.body, consts);
        }
        DoWhile(w) => {
            spread_in_expr(&mut w.test, consts);
            spread_in_stmt(&mut w.body, consts);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            spread_in_expr(e, consts);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                spread_in_expr(t, consts);
            }
            if let Some(u) = f.update.as_deref_mut() {
                spread_in_expr(u, consts);
            }
            spread_in_stmt(&mut f.body, consts);
        }
        ForOf(f) => {
            spread_in_expr(&mut f.right, consts);
            spread_in_stmt(&mut f.body, consts);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    spread_in_expr(e, consts);
                }
            }
        }
        _ => {}
    }
}

fn spread_in_expr(
    expr: &mut Expr,
    consts: &std::collections::HashMap<String, swc_ecma_ast::ArrayLit>,
) {
    match expr {
        Expr::Call(call) => {
            // Recurse primeiro nos args.
            for a in call.args.iter_mut() {
                spread_in_expr(&mut a.expr, consts);
            }
            if let Callee::Expr(e) = &mut call.callee {
                spread_in_expr(e, consts);
            }
            expand_spread_in_args(&mut call.args, consts);
        }
        Expr::New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    spread_in_expr(&mut a.expr, consts);
                }
                expand_spread_in_args(args, consts);
            }
        }
        Expr::Member(m) => spread_in_expr(&mut m.obj, consts),
        Expr::Bin(b) => {
            spread_in_expr(&mut b.left, consts);
            spread_in_expr(&mut b.right, consts);
        }
        Expr::Unary(u) => spread_in_expr(&mut u.arg, consts),
        Expr::Update(u) => spread_in_expr(&mut u.arg, consts),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                spread_in_expr(&mut m.obj, consts);
            }
            spread_in_expr(&mut a.right, consts);
        }
        Expr::Cond(c) => {
            spread_in_expr(&mut c.test, consts);
            spread_in_expr(&mut c.cons, consts);
            spread_in_expr(&mut c.alt, consts);
        }
        Expr::Paren(p) => spread_in_expr(&mut p.expr, consts),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                spread_in_expr(e, consts);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                spread_in_expr(&mut el.expr, consts);
            }
        }
        _ => {}
    }
}

/// Substitui args com spread de array literal pelos elementos do array.
/// `f(a, ...[b, c], d)` → `f(a, b, c, d)`.
/// Tambem resolve `f(...x)` quando `x` eh ident referenciando const top-level
/// inicializado com array literal estatico.
fn expand_spread_in_args(
    args: &mut Vec<swc_ecma_ast::ExprOrSpread>,
    consts: &std::collections::HashMap<String, swc_ecma_ast::ArrayLit>,
) {
    if !args.iter().any(|a| a.spread.is_some()) {
        return;
    }
    let original = std::mem::take(args);
    for arg in original {
        if arg.spread.is_some() {
            // Spread de literal array: expande inline.
            let arr_opt: Option<&swc_ecma_ast::ArrayLit> = match arg.expr.as_ref() {
                Expr::Array(arr) => Some(arr),
                Expr::Ident(id) => consts.get(id.sym.as_ref()),
                _ => None,
            };
            if let Some(arr) = arr_opt {
                for elem in &arr.elems {
                    if let Some(el) = elem {
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
            // Spread de não-literal/nao-resolvivel: deixa como esta.
            // Codegen vai rejeitar com erro claro em runtime.
            args.push(arg);
        } else {
            args.push(arg);
        }
    }
}
