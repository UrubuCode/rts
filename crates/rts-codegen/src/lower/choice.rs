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
    /// Evaluate this expression and store it -- the writing arm of a logical
    /// assignment (`compound.rs`), which answers what it wrote.
    Store(&'a Expr, super::compound::Store),
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
    pub(super) fn choice(
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

        // EACH ARM FROM THE SAME MAP, and what they disagree about merged at the join
        // -- `branch.rs`'s reason, for an expression: `c ? (x = 1) : 2` writes `x` on
        // one path only. Lowering the second arm over the first one's map answered
        // `x === 1` on BOTH paths, silently, and once the constant moved it was a
        // value that does not dominate the join, which the verifier refused.
        let before = self.values.clone();
        self.builder.switch_to(then_block);
        let from_then = self.arm(when_true)?;
        let mut then_values = std::mem::replace(&mut self.values, before);
        let then_exit = self.builder.current();

        self.builder.switch_to(else_block);
        let from_else = self.arm(when_false)?;
        let mut else_values = std::mem::take(&mut self.values);
        let else_exit = self.builder.current();

        self.settle_one_sided(&mut then_values, &else_values, then_exit)?;
        self.settle_one_sided(&mut else_values, &then_values, else_exit)?;
        let merged: Vec<crate::names::resolve::BindingId> = then_values
            .iter()
            .filter(|(binding, held)| else_values.get(binding).is_some_and(|other| other != *held))
            .map(|(binding, _)| *binding)
            .collect();

        let held = self.builder.param(join);
        let of = self
            .domain
            .join(&self.type_of(from_then), &self.type_of(from_else));
        self.types.insert(held, of);
        let mut params = Vec::with_capacity(merged.len());
        for binding in &merged {
            let param = self.builder.param(join);
            let of = self.domain.join(
                &self.type_of(then_values[binding]),
                &self.type_of(else_values[binding]),
            );
            self.types.insert(param, of);
            params.push(param);
        }

        // The jumps are written AFTER both arms are lowered, because an arm that
        // nests another choice moves where building is — `builder.current()` is what
        // says where each one actually ended, and using the block it started in would
        // terminate the wrong one.
        self.builder.switch_to(then_exit);
        let mut args = vec![from_then];
        args.extend(merged.iter().map(|binding| then_values[binding]));
        self.builder.end(Terminator::Jump { target: join, args });
        self.builder.switch_to(else_exit);
        let mut args = vec![from_else];
        args.extend(merged.iter().map(|binding| else_values[binding]));
        self.builder.end(Terminator::Jump { target: join, args });

        self.values = then_values;
        for (binding, param) in merged.iter().zip(params) {
            self.values.insert(*binding, param);
        }
        self.builder.switch_to(join);
        Ok(held)
    }

    fn arm(&mut self, which: Arm<'_>) -> Result<ValueId, Unsupported> {
        match which {
            Arm::Eval(expr) => self.expression(expr),
            Arm::Subject(held) => Ok(held),
            Arm::Store(value, store) => {
                let held = self.expression(value)?;
                self.store(&store, held, value)?;
                Ok(held)
            }
        }
    }
}
