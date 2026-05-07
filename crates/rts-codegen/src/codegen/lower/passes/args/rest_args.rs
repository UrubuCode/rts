//! Pass que empacota `...rest` no callsite.
//!
//! Para uma fn user com ultimo param marcado `variadic` (sintaxe
//! `...rest`), todos os args do callsite a partir da posicao desse
//! param sao coletados num `Expr::Array` e passados como unico arg
//! na posicao do rest. Codegen do callee ve `rest` como Handle de
//! array normal — pode iterar via `for...of`.

use swc_ecma_ast::{Callee, Expr, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, Statement};

pub(crate) fn expand_rest_args(program: &mut Program) {
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
