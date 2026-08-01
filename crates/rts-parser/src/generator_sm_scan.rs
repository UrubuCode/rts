//! Free-variable / mutation SCAN over a generator body, before it becomes a
//! state machine.
//!
//! Split out of [`crate::generator_sm`] purely as file layout: it is ~230 lines
//! of exhaustive AST walking that says nothing about the machine itself, and the
//! parent file is far past the layer size ceiling.
//!
//! What it answers, for the `let`s a generator body declares: which ones a
//! nested closure CAPTURES, and which ones anything MUTATES. It matters because
//! the machine keeps its locals in the generator FRAME — a name a closure
//! captures cannot simply become a frame slot, or the closure reads a stale copy.

use swc_ecma_ast::{Callee, Decl, Expr, Stmt};

pub(crate) fn scan_caps_muts_stmt(
    s: &Stmt,
    in_closure: bool,
    lets: &std::collections::HashSet<String>,
    cap: &mut std::collections::HashSet<String>,
    mutd: &mut std::collections::HashSet<String>,
) {
    match s {
        Stmt::Expr(e) => scan_caps_muts_expr(&e.expr, in_closure, lets, cap, mutd),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                scan_caps_muts_expr(a, in_closure, lets, cap, mutd);
            }
        }
        Stmt::Decl(Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
                }
            }
        }
        Stmt::If(i) => {
            scan_caps_muts_expr(&i.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&i.cons, in_closure, lets, cap, mutd);
            if let Some(a) = i.alt.as_deref() {
                scan_caps_muts_stmt(a, in_closure, lets, cap, mutd);
            }
        }
        Stmt::Block(b) => b
            .stmts
            .iter()
            .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd)),
        Stmt::While(w) => {
            scan_caps_muts_expr(&w.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&w.body, in_closure, lets, cap, mutd);
        }
        Stmt::DoWhile(w) => {
            scan_caps_muts_expr(&w.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&w.body, in_closure, lets, cap, mutd);
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_deref() {
                        scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
                    }
                }
            } else if let Some(swc_ecma_ast::VarDeclOrExpr::Expr(e)) = f.init.as_ref() {
                scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
            }
            if let Some(t) = f.test.as_deref() {
                scan_caps_muts_expr(t, in_closure, lets, cap, mutd);
            }
            if let Some(u) = f.update.as_deref() {
                scan_caps_muts_expr(u, in_closure, lets, cap, mutd);
            }
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::ForOf(f) => {
            scan_caps_muts_expr(&f.right, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::ForIn(f) => {
            scan_caps_muts_expr(&f.right, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::Try(t) => {
            t.block
                .stmts
                .iter()
                .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            if let Some(h) = &t.handler {
                h.body
                    .stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
            if let Some(fin) = &t.finalizer {
                fin.stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
        }
        Stmt::Throw(t) => scan_caps_muts_expr(&t.arg, in_closure, lets, cap, mutd),
        Stmt::Switch(sw) => {
            scan_caps_muts_expr(&sw.discriminant, in_closure, lets, cap, mutd);
            for c in &sw.cases {
                if let Some(t) = &c.test {
                    scan_caps_muts_expr(t, in_closure, lets, cap, mutd);
                }
                c.cons
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
        }
        _ => {}
    }
}

fn scan_caps_muts_expr(
    e: &Expr,
    in_closure: bool,
    lets: &std::collections::HashSet<String>,
    cap: &mut std::collections::HashSet<String>,
    mutd: &mut std::collections::HashSet<String>,
) {
    match e {
        Expr::Ident(id) => {
            if in_closure {
                let n = id.sym.to_string();
                if lets.contains(&n) {
                    cap.insert(n);
                }
            }
        }
        Expr::Update(u) => {
            if let Expr::Ident(id) = u.arg.as_ref() {
                let n = id.sym.to_string();
                if lets.contains(&n) {
                    mutd.insert(n);
                }
            }
            scan_caps_muts_expr(&u.arg, in_closure, lets, cap, mutd);
        }
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Ident(id),
            ) = &a.left
            {
                let n = id.id.sym.to_string();
                if lets.contains(&n) {
                    mutd.insert(n);
                }
            }
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &a.left
            {
                scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            }
            scan_caps_muts_expr(&a.right, in_closure, lets, cap, mutd);
        }
        Expr::Arrow(arrow) => match arrow.body.as_ref() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b
                .stmts
                .iter()
                .for_each(|s| scan_caps_muts_stmt(s, true, lets, cap, mutd)),
            swc_ecma_ast::BlockStmtOrExpr::Expr(ex) => {
                scan_caps_muts_expr(ex, true, lets, cap, mutd)
            }
        },
        Expr::Fn(fnx) => {
            if let Some(b) = &fnx.function.body {
                b.stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, true, lets, cap, mutd));
            }
        }
        Expr::Call(c) => {
            if let Callee::Expr(ce) = &c.callee {
                scan_caps_muts_expr(ce, in_closure, lets, cap, mutd);
            }
            for a in &c.args {
                scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::New(n) => {
            scan_caps_muts_expr(&n.callee, in_closure, lets, cap, mutd);
            if let Some(args) = &n.args {
                for a in args {
                    scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
                }
            }
        }
        Expr::Member(m) => {
            scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                scan_caps_muts_expr(&c.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::Bin(b) => {
            scan_caps_muts_expr(&b.left, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&b.right, in_closure, lets, cap, mutd);
        }
        Expr::Unary(u) => scan_caps_muts_expr(&u.arg, in_closure, lets, cap, mutd),
        Expr::Paren(p) => scan_caps_muts_expr(&p.expr, in_closure, lets, cap, mutd),
        Expr::Await(a) => scan_caps_muts_expr(&a.arg, in_closure, lets, cap, mutd),
        Expr::Yield(y) => {
            if let Some(arg) = &y.arg {
                scan_caps_muts_expr(arg, in_closure, lets, cap, mutd);
            }
        }
        Expr::Cond(c) => {
            scan_caps_muts_expr(&c.test, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&c.cons, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&c.alt, in_closure, lets, cap, mutd);
        }
        Expr::Seq(s) => s
            .exprs
            .iter()
            .for_each(|x| scan_caps_muts_expr(x, in_closure, lets, cap, mutd)),
        Expr::Tpl(t) => t
            .exprs
            .iter()
            .for_each(|x| scan_caps_muts_expr(x, in_closure, lets, cap, mutd)),
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                scan_caps_muts_expr(&el.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::Object(o) => {
            for p in &o.props {
                if let swc_ecma_ast::PropOrSpread::Prop(prop) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() {
                        scan_caps_muts_expr(&kv.value, in_closure, lets, cap, mutd);
                    }
                }
            }
        }
        Expr::TsAs(a) => scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd),
        Expr::TsNonNull(a) => scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd),
        Expr::OptChain(o) => {
            if let swc_ecma_ast::OptChainBase::Member(m) = o.base.as_ref() {
                scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            } else if let swc_ecma_ast::OptChainBase::Call(c) = o.base.as_ref() {
                scan_caps_muts_expr(&c.callee, in_closure, lets, cap, mutd);
                for a in &c.args {
                    scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
                }
            }
        }
        _ => {}
    }
}
