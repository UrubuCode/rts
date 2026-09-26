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
        let ExprKind::Member {
            object,
            property,
            optional: false,
        } = &callee.kind
        else {
            return Ok(None);
        };
        if !self.is_math(object) {
            return Ok(None);
        }
        let name = self.names.spelled(*property).unwrap_or("");
        if name == "random" && arguments.is_empty() {
            return Ok(Some(self.entry(crate::runtime::RuntimeOp::MathRandom, Vec::new(), at)));
        }
        // Decided BEFORE any operand is lowered, so a member this does not take
        // leaves the graph exactly as it found it and the ordinary call lowers
        // the arguments itself, once.
        let shape = match (name, arguments.len()) {
            ("sqrt", 1) => Shape::Prim(JsPrim::MathSqrt),
            ("floor", 1) => Shape::Prim(JsPrim::MathFloor),
            ("ceil", 1) => Shape::Prim(JsPrim::MathCeil),
            ("trunc", 1) => Shape::Prim(JsPrim::MathTrunc),
            ("abs", 1) => Shape::Prim(JsPrim::MathAbs),
            ("round", 1) => Shape::Prim(JsPrim::MathRound),
            ("sign", 1) => Shape::Prim(JsPrim::MathSign),
            ("fround", 1) => Shape::Prim(JsPrim::MathFround),
            ("clz32", 1) => Shape::Prim(JsPrim::MathClz32),
            ("min", 2) => Shape::Prim(JsPrim::MathMin),
            ("max", 2) => Shape::Prim(JsPrim::MathMax),
            ("imul", 2) => Shape::Prim(JsPrim::MathImul),
            // `Math.max(x)` is `ToNumber(x)`: the fold over one operand is the
            // operand, and the conversion is what the argument pays anyway.
            ("min" | "max", 1) => Shape::Identity,
            (_, 1) => match crate::runtime::math_direct::unary_index(name) {
                Some(which) => Shape::Direct(crate::runtime::RuntimeOp::MathDirect1, which),
                None => return Ok(None),
            },
            (_, 2) => match crate::runtime::math_direct::binary_index(name) {
                Some(which) => Shape::Direct(crate::runtime::RuntimeOp::MathDirect2, which),
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        // Every operand in source order, each through `ToNumber`: that IS what
        // each member does to its argument, so a proven number pays nothing and
        // anything else pays the conversion the call would have paid too.
        let mut numbers = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Spreadable::Single(argument) = argument else {
                return Ok(None);
            };
            let value = self.expression(argument)?;
            numbers.push(self.prim(JsPrim::ToNumber, vec![value], at));
        }
        Ok(Some(match shape {
            Shape::Prim(op) => self.prim(op, numbers, at),
            Shape::Identity => numbers[0],
            Shape::Direct(door, which) => {
                let selector = self.domain.constant(JsConst::Count(which as u32));
                let selector = self.declared(selector, at);
                let mut args = vec![selector];
                args.extend(numbers);
                self.entry(door, args, at)
            }
        }))
    }

    /// `Math.PI` and its siblings as the number, under the same proof, or `None`
    /// for a read this does not decide. The table of values is `emit/math`'s,
    /// asked here so the two emitters cannot disagree about a digit.
    pub(super) fn math_constant(
        &mut self,
        object: &Expr,
        property: crate::names::Name,
        at: &Expr,
    ) -> Result<Option<ValueId>, Unsupported> {
        if !self.is_math(object) {
            return Ok(None);
        }
        let Some(spelled) = self.names.spelled(property) else {
            return Ok(None);
        };
        let Some(value) = crate::emit::math::constant_named(spelled) else {
            return Ok(None);
        };
        Ok(Some(self.literal(&Literal::Number(value), at)?))
    }

    /// Whether `object` is the language's `Math` here: the whole program leaves it
    /// alone (the running emitter's proof, handed over as one flag), and nothing
    /// this function sees binds the name — its own scope, or the layout it was
    /// made in.
    fn is_math(&self, object: &Expr) -> bool {
        if !self.callees.math_primordial() {
            return false;
        }
        let ExprKind::Ident(name) = &object.kind else {
            return false;
        };
        self.names.spelled(*name) == Some("Math")
            && self.resolution.binding_in(self.scope, *name).is_none()
            && !self.outer.is_some_and(|outer| outer(*name).is_some())
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

/// How a `Math` member is answered, decided before its operands are lowered.
enum Shape {
    /// A primitive of this language over the converted operands.
    Prim(JsPrim),
    /// The converted operand itself.
    Identity,
    /// A library door, by number.
    Direct(crate::runtime::RuntimeOp, i64),
}
