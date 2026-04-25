//! Desugar de generator functions (`function*`) para arrays buffered.
//!
//! Limitacao MVP: lazy evaluation real exige coroutines/state machine.
//! Aqui transformamos o body para enfileirar todos os valores em um
//! array `__gen_buf` e retornar esse array no final. Funciona para o
//! padrao mais comum (`for...of gen()`), mas executa o body inteiro
//! de uma vez, sem suspensao real.
//!
//! Transformacoes:
//! - Prepend `const __gen_buf = []`
//! - Cada `yield expr` (em ExprStmt) vira `__gen_buf.push(expr)`
//! - Cada `yield expr` em outros contextos vira chamada inline
//!   `(__gen_buf.push(expr), 0)` — mas como nao temos comma op,
//!   usamos `__gen_buf.push(expr)` direto e descartamos o valor de
//!   yield (que nunca eh consumido sem .next() real).
//! - Append `return __gen_buf` ao final.

use swc_ecma_ast::{
    BlockStmt, CallExpr, Callee, Decl, Expr, ExprOrSpread, ExprStmt, Ident, MemberExpr, MemberProp,
    Pat, Stmt, VarDecl, VarDeclKind, VarDeclarator,
};

const BUF_NAME: &str = "__gen_buf";

pub fn desugar_generator_body(body: &BlockStmt) -> BlockStmt {
    let span = body.span;
    let mut stmts: Vec<Stmt> = Vec::with_capacity(body.stmts.len() + 2);

    // const __gen_buf = [];
    stmts.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span,
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span,
            name: Pat::Ident(Ident::new(BUF_NAME.into(), span, Default::default()).into()),
            init: Some(Box::new(Expr::Array(swc_ecma_ast::ArrayLit {
                span,
                elems: Vec::new(),
            }))),
            definite: false,
        }],
    }))));

    for stmt in &body.stmts {
        stmts.push(transform_stmt(stmt.clone()));
    }

    // return __gen_buf;
    stmts.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
        span,
        arg: Some(Box::new(Expr::Ident(Ident::new(
            BUF_NAME.into(),
            span,
            Default::default(),
        )))),
    }));

    BlockStmt {
        span,
        ctxt: body.ctxt,
        stmts,
    }
}

fn transform_stmt(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::Expr(es) => {
            // Caso comum: `yield expr;` -> __gen_buf.push(expr);
            if let Expr::Yield(y) = es.expr.as_ref() {
                return push_call_stmt(y.arg.as_deref().cloned(), es.span);
            }
            Stmt::Expr(ExprStmt {
                span: es.span,
                expr: Box::new(transform_expr(*es.expr)),
            })
        }
        Stmt::Block(b) => Stmt::Block(BlockStmt {
            span: b.span,
            ctxt: b.ctxt,
            stmts: b.stmts.into_iter().map(transform_stmt).collect(),
        }),
        Stmt::If(i) => Stmt::If(swc_ecma_ast::IfStmt {
            span: i.span,
            test: Box::new(transform_expr(*i.test)),
            cons: Box::new(transform_stmt(*i.cons)),
            alt: i.alt.map(|a| Box::new(transform_stmt(*a))),
        }),
        Stmt::While(w) => Stmt::While(swc_ecma_ast::WhileStmt {
            span: w.span,
            test: Box::new(transform_expr(*w.test)),
            body: Box::new(transform_stmt(*w.body)),
        }),
        Stmt::DoWhile(d) => Stmt::DoWhile(swc_ecma_ast::DoWhileStmt {
            span: d.span,
            test: Box::new(transform_expr(*d.test)),
            body: Box::new(transform_stmt(*d.body)),
        }),
        Stmt::For(f) => Stmt::For(swc_ecma_ast::ForStmt {
            span: f.span,
            init: f.init,
            test: f.test.map(|t| Box::new(transform_expr(*t))),
            update: f.update.map(|u| Box::new(transform_expr(*u))),
            body: Box::new(transform_stmt(*f.body)),
        }),
        Stmt::ForOf(fo) => Stmt::ForOf(swc_ecma_ast::ForOfStmt {
            span: fo.span,
            is_await: fo.is_await,
            left: fo.left,
            right: Box::new(transform_expr(*fo.right)),
            body: Box::new(transform_stmt(*fo.body)),
        }),
        Stmt::ForIn(fi) => Stmt::ForIn(swc_ecma_ast::ForInStmt {
            span: fi.span,
            left: fi.left,
            right: Box::new(transform_expr(*fi.right)),
            body: Box::new(transform_stmt(*fi.body)),
        }),
        other => other,
    }
}

fn transform_expr(expr: Expr) -> Expr {
    if let Expr::Yield(y) = &expr {
        // `yield expr` em posicao de expressao (raro): substituir por
        // `__gen_buf.push(expr)` (chamada que tambem retorna len novo).
        return push_call_expr(y.arg.as_deref().cloned(), y.span);
    }
    expr
}

fn push_call_stmt(arg: Option<Expr>, span: swc_common::Span) -> Stmt {
    Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(push_call_expr(arg, span)),
    })
}

fn push_call_expr(arg: Option<Expr>, span: swc_common::Span) -> Expr {
    let value = arg.unwrap_or(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
        span,
        value: 0.0,
        raw: None,
    })));
    Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
            span,
            obj: Box::new(Expr::Ident(Ident::new(
                BUF_NAME.into(),
                span,
                Default::default(),
            ))),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName::new("push".into(), span)),
        }))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: Box::new(value),
        }],
        type_args: None,
    })
}
