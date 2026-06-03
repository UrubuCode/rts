//! Pass que desugara metodos `{ foo() {...} }` em `{ foo: function() {...} }`.
//!
//! Apos esse pass, fn expressions nessas posicoes podem ser hoisted como
//! user fns top-level e despachadas como handles. `this` no body funciona
//! via thread-local slot (PR #451) — o call site `obj.method()` empilha
//! obj antes do invoke.

use swc_ecma_ast::{Expr, Stmt};

use crate::parser::ast::Program;

pub(crate) fn desugar_object_methods(program: &mut Program) {
    use swc_ecma_ast::{Prop, PropOrSpread};

    fn visit_expr(expr: &mut Expr) {
        match expr {
            Expr::Object(obj) => {
                for prop in obj.props.iter_mut() {
                    if let PropOrSpread::Prop(p) = prop {
                        // Visita sub-exprs em KeyValue antes de tentar desugar.
                        if let Prop::KeyValue(kv) = p.as_mut() {
                            visit_expr(kv.value.as_mut());
                            continue;
                        }
                        // Desugar Method para KeyValue + Fn.
                        if matches!(p.as_ref(), Prop::Method(_)) {
                            let owned = std::mem::replace(
                                p.as_mut(),
                                Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: swc_ecma_ast::PropName::Ident(
                                        swc_ecma_ast::IdentName::new("__placeholder".into(), Default::default()),
                                    ),
                                    value: Box::new(Expr::Invalid(swc_ecma_ast::Invalid {
                                        span: Default::default(),
                                    })),
                                }),
                            );
                            if let Prop::Method(method) = owned {
                                let mut function = method.function;
                                if let Some(body) = function.body.as_mut() {
                                    for stmt in body.stmts.iter_mut() {
                                        visit_stmt(stmt);
                                    }
                                }
                                let fn_expr = swc_ecma_ast::FnExpr {
                                    ident: None,
                                    function,
                                };
                                **p = Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: method.key,
                                    value: Box::new(Expr::Fn(fn_expr)),
                                });
                            }
                            continue;
                        }
                        // Desugar Getter para KeyValue("__get_<name>", fn).
                        if matches!(p.as_ref(), Prop::Getter(_)) {
                            use swc_ecma_ast::PropName;
                            let owned = std::mem::replace(
                                p.as_mut(),
                                Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: PropName::Ident(
                                        swc_ecma_ast::IdentName::new("__placeholder".into(), Default::default()),
                                    ),
                                    value: Box::new(Expr::Invalid(swc_ecma_ast::Invalid {
                                        span: Default::default(),
                                    })),
                                }),
                            );
                            if let Prop::Getter(g) = owned {
                                let key_name: String = match &g.key {
                                    PropName::Ident(id) => id.sym.as_str().to_string(),
                                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                                    PropName::Num(n) => n.value.to_string(),
                                    _ => "__getter".to_string(),
                                };
                                let new_key = format!("__get_{}", key_name);
                                let mut function = swc_ecma_ast::Function {
                                    params: Vec::new(),
                                    decorators: Vec::new(),
                                    span: g.span,
                                    ctxt: Default::default(),
                                    body: g.body,
                                    is_generator: false,
                                    is_async: false,
                                    type_params: None,
                                    return_type: g.type_ann,
                                };
                                if let Some(body) = function.body.as_mut() {
                                    for stmt in body.stmts.iter_mut() {
                                        visit_stmt(stmt);
                                    }
                                }
                                let fn_expr = swc_ecma_ast::FnExpr {
                                    ident: None,
                                    function: Box::new(function),
                                };
                                **p = Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: PropName::Str(swc_ecma_ast::Str {
                                        span: g.span,
                                        value: new_key.into(),
                                        raw: None,
                                    }),
                                    value: Box::new(Expr::Fn(fn_expr)),
                                });
                            }
                            continue;
                        }
                        // Desugar Setter para KeyValue("__set_<name>", fn).
                        if matches!(p.as_ref(), Prop::Setter(_)) {
                            use swc_ecma_ast::PropName;
                            let owned = std::mem::replace(
                                p.as_mut(),
                                Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: PropName::Ident(
                                        swc_ecma_ast::IdentName::new("__placeholder".into(), Default::default()),
                                    ),
                                    value: Box::new(Expr::Invalid(swc_ecma_ast::Invalid {
                                        span: Default::default(),
                                    })),
                                }),
                            );
                            if let Prop::Setter(s) = owned {
                                let key_name: String = match &s.key {
                                    PropName::Ident(id) => id.sym.as_str().to_string(),
                                    PropName::Str(st) => st.value.to_string_lossy().to_string(),
                                    PropName::Num(n) => n.value.to_string(),
                                    _ => "__setter".to_string(),
                                };
                                let new_key = format!("__set_{}", key_name);
                                use swc_common::Spanned;
                                let mut function = swc_ecma_ast::Function {
                                    params: vec![swc_ecma_ast::Param {
                                        span: s.param.span(),
                                        decorators: Vec::new(),
                                        pat: (*s.param).clone(),
                                    }],
                                    decorators: Vec::new(),
                                    span: s.span,
                                    ctxt: Default::default(),
                                    body: s.body,
                                    is_generator: false,
                                    is_async: false,
                                    type_params: None,
                                    return_type: None,
                                };
                                if let Some(body) = function.body.as_mut() {
                                    for stmt in body.stmts.iter_mut() {
                                        visit_stmt(stmt);
                                    }
                                }
                                let fn_expr = swc_ecma_ast::FnExpr {
                                    ident: None,
                                    function: Box::new(function),
                                };
                                **p = Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                                    key: PropName::Str(swc_ecma_ast::Str {
                                        span: s.span,
                                        value: new_key.into(),
                                        raw: None,
                                    }),
                                    value: Box::new(Expr::Fn(fn_expr)),
                                });
                            }
                            continue;
                        }
                    }
                }
            }
            Expr::Array(arr) => {
                for elem in arr.elems.iter_mut().flatten() {
                    visit_expr(elem.expr.as_mut());
                }
            }
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(callee) = &mut c.callee {
                    visit_expr(callee.as_mut());
                }
                for arg in c.args.iter_mut() {
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
            Expr::Member(m) => visit_expr(m.obj.as_mut()),
            Expr::Paren(p) => visit_expr(p.expr.as_mut()),
            Expr::TsAs(a) => visit_expr(a.expr.as_mut()),
            Expr::TsTypeAssertion(a) => visit_expr(a.expr.as_mut()),
            Expr::TsConstAssertion(a) => visit_expr(a.expr.as_mut()),
            Expr::TsSatisfies(a) => visit_expr(a.expr.as_mut()),
            Expr::TsNonNull(n) => visit_expr(n.expr.as_mut()),
            Expr::Await(a) => visit_expr(a.arg.as_mut()),
            Expr::Tpl(t) => {
                for e in t.exprs.iter_mut() {
                    visit_expr(e.as_mut());
                }
            }
            Expr::Unary(u) => visit_expr(u.arg.as_mut()),
            Expr::Update(u) => visit_expr(u.arg.as_mut()),
            Expr::Seq(s) => {
                for e in s.exprs.iter_mut() {
                    visit_expr(e.as_mut());
                }
            }
            Expr::Fn(f) => {
                if let Some(body) = f.function.body.as_mut() {
                    for stmt in body.stmts.iter_mut() {
                        visit_stmt(stmt);
                    }
                }
            }
            Expr::Arrow(arrow) => match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for stmt in b.stmts.iter_mut() {
                        visit_stmt(stmt);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => visit_expr(e.as_mut()),
            },
            _ => {}
        }
    }

    fn visit_stmt(stmt: &mut Stmt) {
        match stmt {
            Stmt::Expr(e) => visit_expr(e.expr.as_mut()),
            Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
                for d in v.decls.iter_mut() {
                    if let Some(init) = &mut d.init {
                        visit_expr(init.as_mut());
                    }
                }
            }
            Stmt::Return(r) => {
                if let Some(arg) = &mut r.arg {
                    visit_expr(arg.as_mut());
                }
            }
            Stmt::If(i) => {
                visit_expr(i.test.as_mut());
                visit_stmt(i.cons.as_mut());
                if let Some(alt) = &mut i.alt {
                    visit_stmt(alt.as_mut());
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    visit_stmt(s);
                }
            }
            Stmt::While(w) => {
                visit_expr(w.test.as_mut());
                visit_stmt(w.body.as_mut());
            }
            Stmt::DoWhile(w) => {
                visit_expr(w.test.as_mut());
                visit_stmt(w.body.as_mut());
            }
            Stmt::For(f) => {
                if let Some(init) = &mut f.init {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                            for d in v.decls.iter_mut() {
                                if let Some(i) = &mut d.init {
                                    visit_expr(i.as_mut());
                                }
                            }
                        }
                        swc_ecma_ast::VarDeclOrExpr::Expr(e) => visit_expr(e.as_mut()),
                    }
                }
                if let Some(t) = &mut f.test {
                    visit_expr(t.as_mut());
                }
                if let Some(u) = &mut f.update {
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
            Stmt::Throw(t) => visit_expr(t.arg.as_mut()),
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    visit_stmt(s);
                }
                if let Some(handler) = &mut t.handler {
                    for s in handler.body.stmts.iter_mut() {
                        visit_stmt(s);
                    }
                }
                if let Some(finalizer) = &mut t.finalizer {
                    for s in finalizer.stmts.iter_mut() {
                        visit_stmt(s);
                    }
                }
            }
            Stmt::Switch(s) => {
                visit_expr(s.discriminant.as_mut());
                for case in s.cases.iter_mut() {
                    if let Some(test) = &mut case.test {
                        visit_expr(test.as_mut());
                    }
                    for stmt in case.cons.iter_mut() {
                        visit_stmt(stmt);
                    }
                }
            }
            _ => {}
        }
    }

    for item in program.items.iter_mut() {
        match item {
            crate::parser::ast::Item::Function(fdecl) => {
                for stmt in fdecl.body.iter_mut() {
                    let crate::parser::ast::Statement::Raw(raw) = stmt;
                    if let Some(s) = raw.stmt.as_mut() {
                        visit_stmt(s);
                    }
                }
            }
            crate::parser::ast::Item::Class(class) => {
                use crate::parser::ast::{ClassMember, Statement};
                for member in class.members.iter_mut() {
                    let body = match member {
                        ClassMember::Method(m) => &mut m.body,
                        ClassMember::Constructor(c) => &mut c.body,
                        _ => continue,
                    };
                    for s in body.iter_mut() {
                        let Statement::Raw(raw) = s;
                        if let Some(stmt) = raw.stmt.as_mut() {
                            visit_stmt(stmt);
                        }
                    }
                }
            }
            crate::parser::ast::Item::Statement(stmt) => {
                let crate::parser::ast::Statement::Raw(raw) = stmt;
                if let Some(s) = raw.stmt.as_mut() {
                    visit_stmt(s);
                }
            }
            _ => {}
        }
    }
}
