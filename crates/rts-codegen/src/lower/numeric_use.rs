//! Which names a body uses where a number is coerced anyway.
//!
//! # Why a syntactic pre-pass and not a decision at the operation
//!
//! Because WHERE a guard goes decides whether it can fall at all, and the operation is in
//! the wrong place to decide it. `lower/claim.rs` guards at the use, which works until the
//! use is inside a loop: a guard in a loop header is not in the entry block, so
//! `rts_mir::lower` refuses its side exit and the whole function is turned away.
//!
//! Measured rather than reasoned. After operation-level speculation landed, `bench/`'s
//! largest refusal became `NeedsSideExit` at 125 — and reading one graph showed why:
//!
//! ```text
//! b0(v0, v1):  jump b1(v0)
//! b1(v3):      v4 = guard IsDouble of v3 else p0     ← in the header
//!              v5 = guard IsDouble of v1 else p1     ← loop-INVARIANT, and still here
//! ```
//!
//! Both belong in `b0`. The second is invariant and could simply move. The first cannot —
//! `v3` is the loop's own parameter — but it does not need to: guard `v0` at the entry and
//! the back edge carries a subtraction's result, which is already a double, so the
//! header's join proves `Double` and no guard is wanted there at all.
//!
//! So the answer is to guard the PARAMETER at the entry, and the question this file
//! answers is which parameters are worth it.
//!
//! # Why "used where a number is coerced" and not "every parameter"
//!
//! Guarding every parameter would be sound — a fall is always correct — and pessimal:
//! `function f(s) { return s.length; }` would fall on every call and run the generic body
//! anyway, having paid for a check first. A parameter earns its guard by appearing where
//! the specification coerces to a number, because there the guard is the case the
//! operation is FOR rather than a guess about the program.
//!
//! # Why this must not depend on a type
//!
//! The answer is used to mint deoptimisation points, and a point's number has to be the
//! SAME in both tiers — a fall from point three lands at point three. The two tiers infer
//! different types, so a decision that consulted one would number the bodies differently.
//!
//! Syntax is the same in both. That is the whole reason this reads the tree and not the
//! lattice, and `guard::pair` is what would catch it if it ever stopped.

use std::collections::BTreeSet;

use crate::emit::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::names::Name;
use crate::syntax::{BinaryOp, Expr, ExprKind, Function, FunctionBody, Stmt, UnaryOp};

/// Whether this operator coerces its operands to numbers whatever they are.
///
/// The same list `lower/push.rs` speculates on, and it is stated there as the primitive
/// rows. Here it is the SYNTAX, because this pass runs before anything is lowered — which
/// is two spellings of one rule and the place it would drift. Held to by the test that
/// counts guards on one fixture both ways.
fn coerces(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual
    )
}

/// Every name this function's body reads where a number is coerced anyway.
///
/// Descends into nested functions, because a name they read is this function's binding and
/// guarding it here still proves it there. Does not try to tell which binding a name is —
/// that is the resolver's, and a name shadowed inside is at worst a parameter guarded for
/// nothing, which costs one check on a path that was going to coerce regardless.
pub(super) fn coerced_names(function: &Function) -> BTreeSet<Name> {
    let mut found = BTreeSet::new();
    match &function.body {
        FunctionBody::Expression(expr) => in_expr(expr, &mut found),
        FunctionBody::Block(statements) => {
            for statement in statements {
                in_stmt(statement, &mut found);
            }
        }
    }
    found
}

fn in_stmt(statement: &Stmt, found: &mut BTreeSet<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => in_stmt(inner, found),
        StmtChild::Expr(expr) => in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(init) = &binding.value {
                in_expr(init, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                in_stmt(inner, found);
            }
        }
        StmtChild::Function(nested) => found.extend(coerced_names(nested)),
        StmtChild::Class(_) => {}
    });
}

fn in_expr(expr: &Expr, found: &mut BTreeSet<Name>) {
    // THE OPERANDS OF A COERCING OPERATOR, taken here and not by the walk below: the walk
    // reports children without saying which position they are in, and the position is the
    // whole question.
    match &expr.kind {
        ExprKind::Binary { op, left, right } if coerces(*op) => {
            named(left, found);
            named(right, found);
        }
        // `-x` and `+x` coerce, and they are the two unaries that do. `+` is ToNumber
        // spelled short, which the tree's own comment on it says. `!x` reads truth and
        // `typeof x` reads nothing.
        ExprKind::Unary {
            op: UnaryOp::Negate | UnaryOp::Plus,
            operand,
        } => named(operand, found),
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => in_expr(inner, found),
        Child::Function(nested) => found.extend(coerced_names(nested)),
        Child::Class(_) => {}
    });
}

/// The name an expression is, where it is one.
fn named(expr: &Expr, found: &mut BTreeSet<Name>) {
    if let ExprKind::Ident(name) = &expr.kind {
        found.insert(*name);
    }
}
