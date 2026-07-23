//! Free-variable + mutation analysis over the lowering subset (P5.7) — split out
//! of [`super`] (the <500-line module rule). Pure, allocation-light walks the
//! capture analysis relies on:
//!
//! - [`collect_free_stmt`] / [`collect_free_expr`] — the free identifiers of a
//!   statement / expression (those not bound by a param or an inner `let`/`const`),
//!   used to decide which names an arrow CAPTURES.
//! - [`mutated_names`] / [`arrow_assigned_names`] — the identifiers that appear as
//!   an ASSIGNMENT TARGET, used to reject UNSOUND by-value captures (a captured
//!   var the closure writes, or one reassigned in the enclosing scope).

use std::collections::HashSet;

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirStmt};

// ---------------------------------------------------------------------------
// Mutation analysis (which locals are reassigned — captures of these are unsound
// by-value).
// ---------------------------------------------------------------------------

/// The set of identifier names that appear as an ASSIGNMENT TARGET anywhere in
/// `stmts` (`x = …`, `x += …`, `x++`, `--x`). A captured var in this set cannot
/// be captured by value soundly (its value may change after the snapshot).
pub(super) fn mutated_names(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_mutated_stmt(s, &mut out);
    }
    out
}

/// The names a closure body ASSIGNS to — same shape as [`mutated_names`] but over
/// the arrow's own body (detects the closure WRITING a captured var).
pub(super) fn arrow_assigned_names(stmts: &[HirStmt]) -> HashSet<String> {
    mutated_names(stmts)
}

/// The names declared with a FUNCTION-VALUED initializer (`const f = (..) =>
/// …` / a nested fn-decl lowered to that shape) anywhere in `stmts`. A capture
/// of one of these is routed to a CELL: mutually-recursive nested fns
/// FORWARD-reference each other (`factor` calls `expr` declared later), so a
/// by-value snapshot at reify time would capture `undefined` — the cell reads
/// the live fn value at call time.
pub(crate) fn arrow_decl_names(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_arrow_decls(stmts, &mut out);
    out
}

fn collect_arrow_decls(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Let {
                name,
                init: Some(e),
                ..
            }
            | HirStmt::Const { name, init: e, .. } => {
                // Pre-rewrite the init is an inline `Arrow`; post-rewrite (the
                // prologue's cell-hoisting scan runs on the REWRITTEN body) it
                // is an `Ident` of the synthesized fn — accept both.
                let is_fn_init = matches!(e.kind, HirExprKind::Arrow { .. })
                    || matches!(&e.kind, HirExprKind::Ident(n)
                        if n.starts_with("__rtsn_arrow_") || n.starts_with("__rtsl_"));
                if is_fn_init {
                    out.insert(name.clone());
                }
            }
            HirStmt::If { then, else_, .. } => {
                collect_arrow_decls(then, out);
                if let Some(e) = else_ {
                    collect_arrow_decls(e, out);
                }
            }
            HirStmt::While { body, .. } => collect_arrow_decls(body, out),
            HirStmt::Block(b) => collect_arrow_decls(b, out),
            _ => {}
        }
    }
}

/// The names DECLARED (`let`/`const`) anywhere in `stmts` (top-level + nested
/// blocks/if/while). Used to exclude a function's OWN locals when deciding which
/// names it writes are FREE (module-global) writes (see `module_globals`).
pub fn declared_names(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_declared_stmt(s, &mut out);
    }
    out
}

fn collect_declared_stmt(s: &HirStmt, out: &mut HashSet<String>) {
    if let HirStmt::Let { name, .. } | HirStmt::Const { name, .. } = s {
        out.insert(name.clone());
    }
    for_each_child_stmt(s, &mut |st| collect_declared_stmt(st, out));
}

fn collect_mutated_stmt(s: &HirStmt, out: &mut HashSet<String>) {
    // Descend into EVERY child expr + child stmt. Skipping a kind (the old bug:
    // for / for-of / for-in / try / switch / do-while / labeled were dropped)
    // mis-classified a WRITTEN module-global as immutable, so it got memoized and
    // read stale — #1978: a counter written inside a `for` in a giant top-level
    // `while` stayed 0. Over-approximating mutation is always sound: it only makes
    // us bail / reload more, never yields a wrong value.
    for_each_child_expr(s, &mut |e| collect_mutated_expr(e, out));
    for_each_child_stmt(s, &mut |st| collect_mutated_stmt(st, out));
}

/// Apply `f` to every direct child STATEMENT of `s` (all loop / try / switch /
/// labeled bodies + `for`-init). Centralized so no statement kind is ever
/// silently skipped by a structural walk again.
fn for_each_child_stmt(s: &HirStmt, f: &mut impl FnMut(&HirStmt)) {
    match s {
        HirStmt::If { then, else_, .. } => {
            for st in then {
                f(st);
            }
            if let Some(e) = else_ {
                for st in e {
                    f(st);
                }
            }
        }
        HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::ForOf { body, .. }
        | HirStmt::ForIn { body, .. }
        | HirStmt::Block(body) => {
            for st in body {
                f(st);
            }
        }
        HirStmt::For { init, body, .. } => {
            if let Some(i) = init {
                f(i);
            }
            for st in body {
                f(st);
            }
        }
        HirStmt::Try { body, catch, finally } => {
            for st in body {
                f(st);
            }
            if let Some(c) = catch {
                for st in &c.body {
                    f(st);
                }
            }
            if let Some(fin) = finally {
                for st in fin {
                    f(st);
                }
            }
        }
        HirStmt::Switch { cases, .. } => {
            for c in cases {
                for st in &c.body {
                    f(st);
                }
            }
        }
        HirStmt::Labeled { body, .. } => f(body),
        _ => {}
    }
}

/// Apply `f` to every direct child EXPRESSION of `s` (conditions, iterables, the
/// discriminant + case tests, initializers, the throw / return / expr value).
fn for_each_child_expr(s: &HirStmt, f: &mut impl FnMut(&HirExpr)) {
    match s {
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::Throw(e) => f(e),
        HirStmt::Let { init: Some(e), .. } => f(e),
        HirStmt::Const { init, .. } => f(init),
        HirStmt::If { cond, .. } | HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => {
            f(cond)
        }
        HirStmt::For { cond, update, .. } => {
            if let Some(c) = cond {
                f(c);
            }
            if let Some(u) = update {
                f(u);
            }
        }
        HirStmt::ForOf { iterable, .. } => f(iterable),
        HirStmt::ForIn { object, .. } => f(object),
        HirStmt::Switch { discriminant, cases } => {
            f(discriminant);
            for c in cases {
                if let Some(t) = &c.test {
                    f(t);
                }
            }
        }
        _ => {}
    }
}

fn collect_mutated_expr(e: &HirExpr, out: &mut HashSet<String>) {
    match &e.kind {
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            if let HirExprKind::Ident(n) = &target.kind {
                out.insert(n.clone());
            }
            collect_mutated_expr(target, out);
            collect_mutated_expr(value, out);
        }
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => {
            if let HirExprKind::Ident(n) = &t.kind {
                out.insert(n.clone());
            }
            collect_mutated_expr(t, out);
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            collect_mutated_expr(lhs, out);
            collect_mutated_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. } => collect_mutated_expr(operand, out),
        HirExprKind::Call { callee, args } => {
            collect_mutated_expr(callee, out);
            args.iter().for_each(|a| collect_mutated_expr(a, out));
        }
        HirExprKind::MethodCall { object, args, .. } => {
            collect_mutated_expr(object, out);
            args.iter().for_each(|a| collect_mutated_expr(a, out));
        }
        HirExprKind::Member { object, .. } => collect_mutated_expr(object, out),
        HirExprKind::Index { object, index } => {
            collect_mutated_expr(object, out);
            collect_mutated_expr(index, out);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            collect_mutated_expr(cond, out);
            collect_mutated_expr(then, out);
            collect_mutated_expr(else_, out);
        }
        HirExprKind::Array(elems) => elems.iter().for_each(|el| collect_mutated_expr(el, out)),
        HirExprKind::Object(fields) => fields
            .iter()
            .for_each(|(_, v)| collect_mutated_expr(v, out)),
        // A nested arrow's assignments to its OWN params/locals are irrelevant to
        // the outer scope; conservatively descend (over-approximating mutation is
        // sound — it only makes us bail more). Its captures are handled when it is
        // itself extracted.
        HirExprKind::Arrow { body, .. } => match body {
            HirArrowBody::Expr(inner) => collect_mutated_expr(inner, out),
            HirArrowBody::Block(stmts) => stmts.iter().for_each(|st| collect_mutated_stmt(st, out)),
        },
        HirExprKind::Cast { expr, .. } => collect_mutated_expr(expr, out),
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => collect_mutated_expr(inner, out),
        HirExprKind::Seq(items) => items.iter().for_each(|it| collect_mutated_expr(it, out)),
        HirExprKind::New { args, .. } => args.iter().for_each(|a| collect_mutated_expr(a, out)),
        HirExprKind::Ident(_) | HirExprKind::Lit(_) | HirExprKind::Raw(_) => {}
    }
}

// Arrow-capture + free-variable analysis lives in the sibling `scan_free` module
// (the <500-line rule). Re-exported here so callers keep the `scan::` path.
pub(super) use super::scan_free::arrow_free_idents;
pub use super::scan_free::collect_free_stmt;
