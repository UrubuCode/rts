//! Pre-scan: numeric ACCUMULATOR locals that must bind as `Float64`.
//!
//! `let s = 0; for (const p of ps) { s = s + p.age; }` — the init proves `Int64`,
//! but a later accumulation folds in a heap-read number (`p.age`, a `number`
//! field, F64 at runtime). Binding `s` as `Int64` makes the assign's coerce
//! TRUNCATE every fractional carry (`72.7` became `72`). This scan walks a
//! function body BEFORE lowering and collects every local that is the target of
//! an arithmetic self-accumulation whose value tree contains a heap read
//! (member/index/call — a value the front cannot prove integral). The `let`
//! numeric tail then binds such a local as `Float64` from the start — JS numbers
//! are f64, so this is always semantically sound; only reprs that WOULD have
//! been `Int*` are widened (a string/object/Tagged binding is untouched).

use std::collections::HashSet;

use rts_hir::HirExpr;
use rts_hir::ir::{HirBinOp, HirExprKind, HirStmt};

/// Collect the names needing a `Float64` binding (see module doc).
pub(super) fn float_promoted_locals(body: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    walk_stmts(body, &mut out);
    out
}

fn walk_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        walk_stmt(s, out);
    }
}

fn walk_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Expr(e) => walk_expr(e, out),
        HirStmt::Let { init: Some(e), .. } => walk_expr(e, out),
        HirStmt::Const { init, .. } => walk_expr(init, out),
        HirStmt::Return(Some(e)) | HirStmt::Throw(e) => walk_expr(e, out),
        HirStmt::If { cond, then, else_ } => {
            walk_expr(cond, out);
            walk_stmts(then, out);
            if let Some(e) = else_ {
                walk_stmts(e, out);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            walk_expr(cond, out);
            walk_stmts(body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, out);
            }
            if let Some(c) = cond {
                walk_expr(c, out);
            }
            if let Some(u) = update {
                walk_expr(u, out);
            }
            walk_stmts(body, out);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            walk_expr(iterable, out);
            walk_stmts(body, out);
        }
        HirStmt::ForIn { object, body, .. } => {
            walk_expr(object, out);
            walk_stmts(body, out);
        }
        HirStmt::Block(stmts) => walk_stmts(stmts, out),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            walk_stmts(body, out);
            if let Some(c) = catch {
                walk_stmts(&c.body, out);
            }
            if let Some(f) = finally {
                walk_stmts(f, out);
            }
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            walk_expr(discriminant, out);
            for c in cases {
                if let Some(t) = &c.test {
                    walk_expr(t, out);
                }
                walk_stmts(&c.body, out);
            }
        }
        _ => {}
    }
}

fn walk_expr(e: &HirExpr, out: &mut HashSet<String>) {
    match &e.kind {
        // `s = s <op> …heapRead…` / `s <op>= …heapRead…` — the accumulator shape.
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if is_arith_self_accum(name, value) && tree_has_heap_read(value) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, out);
        }
        HirExprKind::AssignOp { op, target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if is_arith_op(*op) && tree_has_heap_read(value) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, out);
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr(lhs, out);
            walk_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. } => walk_expr(operand, out),
        HirExprKind::Call { callee, args } => {
            walk_expr(callee, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            walk_expr(cond, out);
            walk_expr(then, out);
            walk_expr(else_, out);
        }
        _ => {}
    }
}

/// `value` is an arithmetic tree that REFERENCES `name` (the `s = s + x` shape).
fn is_arith_self_accum(name: &str, value: &HirExpr) -> bool {
    match &value.kind {
        HirExprKind::Bin { op, lhs, rhs } if is_arith_op(*op) => {
            tree_refs_ident(name, lhs) || tree_refs_ident(name, rhs)
        }
        _ => false,
    }
}

fn tree_refs_ident(name: &str, e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Ident(n) => n == name,
        HirExprKind::Bin { lhs, rhs, .. } => {
            tree_refs_ident(name, lhs) || tree_refs_ident(name, rhs)
        }
        HirExprKind::Unary { operand, .. } => tree_refs_ident(name, operand),
        _ => false,
    }
}

/// The value tree contains a HEAP READ the front cannot prove integral (a
/// member/index read — a `number` field is F64 at runtime). A pure literal /
/// local-only tree stays out (int counters keep their unboxed `Int64`).
fn tree_has_heap_read(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Member { .. } | HirExprKind::Index { .. } => true,
        HirExprKind::Bin { lhs, rhs, .. } => tree_has_heap_read(lhs) || tree_has_heap_read(rhs),
        HirExprKind::Unary { operand, .. } => tree_has_heap_read(operand),
        _ => false,
    }
}

fn is_arith_op(op: HirBinOp) -> bool {
    matches!(
        op,
        HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div
    )
}
