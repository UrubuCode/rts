//! Pre-scan: `Bool`-inferred locals that must bind as `Tagged`.
//!
//! Sibling of [`super::floatscan`], same shape, opposite direction: `floatscan`
//! WIDENS an int-inferred local that later receives a fractional value;
//! this one DEMOTES a bool-inferred local that later receives a value the front
//! cannot prove is a boolean.
//!
//! A `let` initialized with `true`/`false` (or a comparison) binds `Repr::Bool`.
//! If that same local is later ASSIGNED an arbitrary value, the assign's coerce
//! asks for `Bool` and there is no such coercion — the whole enclosing function
//! bailed with `cannot coerce Tagged to Bool` / `cannot coerce Int64 to Bool`.
//! Measured in a `__rtsn_ctor_*` on a real WhatsApp Web bundle; minimal form:
//!
//!   let b = false; b = o.x;            // `o.x` is Tagged
//!
//! and it is the ordinary shape of minified JS, where nothing is annotated and
//! every flag starts life as a literal `false`.
//!
//! **Applying `ToBoolean` at that assign would be WRONG, not merely lossy.** TS
//! types are erased and JS assignment does not convert: after `b = o.x` with
//! `o.x === 1`, Node has `b === 1` and `typeof b === "number"`. Coercing would
//! print `true` — a plausible wrong answer. The sound fix is representational:
//! the local was never provably a boolean, so it binds `Tagged` from the start
//! (`join(Bool, Tagged) = Tagged`, the Repr lattice's own rule). Demotion is
//! always semantically safe — it only gives up the unboxed slot.
//!
//! A local only ever assigned provable booleans (`b = true`, `b = x > 1`,
//! `b = !y`) is NOT demoted and keeps its native `Bool` fast path.

use std::collections::HashSet;

use rts_hir::HirExpr;
use rts_hir::ir::{HirBinOp, HirExprKind, HirLit, HirStmt, HirUnOp};

/// Collect the names that must NOT bind `Repr::Bool` (see module doc).
pub(super) fn bool_demoted_locals(body: &[HirStmt]) -> HashSet<String> {
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
        HirStmt::Labeled { body, .. } => walk_stmt(body, out),
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
        // `x = …` — demote `x` unless the assigned tree is PROVABLY boolean.
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if !tree_is_bool(value) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, out);
        }
        // `x op= …` — no compound operator yields a boolean (`&&=`/`||=`/`??=`
        // carry the OPERAND, which may be any value), so this always demotes.
        HirExprKind::AssignOp { target, value, .. } => {
            if let HirExprKind::Ident(name) = &target.kind {
                out.insert(name.clone());
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
        HirExprKind::Seq(items) => {
            for i in items {
                walk_expr(i, out);
            }
        }
        _ => {}
    }
}

/// The value tree provably evaluates to a JS boolean, so it can land in a `Bool`
/// slot verbatim. Deliberately SYNTACTIC and conservative — this runs before
/// lowering, and every "don't know" must answer `false` (demote), since a false
/// `true` here re-creates the bail this scan exists to remove.
///
/// `&&`/`||` are NOT boolean-producing in JS: they carry an OPERAND (`a || 0` is
/// `0`), so they only qualify when BOTH sides do.
fn tree_is_bool(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Lit(HirLit::Bool(_)) => true,
        HirExprKind::Unary {
            op: HirUnOp::Not, ..
        } => true,
        HirExprKind::Bin { op, lhs, rhs } => match op {
            HirBinOp::Eq
            | HirBinOp::Ne
            | HirBinOp::StrictEq
            | HirBinOp::StrictNe
            | HirBinOp::Lt
            | HirBinOp::Le
            | HirBinOp::Gt
            | HirBinOp::Ge
            | HirBinOp::In => true,
            HirBinOp::LogAnd | HirBinOp::LogOr => tree_is_bool(lhs) && tree_is_bool(rhs),
            _ => false,
        },
        // Both arms boolean ⇒ the ternary is boolean (`c ? true : x > 1`).
        HirExprKind::Ternary { then, else_, .. } => tree_is_bool(then) && tree_is_bool(else_),
        _ => false,
    }
}
