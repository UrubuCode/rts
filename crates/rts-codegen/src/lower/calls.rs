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
use crate::domain::{JsConst, JsPrim};
use crate::names::Name;
use crate::names::resolve::BindingId;
use crate::syntax::{Expr, ExprKind};

impl Lowering<'_> {
    /// A call as the program wrote it: the argument list, or -- where it holds a spread,
    /// whose count only the run time knows -- a vector and `CallWithArgs`, the door the
    /// running emitter takes for the same call. A function of this module called by
    /// number with a spread is refused; it has no value to hand that door.
    pub(super) fn call_written(
        &mut self,
        callee: rts_mir::cfg::Callee,
        receiver: Option<ValueId>,
        arguments: &[crate::syntax::Spreadable],
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        if let Some(vector) = self.spread_vector(arguments, at)? {
            let rts_mir::cfg::Callee::Dynamic(function) = callee else {
                return Err(Unsupported::Expression(
                    "a spread into a function called by number",
                ));
            };
            let receiver = match receiver {
                Some(held) => held,
                None => self.singleton_at(crate::values::Singleton::Undefined, at),
            };
            return Ok(self.entry(
                crate::runtime::RuntimeOp::CallWithArgs,
                vec![function, receiver, vector],
                at,
            ));
        }
        let args = self.arguments(arguments)?;
        Ok(self.call(callee, receiver, args, at))
    }

    /// What a call written with this callee reaches, and the receiver it passes.
    pub(super) fn callee_of(
        &mut self,
        callee: &Expr,
        expr: &Expr,
    ) -> Result<(rts_mir::cfg::Callee, Option<ValueId>), Unsupported> {
        // A METHOD CALL: the receiver is read once, the callee is read from
        // it, and the receiver travels as itself.
        //
        // Once is the whole point. `o.m()` evaluates `o` a single time, so
        // lowering it as "read o, read o.m, call with o" would evaluate it
        // twice and call a getter twice — which is observable and is the
        // same mistake `a[i()] += 1` is refused for one arm up.
        //
        // The receiver is a FIELD of the call and not its first argument.
        // `rts_mir::cfg::Op::Call` carries the reason: a convention held in
        // two places drifts, and a field cannot.
        if let ExprKind::Member {
            object,
            property,
            optional: false,
        } = &callee.kind
        {
            let receiver = self.expression(object)?;
            let key = self.domain.constant(JsConst::Key(*property));
            let key = self.declared(key, expr);
            let held = self.prim(JsPrim::FieldRead, vec![receiver, key], expr);
            return Ok((rts_mir::cfg::Callee::Dynamic(held), Some(receiver)));
        }
        // `o[k]()` is a method call as much as `o.m()` is: the receiver is `o`.
        if let ExprKind::Index {
            object,
            index,
            optional: false,
        } = &callee.kind
        {
            let receiver = self.expression(object)?;
            let key = self.expression(index)?;
            let held = self.prim(JsPrim::IndexRead, vec![receiver, key], expr);
            return Ok((rts_mir::cfg::Callee::Dynamic(held), Some(receiver)));
        }
        // ANY OTHER CALLEE is a value, called with no receiver: `f()()`,
        // `(a || b)(x)`, `(() => 1)()`. What the expression cannot express --
        // `super`, an optional chain -- its own lowering refuses by name.
        let ExprKind::Ident(name) = &callee.kind else {
            let held = self.expression(callee)?;
            return Ok((rts_mir::cfg::Callee::Dynamic(held), None));
        };
        let binding = self.resolution.binding_in(self.scope, *name);
        // A CALL TO A GLOBAL: read it, then call what it held. No receiver
        // travels -- parseInt(x) passes none, and the global object is not
        // a receiver. The callee is read BEFORE the arguments run, which is
        // the language's order; this read them the other way round.
        let Some(binding) = binding else {
            let held = self.global(*name, expr);
            return Ok((rts_mir::cfg::Callee::Dynamic(held), None));
        };
        // A NAME THAT HOLDS NO FUNCTION OF THIS MODULE is still a call — an
        // imported binding, or a parameter holding a function. It reaches
        // whatever the value is, which is exactly `Callee::Dynamic`, and it
        // passes no receiver.
        //
        // It was refused before this, and the refusal was about the MIR not
        // having a receiver rather than about this shape: a dynamic call was
        // already expressible. 218 refusals in `tests/` said so.
        let callee = match self.callees.of_binding(binding) {
            Some(id) => rts_mir::cfg::Callee::Func(id),
            None => {
                let held = self.read_binding(binding, *name, expr)?;
                rts_mir::cfg::Callee::Dynamic(held)
            }
        };
        Ok((callee, None))
    }

    /// The arguments as ONE array, where any of them is a spread -- built the way an
    /// array literal with a spread is -- or `None` where the count is the one written.
    pub(super) fn spread_vector(
        &mut self,
        arguments: &[crate::syntax::Spreadable],
        at: &Expr,
    ) -> Result<Option<ValueId>, Unsupported> {
        if !arguments
            .iter()
            .any(|held| matches!(held, crate::syntax::Spreadable::Spread(_)))
        {
            return Ok(None);
        }
        let elements: Vec<Option<crate::syntax::Spreadable>> =
            arguments.iter().cloned().map(Some).collect();
        self.array_literal(&elements, at).map(Some)
    }

    /// The arguments of a call, in source order.
    ///
    /// A spread is refused here: the count would stop being the count written, and
    /// every consumer of this graph reads the argument list as what the program wrote.
    /// [`Self::call_written`] is where a spread goes instead.
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

    /// Reads a binding, as a value.
    ///
    /// Apart from the identifier arm because a call needs the same answer without
    /// going through an `Expr` it does not have: the callee of `f(1)` is the name
    /// `f`, and reading it is the same question the arm asks.
    ///
    /// # Four answers, and why they are told apart
    ///
    /// - held in a register of this function: the value it holds now;
    /// - CAPTURED, by this function or by one inside it: a read of the environment
    ///   that owns it, `names::resolve::captured` having said which and how far;
    /// - a function of this module read as a value: it needs a closure, which is a
    ///   different missing piece;
    /// - declared LATER in this function: its dead zone, and reading it throws.
    ///
    /// Captured comes before the function arm because a nested declaration a closure
    /// reads is bound into the environment and read back out of it, and asking the
    /// callee map first turned that ordinary read into a refusal.
    pub(super) fn read_binding(
        &mut self,
        binding: BindingId,
        name: Name,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        if let Some(value) = self.values.get(&binding) {
            return Ok(*value);
        }
        if self.resolution.captured(binding) {
            return self.env_read(binding, at);
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
            // OUTSIDE THIS FUNCTION AND NOT CAPTURED is a contradiction: the scope walk
            // records every use, and a use from another function is what capturing
            // means. Refused rather than guessed at, because the guess is a read of a
            // place nothing writes.
            false => Err(Unsupported::Expression(
                "an outer binding the scope walk did not record as captured",
            )),
        }
    }

    /// A closure value for a function the module numbered, over the environment in
    /// force here -- or `undefined` where nothing inside reaches past this function,
    /// which is what `emit/function.rs` hands such a closure too.
    pub(super) fn closure(&mut self, id: rts_mir::cfg::FuncId, at: &Expr) -> ValueId {
        let index = self.domain.constant(crate::domain::JsConst::Function(id.0));
        let named = self.declared(index, at);
        let environment = match self.environment {
            Some(held) => held,
            None => self.singleton_at(crate::values::Singleton::Undefined, at),
        };
        self.prim(JsPrim::MakeClosure, vec![named, environment], at)
    }

    /// A name no scope declares, read through the global object.
    ///
    /// The key is the same declared constant a property read takes, because that is
    /// what this is: `emit/inline.rs` already states that a name the whole program
    /// declares nowhere is resolved through the global object at every site there is.
    ///
    /// A WRITE to one is still refused. `undeclared = 1` in sloppy code creates a
    /// global property, in strict code it throws, and which of the two a module is in
    /// is a fact `check/` holds and this stage does not ask for. Guessing either way
    /// would be wrong in half the programs.
    ///
    /// A name the ENCLOSING LAYOUT holds is not a global at all: `Function` text and a
    /// page script are resolved on their own, so the scope walk calls global what the
    /// running emitter's scope binds around them -- a CommonJS `module`, a page's
    /// `document`. That is read where the layout keeps it.
    pub(super) fn global(&mut self, name: Name, at: &Expr) -> ValueId {
        if let Some((environment, key)) = self.enclosing_slot(name, at) {
            return self.prim(JsPrim::EnvRead, vec![environment, key], at);
        }
        let index = self.domain.constant(crate::domain::JsConst::Key(name));
        let key = self.declared(index, at);
        self.prim(JsPrim::GlobalRead, vec![key], at)
    }

    /// Where the enclosing layout keeps a name the scope walk found no binding for.
    ///
    /// Only a name nothing resolved here DECLARES. One that is declared -- a function
    /// declaration inside a block, which the scope walk keeps in the block -- has a
    /// slot in the enclosing layout that the running emitter may never write: it calls
    /// such a function directly, and the slot stays `undefined`. Measured, on
    /// `tests/claude-bundle-real-gaps-2.test.ts`.
    pub(super) fn enclosing_slot(&mut self, name: Name, at: &Expr) -> Option<(ValueId, ValueId)> {
        if self.resolution.declares(name) {
            return None;
        }
        self.outer?(name)?;
        self.outer_slot(name, at).ok()
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
