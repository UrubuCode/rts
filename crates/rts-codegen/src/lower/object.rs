//! An object literal, as pairs of a declared key and a value.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::syntax::{Expr, Property};

impl Lowering<'_> {
    /// AN OBJECT LITERAL, as pairs of a declared key and a value.
    ///
    /// In SOURCE ORDER, and that is load-bearing rather than tidy: the order
    /// properties are added is what decides the layout, which the tree's own
    /// comment on this node says, so reordering the pairs here would mint a
    /// different shape at run time and nothing would report it.
    ///
    /// No shape is asserted. `Type::Shaped` carries the reason — the shape
    /// tree that decides layouts is the RUNTIME's, and claiming a number only
    /// it mints is what this crate's rules 1 and 2 forbid by name.
    ///
    /// A method, a getter, a setter and a spread are each refused apart. A
    /// method is not a value under a key: it is installed with a home object,
    /// which is what `super.x` inside it reads from, and a function stored
    /// under a key has none. Collapsing the two would compile and would make
    /// `super` mean nothing.
    pub(super) fn object_literal(
        &mut self,
        properties: &[Property],
        expr: &Expr,
    ) -> Result<ValueId, Unsupported> {
        // UNDER ANOTHER STAGE'S LAYOUT, a literal this stage does not build is that
        // stage's, in a helper -- as a class is, and for the same reason: a method's home
        // object, an accessor and a spread are the running emitter's to install.
        if built_elsewhere(properties) && self.outer.is_some() {
            return self.helper_call(expr.at, expr);
        }
        let mut pairs = Vec::with_capacity(properties.len() * 2 + 1);
        // A literal that opens with its prototype is born under it: the
        // prototype is the first operand, evaluated first as it is written.
        let (under, properties) = match properties {
            [Property::Prototype(value), rest @ ..] => (true, {
                pairs.push(self.expression(value)?);
                rest
            }),
            _ => (false, properties),
        };
        for property in properties {
            let crate::syntax::Property::Value { key, value, .. } = property else {
                return Err(Unsupported::Expression(
                    "an object literal with a method, an accessor or a spread",
                ));
            };
            let crate::syntax::PropertyKey::Named(name) = key else {
                return Err(Unsupported::Expression(
                    "a computed key is a value, so the layout is not the one written",
                ));
            };
            let index = self.domain.constant(JsConst::Key(*name));
            pairs.push(self.declared(index, expr));
            pairs.push(self.expression(value)?);
        }
        let prim = match under {
            true => JsPrim::NewObjectUnder,
            false => JsPrim::NewObject,
        };
        Ok(self.prim(prim, pairs, expr))
    }
}

/// How many fields an object born under its prototype holds in its own cell --
/// the running emitter's `INLINE_FIELDS`, for the same literal.
const INLINE_FIELDS: usize = 15;

/// Whether a literal holds anything but values under written names -- a method, an
/// accessor, a spread, a computed key, a prototype -- which this stage does not build.
///
/// One answer for the three places that ask: the scope tree, which counts what such a
/// literal reads as a helper's; the door, which compiles that helper; and this lowering.
pub(crate) fn built_elsewhere(properties: &[Property]) -> bool {
    // A prototype written FIRST, over no more fields than a cell holds, is this
    // stage's: the object is born under it. Anywhere else it is a relink of an
    // object that already has properties, which is the running emitter's.
    let properties = match properties {
        [Property::Prototype(_), rest @ ..] if rest.len() <= INLINE_FIELDS => rest,
        _ => properties,
    };
    properties.iter().any(|property| {
        !matches!(
            property,
            Property::Value {
                key: crate::syntax::PropertyKey::Named(_),
                ..
            }
        )
    })
}
