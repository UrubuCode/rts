//! Pre-scan: numeric locals that must bind as `Float64`.
//!
//! A `let` initialized with an integer-valued literal proves `Int64` (swc lowers
//! `0`/`0.0`/`20.0` — no fractional part — to an integer literal, see
//! `rts-hir::lower::lower_lit`). If that same local is later ASSIGNED a value the
//! front cannot prove is integral, binding it `Int64` makes the assign's coerce
//! TRUNCATE the fraction (`fcvt_to_sint_sat`), silently losing it. Two shapes hit
//! this:
//!
//!   1. accumulation of a heap-read number —
//!      `let s = 0; for (const p of ps) { s = s + p.age; }` (`p.age` is a
//!      `number` field, F64 at runtime; `72.7` became `72`);
//!   2. assignment of a fractional value —
//!      `let vy = 0.0; vy = vy - 0.272;` (the literal `0.272` is fractional; `vy`
//!      printed `0` instead of `-0.272` — issue #1869).
//!
//! This scan walks a function body BEFORE lowering and collects every local that
//! is the target of an assignment (`=` or `op=`) whose value tree REQUIRES float
//! — it contains a heap read (member/index the front cannot prove integral) OR a
//! genuinely fractional numeric literal. The `let` numeric tail then binds such a
//! local as `Float64` from the start. JS numbers are f64, so widening an
//! int-inferred local to `Float64` is ALWAYS semantically sound; only reprs that
//! WOULD have been `Int*` are widened (a string/object/Tagged binding is
//! untouched). A local only ever assigned integer-valued literals/locals is NOT
//! promoted — a Fibonacci-style `let a = 0.0; a = a + 1.0;` loop keeps its native
//! unboxed `Int64` fast path.

use std::collections::HashSet;

use rts_hir::HirExpr;
use rts_hir::ir::{HirBinOp, HirExprKind, HirLit, HirStmt};

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
        // `x = …` — promote `x` when the assigned value REQUIRES float: it folds
        // in a heap read (`s = s + p.age`, an F64 field) OR a genuinely fractional
        // literal (`vy = vy - 0.272`). Either would truncate against an `Int*`
        // slot. A pure integer-valued tree (`a = b + 1`) leaves `x` unpromoted.
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if tree_requires_float(value) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, out);
        }
        // `x op= …` — same rule; `op=` desugars to `x = x op value`, so a
        // fractional/heap-read RHS forces `x` to `Float64` (`vy -= 0.272`).
        HirExprKind::AssignOp { op, target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if is_arith_op(*op) && tree_requires_float(value) {
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

/// The value tree REQUIRES a float slot — the front cannot prove it integral, so
/// truncating it into an `Int*` slot would drop a fraction. True when the tree
/// contains EITHER:
///   - a HEAP READ (member/index — a `number` field is F64 at runtime), or
///   - a genuinely FRACTIONAL numeric literal (`2.7`, `0.272`; a `.fract() != 0`
///     `Number`/`Float`). Integer-valued literals (`1.0`, which swc already
///     lowered to `Int`; or a `Number(3.0)`) do NOT force a float — a pure
///     integer tree keeps its unboxed `Int64` fast path.
fn tree_requires_float(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Member { .. } | HirExprKind::Index { .. } => true,
        HirExprKind::Lit(HirLit::Number(v) | HirLit::Float(v)) => v.fract() != 0.0,
        HirExprKind::Bin { lhs, rhs, .. } => tree_requires_float(lhs) || tree_requires_float(rhs),
        HirExprKind::Unary { operand, .. } => tree_requires_float(operand),
        _ => false,
    }
}

fn is_arith_op(op: HirBinOp) -> bool {
    matches!(
        op,
        HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div
    )
}
