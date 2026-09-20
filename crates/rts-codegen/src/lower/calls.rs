//! What a call needs beside the arguments: a callee, a receiver, and the answer to
//! "does this binding hold a value here".
//!
//! Apart from the rest of the lowering because `mod.rs` reached 1001 lines against
//! this crate's ceiling of 1000, and the seam is the one the code already had: a
//! call is the only expression that reaches OUTSIDE the function being lowered --
//! for a function of the module, for a binding of an enclosing scope, for whatever
//! a value holds. Everything in this file is a form of that question.

use rts_mir::cfg::{Op, ValueId};
use rts_mir::{Domain, Effect};

use super::{Lowering, Unsupported};
use crate::names::Name;
use crate::names::resolve::BindingId;
use crate::domain::JsPrim;
use crate::syntax::Expr;

impl Lowering<'_> {
    /// The arguments of a call, in source order.
    ///
    /// A spread is refused: the count would stop being the count written, and every
    /// consumer of this graph reads the argument list as what the program wrote.
    pub(super) fn arguments(
        &mut self,
        arguments: &[crate::syntax::Spreadable],
    ) -> Result<Vec<ValueId>, Unsupported> {
        let mut held = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let crate::syntax::Spreadable::Single(value) = argument else {
                return Err(Unsupported::Expression(
                    "a spread argument has a run-time count",
                ));
            };
            held.push(self.expression(value)?);
        }
        Ok(held)
    }

    /// Pushes a call, with the effect a call always has.
    ///
    /// `CALLS_USER` and `THROWS`, because what the callee does is not known here.
    /// An interprocedural pass narrows it, and `passes::refine_effects` is already
    /// the shape that would apply the answer.
    pub(super) fn call(
        &mut self,
        callee: rts_mir::cfg::Callee,
        receiver: Option<ValueId>,
        args: Vec<ValueId>,
        at: &Expr,
    ) -> ValueId {
        let held = self.builder.push(
            Op::Call {
                callee,
                receiver,
                args,
            },
            Effect::CALLS_USER.and(Effect::THROWS),
            at.at,
        );
        let of = self.domain.top();
        self.types.insert(held, of);
        held
    }

    /// Reads a binding this function holds, as a value.
    ///
    /// Apart from the identifier arm because a call needs the same answer without
    /// going through an `Expr` it does not have: the callee of `f(1)` is the name
    /// `f`, and reading it is the same question the arm asks.
    ///
    /// # Three refusals, and why they are told apart
    ///
    /// A binding with no value here is one of three things, and the first version of
    /// this reported all of them as a temporal dead zone — which made the survey
    /// name the wrong work. `bench/` and `tests/` between them had 66 of these, and
    /// an import is not a dead zone.
    ///
    /// - declared LATER in this function: its dead zone, and reading it throws;
    /// - declared OUTSIDE this function: a module binding or a captured one, which
    ///   needs the environment this stage does not build;
    /// - a function of this module read as a value: it needs a closure, which is a
    ///   different missing piece from either.
    pub(super) fn read_binding(
        &mut self,
        binding: BindingId,
        name: Name,
        _at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        if let Some(value) = self.values.get(&binding) {
            return Ok(*value);
        }
        if self.callees.of_binding(binding).is_some() {
            return Err(Unsupported::Expression(
                "a function of this module read as a value needs a closure",
            ));
        }
        let _ = name;
        match self.declared_in_this_function(binding) {
            true => Err(Unsupported::Expression(
                "a binding read before its declaration is in its dead zone",
            )),
            // OUTSIDE THIS FUNCTION: an operation that names the binding, rather
            // than a refusal.
            //
            // It was 912 refusals in `tests/` and 54 in `bench/`, the top of both
            // tables, and the reason it is expressible after all is rule 2: WHERE a
            // captured cell lives — a module record, an environment object, a slot
            // an enclosing activation holds — is a machine question, and this layer
            // says which binding and stops.
            //
            // Closure conversion is the other answer and does not fit yet: it makes
            // every free binding an extra parameter, and a `Callee::Dynamic` site
            // does not know the callee's free set, so it would refuse exactly the
            // calls that most need it.
            false => Ok(self.outer(binding, JsPrim::OuterRead, None, _at)),
        }
    }

    /// An outer binding read or written, named by a declared constant.
    ///
    /// The name travels as a constant of the language's table so that two accesses
    /// to one outer binding carry ONE index and compare equal — which is what a pass
    /// hoisting a load out of a loop needs, and what comparing `BindingId`s inside
    /// the lowering could not give a pass reading the finished graph.
    pub(super) fn outer(
        &mut self,
        binding: BindingId,
        which: JsPrim,
        value: Option<ValueId>,
        at: &Expr,
    ) -> ValueId {
        let index = self
            .domain
            .constant(crate::domain::JsConst::Binding(binding.index() as u32));
        let named = self.declared(index, at);
        let mut args = vec![named];
        args.extend(value);
        self.prim(which, args, at)
    }

    /// Whether the declaration belongs to this function rather than to something
    /// around it.
    ///
    /// Walks out from the scope being lowered and stops at the function boundary,
    /// which the scope tree marks — so a block of this body counts and the module
    /// does not.
    pub(super) fn declared_in_this_function(&self, binding: BindingId) -> bool {
        let held = self.resolution.binding(binding).scope;
        let mut at = Some(self.scope);
        while let Some(scope) = at {
            if scope == held {
                return true;
            }
            if self.resolution.scope(scope).kind == crate::names::resolve::ScopeKind::Function {
                return false;
            }
            at = self.resolution.scope(scope).parent;
        }
        false
    }
}
