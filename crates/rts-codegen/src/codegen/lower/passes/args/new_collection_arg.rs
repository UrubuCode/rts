//! (#374) Desugar de `new Map(<call>)` / `new Set(<call>)` com arg CallExpr.
//!
//! `new Map(entries.map(([k,v]) => [v,k]))` inline nao popula — o codegen de
//! `new Map` so' aceita arg Ident/Member (var ja' materializada), porque o Vec
//! de pares do `.map` inline nao materializa estavel no contexto do `new`. A
//! variante via var (`const t = arr.map(...); new Map(t)`) funciona.
//!
//! Este pass faz exatamente essa extracao: quando um statement contem
//! `new Map/Set(<call>)`, insere `const __nc_tmp_N = <call>;` antes e troca o
//! arg pelo ident temporario. Roda ANTES de lift_inline_arrows_in_array_methods
//! para que o `.map` extraido seja liftado como statement normal.

use swc_ecma_ast::{Callee, Decl, Expr, Stmt, VarDecl, VarDeclKind, VarDeclarator, Pat};

use crate::parser::ast::{ClassMember, Item, Program, RawStmt, Statement};
use crate::parser::span::Span;

pub(crate) fn desugar_new_collection_call_arg(program: &mut Program) {
    let mut counter: u32 = 0;
    // Top-level statements + fn bodies + class methods/constructors.
    let mut items = std::mem::take(&mut program.items);
    for item in items.iter_mut() {
        match item {
            Item::Function(f) => rewrite_body(&mut f.body, &mut counter),
            Item::Class(c) => {
                for mem in c.members.iter_mut() {
                    match mem {
                        ClassMember::Method(m) => rewrite_body(&mut m.body, &mut counter),
                        ClassMember::Constructor(ct) => rewrite_body(&mut ct.body, &mut counter),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // Top-level Item::Statement: reescreve como lista (pode expandir 1 -> 2).
    let mut new_items: Vec<Item> = Vec::with_capacity(items.len());
    for item in items {
        if let Item::Statement(Statement::Raw(raw)) = &item {
            if let Some(stmt) = raw.stmt.as_ref() {
                let mut s = stmt.clone();
                let mut pre: Vec<Stmt> = Vec::new();
                extract_in_stmt(&mut s, &mut pre, &mut counter);
                if !pre.is_empty() {
                    for p in pre {
                        new_items.push(Item::Statement(Statement::Raw(
                            RawStmt::new("<new-coll-tmp>".to_string(), Span::default()).with_stmt(p),
                        )));
                    }
                    new_items.push(Item::Statement(Statement::Raw(
                        RawStmt::new("<new-coll>".to_string(), Span::default()).with_stmt(s),
                    )));
                    continue;
                }
            }
        }
        new_items.push(item);
    }
    program.items = new_items;
}

fn rewrite_body(body: &mut Vec<Statement>, counter: &mut u32) {
    let old = std::mem::take(body);
    for stmt_raw in old {
        let Statement::Raw(raw) = stmt_raw;
        if let Some(mut stmt) = raw.stmt.clone() {
            let mut pre: Vec<Stmt> = Vec::new();
            extract_in_stmt(&mut stmt, &mut pre, counter);
            for p in pre {
                body.push(Statement::Raw(
                    RawStmt::new("<new-coll-tmp>".to_string(), Span::default()).with_stmt(p),
                ));
            }
            body.push(Statement::Raw(
                RawStmt::new(raw.text.clone(), raw.span).with_stmt(stmt),
            ));
        } else {
            body.push(Statement::Raw(raw));
        }
    }
}

/// Procura `new Map/Set(<call>)` no statement; extrai o arg CallExpr p/ uma var
/// temporaria (empurrada em `pre`) e troca o arg pelo ident. Cobre VarDecl init,
/// Expr, Return.
fn extract_in_stmt(stmt: &mut Stmt, pre: &mut Vec<Stmt>, counter: &mut u32) {
    match stmt {
        Stmt::Decl(Decl::Var(vd)) => {
            for d in vd.decls.iter_mut() {
                if let Some(init) = d.init.as_deref_mut() {
                    extract_in_expr(init, pre, counter);
                }
            }
        }
        Stmt::Expr(e) => extract_in_expr(&mut e.expr, pre, counter),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                extract_in_expr(a, pre, counter);
            }
        }
        _ => {}
    }
}

fn extract_in_expr(expr: &mut Expr, pre: &mut Vec<Stmt>, counter: &mut u32) {
    if let Expr::New(ne) = expr {
        // Eh `new Map(...)` ou `new Set(...)`?
        let is_coll = matches!(
            ne.callee.as_ref(),
            Expr::Ident(id) if matches!(id.sym.as_str(), "Map" | "Set" | "WeakMap" | "WeakSet")
        );
        if is_coll {
            if let Some(args) = ne.args.as_mut() {
                if let Some(first) = args.first_mut() {
                    if first.spread.is_none()
                        && matches!(first.expr.as_ref(), Expr::Call(_))
                    {
                        // Extrai o call para var temporaria.
                        let tmp = format!("__nc_tmp_{}", *counter);
                        *counter += 1;
                        let call_expr = std::mem::replace(
                            first.expr.as_mut(),
                            Expr::Ident(swc_ecma_ast::Ident {
                                span: Default::default(),
                                ctxt: Default::default(),
                                sym: tmp.clone().into(),
                                optional: false,
                            }),
                        );
                        pre.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                            span: Default::default(),
                            ctxt: Default::default(),
                            kind: VarDeclKind::Const,
                            declare: false,
                            decls: vec![VarDeclarator {
                                span: Default::default(),
                                name: Pat::Ident(swc_ecma_ast::Ident {
                                    span: Default::default(),
                                    ctxt: Default::default(),
                                    sym: tmp.into(),
                                    optional: false,
                                }.into()),
                                init: Some(Box::new(call_expr)),
                                definite: false,
                            }],
                        }))));
                    }
                }
            }
        }
    }
    // Recursa em sub-exprs comuns (init pode ser `cond ? new Map(...) : ...`,
    // ou `obj.x = new Map(...)` em static block / assignment).
    match expr {
        Expr::Paren(p) => extract_in_expr(&mut p.expr, pre, counter),
        Expr::Cond(c) => {
            extract_in_expr(&mut c.cons, pre, counter);
            extract_in_expr(&mut c.alt, pre, counter);
        }
        Expr::TsAs(a) => extract_in_expr(&mut a.expr, pre, counter),
        Expr::TsNonNull(a) => extract_in_expr(&mut a.expr, pre, counter),
        Expr::Assign(a) => extract_in_expr(&mut a.right, pre, counter),
        _ => {}
    }
}
