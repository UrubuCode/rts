//! Function-DECLARATION hoisting — a pre-pass over a function/module body.
//!
//! JS hoists a `function f(){…}` DECLARATION fully — both the NAME and its
//! VALUE — to the top of the enclosing scope, so a call `f()` textually BEFORE
//! the declaration resolves. This differs from `var` hoisting (name only,
//! value `undefined` until the assignment) and from `const f = () => …` /
//! `const f = function g(){}` (a plain binding, NOT hoisted).
//!
//! rts-hir lowers a nested `function f(){…}` declaration to
//! `HirStmt::Let { name: f, init: Arrow { self_name: Some(f), … } }` (see
//! `lower_decl`'s `Decl::Fn` arm) — the marker `self_name == name` identifies a
//! DECLARATION (a named fn-EXPRESSION assigned to a `const`/`let` lowers to a
//! `Const`, or to a `Let` whose `self_name != name`).
//!
//! ## Only FORWARD references are hoisted, and only as far as needed
//! Moving every declaration to the very top would place it BEFORE a `let`/`const`
//! it captures (the arrow-extraction then sees an undeclared capture and bails).
//! So this pass hoists a declaration ONLY when it is referenced by a statement
//! that comes BEFORE it, and moves it to just BEFORE that first referencing
//! statement — never earlier. In a VALID program every variable the function
//! captures is already declared before that first use (reaching it earlier would
//! be a TDZ error), so the moved declaration never jumps ahead of a live capture.
//! A declaration with no forward reference is left exactly where it was — the
//! common `function f(){…}; … f()` case is untouched.
//!
//! Scope: only DIRECT body statements are considered (a function declared inside
//! a nested `{ }` block is block-scoped in a strict/module context — TS is always
//! strict). Nested fn/arrow bodies are separate `HirFunc`s handled on their own.

use rts_hir::HirExpr;
use rts_hir::ir::{HirExprKind, HirStmt};

/// The declaration NAME of a direct function DECLARATION (`function f(){…}`),
/// identified by the `self_name == name` marker rts-hir stamps on the lowered
/// arrow. `None` for any other statement (incl. `const f = () => …`).
fn fn_decl_name(s: &HirStmt) -> Option<&str> {
    if let HirStmt::Let {
        name,
        init: Some(init),
        ..
    } = s
    {
        if let HirExprKind::Arrow {
            self_name: Some(sn),
            ..
        } = &init.kind
        {
            if sn == name {
                return Some(name);
            }
        }
    }
    None
}

/// Hoist each FORWARD-referenced function declaration to just before its first
/// referencing statement. Leaves every other statement (incl. declarations used
/// only after their line) exactly in place.
pub(crate) fn hoist_fn_decls(body: &mut Vec<HirStmt>) {
    // Positions of the direct fn declarations, by name.
    let decls: Vec<(usize, String)> = body
        .iter()
        .enumerate()
        .filter_map(|(i, s)| fn_decl_name(s).map(|n| (i, n.to_string())))
        .collect();
    if decls.is_empty() {
        return;
    }

    // For each declaration, the FIRST statement index that references its name.
    // A declaration whose first reference is BEFORE it must be hoisted to that
    // index; one referenced only after its line (or not at all) stays put
    // (`target == None`). Reference scan ignores the declaration's OWN statement
    // (self-recursion is not a forward reference).
    let hoist_target: Vec<Option<usize>> = decls
        .iter()
        .map(|(decl_i, name)| {
            (0..*decl_i)
                .find(|&j| stmt_references(&body[j], name))
                .filter(|&first| first < *decl_i)
        })
        .collect();
    if hoist_target.iter().all(Option::is_none) {
        return;
    }

    // Rebuild the body in one pass: at each position, emit every to-be-hoisted
    // declaration whose target is this position (in declaration source order),
    // then the position's own statement UNLESS it is a declaration already
    // hoisted here. A stable, index-shift-free reconstruction.
    let decl_at: std::collections::HashMap<usize, usize> = decls
        .iter()
        .enumerate()
        .map(|(k, (i, _))| (*i, k))
        .collect();
    let mut out: Vec<HirStmt> = Vec::with_capacity(body.len());
    let taken = std::mem::take(body);
    // Move the declarations out so we can re-place them; keep others in `slots`.
    let mut slots: Vec<Option<HirStmt>> = taken.into_iter().map(Some).collect();
    for pos in 0..slots.len() {
        // Emit any hoisted declaration targeted at `pos`, in source order.
        for (k, (decl_i, _)) in decls.iter().enumerate() {
            if hoist_target[k] == Some(pos) {
                if let Some(stmt) = slots[*decl_i].take() {
                    out.push(stmt);
                }
            }
        }
        // Emit this position's own statement unless it was a hoisted declaration
        // already emitted above.
        let is_hoisted_decl = decl_at
            .get(&pos)
            .is_some_and(|&k| hoist_target[k].is_some());
        if !is_hoisted_decl {
            if let Some(stmt) = slots[pos].take() {
                out.push(stmt);
            }
        }
    }
    *body = out;
}

/// Whether statement `s` references the identifier `name` anywhere in its
/// expressions or nested statement bodies (NOT descending into a nested fn/arrow
/// body — that is a separate scope whose free `name` is resolved as a capture, not
/// a same-body reference that would force a hoist).
fn stmt_references(s: &HirStmt, name: &str) -> bool {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => expr_references(e, name),
        HirStmt::Return(Some(e)) => expr_references(e, name),
        HirStmt::Return(None) | HirStmt::Break(_) | HirStmt::Continue(_) | HirStmt::Raw(_) => false,
        HirStmt::Let { init, .. } => init.as_ref().is_some_and(|e| expr_references(e, name)),
        HirStmt::Const { init, .. } => expr_references(init, name),
        HirStmt::If { cond, then, else_ } => {
            expr_references(cond, name)
                || then.iter().any(|s| stmt_references(s, name))
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(|s| stmt_references(s, name)))
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_references(cond, name) || body.iter().any(|s| stmt_references(s, name))
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|s| stmt_references(s, name))
                || cond.as_ref().is_some_and(|e| expr_references(e, name))
                || update.as_ref().is_some_and(|e| expr_references(e, name))
                || body.iter().any(|s| stmt_references(s, name))
        }
        HirStmt::ForOf { iterable, body, .. } => {
            expr_references(iterable, name) || body.iter().any(|s| stmt_references(s, name))
        }
        HirStmt::ForIn { object, body, .. } => {
            expr_references(object, name) || body.iter().any(|s| stmt_references(s, name))
        }
        HirStmt::Block(b) => b.iter().any(|s| stmt_references(s, name)),
        HirStmt::Labeled { body, .. } => stmt_references(body, name),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            body.iter().any(|s| stmt_references(s, name))
                || catch
                    .as_ref()
                    .is_some_and(|c| c.body.iter().any(|s| stmt_references(s, name)))
                || finally
                    .as_ref()
                    .is_some_and(|f| f.iter().any(|s| stmt_references(s, name)))
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            expr_references(discriminant, name)
                || cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(|e| expr_references(e, name))
                        || c.body.iter().any(|s| stmt_references(s, name))
                })
        }
    }
}

/// Whether expression `e` references the identifier `name`. Descends into every
/// sub-expression BUT NOT into a nested `Arrow` body (a separate scope). A bare
/// `Ident(name)` — a call target, an argument, an operand — counts.
fn expr_references(e: &HirExpr, name: &str) -> bool {
    use HirExprKind::*;
    match &e.kind {
        Ident(n) => n == name,
        Lit(_) | Raw(_) => false,
        // A nested arrow/fn body is a separate scope — do not descend (a free
        // `name` there is a capture, resolved once the value is bound, not a
        // same-body forward reference).
        Arrow { .. } => false,
        Bin { lhs, rhs, .. }
        | Index {
            object: lhs,
            index: rhs,
        }
        | Assign {
            target: lhs,
            value: rhs,
        }
        | AssignOp {
            target: lhs,
            value: rhs,
            ..
        } => expr_references(lhs, name) || expr_references(rhs, name),
        Unary { operand, .. }
        | Cast { expr: operand, .. }
        | Member { object: operand, .. }
        | Spread(operand)
        | PreInc(operand)
        | PreDec(operand)
        | PostInc(operand)
        | PostDec(operand)
        | Await(operand) => expr_references(operand, name),
        Call { callee, args } => {
            expr_references(callee, name) || args.iter().any(|a| expr_references(a, name))
        }
        MethodCall { object, args, .. } => {
            expr_references(object, name) || args.iter().any(|a| expr_references(a, name))
        }
        New { args, .. } => args.iter().any(|a| expr_references(a, name)),
        Array(els) => els.iter().any(|a| expr_references(a, name)),
        Object(fields) => fields.iter().any(|(_, v)| expr_references(v, name)),
        Ternary { cond, then, else_ } => {
            expr_references(cond, name)
                || expr_references(then, name)
                || expr_references(else_, name)
        }
        Seq(es) => es.iter().any(|e| expr_references(e, name)),
    }
}
