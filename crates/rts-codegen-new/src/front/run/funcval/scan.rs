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

fn collect_mutated_stmt(s: &HirStmt, out: &mut HashSet<String>) {
    match s {
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => collect_mutated_expr(e, out),
        HirStmt::Return(None) => {}
        HirStmt::Let { init: Some(e), .. } => collect_mutated_expr(e, out),
        HirStmt::Let { init: None, .. } => {}
        HirStmt::Const { init, .. } => collect_mutated_expr(init, out),
        HirStmt::If { cond, then, else_ } => {
            collect_mutated_expr(cond, out);
            then.iter().for_each(|st| collect_mutated_stmt(st, out));
            if let Some(e) = else_ {
                e.iter().for_each(|st| collect_mutated_stmt(st, out));
            }
        }
        HirStmt::While { cond, body } => {
            collect_mutated_expr(cond, out);
            body.iter().for_each(|st| collect_mutated_stmt(st, out));
        }
        HirStmt::Block(b) => b.iter().for_each(|st| collect_mutated_stmt(st, out)),
        // Other statement kinds (for/for-of/try/…) are outside the lowering subset
        // — an arrow inside them bails regardless, and a capture across them is
        // never reached, so not descending is sound here.
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
        HirExprKind::Object(fields) => {
            fields.iter().for_each(|(_, v)| collect_mutated_expr(v, out))
        }
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

// ---------------------------------------------------------------------------
// Free-variable collection over the lowering subset.
// ---------------------------------------------------------------------------

pub(super) fn collect_free_stmt(
    s: &HirStmt,
    bound: &mut HashSet<String>,
    free: &mut HashSet<String>,
) {
    match s {
        HirStmt::Expr(e) => collect_free_expr(e, bound, free),
        HirStmt::Return(Some(e)) => collect_free_expr(e, bound, free),
        HirStmt::Return(None) => {}
        HirStmt::Let { name, init, .. } => {
            if let Some(e) = init {
                collect_free_expr(e, bound, free);
            }
            bound.insert(name.clone());
        }
        HirStmt::Const { name, init, .. } => {
            collect_free_expr(init, bound, free);
            bound.insert(name.clone());
        }
        HirStmt::If { cond, then, else_ } => {
            collect_free_expr(cond, bound, free);
            for st in then {
                collect_free_stmt(st, bound, free);
            }
            if let Some(e) = else_ {
                for st in e {
                    collect_free_stmt(st, bound, free);
                }
            }
        }
        HirStmt::While { cond, body } => {
            collect_free_expr(cond, bound, free);
            for st in body {
                collect_free_stmt(st, bound, free);
            }
        }
        HirStmt::Block(b) => {
            for st in b {
                collect_free_stmt(st, bound, free);
            }
        }
        // A statement kind outside the subset: be conservative and treat any
        // identifier it would reference as free (forces a bail). We approximate by
        // not descending — the lowering will bail on the construct itself anyway.
        _ => {}
    }
}

fn collect_free_expr(e: &HirExpr, bound: &HashSet<String>, free: &mut HashSet<String>) {
    match &e.kind {
        HirExprKind::Ident(name) => {
            if !bound.contains(name) {
                free.insert(name.clone());
            }
        }
        HirExprKind::Lit(_) => {}
        HirExprKind::Bin { lhs, rhs, .. } => {
            collect_free_expr(lhs, bound, free);
            collect_free_expr(rhs, bound, free);
        }
        HirExprKind::Unary { operand, .. } => collect_free_expr(operand, bound, free),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            collect_free_expr(target, bound, free);
            collect_free_expr(value, bound, free);
        }
        HirExprKind::Call { callee, args } => {
            collect_free_expr(callee, bound, free);
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            collect_free_expr(object, bound, free);
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::Member { object, .. } => collect_free_expr(object, bound, free),
        HirExprKind::Index { object, index } => {
            collect_free_expr(object, bound, free);
            collect_free_expr(index, bound, free);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            collect_free_expr(cond, bound, free);
            collect_free_expr(then, bound, free);
            collect_free_expr(else_, bound, free);
        }
        HirExprKind::Array(elems) => {
            for el in elems {
                collect_free_expr(el, bound, free);
            }
        }
        HirExprKind::Object(fields) => {
            for (_, v) in fields {
                collect_free_expr(v, bound, free);
            }
        }
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => collect_free_expr(t, bound, free),
        // A nested arrow's OWN params shadow; collect its free vars minus its params.
        HirExprKind::Arrow { params, body, .. } => {
            let mut inner_bound = bound.clone();
            for p in params {
                inner_bound.insert(p.name.clone());
            }
            match body {
                HirArrowBody::Expr(inner) => collect_free_expr(inner, &inner_bound, free),
                HirArrowBody::Block(stmts) => {
                    for st in stmts {
                        collect_free_stmt(st, &mut inner_bound.clone(), free);
                    }
                }
            }
        }
        // Anything else (await/spread/cast/seq/new/raw): conservatively descend
        // where there is an obvious child; otherwise ignore (lowering will bail).
        HirExprKind::Cast { expr, .. } => collect_free_expr(expr, bound, free),
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => {
            collect_free_expr(inner, bound, free)
        }
        HirExprKind::Seq(items) => {
            for it in items {
                collect_free_expr(it, bound, free);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::Raw(_) => {}
    }
}
