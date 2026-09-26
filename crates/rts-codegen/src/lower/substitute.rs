//! A call by name, replaced by the body it would run.
//!
//! # Whose proof this is
//!
//! Not this file's. `emit/inline.rs` decides, over the whole program, which function
//! a name is and that its body is one expression reading only its parameters and
//! names declared once program-wide -- so the body means the same thing written at
//! the call site as it does in its own. That proof is the running emitter's and is
//! kept there; the door hands the lowering what it proved as a [`Substitute`], and
//! this file only does the substitution: the arguments evaluated in order, each
//! parameter bound to its argument's VALUE, the body lowered with those bindings in
//! force, and no call.
//!
//! # Why the parameters are values and not names
//!
//! Because the body is lowered in the caller's scope, a parameter looked up by name
//! there would find the caller's binding of that spelling. The bindings in force are
//! a map from spelling to value consulted before any scope is asked, and only
//! while the body is lowered -- the arguments were lowered before it went in, so an
//! argument naming the same spelling reads the caller's.
//!
//! # What is refused, which is a real call rather than an error
//!
//! A spread argument, whose count only the run time knows; a callee already being
//! substituted further out, which is a cycle; and a callee name the function being
//! lowered binds itself, which is some other function than the one proved.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::names::Name;
use crate::syntax::{Expr, Spreadable};

/// A function a call by name may be replaced by.
#[derive(Clone, Debug)]
pub struct Substitute {
    /// Its parameters, in order.
    pub parameters: Vec<Name>,
    /// The one expression it answers.
    pub body: Expr,
}

impl Lowering<'_> {
    /// `name(arguments)` as its body, or `None` where it stays a call.
    pub(super) fn substituted(
        &mut self,
        name: Name,
        arguments: &[Spreadable],
    ) -> Result<Option<ValueId>, Unsupported> {
        let shadowed = self
            .resolution
            .binding_in(self.scope, name)
            .is_some_and(|held| self.declared_in_this_function(held));
        if shadowed || self.substituting.iter().any(|(held, _)| *held == name)
        {
            return Ok(None);
        }
        let Some(substitute) = self.callees.substitute(name).cloned() else {
            return Ok(None);
        };
        if arguments.iter().any(|held| matches!(held, Spreadable::Spread(_))) {
            return Ok(None);
        }
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Spreadable::Single(value) = argument else {
                return Ok(None);
            };
            values.push(self.expression(value)?);
        }
        let mut bound = std::collections::BTreeMap::new();
        for (at, parameter) in substitute.parameters.iter().enumerate() {
            let value = match values.get(at) {
                Some(held) => *held,
                None => self.singleton_at(crate::values::Singleton::Undefined, &substitute.body),
            };
            bound.insert(*parameter, value);
        }
        self.substituting.push((name, bound));
        let answered = self.expression(&substitute.body);
        self.substituting.pop();
        answered.map(Some)
    }

    /// A spelling the body being substituted binds, as the value it was bound to.
    pub(super) fn substituted_name(&self, name: Name) -> Option<ValueId> {
        self.substituting.last()?.1.get(&name).copied()
    }
}
