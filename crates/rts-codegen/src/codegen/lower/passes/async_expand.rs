//! Passes que reescrevem `async function` e `await x`.
//!
//! - `expand_async_functions`: `async function f(args) { body }` vira
//!   `function __async_inner_f(args) { body }` + um wrapper `function f(args)`
//!   que aloca um Vec<i64> com os args e chama `promise.create(__async_inner_f, __args)`.
//!   Concentra spawn+state em `promise.create` (Rust); nao precisa mais
//!   de watcher sintetico.
//! - `expand_await_exprs`: `await x` vira `promise.wait(x)`. Sem isso,
//!   `await p` retornava o handle da Promise em vez do valor resolvido
//!   (issue #414, F3 da epic #411).

use crate::parser::ast::{ClassMember, FunctionDecl, Item, Program, RawStmt, Statement};
use crate::parser::span::Span;

/// Reescreve `Expr::Await(x)` em `promise.wait(x)` em todo o programa
/// (issue #414, F3 da epic #411).
///
/// Antes desta passagem, `await x` era stripado silenciosamente em
/// `expressions/mod.rs:54` (`Expr::Await(a) => lower_expr(ctx, &a.arg)`),
/// fazendo o codegen tratar `await p` como `p` — resultando em retornar
/// o handle da Promise em vez do valor resolvido.
///
/// Apos este pass, `await x` vira `promise.wait(x)` que bloqueia ate
/// settle e retorna o valor (i64). Funciona com Promises criadas por
/// `async function f()` (F2 #413) ou explicitas via `promise.new_*`.
///
/// Visita todos os bodies de fn user, top-level statements, e class
/// method bodies. Recursivo em sub-expressoes.
pub(crate) fn expand_await_exprs(program: &mut Program) {
    use swc_ecma_ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident, IdentName, MemberExpr, MemberProp, Stmt};

    fn make_promise_wait(arg: Expr) -> Expr {
        Expr::Call(CallExpr {
            span: Default::default(),
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::Ident(Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: "promise".into(),
                    optional: false,
                })),
                prop: MemberProp::Ident(IdentName {
                    span: Default::default(),
                    sym: "wait".into(),
                }),
            }))),
            args: vec![ExprOrSpread { spread: None, expr: Box::new(arg) }],
            type_args: None,
        })
    }

    fn visit_expr(expr: &mut Expr) {
        // Detecta await — substitui in-place.
        if let Expr::Await(a) = expr {
            // Recurse no argumento primeiro (await pode estar aninhado).
            visit_expr(a.arg.as_mut());
            // Move o arg out e wrappa em promise.wait.
            let arg = std::mem::replace(
                a.arg.as_mut(),
                Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                    span: Default::default(),
                    value: 0.0,
                    raw: None,
                })),
            );
            *expr = make_promise_wait(arg);
            return;
        }
        // Recursao em sub-expressoes.
        match expr {
            Expr::Array(a) => {
                for el in a.elems.iter_mut().flatten() {
                    visit_expr(el.expr.as_mut());
                }
            }
            Expr::Object(o) => {
                for p in &mut o.props {
                    if let swc_ecma_ast::PropOrSpread::Prop(prop) = p {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_mut() {
                            visit_expr(kv.value.as_mut());
                        }
                    }
                }
            }
            Expr::Unary(u) => visit_expr(u.arg.as_mut()),
            Expr::Update(u) => visit_expr(u.arg.as_mut()),
            Expr::Bin(b) => {
                visit_expr(b.left.as_mut());
                visit_expr(b.right.as_mut());
            }
            Expr::Assign(a) => visit_expr(a.right.as_mut()),
            Expr::Cond(c) => {
                visit_expr(c.test.as_mut());
                visit_expr(c.cons.as_mut());
                visit_expr(c.alt.as_mut());
            }
            Expr::Call(c) => {
                if let Callee::Expr(callee) = &mut c.callee {
                    visit_expr(callee.as_mut());
                }
                for arg in &mut c.args {
                    visit_expr(arg.expr.as_mut());
                }
            }
            Expr::New(n) => {
                visit_expr(n.callee.as_mut());
                if let Some(args) = n.args.as_mut() {
                    for arg in args.iter_mut() {
                        visit_expr(arg.expr.as_mut());
                    }
                }
            }
            Expr::Member(m) => {
                visit_expr(m.obj.as_mut());
            }
            Expr::OptChain(o) => {
                use swc_ecma_ast::OptChainBase;
                match o.base.as_mut() {
                    OptChainBase::Call(c) => {
                        visit_expr(c.callee.as_mut());
                        for arg in &mut c.args {
                            visit_expr(arg.expr.as_mut());
                        }
                    }
                    OptChainBase::Member(m) => visit_expr(m.obj.as_mut()),
                }
            }
            Expr::Paren(p) => visit_expr(p.expr.as_mut()),
            Expr::TsAs(a) => visit_expr(a.expr.as_mut()),
            Expr::TsTypeAssertion(a) => visit_expr(a.expr.as_mut()),
            Expr::TsConstAssertion(a) => visit_expr(a.expr.as_mut()),
            Expr::TsSatisfies(a) => visit_expr(a.expr.as_mut()),
            Expr::TsNonNull(n) => visit_expr(n.expr.as_mut()),
            Expr::Tpl(t) => {
                for e in &mut t.exprs {
                    visit_expr(e.as_mut());
                }
            }
            Expr::Seq(s) => {
                for e in &mut s.exprs {
                    visit_expr(e.as_mut());
                }
            }
            Expr::Yield(y) => {
                if let Some(arg) = y.arg.as_mut() {
                    visit_expr(arg.as_mut());
                }
            }
            // Fn/Arrow body: nao descer (rodara em nova chamada de
            // expand_await_exprs caso o pass seja invocado pra fns
            // sinteticas no futuro). Aqui evitamos reentrancia
            // confusa.
            _ => {}
        }
    }

    fn visit_stmt(stmt: &mut Stmt) {
        match stmt {
            Stmt::Expr(e) => visit_expr(e.expr.as_mut()),
            Stmt::Return(r) => {
                if let Some(e) = r.arg.as_mut() {
                    visit_expr(e.as_mut());
                }
            }
            Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) => {
                for d in &mut vd.decls {
                    if let Some(init) = d.init.as_mut() {
                        visit_expr(init.as_mut());
                    }
                }
            }
            Stmt::If(i) => {
                visit_expr(i.test.as_mut());
                visit_stmt(i.cons.as_mut());
                if let Some(alt) = i.alt.as_mut() {
                    visit_stmt(alt.as_mut());
                }
            }
            Stmt::Block(b) => {
                for s in &mut b.stmts {
                    visit_stmt(s);
                }
            }
            Stmt::While(w) => {
                visit_expr(w.test.as_mut());
                visit_stmt(w.body.as_mut());
            }
            Stmt::DoWhile(d) => {
                visit_expr(d.test.as_mut());
                visit_stmt(d.body.as_mut());
            }
            Stmt::For(f) => {
                if let Some(init) = f.init.as_mut() {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::Expr(e) => visit_expr(e.as_mut()),
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                            for d in &mut vd.decls {
                                if let Some(init) = d.init.as_mut() {
                                    visit_expr(init.as_mut());
                                }
                            }
                        }
                    }
                }
                if let Some(t) = f.test.as_mut() {
                    visit_expr(t.as_mut());
                }
                if let Some(u) = f.update.as_mut() {
                    visit_expr(u.as_mut());
                }
                visit_stmt(f.body.as_mut());
            }
            Stmt::ForOf(f) => {
                visit_expr(f.right.as_mut());
                visit_stmt(f.body.as_mut());
            }
            Stmt::ForIn(f) => {
                visit_expr(f.right.as_mut());
                visit_stmt(f.body.as_mut());
            }
            Stmt::Try(t) => {
                for s in &mut t.block.stmts {
                    visit_stmt(s);
                }
                if let Some(h) = t.handler.as_mut() {
                    for s in &mut h.body.stmts {
                        visit_stmt(s);
                    }
                }
                if let Some(f) = t.finalizer.as_mut() {
                    for s in &mut f.stmts {
                        visit_stmt(s);
                    }
                }
            }
            Stmt::Throw(t) => visit_expr(t.arg.as_mut()),
            Stmt::Switch(sw) => {
                visit_expr(sw.discriminant.as_mut());
                for case in &mut sw.cases {
                    if let Some(t) = case.test.as_mut() {
                        visit_expr(t.as_mut());
                    }
                    for s in &mut case.cons {
                        visit_stmt(s);
                    }
                }
            }
            Stmt::Labeled(l) => visit_stmt(l.body.as_mut()),
            _ => {}
        }
    }

    fn visit_body(body: &mut Vec<Statement>) {
        for stmt in body.iter_mut() {
            let Statement::Raw(raw) = stmt;
            if let Some(s) = raw.stmt.as_mut() {
                visit_stmt(s);
            }
        }
    }

    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => visit_body(&mut f.body),
            Item::Class(c) => {
                for member in &mut c.members {
                    match member {
                        ClassMember::Constructor(ctor) => visit_body(&mut ctor.body),
                        ClassMember::Method(m) => visit_body(&mut m.body),
                        ClassMember::Property(_) => {}
                    }
                }
            }
            Item::Statement(stmt) => {
                let Statement::Raw(raw) = stmt;
                if let Some(s) = raw.stmt.as_mut() {
                    visit_stmt(s);
                }
            }
            _ => {}
        }
    }
}

/// Reescreve `async function f(args) { body }` para o desugar
/// Promise-centric (#359 followup, design @drysius):
///
/// ```text
/// async function f(a, b) { body }
///
/// =>
///
/// function __async_inner_f(a, b): i64 { body }
/// function f(a, b): i64 {
///     const __args = [a, b];
///     return promise.create(__async_inner_f, __args);
/// }
/// ```
///
/// Concentra spawn+state em `promise.create` (Rust). Nao precisa mais
/// de watcher sintetico: `promise.create` faz invoke + checa error slot
/// + settle automaticamente.
pub(crate) fn expand_async_functions(program: &mut Program) {
    use swc_ecma_ast::{
        BlockStmt, CallExpr, Callee, Expr, ExprOrSpread, ExprStmt, Ident, IdentName,
        MemberExpr, MemberProp, ReturnStmt, Stmt, VarDecl, VarDeclKind, VarDeclarator,
    };

    fn ident(name: &str) -> Ident {
        Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: name.into(),
            optional: false,
        }
    }
    fn ident_expr(name: &str) -> Expr { Expr::Ident(ident(name)) }
    fn ns_call(ns: &str, method: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(CallExpr {
            span: Default::default(),
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: Default::default(),
                obj: Box::new(ident_expr(ns)),
                prop: MemberProp::Ident(IdentName {
                    span: Default::default(),
                    sym: method.into(),
                }),
            }))),
            args: args
                .into_iter()
                .map(|e| ExprOrSpread { spread: None, expr: Box::new(e) })
                .collect(),
            type_args: None,
        })
    }
    fn const_decl(name: &str, init: Expr) -> Stmt {
        Stmt::Decl(swc_ecma_ast::Decl::Var(Box::new(VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: Default::default(),
                name: swc_ecma_ast::Pat::Ident(swc_ecma_ast::BindingIdent {
                    id: ident(name),
                    type_ann: None,
                }),
                init: Some(Box::new(init)),
                definite: false,
            }],
        })))
    }
    fn expr_stmt(e: Expr) -> Stmt {
        Stmt::Expr(ExprStmt { span: Default::default(), expr: Box::new(e) })
    }
    fn return_stmt(e: Option<Expr>) -> Stmt {
        Stmt::Return(ReturnStmt {
            span: Default::default(),
            arg: e.map(Box::new),
        })
    }
    fn raw_stmt(stmt: Stmt) -> Statement {
        Statement::Raw(RawStmt::new("<async-rewrite>".to_string(), Span::default()).with_stmt(stmt))
    }
    let _ = BlockStmt::default;
    let mut new_fns: Vec<Item> = Vec::new();
    for item in program.items.iter_mut() {
        if let Item::Function(f) = item {
            if !f.is_async {
                continue;
            }
            let param_names: Vec<String> = f.parameters.iter().map(|p| p.name.clone()).collect();
            let inner_name = format!("__async_inner_{}", f.name);

            // 1. Inner fn = body original (mantem parametros + body).
            new_fns.push(Item::Function(FunctionDecl {
                name: inner_name.clone(),
                parameters: f.parameters.clone(),
                return_type: Some("i64".to_string()),
                body: f.body.clone(),
                span: f.span,
                is_async: false,
            }));

            // 2. Substitui o body de `f` por:
            //   const __args = collections.vec_new();
            //   collections.vec_push(__args, p0);
            //   collections.vec_push(__args, p1);
            //   ...
            //   return promise.create(__async_inner_f, __args);
            let mut new_body: Vec<Statement> = Vec::new();
            new_body.push(raw_stmt(const_decl(
                "__args",
                ns_call("collections", "vec_new", vec![]),
            )));
            for pname in &param_names {
                new_body.push(raw_stmt(expr_stmt(ns_call(
                    "collections",
                    "vec_push",
                    vec![ident_expr("__args"), ident_expr(pname)],
                ))));
            }
            new_body.push(raw_stmt(return_stmt(Some(ns_call(
                "promise",
                "create",
                vec![ident_expr(&inner_name), ident_expr("__args")],
            )))));

            f.body = new_body;
            f.is_async = false;
            f.return_type = Some("i64".to_string());
        }
    }
    program.items.extend(new_fns);
}
