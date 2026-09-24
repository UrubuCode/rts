//! Where a captured binding lives, said in the graph.
//!
//! # What was here before, and why it could not reach the machine
//!
//! A binding declared outside the function being lowered was an operation that named
//! the BINDING and left where it lived to "the machine" -- and the declaring function
//! kept the same binding in SSA, because nothing told it a closure would ask. So the
//! two ends of one capture disagreed about where it was, and the boundary was asked a
//! question -- how many `__rts_outer` links to walk -- that only the scope tree could
//! answer. `names::resolve::captured` answers it now, and this file turns the answer
//! into operations over an ordinary object.
//!
//! # The layout
//!
//! An activation that owns a captured binding builds one environment at its entry,
//! linked to the environment it was made in, with every captured binding it owns
//! defined in it. Every read and write of such a binding goes through the
//! environment -- in the owner too, because a closure may have written it since. An
//! activation that owns none holds the environment it was made in, when anything
//! inside it reaches past it, and nothing otherwise.
//!
//! # What is refused, and each is a different missing piece
//!
//! - a captured binding declared in a scope that is fresh per loop pass: one slot per
//!   activation would make every closure see the last pass's value, which is the
//!   divergence the running engine closed with an environment per iteration;
//! - two captured bindings of one activation with one spelling: the environment is
//!   keyed by spelling, so they would be one slot, and the running engine answers that
//!   with an environment per block;
//! - a function expression's own name, captured: nothing binds it in this lowering,
//!   so the environment would hold `undefined` where the language holds the function.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::names::resolve::{BindingId, Origin};
use crate::syntax::Expr;

impl Lowering<'_> {
    /// Builds this activation's environment, or takes the one it was made in, and
    /// moves every captured parameter into it.
    ///
    /// AFTER the parameter guards and not before them, because building one
    /// allocates and a guard behind an allocation is no longer at a point the other
    /// tier can be entered from -- `rts_mir::lower` refuses it by position.
    pub(super) fn open_environment(&mut self, at: &Expr) -> Result<(), Unsupported> {
        let owned = self.resolution.environment_of(self.function);
        let reaches = self.resolution.reaches_out(self.function);
        if owned.is_empty() {
            // UNDER ANOTHER STAGE'S LAYOUT a closure made here is handed the environment
            // this function was made in, always: the running emitter emits the closure's
            // body and may read through it for a reason the scope walk here does not see.
            if reaches || self.outer.is_some() {
                self.environment = Some(self.prim(JsPrim::EnclosingEnvironment, vec![], at));
            }
            return Ok(());
        }

        if self.outer.is_some() {
            return Err(Unsupported::Shape(
                "a function building its own environment under another stage's layout",
            ));
        }
        let mut keys = Vec::with_capacity(owned.len());
        let mut spelled = std::collections::BTreeSet::new();
        for binding in &owned {
            let record = self.resolution.binding(*binding);
            if self.resolution.per_pass(record.scope) {
                return Err(Unsupported::Shape(
                    "a captured binding declared inside a loop is one per pass, and this environment holds one per activation",
                ));
            }
            if record.origin == Origin::OwnName {
                return Err(Unsupported::Shape(
                    "a function expression's own name, captured, is bound by nothing in this lowering",
                ));
            }
            if !spelled.insert(record.name) {
                return Err(Unsupported::Shape(
                    "two captured bindings of one activation share a spelling, and an environment is keyed by spelling",
                ));
            }
            let key = self.domain.constant(JsConst::Key(record.name));
            keys.push(self.declared(key, at));
        }

        let enclosing = self.prim(JsPrim::EnclosingEnvironment, vec![], at);
        let mut operands = vec![enclosing];
        operands.extend(keys);
        let built = self.prim(JsPrim::EnvNew, operands, at);
        self.environment = Some(built);

        // THE PARAMETERS ARRIVED IN REGISTERS, and were held there while the guards
        // ran. A captured one moves now, so that every later read -- this function's
        // and a closure's -- goes to one place.
        for binding in owned {
            if self.resolution.binding(binding).origin != Origin::Parameter {
                continue;
            }
            if let Some(value) = self.values.remove(&binding) {
                self.env_write(binding, value, at)?;
            }
        }
        Ok(())
    }

    /// The environment that owns a captured binding, seen from here, and its key.
    fn env_slot(
        &mut self,
        binding: BindingId,
        at: &Expr,
    ) -> Result<(ValueId, ValueId), Unsupported> {
        let record = self.resolution.binding(binding);
        let (scope, name) = (record.scope, record.name);
        // A BINDING SOMEBODY ELSE LAID OUT is read where they put it. The per-pass
        // refusal below does not apply: the running emitter builds the environment
        // per pass, and its count of links already says which one this closure holds.
        if let Some(outer) = self.outer
            && self.resolution.owner(scope) != self.function
        {
            let Some((hops, key)) = outer(name) else {
                return Err(Unsupported::Expression(
                    "the enclosing layout does not hold this binding in an environment",
                ));
            };
            let Some(mut environment) = self.environment else {
                return Err(Unsupported::Expression(
                    "a captured binding with no environment in force, which the scope walk should have made impossible",
                ));
            };
            for _ in 0..hops {
                environment = self.prim(JsPrim::EnvOuter, vec![environment], at);
            }
            let key = self.domain.constant(JsConst::Key(key));
            let key = self.declared(key, at);
            return Ok((environment, key));
        }
        if self.resolution.per_pass(scope) {
            return Err(Unsupported::Shape(
                "a captured binding declared inside a loop is one per pass, and this environment holds one per activation",
            ));
        }
        // BOTH ARE INVARIANTS of the scope walk rather than gaps: a function that
        // reaches a captured binding holds an environment, and the binding's owner
        // encloses every use that reached it. A refusal keeps a broken invariant from
        // becoming a read of some other activation's variable.
        let (Some(mut environment), Some(reach)) = (
            self.environment,
            self.resolution.hops(self.function, binding),
        ) else {
            return Err(Unsupported::Expression(
                "a captured binding with no environment in force, which the scope walk should have made impossible",
            ));
        };
        for _ in 0..reach.hops {
            environment = self.prim(JsPrim::EnvOuter, vec![environment], at);
        }
        let key = self.domain.constant(JsConst::Key(name));
        let key = self.declared(key, at);
        Ok((environment, key))
    }

    /// A captured binding, read.
    pub(super) fn env_read(
        &mut self,
        binding: BindingId,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let (environment, key) = self.env_slot(binding, at)?;
        Ok(self.prim(JsPrim::EnvRead, vec![environment, key], at))
    }

    /// A captured binding, written.
    pub(super) fn env_write(
        &mut self,
        binding: BindingId,
        value: ValueId,
        at: &Expr,
    ) -> Result<(), Unsupported> {
        let (environment, key) = self.env_slot(binding, at)?;
        self.prim(JsPrim::EnvWrite, vec![environment, key, value], at);
        Ok(())
    }
}

#[cfg(test)]
#[path = "../lower_environment_tests.rs"]
mod tests;
