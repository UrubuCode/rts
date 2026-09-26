//! `a?.b`, `a?.[k]`, `a?.()` — an optional chain, as one join.
//!
//! `emit/optional.rs` is the definition this follows: ONE join for the whole chain, and
//! every link written `?.` tests its object for `null` and `undefined` -- the two
//! singletons, never `0` or `""` -- and leaves straight to that join with `undefined`.
//! A link inside the chain that carries no flag is skipped all the same, which is why
//! the join is the chain's and not the link's: `a?.b.c` does not read `.c` of nothing.
//! A nested `Chain` is walked with the SAME join, which is the bug `optional.rs` records
//! finding when the parser wraps every optional link in its own node.
//!
//! A chain that ASSIGNS a binding hands the join what each such binding holds on every
//! way in -- a skipped link leaves before an argument's `x = 1` ran, so `x` arrives
//! from there unchanged -- which is the merge `branch.rs` makes for an `if`. The
//! running emitter makes none, and its verifier refuses `o?.m(x = 1)` outright.

use rts_mir::cfg::{Callee, Terminator, ValueId};
use rts_mir::BlockId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::syntax::{Expr, ExprKind};
use crate::values::Singleton;

/// The chain's one join, and the bindings its links may leave holding different values.
struct Join {
    block: BlockId,
    carried: Vec<crate::names::resolve::BindingId>,
}

impl Lowering<'_> {
    /// The whole chain, answering its value or `undefined`.
    pub(super) fn chain(&mut self, inner: &Expr) -> Result<ValueId, Unsupported> {
        let assigned = self.assigned_in_expr(inner)?;
        let join = Join {
            block: self.builder.block(),
            carried: self.carried_now(assigned),
        };
        let value = self.link(inner, &join)?;
        let args = self.leaving(value, &join);
        self.builder.end(Terminator::Jump {
            target: join.block,
            args,
        });
        self.builder.switch_to(join.block);
        let answered = self.top_param(join.block);
        for binding in &join.carried {
            let param = self.top_param(join.block);
            self.values.insert(*binding, param);
        }
        Ok(answered)
    }

    /// What a way into the join hands it: the value, then each carried binding as it
    /// stands on that path.
    fn leaving(&self, value: ValueId, join: &Join) -> Vec<ValueId> {
        let mut args = vec![value];
        args.extend(join.carried.iter().map(|binding| self.values[binding]));
        args
    }

    /// One node of the chain's spine.
    fn link(&mut self, expr: &Expr, join: &Join) -> Result<ValueId, Unsupported> {
        match &expr.kind {
            ExprKind::Chain(inner) => self.link(inner, join),
            ExprKind::Member {
                object,
                property,
                optional,
            } => {
                let receiver = self.link(object, join)?;
                let receiver = self.short_circuit(receiver, *optional, join, expr);
                let key = self.domain.constant(JsConst::Key(*property));
                let key = self.declared(key, expr);
                Ok(self.prim(JsPrim::FieldRead, vec![receiver, key], expr))
            }
            ExprKind::Index {
                object,
                index,
                optional,
            } => {
                let receiver = self.link(object, join)?;
                let receiver = self.short_circuit(receiver, *optional, join, expr);
                let key = self.expression(index)?;
                Ok(self.prim(JsPrim::IndexRead, vec![receiver, key], expr))
            }
            ExprKind::Call {
                callee,
                arguments,
                optional,
            } => {
                let (receiver, function) = self.callee(callee, join)?;
                let function = self.short_circuit(function, *optional, join, expr);
                self.call_written(Callee::Dynamic(function), receiver, arguments, expr)
            }
            _ => self.expression(expr),
        }
    }

    /// What a call inside the chain calls, and what it calls it ON.
    fn callee(
        &mut self,
        callee: &Expr,
        join: &Join,
    ) -> Result<(Option<ValueId>, ValueId), Unsupported> {
        match &callee.kind {
            ExprKind::Chain(inner) => self.callee(inner, join),
            ExprKind::Member {
                object,
                property,
                optional,
            } => {
                let receiver = self.link(object, join)?;
                let receiver = self.short_circuit(receiver, *optional, join, callee);
                let key = self.domain.constant(JsConst::Key(*property));
                let key = self.declared(key, callee);
                let function = self.prim(JsPrim::FieldRead, vec![receiver, key], callee);
                Ok((Some(receiver), function))
            }
            ExprKind::Index {
                object,
                index,
                optional,
            } => {
                let receiver = self.link(object, join)?;
                let receiver = self.short_circuit(receiver, *optional, join, callee);
                let key = self.expression(index)?;
                let function = self.prim(JsPrim::IndexRead, vec![receiver, key], callee);
                Ok((Some(receiver), function))
            }
            _ => Ok((None, self.link(callee, join)?)),
        }
    }

    /// A link written `?.`: `null` or `undefined` leaves for the join with `undefined`.
    fn short_circuit(
        &mut self,
        value: ValueId,
        optional: bool,
        join: &Join,
        at: &Expr,
    ) -> ValueId {
        if !optional {
            return value;
        }
        let nullish = self.prim(JsPrim::IsNullish, vec![value], at);
        let nullish = self.prim(JsPrim::Truthy, vec![nullish], at);
        let absent = self.builder.block();
        let present = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: nullish,
            then_block: absent,
            then_args: Vec::new(),
            else_block: present,
            else_args: Vec::new(),
        });
        self.builder.switch_to(absent);
        let undefined = self.singleton_at(Singleton::Undefined, at);
        let args = self.leaving(undefined, join);
        self.builder.end(Terminator::Jump {
            target: join.block,
            args,
        });
        self.builder.switch_to(present);
        value
    }
}
