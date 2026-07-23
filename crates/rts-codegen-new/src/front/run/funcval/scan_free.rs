//! Free-variable + arrow-capture analysis — split out of [`super::scan`] to keep
//! each file under the 500-line module rule. Pure, allocation-light walks:
//!
//! - [`arrow_free_idents`] — every outer name some ARROW body reads (a captured
//!   top-level `let`; if also mutated, capture-by-value is unsound → CELL).
//! - [`collect_free_stmt`] / [`collect_free_expr`] — the free identifiers of a
//!   statement / expression (those not bound by a param or an inner `let`/`const`).

use std::collections::HashSet;

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirStmt};

// ---------------------------------------------------------------------------
// Arrow-referenced identifiers (which outer names some ARROW body reads).
// ---------------------------------------------------------------------------

/// Every identifier referenced FREE inside SOME arrow body in `stmts` (the arrow's
/// own params excluded). A top-level `let` in this set is CAPTURED by a closure; if
/// it is also mutated, capture-by-value is unsound, so it is promoted to a runtime
/// CELL instead (live reference — see `module_globals` force-promotion). Reuses
/// [`collect_free_expr`]: calling it on an `Arrow` node with an empty bound set
/// yields exactly that arrow's (and any nested arrow's) free identifiers.
pub(super) fn arrow_free_idents(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_arrow_frees_stmt(s, &mut out);
    }
    out
}

fn collect_arrow_frees_stmt(s: &HirStmt, out: &mut HashSet<String>) {
    match s {
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::Throw(e) => {
            collect_arrow_frees_expr(e, out)
        }
        HirStmt::Let { init: Some(e), .. } => collect_arrow_frees_expr(e, out),
        HirStmt::Const { init, .. } => collect_arrow_frees_expr(init, out),
        HirStmt::If { cond, then, else_ } => {
            collect_arrow_frees_expr(cond, out);
            then.iter().for_each(|st| collect_arrow_frees_stmt(st, out));
            if let Some(e) = else_ {
                e.iter().for_each(|st| collect_arrow_frees_stmt(st, out));
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_arrow_frees_expr(cond, out);
            body.iter().for_each(|st| collect_arrow_frees_stmt(st, out));
        }
        HirStmt::Block(b) => b.iter().for_each(|st| collect_arrow_frees_stmt(st, out)),
        _ => {}
    }
}

fn collect_arrow_frees_expr(e: &HirExpr, out: &mut HashSet<String>) {
    if matches!(e.kind, HirExprKind::Arrow { .. }) {
        // Collect this arrow's (and its nested arrows') free idents relative to an
        // empty bound set — i.e. every outer name it reads.
        collect_free_expr(e, &HashSet::new(), out);
        return;
    }
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. }
        | HirExprKind::Assign {
            target: lhs,
            value: rhs,
        }
        | HirExprKind::AssignOp {
            target: lhs,
            value: rhs,
            ..
        }
        | HirExprKind::Index {
            object: lhs,
            index: rhs,
        } => {
            collect_arrow_frees_expr(lhs, out);
            collect_arrow_frees_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Member {
            object: operand, ..
        }
        | HirExprKind::Cast { expr: operand, .. }
        | HirExprKind::Await(operand)
        | HirExprKind::Spread(operand)
        | HirExprKind::PreInc(operand)
        | HirExprKind::PreDec(operand)
        | HirExprKind::PostInc(operand)
        | HirExprKind::PostDec(operand) => collect_arrow_frees_expr(operand, out),
        HirExprKind::Call { callee, args } => {
            collect_arrow_frees_expr(callee, out);
            args.iter().for_each(|a| collect_arrow_frees_expr(a, out));
        }
        HirExprKind::MethodCall { object, args, .. } => {
            collect_arrow_frees_expr(object, out);
            args.iter().for_each(|a| collect_arrow_frees_expr(a, out));
        }
        HirExprKind::New { args, .. } => args.iter().for_each(|a| collect_arrow_frees_expr(a, out)),
        HirExprKind::Ternary { cond, then, else_ } => {
            collect_arrow_frees_expr(cond, out);
            collect_arrow_frees_expr(then, out);
            collect_arrow_frees_expr(else_, out);
        }
        HirExprKind::Array(elems) => elems
            .iter()
            .for_each(|el| collect_arrow_frees_expr(el, out)),
        HirExprKind::Object(fields) => fields
            .iter()
            .for_each(|(_, v)| collect_arrow_frees_expr(v, out)),
        HirExprKind::Seq(items) => items
            .iter()
            .for_each(|it| collect_arrow_frees_expr(it, out)),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Free-variable collection over the lowering subset.
// ---------------------------------------------------------------------------

pub fn collect_free_stmt(s: &HirStmt, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
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
        HirStmt::Throw(e) => collect_free_expr(e, bound, free),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            for st in body {
                collect_free_stmt(st, bound, free);
            }
            if let Some(c) = catch {
                // The catch param binds inside the handler only; snapshot/restore
                // `bound` so it doesn't leak out (a `console` AFTER the try still
                // counts as free).
                let had = c.binding.as_ref().map(|b| bound.contains(b));
                if let Some(b) = &c.binding {
                    bound.insert(b.clone());
                }
                for st in &c.body {
                    collect_free_stmt(st, bound, free);
                }
                if let (Some(b), Some(false)) = (&c.binding, had) {
                    bound.remove(b);
                }
            }
            if let Some(fin) = finally {
                for st in fin {
                    collect_free_stmt(st, bound, free);
                }
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_free_stmt(i, bound, free);
            }
            if let Some(c) = cond {
                collect_free_expr(c, bound, free);
            }
            if let Some(u) = update {
                collect_free_expr(u, bound, free);
            }
            for st in body {
                collect_free_stmt(st, bound, free);
            }
        }
        HirStmt::ForOf {
            binding,
            iterable,
            body,
            ..
        } => {
            collect_free_expr(iterable, bound, free);
            bound.insert(binding.clone());
            for st in body {
                collect_free_stmt(st, bound, free);
            }
        }
        HirStmt::ForIn {
            binding,
            object,
            body,
        } => {
            collect_free_expr(object, bound, free);
            bound.insert(binding.clone());
            for st in body {
                collect_free_stmt(st, bound, free);
            }
        }
        // Remaining kinds (Switch/Labeled/Break/Continue/Raw) carry no singleton
        // reference the promotion cares about; not descending is safe (the lowering
        // bails on an unsupported construct itself).
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
                    // ONE shared `bound` across the block so a `const`/`let` declared
                    // in an earlier statement is visible to a later one — cloning per
                    // statement would drop the binding and report the local as free.
                    for st in stmts {
                        collect_free_stmt(st, &mut inner_bound, free);
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
