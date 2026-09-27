//! `a op= b` and `a &&= b` / `a ||= b` / `a ??= b`: an assignment that reads its
//! target first.
//!
//! The target is evaluated ONCE -- `o[k()] += 1` calls `k` a single time, and the
//! tree carries the operator rather than a rewrite to `o[k()] = o[k()] + 1` for that
//! reason -- so the object and the key become values, and the place is read and
//! written through them. `places.rs` does the same for `++`/`--`.
//!
//! A LOGICAL assignment writes on one path only: `o.x ||= f()` neither calls `f` nor
//! runs a setter when `o.x` is truthy. So it is a `choice` (`choice.rs`) whose writing
//! arm evaluates the value and stores it, and whose other arm answers what was read --
//! merged at the join like any assignment inside a branch.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::names::Name;
use crate::syntax::{BinaryOp, Expr, ExprKind, LogicalOp};

/// Where a read-then-write assignment stores, once its object and key are values.
pub(super) enum Store {
    /// A binding.
    Name(Name),
    /// `o.k`: the object and the key's constant.
    Field(ValueId, ValueId),
    /// `o[k]`: the object and the key's value.
    Element(ValueId, ValueId),
}

impl Lowering<'_> {
    /// `place op= value`.
    pub(super) fn compound_assign(
        &mut self,
        op: BinaryOp,
        place: &Expr,
        value: &Expr,
        expr: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let Some(prim) = super::named::primitive(op) else {
            return Err(Unsupported::Operator(op));
        };
        let (store, held) = self.place_read(place)?;
        let with = self.expression(value)?;
        let answered = self.prim(prim, vec![held, with], expr);
        self.store(&store, answered, expr)?;
        Ok(answered)
    }

    /// `place &&= value`, `place ||= value`, `place ??= value`.
    pub(super) fn logical_assign(
        &mut self,
        op: LogicalOp,
        place: &Expr,
        value: &Expr,
        expr: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let (store, held) = self.place_read(place)?;
        use super::choice::Arm;
        match op {
            LogicalOp::And => {
                let tested = self.prim(JsPrim::Truthy, vec![held], expr);
                self.choice(tested, Arm::Store(value, store), Arm::Subject(held))
            }
            LogicalOp::Or => {
                let tested = self.prim(JsPrim::Truthy, vec![held], expr);
                self.choice(tested, Arm::Subject(held), Arm::Store(value, store))
            }
            LogicalOp::Coalesce => {
                let tested = self.prim(JsPrim::IsNullish, vec![held], expr);
                self.choice(tested, Arm::Store(value, store), Arm::Subject(held))
            }
        }
    }

    /// The place `target` names, its object and key evaluated, and what it holds now.
    fn place_read(&mut self, target: &Expr) -> Result<(Store, ValueId), Unsupported> {
        match &target.kind {
            // `(k as any) ||= v` writes `k`: a type assertion is not a different place.
            ExprKind::Asserted { value, .. } => self.place_read(value),
            ExprKind::Ident(name) => Ok((Store::Name(*name), self.expression(target)?)),
            ExprKind::Member {
                object,
                property,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.domain.constant(JsConst::Key(*property));
                let key = self.declared(key, target);
                let held = self.prim(JsPrim::FieldRead, vec![receiver, key], target);
                Ok((Store::Field(receiver, key), held))
            }
            ExprKind::Index {
                object,
                index,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.expression(index)?;
                let held = self.prim(JsPrim::IndexRead, vec![receiver, key], target);
                Ok((Store::Element(receiver, key), held))
            }
            _ => Err(Unsupported::Expression(
                "a compound assignment to something that is not a place",
            )),
        }
    }

    /// Writes `value` where `store` says.
    pub(super) fn store(&mut self, store: &Store, value: ValueId, at: &Expr) -> Result<(), Unsupported> {
        match store {
            Store::Name(name) => {
                let of = self.type_of(value);
                self.bind(*name, value, of, at)
            }
            Store::Field(receiver, key) => {
                self.prim(JsPrim::FieldWrite, vec![*receiver, *key, value], at);
                Ok(())
            }
            Store::Element(receiver, key) => {
                self.prim(JsPrim::IndexWrite, vec![*receiver, *key, value], at);
                Ok(())
            }
        }
    }
}
