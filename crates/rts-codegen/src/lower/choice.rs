//! A choice: `?:` and the three short-circuiting operators.
//!
//! One file because they are one shape with two knobs — what the condition is, and
//! what the arm that does NOT evaluate the right side answers. Four copies of a join
//! is where three of them stop agreeing.
//!
//! The knobs, and each is a semantic rather than a style:
//!
//! - `a ? b : c` tests the truth of `a`, and neither arm answers `a`.
//! - `a && b` tests the truth of `a`, and the false arm answers **`a` itself** and
//!   not `false`: `0 && 1` is `0` and `"" && 1` is `""`.
//! - `a || b` is the same with the arms exchanged.
//! - `a ?? b` does not test truth at all. It asks whether `a` is null or undefined,
//!   so `0 ?? 1` is `0` where `0 || 1` is `1` — which is the whole reason the
//!   operator exists, and the reason it needs a row of its own rather than `Truthy`.

use rts_mir::Domain;
use rts_mir::cfg::{Terminator, ValueId};

use super::{Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::syntax::{Expr, LogicalOp};

/// What one arm of a choice answers.
pub(super) enum Arm<'a> {
    /// Evaluate this expression — the arm that runs the right side.
    Eval(&'a Expr),
    /// Answer the value the condition was asked about.
    ///
    /// Not a boolean: `0 && 1` is `0`, so the arm that short-circuited answers the
    /// SUBJECT. A lowering that answered `false` here would be wrong for every falsy
    /// value that is not `false`, which is five of the seven.
    Subject(ValueId),
}

impl Lowering<'_> {
    /// A conditional expression.
    pub(super) fn conditional(
        &mut self,
        condition: &Expr,
        when_true: &Expr,
        when_false: &Expr,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let held = self.expression(condition)?;
        let tested = self.prim(JsPrim::Truthy, vec![held], at);
        self.choice(tested, Arm::Eval(when_true), Arm::Eval(when_false))
    }

    /// A short-circuiting operator.
    pub(super) fn logical(
        &mut self,
        op: LogicalOp,
        left: &Expr,
        right: &Expr,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let held = self.expression(left)?;
        match op {
            LogicalOp::And => {
                let tested = self.prim(JsPrim::Truthy, vec![held], at);
                self.choice(tested, Arm::Eval(right), Arm::Subject(held))
            }
            LogicalOp::Or => {
                let tested = self.prim(JsPrim::Truthy, vec![held], at);
                self.choice(tested, Arm::Subject(held), Arm::Eval(right))
            }
            // NOT a truth test, which is this operator's whole point.
            LogicalOp::Coalesce => {
                let tested = self.prim(JsPrim::IsNullish, vec![held], at);
                self.choice(tested, Arm::Eval(right), Arm::Subject(held))
            }
        }
    }

    /// A branch whose two arms answer a value, joined into one.
    ///
    /// The value arrives at the join as a block PARAMETER, which is the same
    /// mechanism [`Lowering::branch`] uses for the bindings two statement arms
    /// disagree about — one join, two shapes of question. Its type is the domain's
    /// join of the two arms, which is the one thing a lattice is for.
    fn choice(
        &mut self,
        tested: ValueId,
        when_true: Arm<'_>,
        when_false: Arm<'_>,
    ) -> Result<ValueId, Unsupported> {
        let then_block = self.builder.block();
        let else_block = self.builder.block();
        let join = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block,
            then_args: Vec::new(),
            else_block,
            else_args: Vec::new(),
        });

        self.builder.switch_to(then_block);
        let from_then = self.arm(when_true)?;
        let then_exit = self.builder.current();

        self.builder.switch_to(else_block);
        let from_else = self.arm(when_false)?;
        let else_exit = self.builder.current();

        let held = self.builder.param(join);
        let of = self
            .domain
            .join(&self.type_of(from_then), &self.type_of(from_else));
        self.types.insert(held, of);

        // The jumps are written AFTER both arms are lowered, because an arm that
        // nests another choice moves where building is — `builder.current()` is what
        // says where each one actually ended, and using the block it started in would
        // terminate the wrong one.
        self.builder.switch_to(then_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: vec![from_then],
        });
        self.builder.switch_to(else_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: vec![from_else],
        });
        self.builder.switch_to(join);
        Ok(held)
    }

    fn arm(&mut self, which: Arm<'_>) -> Result<ValueId, Unsupported> {
        match which {
            Arm::Eval(expr) => self.expression(expr),
            Arm::Subject(held) => Ok(held),
        }
    }
}
