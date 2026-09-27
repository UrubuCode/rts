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
        let mut pairs = Vec::with_capacity(properties.len() * 2);
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
        Ok(self.prim(JsPrim::NewObject, pairs, expr))
    }
}

/// Whether a literal holds anything but values under written names -- a method, an
/// accessor, a spread, a computed key, a prototype -- which this stage does not build.
///
/// One answer for the three places that ask: the scope tree, which counts what such a
/// literal reads as a helper's; the door, which compiles that helper; and this lowering.
pub(crate) fn built_elsewhere(properties: &[Property]) -> bool {
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
