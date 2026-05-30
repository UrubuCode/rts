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

thread_local! {
    /// Contador monotonico p/ nomes unicos de temporarios
    /// `__yt_delegate_N` em `const r = yield* gen()` — unicidade global
    /// entre todas as generator fns da compilacao.
    static YT_COUNTER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn next_yt_id() -> u32 {
    YT_COUNTER.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}

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
        stmts.extend(transform_stmt(stmt.clone()));
    }

    // return __RTS_GEN_FINISH(__gen_buf, undefined);
    // O codegen mapeia FINISH(vec, ret) -> registra o ret_value do Vec no
    // side-table do generator e retorna o Vec. `return X` no corpo (tratado
    // em transform_stmt) ja' usa FINISH com o X. Este eh o caso "esgotou sem
    // return explicito" -> ret undefined.
    stmts.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
        span,
        arg: Some(Box::new(gen_finish_call(
            Expr::Ident(Ident::new(BUF_NAME.into(), span, Default::default())),
            None,
            span,
        ))),
    }));

    BlockStmt {
        span,
        ctxt: body.ctxt,
        stmts,
    }
}

/// Variante que sempre devolve UM Stmt (envolvendo em Block quando o
/// transform produz multiplos) — p/ posicoes que exigem statement unico
/// (cons/alt de if, body de loop). Nessas posicoes o Block extra nao
/// quebra escopo de forma observavel (corpos de loop ja' tem escopo).
fn transform_stmt_single(stmt: Stmt) -> Stmt {
    let span = stmt_span(&stmt);
    let mut v = transform_stmt(stmt);
    if v.len() == 1 {
        v.pop().unwrap()
    } else {
        Stmt::Block(BlockStmt { span, ctxt: Default::default(), stmts: v })
    }
}

fn stmt_span(stmt: &Stmt) -> swc_common::Span {
    match stmt {
        Stmt::Expr(s) => s.span,
        Stmt::Block(s) => s.span,
        Stmt::If(s) => s.span,
        Stmt::While(s) => s.span,
        Stmt::DoWhile(s) => s.span,
        Stmt::For(s) => s.span,
        Stmt::ForOf(s) => s.span,
        Stmt::ForIn(s) => s.span,
        Stmt::Return(s) => s.span,
        Stmt::Decl(Decl::Var(s)) => s.span,
        _ => Default::default(),
    }
}

fn transform_stmt(stmt: Stmt) -> Vec<Stmt> {
    match stmt {
        Stmt::Expr(es) => {
            // Caso comum: `yield expr;` -> __gen_buf.push(expr);
            // `yield* expr;` -> for (const __t of expr) __gen_buf.push(__t);
            if let Expr::Yield(y) = es.expr.as_ref() {
                if y.delegate {
                    return vec![delegate_for_of(y.arg.as_deref().cloned(), es.span)];
                }
                return vec![push_call_stmt(y.arg.as_deref().cloned(), es.span)];
            }
            vec![Stmt::Expr(ExprStmt {
                span: es.span,
                expr: Box::new(transform_expr(*es.expr)),
            })]
        }
        Stmt::Block(b) => {
            let mut stmts: Vec<Stmt> = Vec::with_capacity(b.stmts.len());
            for s in b.stmts {
                stmts.extend(transform_stmt(s));
            }
            vec![Stmt::Block(BlockStmt { span: b.span, ctxt: b.ctxt, stmts })]
        }
        Stmt::If(i) => vec![Stmt::If(swc_ecma_ast::IfStmt {
            span: i.span,
            test: Box::new(transform_expr(*i.test)),
            cons: Box::new(transform_stmt_single(*i.cons)),
            alt: i.alt.map(|a| Box::new(transform_stmt_single(*a))),
        })],
        Stmt::While(w) => vec![Stmt::While(swc_ecma_ast::WhileStmt {
            span: w.span,
            test: Box::new(transform_expr(*w.test)),
            body: Box::new(transform_stmt_single(*w.body)),
        })],
        Stmt::DoWhile(d) => vec![Stmt::DoWhile(swc_ecma_ast::DoWhileStmt {
            span: d.span,
            test: Box::new(transform_expr(*d.test)),
            body: Box::new(transform_stmt_single(*d.body)),
        })],
        Stmt::For(f) => vec![Stmt::For(swc_ecma_ast::ForStmt {
            span: f.span,
            init: f.init,
            test: f.test.map(|t| Box::new(transform_expr(*t))),
            update: f.update.map(|u| Box::new(transform_expr(*u))),
            body: Box::new(transform_stmt_single(*f.body)),
        })],
        Stmt::ForOf(fo) => vec![Stmt::ForOf(swc_ecma_ast::ForOfStmt {
            span: fo.span,
            is_await: fo.is_await,
            left: fo.left,
            right: Box::new(transform_expr(*fo.right)),
            body: Box::new(transform_stmt_single(*fo.body)),
        })],
        Stmt::ForIn(fi) => vec![Stmt::ForIn(swc_ecma_ast::ForInStmt {
            span: fi.span,
            left: fi.left,
            right: Box::new(transform_expr(*fi.right)),
            body: Box::new(transform_stmt_single(*fi.body)),
        })],
        // `return X` num generator: retorna o buffer-ate-aqui com X como
        // ret_value (via FINISH). `return;` (sem arg) -> ret undefined.
        Stmt::Return(r) => vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: r.span,
            arg: Some(Box::new(gen_finish_call(
                Expr::Ident(Ident::new(BUF_NAME.into(), r.span, Default::default())),
                r.arg.as_deref().cloned(),
                r.span,
            ))),
        })],
        // (#275/#379) `const r = yield* gen()` / `let r = yield v` — yield em
        // posicao de init de VarDecl. transform_stmt antes deixava cru
        // (catch-all) e o codegen rejeitava ("unsupported expression: yield").
        // Reescreve cada declarator com init yield:
        //   `const r = yield* gen()` ->
        //     const __yt_delegate_N = gen();
        //     for (const __t of __yt_delegate_N) __gen_buf.push(__t);
        //     const r = __RTS_GEN_GET_RET(__yt_delegate_N);
        //   `const r = yield v` -> __gen_buf.push(v); const r = undefined;
        //     (eager model nao alimenta valor de volta no yield — r=undefined).
        Stmt::Decl(Decl::Var(vd))
            if vd.decls.iter().any(|d| {
                matches!(d.init.as_deref(), Some(Expr::Yield(_)))
            }) =>
        {
            let mut out: Vec<Stmt> = Vec::new();
            let span = vd.span;
            let kind = vd.kind;
            for d in vd.decls.iter() {
                match d.init.as_deref() {
                    Some(Expr::Yield(y)) => {
                        if y.delegate {
                            // const __yt_delegate_N = <arg>;
                            let tmp = format!("__yt_delegate_{}", next_yt_id());
                            let iter = y.arg.as_deref().cloned().unwrap_or(Expr::Ident(
                                Ident::new("undefined".into(), span, Default::default()),
                            ));
                            out.push(decl_const(&tmp, Some(iter), VarDeclKind::Const, span));
                            // for (const __t of __yt_delegate_N) __gen_buf.push(__t);
                            out.push(delegate_for_of(
                                Some(Expr::Ident(Ident::new(tmp.clone().into(), span, Default::default()))),
                                span,
                            ));
                            // <kind> r = __RTS_GEN_GET_RET(__yt_delegate_N);
                            let get_ret = gen_get_ret_call(
                                Expr::Ident(Ident::new(tmp.into(), span, Default::default())),
                                span,
                            );
                            out.push(decl_from_pat(d.name.clone(), Some(get_ret), kind, span));
                        } else {
                            // const r = yield v -> push(v); const r = undefined;
                            out.push(push_call_stmt(y.arg.as_deref().cloned(), span));
                            let undef = Expr::Ident(Ident::new(
                                "undefined".into(), span, Default::default(),
                            ));
                            out.push(decl_from_pat(d.name.clone(), Some(undef), kind, span));
                        }
                    }
                    other_init => {
                        // declarator sem yield: preserva (transform no init p/
                        // pegar yields aninhados raros).
                        let init = other_init.cloned().map(|e| Box::new(transform_expr(e)));
                        out.push(decl_from_pat(d.name.clone(), init.map(|b| *b), kind, span));
                    }
                }
            }
            // Inline (sem Block) p/ preservar escopo: `const r` precisa ficar
            // visivel aos statements seguintes do mesmo bloco.
            out
        }
        other => vec![other],
    }
}

/// Helper: `<kind> <name> = <init>;` com nome ident.
fn decl_const(name: &str, init: Option<Expr>, kind: VarDeclKind, span: swc_common::Span) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span,
        ctxt: Default::default(),
        kind,
        declare: false,
        decls: vec![VarDeclarator {
            span,
            name: Pat::Ident(Ident::new(name.into(), span, Default::default()).into()),
            init: init.map(Box::new),
            definite: false,
        }],
    })))
}

/// Helper: `<kind> <pat> = <init>;` reusando o Pat original do declarator.
fn decl_from_pat(pat: Pat, init: Option<Expr>, kind: VarDeclKind, span: swc_common::Span) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span,
        ctxt: Default::default(),
        kind,
        declare: false,
        decls: vec![VarDeclarator {
            span,
            name: pat,
            init: init.map(Box::new),
            definite: false,
        }],
    })))
}

/// `__RTS_GEN_GET_RET(vec)` — sentinela que o codegen mapeia p/ ler o
/// ret_value (`return X`) de uma generator delegada via `yield*`.
fn gen_get_ret_call(vec: Expr, span: swc_common::Span) -> Expr {
    Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
            "__RTS_GEN_GET_RET".into(),
            span,
            Default::default(),
        )))),
        args: vec![ExprOrSpread { spread: None, expr: Box::new(vec) }],
        type_args: None,
    })
}

/// Constroi `__RTS_GEN_FINISH(buf, ret)` — sentinela que o codegen mapeia
/// pra registrar o ret_value do generator e devolver o Vec.
fn gen_finish_call(buf: Expr, ret: Option<Expr>, span: swc_common::Span) -> Expr {
    let ret_expr = ret.unwrap_or(Expr::Ident(Ident::new(
        "undefined".into(),
        span,
        Default::default(),
    )));
    Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
            "__RTS_GEN_FINISH".into(),
            span,
            Default::default(),
        )))),
        args: vec![
            ExprOrSpread { spread: None, expr: Box::new(buf) },
            ExprOrSpread { spread: None, expr: Box::new(ret_expr) },
        ],
        type_args: None,
    })
}

fn transform_expr(expr: Expr) -> Expr {
    if let Expr::Yield(y) = &expr {
        // `yield expr` em posicao de expressao (raro): substituir por
        // `__gen_buf.push(expr)` (chamada que tambem retorna len novo).
        return push_call_expr(y.arg.as_deref().cloned(), y.span);
    }
    expr
}

fn delegate_for_of(arg: Option<Expr>, span: swc_common::Span) -> Stmt {
    let iter = arg.unwrap_or(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
        span,
        value: 0.0,
        raw: None,
    })));
    let tmp = "__yt";
    let var_decl = VarDecl {
        span,
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span,
            name: Pat::Ident(Ident::new(tmp.into(), span, Default::default()).into()),
            init: None,
            definite: false,
        }],
    };
    let body = Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(push_call_expr(
            Some(Expr::Ident(Ident::new(tmp.into(), span, Default::default()))),
            span,
        )),
    });
    Stmt::ForOf(swc_ecma_ast::ForOfStmt {
        span,
        is_await: false,
        left: swc_ecma_ast::ForHead::VarDecl(Box::new(var_decl)),
        right: Box::new(iter),
        body: Box::new(body),
    })
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
