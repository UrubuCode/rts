//! Which locals hold a 32-bit integer, everywhere they hold anything.
//!
//! # What this buys, measured before it was written
//!
//! `a = ~a` in a loop compiles to three machine operations — read the double as
//! an integer, exclusive-or, write it back as a double — and only the middle one
//! is the program. Measured 2026-08-25, `bench/analytic.ts` under
//! `target/release/rts.exe` on a Ryzen 7 5700G at 4.35 GHz (230 ps a cycle):
//! **3.26 ns, which is 14.2 cycles**, against 0.93 for the empty loop.
//!
//! The machine is not slow, and that was checked rather than assumed. The same
//! operator across 4 INDEPENDENT accumulators costs 1.17 ns each — 4× the work
//! for 1.42× the time — so its *throughput* is about 2 cycles. What costs 14 is
//! that a loop carries its accumulator, so each pass waits for the one before,
//! and the two conversions on that chain each cross the FP↔integer register
//! file. Removing them leaves a chain of one operation, whose latency then hides
//! under the loop's own back edge — which is already what happens to `float
//! add`, a 3-cycle instruction that measures at the empty loop's floor.
//!
//! # Why this is an analysis and not an annotation
//!
//! The same reason [`super::proven`] is, and rule 4: a type annotation is
//! evidence, not proof. Nothing a program declares can contradict what the
//! function itself does to a binding.
//!
//! # The rule
//!
//! A local holds an int32 when it is **already proven numeric**, its initialiser
//! produces an int32, and every assignment to it produces one. "Produces an
//! int32" is a property of the OPERATOR and not of its operands: `&`, `|`, `^`,
//! `<<`, `>>` and `~` answer a value in `[-2^31, 2^31)` whatever they are given,
//! which is what makes this decidable without a fixpoint.
//!
//! `>>>` is not among them, and that is the one exclusion worth stating: its
//! result is `ToUint32`, so `-1 >>> 0` is 4 294 967 295, which is not an int32.
//! `rts-cranelift`'s `Inst::ToF64` documents the same boundary from the machine
//! side.
//!
//! # No fixpoint, and what that costs
//!
//! [`super::proven`] needs one because "numeric" for one local depends on
//! another — `let a = 1; let b = a;`. This does not, because the int32-ness of
//! `a & 255` does not depend on the int32-ness of `a`. The price is exactly that
//! case: `let b = a;` with `a` an int32 leaves `b` a plain double. It is a real
//! gap and it is deliberate, because closing it means iterating to convergence
//! for a shape that does not appear in the loops this exists for.
//!
//! # Why proving it is only half, and the half that was learned the hard way
//!
//! `proven.rs` records what happens when an analysis and an emission disagree:
//! the emitter widens at every store, the operand is never in the representation
//! the fast path looks for, and the fast path never runs. Two tables, one answer.
//!
//! So this pass is spent in exactly two places, both in `expr.rs`, and neither
//! touches how a bitwise operator is emitted:
//!
//! - `stored` converts an int32-proven binding's value with `to_int32`;
//! - `binding::read` converts back with `to_f64`, so a READ of a binding never
//!   answers `Repr::I32` and nothing downstream had to learn a third case.
//!
//! Those two look like they cancel, and for a non-bitwise use they do — one
//! conversion each way, at the boundary, outside the loop. Inside one they
//! vanish: the machine folds `ToInt32(ToF64(x))` to `x`, so the read's widening
//! meets the operator's narrowing and both are gone before lowering. That fold
//! is `rts_cranelift::ir::fold::to_int32_answer`, and it is why this pass needed
//! no change to `proven_instruction` at all.
//!
//! # What the reuse check found
//!
//! `proven.rs` already answers "which locals hold a number", by a fixpoint over
//! this same tree, using `capture::walk_stmt` to traverse it. Nothing answers
//! "which hold an int32". This pass reuses that traversal and that predicate —
//! [`super::proven::is_numeric`] is called, not restated — and adds only the
//! question the other one does not ask.

use std::collections::{BTreeSet, HashSet};

use super::capture::{self, StmtChild};
use super::proven::{Numeric, is_numeric};
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, Literal, Pattern, Stmt, StmtKind, UnaryOp,
};

/// The locals a function body only ever puts 32-bit integers in.
#[derive(Default, Debug, Clone)]
pub struct Int32 {
    names: HashSet<Name>,
}

impl Int32 {
    /// Whether this binding's machine representation is the integer one.
    pub fn holds_int32(&self, name: Name) -> bool {
        self.names.contains(&name)
    }
}

/// Which locals hold an int32, given what is already known to hold a number.
///
/// Takes the FINISHED [`Numeric`] rather than computing beside it, because
/// every question this asks about an operand is one that pass already answers,
/// and asking it mid-fixpoint would read a set still shrinking.
pub(super) fn analyse(
    body: &[Stmt],
    numeric: &Numeric,
    captured: &BTreeSet<Name>,
) -> Int32 {
    let mut candidates = HashSet::new();
    for statement in body {
        collect(statement, numeric, &mut candidates);
    }
    // A captured name lives in an environment OBJECT, whose properties hold
    // tagged values. Widening an `I32` produces a tagged INTEGER where every
    // reader of a number expects a tagged double, so a captured binding in this
    // representation reads back as `NaN` — which is what
    // `rts-host/tests/running.rs`'s
    // `a_function_reads_a_variable_from_where_it_was_written` found, on
    // `let k = 4; function get() { return k; }`.
    //
    // `proven::Numeric` does NOT exclude these, and the first version of this
    // pass assumed it did. It declines to prove a READ of a captured local
    // numeric, which is a different sentence from declining to prove the
    // binding — and `let k = 4` is proved.
    candidates.retain(|name| !captured.contains(name));
    // One removal pass, and one is enough for the same reason there is no
    // fixpoint: what disqualifies a name is a store to it, and whether a store
    // qualifies never depends on which names survived.
    let mut rejected = HashSet::new();
    for statement in body {
        reject(statement, numeric, &candidates, &mut rejected);
    }
    Int32 {
        names: candidates.difference(&rejected).copied().collect(),
    }
}

/// Every block-scoped local whose initialiser produces an int32.
///
/// `var` is excluded for the reason `proven::collect_candidates` states at
/// length and does not need restating here: a hoisted `var` is `undefined` at
/// every point before its line, so a merge sees two representations meeting at
/// one block parameter — the `ImplicitNarrowing` the verifier is right to
/// refuse.
///
/// Requiring the name to be proven numeric is not belt-and-braces. It is what
/// makes the conversion in `stored` legal at all: an unproven name is widened at
/// every store, so its binding is `Repr::Tagged` and `to_int32` would be a
/// narrowing without a guard.
fn collect(statement: &Stmt, numeric: &Numeric, into: &mut HashSet<Name>) {
    if let StmtKind::Declare { kind, bindings } = &statement.kind
        && kind.is_block_scoped()
    {
        for binding in bindings {
            if let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value)
                && numeric.holds_number(*name)
                && produces_int32(value, numeric)
            {
                into.insert(*name);
            }
        }
    }
    capture::walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => collect(inner, numeric, into),
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                collect(inner, numeric, into);
            }
        }
        // A `for` header's own `let`, which is a declaration this pass wants
        // exactly as much as a statement-level one.
        StmtChild::Binding(binding) => {
            if let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value)
                && numeric.holds_number(*name)
                && produces_int32(value, numeric)
            {
                into.insert(*name);
            }
        }
        // A nested function's locals are its own, and descending would risk
        // taking a shadowing declaration for this body's. A name it SHARES with
        // this body is a captured one, and `analyse` removes every captured name
        // from the candidates — which is where that exclusion lives, in one
        // place, rather than being assumed here.
        StmtChild::Expr(_) | StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// Every candidate an assignment disqualifies.
///
/// Collected as a rejection set rather than by removing from the candidates in
/// place, because the walk reaches a name's assignments in source order and a
/// name assigned before its declaration is read (a `var`-like use of a `let` in
/// a closure) would otherwise be judged against a set that did not hold it yet.
fn reject(
    statement: &Stmt,
    numeric: &Numeric,
    candidates: &HashSet<Name>,
    into: &mut HashSet<Name>,
) {
    capture::walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => reject(inner, numeric, candidates, into),
        StmtChild::Expr(expr) => disqualify(expr, numeric, candidates, into),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                disqualify(value, numeric, candidates, into);
            }
        }
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                reject(inner, numeric, candidates, into);
            }
        }
        // A nested function can only reach a name of this body by capturing it,
        // and `analyse` has already removed every captured name — so there is
        // nothing here left to take away.
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// Whether one expression stores something that is not an int32.
fn disqualify(
    expr: &Expr,
    numeric: &Numeric,
    candidates: &HashSet<Name>,
    into: &mut HashSet<Name>,
) {
    if let ExprKind::Assign { target, value, op } = &expr.kind
        && let AssignTarget::Place(place) = target
        && let ExprKind::Ident(name) = &place.kind
        && candidates.contains(name)
    {
        // A compound assignment is its operator applied to the binding, so
        // `a &= 3` produces an int32 and `a += 1` does not. Spelled by asking
        // the same question of the operator, so the two forms cannot drift.
        let stays = match op {
            AssignOp::Plain => produces_int32(value, numeric),
            AssignOp::Compound(binary) => operator_produces_int32(*binary),
            AssignOp::Logical(_) => false,
        };
        if !stays {
            into.insert(*name);
        }
    }
    // The update of `i++` is an addition, which is not an int32 operation.
    if let ExprKind::Update { target, .. } = &expr.kind
        && let ExprKind::Ident(name) = &target.kind
    {
        into.insert(*name);
    }
    capture::walk_expr(expr, &mut |child| {
        if let super::capture::Child::Expr(inner) = child {
            disqualify(inner, numeric, candidates, into);
        }
    });
}

/// Whether an expression certainly produces a value in `[-2^31, 2^31)`.
fn produces_int32(expr: &Expr, numeric: &Numeric) -> bool {
    match &expr.kind {
        // A literal that IS one. `-0` is excluded by the round trip rather than
        // by a case of its own: it is not equal to `0 as i32 as f64` under the
        // comparison used here only if that comparison distinguishes them, so
        // the sign bit is tested explicitly.
        ExprKind::Literal(Literal::Number(n)) => is_int32_double(*n),

        // The operators whose RESULT is an int32 whatever the operands were.
        // Each still asks `is_numeric` of both sides, and that is not about the
        // result: it is about whether the fast path is the one emitted. Where it
        // is not, `emit_binary` answers a runtime call and a `Repr::Tagged`
        // value, and claiming an int32 for it is the false claim `proven.rs`
        // records paying for three times.
        ExprKind::Binary { op, left, right } => {
            operator_produces_int32(*op)
                && is_numeric(left, numeric)
                && is_numeric(right, numeric)
        }

        // `~x` is the same claim with one operand.
        ExprKind::Unary {
            op: UnaryOp::BitNot,
            operand,
        } => is_numeric(operand, numeric),

        // `-1`, which the tree holds as a negation of `1` rather than as a
        // literal — there is no negative numeric literal in the grammar. Left
        // out at first, and `let a = -1` is how a bit-twiddling loop starts
        // often enough that leaving it out silently cost the whole optimisation
        // in exactly the programs it was written for.
        //
        // Asks about the NEGATED value rather than the operand, which is not
        // pedantry: `2147483648` is not an int32 and `-2147483648` is.
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => match &operand.kind {
            ExprKind::Literal(Literal::Number(n)) => is_int32_double(-*n),
            _ => false,
        },

        // Deliberately absent: `Ident`. See the module header — admitting it is
        // what would require a fixpoint, and the shape it buys (`let b = a;`)
        // is not the one this exists for.
        _ => false,
    }
}

/// Whether an operator's result is an int32 however it was reached.
///
/// `>>>` is absent and every other bitwise operator is present. Its result is
/// `ToUint32`, so `-1 >>> 0` is 4 294 967 295 — a number this representation
/// cannot hold, and the one case where "bitwise" and "int32" come apart.
fn operator_produces_int32(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr
    )
}

/// Whether a double is exactly a 32-bit integer, `-0` excluded.
///
/// `-0` is excluded because it is not `0` in this representation: `Object.is`
/// and `1 / x` both tell them apart, and narrowing to an integer loses the sign
/// permanently. It is the one value that passes the round trip below and must
/// still be refused, which is why the test is written beside it rather than
/// trusted to it.
fn is_int32_double(value: f64) -> bool {
    value == f64::from(value as i32) && !(value == 0.0 && value.is_sign_negative())
}
