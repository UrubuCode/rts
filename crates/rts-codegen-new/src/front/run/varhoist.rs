//! `var` HOISTING (#301 fase 1) — a pre-pass over a function/module body.
//!
//! JS `var` is FUNCTION-scoped and hoisted: the NAME is visible (undefined)
//! from the top of the enclosing function, and a `var` declared inside a
//! `for`/block outlives it. The engine's locals are already FLAT per function
//! (a re-`let` reuses the slot), so the only missing piece is the PREDECLARE:
//! reading `x` before its `var x = 5` line bailed "unbound identifier".
//!
//! This pass walks the RAW swc statements of a body (the parser keeps
//! `Statement::Raw`), collects every `var`-kind declarator name — recursing
//! through nested blocks/if/loops/try/switch but NOT into nested
//! function/arrow/class bodies (their `var`s belong to THEIR scope) — and
//! returns one synthetic `let <name> = <zero-ish>` per name, to prepend to the
//! lowered HIR body. The real `var x = init` later in the body is a flat
//! re-`let` (same slot), i.e. an assignment — exactly the `var` semantic.
//!
//! The hoisted initial value: `0` for a numeric annotation (the suite's typed
//! `var x: i64` prints `0` pre-init, the documented #301 proxy for undefined),
//! `undefined` otherwise.

use std::collections::HashSet;

use rts_ast::ast::Statement;
use rts_hir::ir::{HirExprKind, HirLit, HirStmt};
use rts_hir::{HirExpr, HirType};

/// The synthetic hoist prologue for `body` — empty when it declares no `var`s.
pub(crate) fn hoisted_var_lets(body: &[Statement]) -> Vec<HirStmt> {
    let mut names: Vec<(String, Option<String>)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = &raw.stmt {
            collect_stmt(stmt, &mut names, &mut seen);
        }
    }
    names
        .into_iter()
        .map(|(name, ann)| {
            let ty = ann
                .as_deref()
                .map(rts_hir::lower::parse_type_annotation)
                .unwrap_or(HirType::Unknown);
            let init = if matches!(
                ty,
                HirType::I8
                    | HirType::I16
                    | HirType::I32
                    | HirType::I64
                    | HirType::U8
                    | HirType::U16
                    | HirType::U32
                    | HirType::U64
                    | HirType::F32
                    | HirType::F64
            ) {
                HirExpr::new(HirExprKind::Lit(HirLit::Int(0)), ty.clone())
            } else {
                HirExpr::new(HirExprKind::Lit(HirLit::Undefined), HirType::Unknown)
            };
            HirStmt::Let {
                name,
                ty,
                init: Some(init),
            }
        })
        .collect()
}

/// The NAMES the hoist prologue predeclares for `body` (the set the in-body
/// `var` declarations must be REWRITTEN against).
pub(crate) fn hoisted_var_names(body: &[Statement]) -> HashSet<String> {
    let mut names: Vec<(String, Option<String>)> = Vec::new();
    let mut seen = HashSet::new();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = &raw.stmt {
            collect_stmt(stmt, &mut names, &mut seen);
        }
    }
    seen
}

/// Rewrite every in-body `let <name> = init` whose `name` was HOISTED into a
/// plain ASSIGNMENT (`name = init`; an initializer-less decl is dropped — the
/// prologue already bound it). Without this, a `var v = 7` inside an explicit
/// `{ }` block would bind a fresh BLOCK-scoped local that dies at the block
/// exit (the lexical-scope save/restore), instead of assigning the hoisted
/// function-scoped binding. Recurses through every statement body (never into
/// nested fn/arrow bodies — those are separate HirFuncs).
pub(crate) fn rewrite_var_decls_to_assigns(stmts: &mut Vec<HirStmt>, hoisted: &HashSet<String>) {
    for s in stmts.iter_mut() {
        rewrite_stmt(s, hoisted);
    }
}

fn rewrite_stmt(s: &mut HirStmt, hoisted: &HashSet<String>) {
    match s {
        HirStmt::Let { name, init, .. } if hoisted.contains(name.as_str()) => {
            let assign = match init.take() {
                Some(v) => HirStmt::Expr(HirExpr::new(
                    HirExprKind::Assign {
                        target: Box::new(HirExpr::new(
                            HirExprKind::Ident(name.clone()),
                            HirType::Unknown,
                        )),
                        value: Box::new(v),
                    },
                    HirType::Unknown,
                )),
                // `var x;` with no init — the prologue already bound it; the
                // statement becomes a no-op expression.
                None => HirStmt::Expr(HirExpr::new(
                    HirExprKind::Lit(HirLit::Undefined),
                    HirType::Unknown,
                )),
            };
            *s = assign;
        }
        HirStmt::If { then, else_, .. } => {
            rewrite_var_decls_to_assigns(then, hoisted);
            if let Some(el) = else_ {
                rewrite_var_decls_to_assigns(el, hoisted);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            rewrite_var_decls_to_assigns(body, hoisted)
        }
        HirStmt::For { init, body, .. } => {
            if let Some(i) = init {
                rewrite_stmt(i, hoisted);
            }
            rewrite_var_decls_to_assigns(body, hoisted);
        }
        HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
            rewrite_var_decls_to_assigns(body, hoisted)
        }
        HirStmt::Block(b) => rewrite_var_decls_to_assigns(b, hoisted),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            rewrite_var_decls_to_assigns(body, hoisted);
            if let Some(c) = catch {
                rewrite_var_decls_to_assigns(&mut c.body, hoisted);
            }
            if let Some(f) = finally {
                rewrite_var_decls_to_assigns(f, hoisted);
            }
        }
        HirStmt::Switch { cases, .. } => {
            for c in cases.iter_mut() {
                rewrite_var_decls_to_assigns(&mut c.body, hoisted);
            }
        }
        HirStmt::Labeled { body, .. } => rewrite_stmt(body, hoisted),
        _ => {}
    }
}

/// Collect `var`-kind declarator names (+ type annotation text) from `stmt`,
/// recursing through statement bodies but never into a nested fn/arrow/class.
fn collect_stmt(
    stmt: &swc_ecma_ast::Stmt,
    out: &mut Vec<(String, Option<String>)>,
    seen: &mut std::collections::HashSet<String>,
) {
    use swc_ecma_ast::{Decl, ForHead, Stmt, VarDeclKind, VarDeclOrExpr};
    let mut add_var_decl =
        |vd: &swc_ecma_ast::VarDecl,
         out: &mut Vec<(String, Option<String>)>,
         seen: &mut std::collections::HashSet<String>| {
            if vd.kind != VarDeclKind::Var {
                return;
            }
            for d in &vd.decls {
                if let swc_ecma_ast::Pat::Ident(bi) = &d.name {
                    let name = bi.id.sym.to_string();
                    if seen.insert(name.clone()) {
                        let ann = bi.type_ann.as_deref().map(|t| type_ann_text(&t.type_ann));
                        out.push((name, ann.flatten()));
                    }
                }
            }
        };
    match stmt {
        Stmt::Decl(Decl::Var(vd)) => add_var_decl(vd, out, seen),
        Stmt::Block(b) => {
            for s in &b.stmts {
                collect_stmt(s, out, seen);
            }
        }
        Stmt::If(i) => {
            collect_stmt(&i.cons, out, seen);
            if let Some(alt) = &i.alt {
                collect_stmt(alt, out, seen);
            }
        }
        Stmt::While(w) => collect_stmt(&w.body, out, seen),
        Stmt::DoWhile(w) => collect_stmt(&w.body, out, seen),
        Stmt::For(f) => {
            if let Some(VarDeclOrExpr::VarDecl(vd)) = &f.init {
                add_var_decl(vd, out, seen);
            }
            collect_stmt(&f.body, out, seen);
        }
        Stmt::ForIn(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                add_var_decl(vd, out, seen);
            }
            collect_stmt(&f.body, out, seen);
        }
        Stmt::ForOf(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                add_var_decl(vd, out, seen);
            }
            collect_stmt(&f.body, out, seen);
        }
        Stmt::Try(t) => {
            for s in &t.block.stmts {
                collect_stmt(s, out, seen);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    collect_stmt(s, out, seen);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    collect_stmt(s, out, seen);
                }
            }
        }
        Stmt::Switch(sw) => {
            for c in &sw.cases {
                for s in &c.cons {
                    collect_stmt(s, out, seen);
                }
            }
        }
        Stmt::Labeled(l) => collect_stmt(&l.body, out, seen),
        // Nested function/arrow/class bodies own their `var`s — not descended
        // (fn decls are separate Items anyway; exprs cannot contain `var`).
        _ => {}
    }
}

/// The source text of a simple type annotation (`i64`, `number`, …) — `None`
/// for a composite annotation the hoist does not model (falls to `Unknown`).
fn type_ann_text(t: &swc_ecma_ast::TsType) -> Option<String> {
    use swc_ecma_ast::{TsEntityName, TsKeywordTypeKind, TsType};
    match t {
        TsType::TsKeywordType(k) => Some(
            match k.kind {
                TsKeywordTypeKind::TsNumberKeyword => "number",
                TsKeywordTypeKind::TsBooleanKeyword => "boolean",
                TsKeywordTypeKind::TsStringKeyword => "string",
                _ => return None,
            }
            .to_string(),
        ),
        TsType::TsTypeRef(r) => match &r.type_name {
            TsEntityName::Ident(id) => Some(id.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}
