//! The operators whose operand is a PLACE rather than a value: `delete` and `++`/`--`.
//!
//! Lowering the operand as an expression would evaluate what is about to be removed or
//! stepped, and a property operand would be read twice. So the object -- and the key,
//! where it is computed -- is evaluated ONCE, and the place is read and written through
//! it, which is `emit/unary.rs`'s shape for both.

use rts_mir::cfg::{Const, Op, ValueId};
use rts_mir::{Domain as _, Effect};

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, UpdateOp, UpdatePosition};

impl Lowering<'_> {
    /// `delete operand`.
    ///
    /// A property is removed by the runtime, from its key as TEXT -- a written name and
    /// a computed key take the same path there. Anything else is not a removal at all:
    /// the operand is evaluated and the answer is `true`, except for a name that is
    /// declared, which cannot be deleted and answers `false`. A chain is refused: its
    /// short circuit decides whether there is a place, which is a join this does not
    /// build.
    pub(super) fn delete(&mut self, operand: &Expr, at: &Expr) -> Result<ValueId, Unsupported> {
        let (receiver, key) = match &operand.kind {
            ExprKind::Member {
                object,
                property,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let text = crate::syntax::Text::from_units(
                    self.names
                        .spelled(*property)
                        .ok_or(Unsupported::Expression("a property with no spelling"))?
                        .encode_utf16()
                        .collect::<Vec<u16>>(),
                );
                let key = self.domain.constant(JsConst::Text(text));
                (receiver, self.declared(key, at))
            }
            ExprKind::Index {
                object,
                index,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.expression(index)?;
                (receiver, key)
            }
            ExprKind::Chain(_) => {
                return Err(Unsupported::Expression(
                    "delete of an optional chain, whose place the short circuit decides",
                ));
            }
            _ => {
                let bound = matches!(&operand.kind, ExprKind::Ident(name)
                    if self.resolution.binding_in(self.scope, *name).is_some());
                self.expression(operand)?;
                return Ok(self.truth(!bound, at));
            }
        };
        Ok(self.entry(RuntimeOp::DeleteProperty, vec![receiver, key], at))
    }

    /// `++`/`--`, prefix or postfix, of a name, a property or an element.
    ///
    /// The old value is the COERCED one, so `s++` over `"5"` answers 5; the new one is
    /// that plus or minus one, written back where it was read.
    pub(super) fn update(
        &mut self,
        op: UpdateOp,
        position: UpdatePosition,
        target: &Expr,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        enum Place {
            Name(crate::names::Name),
            Field(ValueId, ValueId),
            Element(ValueId, ValueId),
        }
        let (place, held) = match &target.kind {
            ExprKind::Ident(name) => (Place::Name(*name), self.expression(target)?),
            ExprKind::Member {
                object,
                property,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.domain.constant(JsConst::Key(*property));
                let key = self.declared(key, target);
                let held = self.prim(JsPrim::FieldRead, vec![receiver, key], target);
                (Place::Field(receiver, key), held)
            }
            ExprKind::Index {
                object,
                index,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.expression(index)?;
                let held = self.prim(JsPrim::IndexRead, vec![receiver, key], target);
                (Place::Element(receiver, key), held)
            }
            _ => {
                return Err(Unsupported::Expression(
                    "an increment of something that is not a place",
                ));
            }
        };
        let before = self.prim(JsPrim::ToNumber, vec![held], at);
        let one = {
            let value = Const::Int(1);
            let of = self.domain.of_const(&value);
            let pushed = self.builder.push(Op::Const(value), Effect::PURE, at.at);
            self.types.insert(pushed, of);
            pushed
        };
        let after = match op {
            UpdateOp::Increment => self.prim(JsPrim::Add, vec![before, one], at),
            UpdateOp::Decrement => self.prim(JsPrim::Subtract, vec![before, one], at),
        };
        match place {
            Place::Name(name) => {
                let of = self.type_of(after);
                self.bind(name, after, of, at)?;
            }
            Place::Field(receiver, key) => {
                self.prim(JsPrim::FieldWrite, vec![receiver, key, after], at);
            }
            Place::Element(receiver, key) => {
                self.prim(JsPrim::IndexWrite, vec![receiver, key, after], at);
            }
        }
        Ok(match position {
            UpdatePosition::Prefix => after,
            UpdatePosition::Postfix => before,
        })
    }

    /// A truth value the lowering itself writes.
    fn truth(&mut self, value: bool, at: &Expr) -> ValueId {
        let held = Const::Bool(value);
        let of = self.domain.of_const(&held);
        let pushed = self.builder.push(Op::Const(held), Effect::PURE, at.at);
        self.types.insert(pushed, of);
        pushed
    }
}
