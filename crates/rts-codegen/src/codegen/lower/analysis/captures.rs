//! Analise de capturas em arrows + helpers de promote-to-global.
//!
//! Coleta locais de uma fn, scaneia arrows aninhados pra descobrir
//! quais idents sao capturados, e fornece operacoes de renaming/promotion
//! usadas pelo lifter de arrows. Extraido sem mudanca de semantica.

use swc_ecma_ast::{Callee, Expr, Lit, Pat, Stmt};

use crate::parser::ast::{RawStmt, Statement};
use crate::parser::span::Span;

/// Colete nomes declarados via `let`/`const`/`var` em todos os statements
/// do body. Não desce em arrows (escopos próprios) — apenas o escopo da
/// fn. Adiciona ao set existente.
pub(crate) fn collect_local_decls(body: &[Statement], locals: &mut std::collections::HashSet<String>) {
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
pub(crate) fn collect_captures_in_body(
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
        // (cross-runtime #41/#195) Fn expressions tambem capturam:
        // `function nested(base) { return function(extra) { return base + extra; } }`
        Expr::Fn(fn_expr) => {
            collect_captured_from_fn_expr(fn_expr, locals, captured);
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

/// (cross-runtime #41/#195) Mesma logica de Arrow mas para FnExpr.
fn collect_captured_from_fn_expr(
    fn_expr: &swc_ecma_ast::FnExpr,
    enclosing_locals: &std::collections::HashSet<String>,
    captured: &mut std::collections::BTreeSet<String>,
) {
    let mut fn_locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &fn_expr.function.params {
        if let Pat::Ident(id) = &p.pat {
            fn_locals.insert(id.id.sym.to_string());
        }
    }
    let stmts: Vec<Stmt> = fn_expr.function.body.as_ref()
        .map(|b| b.stmts.clone())
        .unwrap_or_default();
    for s in &stmts {
        collect_decls_in_stmt(s, &mut fn_locals);
    }
    for s in &stmts {
        collect_idents_used_in_stmt(s, enclosing_locals, &fn_locals, captured);
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
        // (cross-runtime #41/#195) Fn expressions tambem capturam locais
        // da fn enclosing — recursa no body.
        Expr::Fn(fn_expr) => {
            // Skip se shadow por param.
            let shadowed = fn_expr.function.params.iter().any(|p| {
                if let Pat::Ident(b) = &p.pat {
                    b.id.sym.as_str() == old
                } else { false }
            });
            if !shadowed {
                if let Some(body) = fn_expr.function.body.as_mut() {
                    for s in &mut body.stmts {
                        rename_ident_in_stmt(s, old, new);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Apenas reescreve usos de `old` para `new` em todos os statements
/// (sem tocar declarações). Usado para parâmetros — o param continua
/// recebendo o valor original, mas todos os usos referem à global.
pub(crate) fn rename_uses_in_body(body: &mut Vec<Statement>, old: &str, new: &str) {
    for s in body.iter_mut() {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_mut() {
            rename_ident_in_stmt(stmt, old, new);
        }
    }
}

/// `<global> = <param>;` injetado no topo da fn pra sincronizar valor
/// inicial do parâmetro com a global promovida.
pub(crate) fn make_sync_param_to_global(global: &str, param: &str) -> Statement {
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
pub(crate) fn promote_local_to_global(body: &mut Vec<Statement>, old: &str, new: &str) {
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

/// Detecta se `fn_name` segue o padrao de fn sintetizada de classe e
/// retorna o nome da classe owner. Reconhece: `__class_<C>__init`,
/// `__class_<C>_get_<x>`, `__class_<C>_set_<x>`, `__class_<C>_static_<x>`,
/// `__class_<C>_lifted_arrow_<n>`, e fallback `__class_<C>_<method>`.
pub(crate) fn extract_class_owner(fn_name: &str) -> Option<String> {
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
