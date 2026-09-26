//! `Math.sqrt(x)` as the instruction, where the program leaves `Math` alone.
//!
//! # Whose proof, and what is decided here
//!
//! That `Math` still means the language's `Math` is a whole-program fact the running
//! emitter establishes (`emit/primordial.rs`: no write to it, no `eval`, no
//! `globalThis`), and the door hands it over as one flag. What is decided here is only
//! the other half, which is local: the name is not bound by anything the function
//! sees -- its own scope, or the layout it was made in.
//!
//! # Why `ToNumber` first, where the running emitter asks for a proven double
//!
//! Because that IS what each of these does to its argument, so `sqrt(ToNumber(x))` is
//! the definition rather than a fast path with a condition. The running emitter
//! required the operand to be proven a double already and made the call otherwise; a
//! generic operand here pays the conversion -- which the call would have paid too --
//! and none of the call. A proven one pays nothing: `ToNumber` of a number is erased.
//!
//! `Math.random()` is no instruction, and takes the entry point the running emitter
//! takes for it, skipping the property read and the generic call.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::domain::JsConst;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal, Spreadable, UnaryOp};

impl Lowering<'_> {
    /// `Math.f(...)` as an operation, or `None` where it stays a call.
    pub(super) fn intrinsic(
        &mut self,
        callee: &Expr,
        arguments: &[Spreadable],
        at: &Expr,
    ) -> Result<Option<ValueId>, Unsupported> {
        if !self.callees.math_primordial() {
            return Ok(None);
        }
        let ExprKind::Member {
            object,
            property,
            optional: false,
        } = &callee.kind
        else {
            return Ok(None);
        };
        let ExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if self.names.spelled(*name) != Some("Math")
            || self.resolution.binding_in(self.scope, *name).is_some()
            || self.outer.is_some_and(|outer| outer(*name).is_some())
        {
            return Ok(None);
        }
        let op = match self.names.spelled(*property) {
            Some("random") if arguments.is_empty() => {
                return Ok(Some(self.entry(crate::runtime::RuntimeOp::MathRandom, Vec::new(), at)));
            }
            Some("sqrt") => JsPrim::MathSqrt,
            Some("floor") => JsPrim::MathFloor,
            Some("ceil") => JsPrim::MathCeil,
            Some("trunc") => JsPrim::MathTrunc,
            Some("abs") => JsPrim::MathAbs,
            // Two written arguments, each through `ToNumber` in source order,
            // which is what the runtime's fold does to them before comparing.
            Some("min") | Some("max") if arguments.len() == 2 => {
                let op = match self.names.spelled(*property) {
                    Some("min") => JsPrim::MathMin,
                    _ => JsPrim::MathMax,
                };
                let mut numbers = Vec::with_capacity(2);
                for argument in arguments {
                    let Spreadable::Single(argument) = argument else {
                        return Ok(None);
                    };
                    let value = self.expression(argument)?;
                    numbers.push(self.prim(JsPrim::ToNumber, vec![value], at));
                }
                return Ok(Some(self.prim(op, numbers, at)));
            }
            _ => return Ok(None),
        };
        let [Spreadable::Single(only)] = arguments else {
            return Ok(None);
        };
        let value = self.expression(only)?;
        let number = self.prim(JsPrim::ToNumber, vec![value], at);
        Ok(Some(self.prim(op, vec![number], at)))
    }

    /// `typeof x === "name"` (or `==`, `!==`, `!=`, either side) as `TypeOfIs`, which
    /// compares against the literal by its index and builds no string -- the one
    /// crossing `emit/settled.rs` makes where the plain form makes two and a string.
    /// `typeof` has one answer type, so the loose forms are the strict ones.
    pub(super) fn typeof_is(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let negated = matches!(op, BinaryOp::StrictNotEqual | BinaryOp::LooseNotEqual);
        let (operand, text) = match (&left.kind, &right.kind) {
            (
                ExprKind::Unary {
                    op: UnaryOp::TypeOf,
                    operand,
                },
                ExprKind::Literal(Literal::String(text)),
            )
            | (
                ExprKind::Literal(Literal::String(text)),
                ExprKind::Unary {
                    op: UnaryOp::TypeOf,
                    operand,
                },
            ) => (operand, text),
            _ => return Err(Unsupported::Expression("a typeof comparison of another shape")),
        };
        let value = self.expression(operand)?;
        let index = self.domain.constant(JsConst::LiteralIndex(text.clone()));
        let index = self.declared(index, at);
        let is = self.entry(crate::runtime::RuntimeOp::TypeOfIs, vec![value, index], at);
        Ok(match negated {
            true => self.prim(JsPrim::Not, vec![is], at),
            false => is,
        })
    }
}

/// Whether a binary expression is `typeof x` compared by equality with a string
/// literal -- the shape [`Lowering::typeof_is`] takes.
pub(super) fn compares_typeof(op: BinaryOp, left: &Expr, right: &Expr) -> bool {
    let equality = matches!(
        op,
        BinaryOp::StrictEqual | BinaryOp::LooseEqual | BinaryOp::StrictNotEqual | BinaryOp::LooseNotEqual
    );
    let typeof_of = |held: &Expr| matches!(held.kind, ExprKind::Unary { op: UnaryOp::TypeOf, .. });
    let text = |held: &Expr| matches!(held.kind, ExprKind::Literal(Literal::String(_)));
    equality && ((typeof_of(left) && text(right)) || (text(left) && typeof_of(right)))
}
