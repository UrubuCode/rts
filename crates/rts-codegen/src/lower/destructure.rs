//! An object pattern, read as the property reads it is.
//!
//! # Why the array form is refused and this one is not
//!
//! `const { a } = o` reads the property `a` of `o`. That is all it is, and it is what
//! this file lowers.
//!
//! `const [a] = xs` is **not** `a = xs[0]`. Array destructuring steps the ITERATOR
//! protocol: it reads `xs[Symbol.iterator]`, calls it, and calls `next()` once per
//! element — so it works on a `Set`, on a generator and on anything with a `next`,
//! and it does not work on an object with numeric keys and no iterator. A lowering
//! that indexed would be wrong in both directions at once: it would accept what the
//! language refuses and refuse what the language accepts.
//!
//! So the array form keeps its refusal under the same name as `for`-`of` — an
//! iteration protocol — which is the piece that answers both.
//!
//! # What a default is, exactly
//!
//! `undefined` specifically, and not absent and not falsy. The tree's own comment
//! says it: `[a = 1] = [null]` binds `null`, because `null` is a value that was
//! there. And the default is evaluated only when it is needed, so `{ a = f() }` over
//! an object that has `a` never calls `f` — which is why it is a branch and not an
//! argument to a coalescing operation.

use super::choice::Arm;
use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::names::Name;
use crate::syntax::{Expr, Pattern, PropertyKey};
use rts_mir::cfg::ValueId;

impl Lowering<'_> {
    /// Binds every name an object pattern names, from a value already lowered.
    ///
    /// Answers the names it bound, so a caller that needs to know what a declaration
    /// introduced does not have to walk the pattern a second time.
    pub(super) fn destructure(
        &mut self,
        pattern: &Pattern,
        from: ValueId,
        at: &Expr,
    ) -> Result<Vec<Name>, Unsupported> {
        let Pattern::Object(object) = pattern else {
            return match pattern {
                Pattern::Array(_) => Err(Unsupported::Expression(
                    "an array pattern steps the iteration protocol, which is not indexing",
                )),
                _ => Err(Unsupported::Pattern),
            };
        };
        if object.rest.is_some() {
            // A rest target collects the own enumerable properties NOT already named,
            // which needs the key set of the object at run time -- an operation this
            // table does not have, and not one an ordinary read can stand in for.
            return Err(Unsupported::Expression(
                "an object rest target needs the own keys at run time",
            ));
        }

        let mut bound = Vec::with_capacity(object.properties.len());
        for property in &object.properties {
            let PropertyKey::Named(key) = &property.key else {
                return Err(Unsupported::Expression(
                    "a computed key in a pattern is a value, so which property is read is not written",
                ));
            };
            let index = self.domain.constant(JsConst::Key(*key));
            let named = self.declared(index, at);
            let read = self.prim(JsPrim::FieldRead, vec![from, named], at);

            // THE DEFAULT IS A BRANCH, because it runs only when the value read was
            // `undefined` -- so `{ a = f() }` over an object that has `a` must not
            // call `f`. A coalescing operation would also be wrong for `null`, which
            // takes no default.
            let held = match &property.value.default {
                None => read,
                Some(default) => {
                    let undefined = self.singleton_at(crate::values::Singleton::Undefined, at);
                    let absent = self.prim(JsPrim::StrictEquals, vec![read, undefined], at);
                    self.choice(absent, Arm::Eval(default), Arm::Subject(read))?
                }
            };

            let Pattern::Name(name) = &property.value.pattern else {
                return Err(Unsupported::Expression(
                    "a nested pattern needs the value read to be destructured again",
                ));
            };
            let of = self.type_of(held);
            let target = Expr {
                kind: crate::syntax::ExprKind::Ident(*name),
                at: at.at,
            };
            self.bind(*name, held, of, &target)?;
            bound.push(*name);
        }
        Ok(bound)
    }
}
