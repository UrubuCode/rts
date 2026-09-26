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
//!   with an environment per block.
//!
//! # Under another stage's layout
//!
//! The environment built here is the running emitter's shape exactly -- an ordinary
//! object, a slot per spelling, the `__rts_outer` link -- so a function the running
//! emitter compiles inside this one reads it through one more layer of ITS scope, at
//! hops zero. `emit/through_mir.rs` builds that layer from the same list this one is
//! built from, which is what keeps the two agreeing about which names are in it.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::names::resolve::{BindingId, Origin, ScopeId};
use crate::syntax::Expr;

impl Lowering<'_> {
    /// Builds this activation's environment, or takes the one it was made in, and
    /// moves every captured parameter into it.
    ///
    /// AFTER the parameter guards and not before them, because building one
    /// allocates and a guard behind an allocation is no longer at a point the other
    /// tier can be entered from -- `rts_mir::lower` refuses it by position.
    pub(super) fn open_environment(&mut self, at: &Expr) -> Result<(), Unsupported> {
        let owned = self.owned_environment();
        let reaches = self.resolution.reaches_out(self.function);
        if !self.builds_own() {
            // UNDER ANOTHER STAGE'S LAYOUT a closure made here is handed the environment
            // this function was made in, always: the running emitter emits the closure's
            // body and may read through it for a reason the scope walk here does not see.
            if reaches || self.outer.is_some() {
                self.environment = Some(self.prim(JsPrim::EnclosingEnvironment, vec![], at));
                self.base_environment = self.environment;
            }
            return Ok(());
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
            if !spelled.insert(record.name) {
                return Err(Unsupported::Shape(
                    "two captured bindings of one activation share a spelling, and an environment is keyed by spelling",
                ));
            }
            let key = self.domain.constant(JsConst::Key(record.name));
            keys.push(self.declared(key, at));
        }
        // THE LEXICAL SLOTS, where an arrow written inside reads this activation's
        // `this` or `arguments`: the running emitter hands both to its arrows through
        // the environment, under these spellings, and the arrow it compiles reads them
        // there.
        let slots = self.lexical_slots.clone();
        for name in &slots {
            let key = self.domain.constant(JsConst::Key(*name));
            keys.push(self.declared(key, at));
        }

        let enclosing = self.prim(JsPrim::EnclosingEnvironment, vec![], at);
        let mut operands = vec![enclosing];
        operands.extend(keys);
        let built = self.prim(JsPrim::EnvNew, operands, at);
        self.environment = Some(built);
        self.base_environment = Some(built);
        for name in slots {
            let spelled = self.names.spelled(name).unwrap_or_default().to_owned();
            let value = match (spelled.as_str(), self.lexical_this) {
                // An arrow's own are its enclosing function's, read from their slots.
                (_, true) => self.lexical(&spelled, at).ok_or(Unsupported::Expression(
                    "an arrow's own lexical slot, which the enclosing layout does not hold",
                ))?,
                ("__rts_this", false) => self.this_value(at),
                (_, false) => match self.arguments {
                    Some(held) => held,
                    None => self.singleton_at(crate::values::Singleton::Undefined, at),
                },
            };
            let key = self.domain.constant(JsConst::Key(name));
            let key = self.declared(key, at);
            self.prim(JsPrim::EnvWrite, vec![built, key, value], at);
        }

        // THE PARAMETERS ARRIVED IN REGISTERS, and were held there while the guards
        // ran -- and the own name was bound beside them. A captured one moves now, so
        // that every later read -- this function's and a closure's -- goes to one place.
        for binding in owned {
            if !matches!(
                self.resolution.binding(binding).origin,
                Origin::Parameter | Origin::OwnName
            ) {
                continue;
            }
            if let Some(value) = self.values.remove(&binding) {
                self.env_write(binding, value, at)?;
            }
        }
        Ok(())
    }

    /// Whether this activation builds an environment of its own: it owns a captured
    /// binding, or an arrow inside reads its `this` or `arguments` from a slot.
    pub(super) fn builds_own(&self) -> bool {
        !self.owned_environment().is_empty() || !self.lexical_slots.is_empty()
    }

    /// The captured bindings this activation lays out -- all it owns, except, under
    /// another stage's layout, those inside a class body (the class is that stage's,
    /// compiled in a helper, and so is where its own bindings live) and those of a
    /// scope with an environment per pass, which [`Self::open_pass`] lays out.
    pub(super) fn owned_environment(&self) -> Vec<BindingId> {
        let mut owned = self.resolution.environment_of(self.function);
        if self.outer.is_some() {
            owned.retain(|held| {
                let scope = self.resolution.binding(*held).scope;
                !self.resolution.in_class_body(scope) && !self.resolution.pass_environment(scope)
            });
        }
        owned
    }

    /// Enters `scope`: where it has an environment per pass, builds this pass's -- linked
    /// to the environment in force, holding the scope's captured bindings -- and makes
    /// it the one closures made inside are handed. Answers what to restore on leaving
    /// and the parent it linked to, or `None` where the scope builds nothing.
    ///
    /// Only under another stage's layout, which is the one whose nested bodies are laid
    /// out by where each closure was made; elsewhere the binding is refused at its use.
    pub(super) fn open_pass(
        &mut self,
        scope: ScopeId,
        at: &Expr,
    ) -> Option<(Option<ValueId>, ValueId)> {
        if self.outer.is_none() || !self.resolution.pass_environment(scope) {
            return None;
        }
        let parent = self.environment_in_force(at);
        let built = self.new_pass(scope, parent, at);
        self.passes.push((scope, built));
        Some((std::mem::replace(&mut self.environment, Some(built)), parent))
    }

    /// Leaves a scope [`Self::open_pass`] entered.
    pub(super) fn close_pass(&mut self, restored: Option<ValueId>) {
        self.passes.pop();
        self.environment = restored;
    }

    /// A fresh environment for `scope`, linked to `parent`, every slot `undefined`.
    pub(super) fn new_pass(&mut self, scope: ScopeId, parent: ValueId, at: &Expr) -> ValueId {
        let mut operands = vec![parent];
        for binding in self.resolution.captured_in(scope) {
            let key = self.domain.constant(JsConst::Key(self.resolution.binding(binding).name));
            operands.push(self.declared(key, at));
        }
        self.prim(JsPrim::EnvNew, operands, at)
    }

    /// `CreatePerIterationEnvironment`: a new environment for the innermost pass scope,
    /// holding what the current one holds, made the one in force. Answers it.
    pub(super) fn copy_pass(&mut self, parent: ValueId, at: &Expr) -> ValueId {
        let (scope, current) = *self.passes.last().expect("a pass is open");
        let built = self.new_pass(scope, parent, at);
        for binding in self.resolution.captured_in(scope) {
            let key = self.domain.constant(JsConst::Key(self.resolution.binding(binding).name));
            let key = self.declared(key, at);
            let held = self.prim(JsPrim::EnvRead, vec![current, key], at);
            self.prim(JsPrim::EnvWrite, vec![built, key, held], at);
        }
        self.enter_pass_value(built);
        built
    }

    /// Makes `value` the innermost pass's environment -- a loop header's parameter,
    /// which is the environment of whichever pass arrived.
    pub(super) fn enter_pass_value(&mut self, value: ValueId) {
        self.passes.last_mut().expect("a pass is open").1 = value;
        self.environment = Some(value);
    }

    /// The environment a closure made here would be handed, as a value.
    pub(super) fn environment_in_force(&mut self, at: &Expr) -> ValueId {
        match self.environment {
            Some(held) => held,
            None => self.prim(JsPrim::EnclosingEnvironment, vec![], at),
        }
    }

    /// The environment that owns a captured binding, seen from here, and its key.
    fn env_slot(
        &mut self,
        binding: BindingId,
        at: &Expr,
    ) -> Result<(ValueId, ValueId), Unsupported> {
        let record = self.resolution.binding(binding);
        let (scope, name) = (record.scope, record.name);
        // A BINDING OF A PASS lives in that pass's environment, by value.
        if let Some((_, environment)) = self.passes.iter().rev().find(|(held, _)| *held == scope) {
            let environment = *environment;
            let key = self.domain.constant(JsConst::Key(name));
            let key = self.declared(key, at);
            return Ok((environment, key));
        }
        // A BINDING SOMEBODY ELSE LAID OUT is read where they put it. The per-pass
        // refusal below does not apply: the running emitter builds the environment
        // per pass, and its count of links already says which one this closure holds.
        if self.outer.is_some() && self.resolution.owner(scope) != self.function {
            return self.outer_slot(name, at);
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
            self.base_environment,
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

    /// Where the ENCLOSING layout keeps `name`, seen from here: the environment and
    /// the key. The enclosing layout counts from the environment this function was
    /// MADE in; one this function built itself stands one link in front of it.
    pub(super) fn outer_slot(
        &mut self,
        name: crate::names::Name,
        at: &Expr,
    ) -> Result<(ValueId, ValueId), Unsupported> {
        let Some((hops, key)) = self.outer.and_then(|outer| outer(name)) else {
            return Err(Unsupported::Expression(
                "the enclosing layout does not hold this binding in an environment",
            ));
        };
        let Some(mut environment) = self.base_environment else {
            return Err(Unsupported::Expression(
                "a captured binding with no environment in force, which the scope walk should have made impossible",
            ));
        };
        let built = self.builds_own();
        for _ in 0..hops + u32::from(built) {
            environment = self.prim(JsPrim::EnvOuter, vec![environment], at);
        }
        let key = self.domain.constant(JsConst::Key(key));
        let key = self.declared(key, at);
        Ok((environment, key))
    }

    /// What an ARROW reads from where it was written -- `this` as `__rts_this`, or
    /// `arguments` -- out of the slot the enclosing function put it in, which is how
    /// the running emitter hands both to its arrows. `None` where there is no such slot:
    /// no enclosing layout, or one that holds nothing under that spelling.
    pub(super) fn lexical(&mut self, spelled: &str, at: &Expr) -> Option<ValueId> {
        let name = self.names.find(spelled)?;
        self.outer?(name)?;
        let (environment, key) = self.outer_slot(name, at).ok()?;
        Some(self.prim(JsPrim::EnvRead, vec![environment, key], at))
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
