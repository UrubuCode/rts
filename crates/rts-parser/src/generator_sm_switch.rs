//! `switch` as STATES of the generator state machine.
//!
//! ## Why it is here
//!
//! `switch` was the last statement form [`crate::generator_sm`]'s `lower_stmt`
//! answered with `None`, and `None` aborts the build and falls back to the EAGER
//! buffer — where a `yield` in VALUE position (`const a = yield x`) cannot be
//! expressed and reaches the lowering raw. A `switch` carrying a `yield` is
//! ordinary code, so that whole shape failed.
//!
//! ## The lowering
//!
//! JS evaluates the case tests IN ORDER against the discriminant, jumps to the
//! matching clause, and then runs FORWARD through the following clauses until a
//! `break` — fallthrough is the default, not the exception. Both halves are
//! modelled separately, which is what makes fallthrough fall out for free:
//!
//! ```text
//!   cur:     __sw_N = <discriminant>;      goto test_0
//!   test_i:  if (__sw_N === <test_i>) goto body_i else goto test_{i+1}
//!   test_last→ default's body, or `after` when there is no default
//!   body_i:  <clause i statements>;        goto body_{i+1}   ← fallthrough
//!   body_last→ after
//!   after:
//! ```
//!
//! The clause BODIES are chained in SOURCE order, so a `default:` written in the
//! middle falls through to the clause after it, exactly as JS does — while the
//! test chain skips it and only reaches it after every test failed.
//!
//! The discriminant is evaluated ONCE into a frame slot, so a side-effecting
//! discriminant runs once and its value survives a suspension inside a clause.
//! Comparison is `===` (JS `switch` uses SameValueZero-like strict equality).
//!
//! ## `break` vs `continue`
//!
//! A `switch` is a `break` target but NOT a `continue` target. It pushes a
//! [`TargetKind::Switch`] entry so a `break` inside a clause resolves to the
//! switch's `after`, while a `continue` looks THROUGH it to the enclosing loop
//! (see [`crate::generator_sm_loops::resolve`]). Without that distinction a
//! `while (…) { switch (…) { case 1: continue; } }` would leave the switch
//! instead of re-iterating — a silent change of meaning.
//!
//! ## What it declines
//!
//! A `yield` in the discriminant or in a case TEST — suspending in the middle of
//! choosing a branch is not a state boundary the machine has. Returns `None`,
//! which keeps today's behaviour.

use swc_ecma_ast::{BinExpr, BinaryOp, Expr, SwitchStmt};

use crate::generator_sm::{SmBuilder, assign_stmt, expr_has_yield, ident_expr, sp};
use crate::generator_sm_loops::{LoopTarget, TargetKind};

impl SmBuilder {
    /// `switch (D) { case A: … default: … }`.
    pub(crate) fn lower_switch(&mut self, sw: &SwitchStmt, cur: usize) -> Option<usize> {
        let label = self.pending_label.take();
        if expr_has_yield(&sw.discriminant) {
            return None;
        }
        if sw
            .cases
            .iter()
            .any(|c| c.test.as_deref().is_some_and(expr_has_yield))
        {
            return None;
        }

        let uid = self.tmp_counter;
        self.tmp_counter += 1;
        let disc = format!("__sw_{uid}");
        self.intern_local(&disc);
        self.push_stmt(cur, assign_stmt(&disc, (*sw.discriminant).clone()));

        // One entry state per clause, in SOURCE order (the fallthrough chain),
        // plus the exit. Reserved up front so both chains can point at them.
        let bodies: Vec<usize> = sw.cases.iter().map(|_| self.new_state()).collect();
        let after = self.new_state();
        if bodies.is_empty() {
            self.set_goto(cur, after);
            return Some(after);
        }

        // ---- the TEST chain: `case` clauses in order, then `default` ----------
        let default_target = sw
            .cases
            .iter()
            .position(|c| c.test.is_none())
            .map(|i| bodies[i])
            .unwrap_or(after);
        let mut chain: Vec<(usize, Expr)> = Vec::new();
        for (i, c) in sw.cases.iter().enumerate() {
            if let Some(t) = c.test.as_deref() {
                chain.push((bodies[i], t.clone()));
            }
        }
        let mut entry = cur;
        for (idx, (body, test)) in chain.iter().enumerate() {
            // The last test's failure edge goes to `default` (or the exit); every
            // other one goes to a fresh state holding the next test.
            let els = if idx + 1 == chain.len() {
                default_target
            } else {
                self.new_state()
            };
            self.set_cond(entry, strict_eq(&disc, test.clone()), *body, els);
            entry = els;
        }
        if chain.is_empty() {
            self.set_goto(cur, default_target);
        }

        // ---- the BODY chain: clause i falls through to clause i+1 ------------
        self.loop_stack.push(LoopTarget {
            kind: TargetKind::Switch,
            label,
            brk: after,
            // A switch is never a `continue` target; `resolve` looks past it.
            cont: usize::MAX,
            try_depth: self.try_depth,
        });
        let mut ok = true;
        for (i, c) in sw.cases.iter().enumerate() {
            match self.lower_seq(&c.cons, bodies[i]) {
                Some(exit) => {
                    let next = bodies.get(i + 1).copied().unwrap_or(after);
                    self.set_goto(exit, next);
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        self.loop_stack.pop();
        ok.then_some(after)
    }
}

/// `<disc> === <test>` — JS `switch` matches with strict equality.
fn strict_eq(disc: &str, test: Expr) -> Expr {
    Expr::Bin(BinExpr {
        span: sp(),
        op: BinaryOp::EqEqEq,
        left: Box::new(ident_expr(disc)),
        right: Box::new(test),
    })
}
