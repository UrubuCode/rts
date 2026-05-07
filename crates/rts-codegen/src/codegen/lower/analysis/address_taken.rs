//! Detecta quais user fns tem o endereco potencialmente tomado.
//!
//! Conservador: marca qualquer ident que apareca fora de posicao
//! callee/MemberExpr.obj. Custo de over-marking eh perda de TCO; sem
//! marcar, `thread.spawn(f, ...)` segfauta (#206).

use std::collections::HashSet;

use swc_ecma_ast::{Expr, Stmt};

use crate::parser::ast::{FunctionDecl, Item, Program, Statement};

/// Coleta nomes de user fns cujo endereço é potencialmente tomado: idents
/// usados em qualquer posição que não seja callee de `CallExpr` nem `obj`/
/// `prop` de `MemberExpr`. Conservador: pode marcar fns que jamais cruzam
/// fronteira FFI, mas o custo é só perder TCO nessas (usabilidade > pure
/// optimization). Sem isso, `thread.spawn(f, ...)` segfaulta (#206).
pub(crate) fn collect_address_taken_fns(
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
