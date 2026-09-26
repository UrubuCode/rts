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
use crate::syntax::{Expr, ExprKind, Spreadable};

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
            _ => return Ok(None),
        };
        let [Spreadable::Single(only)] = arguments else {
            return Ok(None);
        };
        let value = self.expression(only)?;
        let number = self.prim(JsPrim::ToNumber, vec![value], at);
        Ok(Some(self.prim(op, vec![number], at)))
    }
}
