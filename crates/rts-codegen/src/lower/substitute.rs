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
    /// Each parameter's default, by position; shorter than `parameters` where the rest
    /// have none. Applied where the argument is missing or `undefined`, in order, so a
    /// default reads the parameters before it with their defaults applied.
    pub defaults: Vec<Option<Expr>>,
    /// The one expression it answers.
    pub body: Expr,
    /// That it is `(...rest) => rest.length`: the answer is how many arguments were
    /// WRITTEN, after they are evaluated -- `emit/inline.rs`'s `rest_length` shape.
    pub counts_arguments: bool,
    /// That the body reads `this`, which a METHOD substituted at `o.m(...)` binds to
    /// the receiver's value -- `emit/receiver.rs` proves which method `o.m` is.
    pub reads_this: bool,
}

impl Lowering<'_> {
    /// `name(arguments)` as its body, or `None` where it stays a call.
    /// A call the program wrote, answered without calling where something proves it
    /// may be: `Math.*`, a proved method, a proved or local function -- or refused where
    /// it names a folded arrow that did not substitute. `None` where it stays a call.
    pub(super) fn call_replaced(
        &mut self,
        callee: &Expr,
        arguments: &[Spreadable],
        expr: &Expr,
    ) -> Result<Option<ValueId>, Unsupported> {
        use crate::syntax::ExprKind;
        if let Some(answered) = self.intrinsic(callee, arguments, expr)? {
            return Ok(Some(answered));
        }
        if let ExprKind::Member {
            object,
            property,
            optional: false,
        } = &callee.kind
            && let ExprKind::Ident(receiver) = &object.kind
            && let Some(answered) =
                self.substituted_method(*receiver, *property, object, arguments)?
        {
            return Ok(Some(answered));
        }
        if let ExprKind::Ident(name) = &callee.kind {
            if let Some(answered) = self.substituted(*name, arguments)? {
                return Ok(Some(answered));
            }
            // A CALL TO A FOLDED ARROW that did not substitute has nothing to
            // call: refused, so the function is compiled where the arrow is one.
            if self
                .resolution
                .binding_in(self.scope, *name)
                .is_some_and(|held| self.resolution.binds_omitted(held))
            {
                return Err(Unsupported::Expression(
                    "a call to a folded arrow that could not be substituted",
                ));
            }
        }
        Ok(None)
    }

    /// `o.m(arguments)` as the method's body, with `this` bound to `o`'s value, or
    /// `None` where it stays a call.
    pub(super) fn substituted_method(
        &mut self,
        receiver: Name,
        method: Name,
        object: &Expr,
        arguments: &[Spreadable],
    ) -> Result<Option<ValueId>, Unsupported> {
        let shadowed = self
            .resolution
            .binding_in(self.scope, receiver)
            .is_some_and(|held| self.declared_in_this_function(held));
        if shadowed
            || self.substituted_name(receiver).is_some()
            || self.substituting.iter().any(|(held, _)| *held == receiver)
            || arguments.iter().any(|held| matches!(held, Spreadable::Spread(_)))
        {
            return Ok(None);
        }
        let Some(substitute) = self.callees.method(receiver, method).cloned() else {
            return Ok(None);
        };
        // THE RECEIVER, then the arguments: the order the language evaluates a member
        // call in. The method is not read -- the proof says which one it is, and that
        // read is the piece this removes.
        let this = self.expression(object)?;
        self.substitute_body(receiver, &substitute, arguments, Some(this))
    }

    pub(super) fn substituted(
        &mut self,
        name: Name,
        arguments: &[Spreadable],
    ) -> Result<Option<ValueId>, Unsupported> {
        // A NAME THE BODY BEING SUBSTITUTED BINDS is that body's parameter, whatever
        // else the spelling names -- substituted only where its argument named a local
        // substitute; and one already being substituted is a cycle.
        let local = match self.substituted_name(name) {
            Some(_) => match self.aliases.last().and_then(|held| held.get(&name)) {
                Some(aliased) => Some(*aliased),
                None => return Ok(None),
            },
            None => self.resolution.binding_in(self.scope, name),
        };
        let name = local.map_or(name, |held| self.resolution.binding(held).name);
        if self.substituting.iter().any(|(held, _)| *held == name) {
            return Ok(None);
        }
        let substitute = match local {
            // A `const` OF THIS FUNCTION bound to an arrow it could substitute: the same
            // function, so the body reads the same variables -- where every free name
            // resolves here to what it resolved to where the arrow was written.
            Some(binding) if self.declared_in_this_function(binding) => {
                match self.local_arrows.get(&binding) {
                    Some((held, written)) if self.same_names(held, *written) => held.clone(),
                    _ => return Ok(None),
                }
            }
            _ => match self.callees.substitute(name).cloned() {
                Some(held) => held,
                None => return Ok(None),
            },
        };
        if arguments.iter().any(|held| matches!(held, Spreadable::Spread(_))) || substitute.reads_this {
            return Ok(None);
        }
        self.substitute_body(name, &substitute, arguments, None)
    }

    /// The arguments in order, each parameter bound to its value, `this` to `this`,
    /// and the body lowered with those in force.
    fn substitute_body(
        &mut self,
        name: Name,
        substitute: &Substitute,
        arguments: &[Spreadable],
        this: Option<ValueId>,
    ) -> Result<Option<ValueId>, Unsupported> {
        let mut values = Vec::with_capacity(arguments.len());
        let mut named = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Spreadable::Single(value) = argument else {
                return Ok(None);
            };
            named.push(self.local_substitute_named(value));
            values.push(self.expression(value)?);
        }
        if substitute.counts_arguments {
            return Ok(Some(self.integer(values.len() as i64, &substitute.body)));
        }
        let mut bound = std::collections::BTreeMap::new();
        let mut aliases = std::collections::BTreeMap::new();
        for (at, parameter) in substitute.parameters.iter().enumerate() {
            let default = substitute.defaults.get(at).and_then(Option::as_ref);
            let value = match (values.get(at), default) {
                (Some(held), None) => *held,
                (None, None) => self.singleton_at(crate::values::Singleton::Undefined, &substitute.body),
                // WITH THE PARAMETERS BEFORE IT IN FORCE, which is what a default reads.
                (arrived, Some(default)) => {
                    self.substituting.push((name, bound.clone()));
                    self.aliases.push(aliases.clone());
                    self.substituted_this.push(this);
                    let applied = match arrived {
                        None => self.expression(default),
                        // `p === undefined ? default : p`, the language's own definition.
                        Some(held) => {
                            let undefined = self.singleton_at(crate::values::Singleton::Undefined, default);
                            let absent =
                                self.prim(crate::domain::JsPrim::StrictEquals, vec![*held, undefined], default);
                            self.choice(
                                absent,
                                super::choice::Arm::Eval(default),
                                super::choice::Arm::Subject(*held),
                            )
                        }
                    };
                    self.substituting.pop();
                    self.aliases.pop();
                    self.substituted_this.pop();
                    applied?
                }
            };
            bound.insert(*parameter, value);
            match named.get(at).copied().flatten().filter(|_| default.is_none()) {
                Some(aliased) => aliases.insert(*parameter, aliased),
                // A LATER parameter of the same spelling is the one the body reads.
                None => aliases.remove(parameter),
            };
        }
        self.substituting.push((name, bound));
        self.aliases.push(aliases);
        self.substituted_this.push(this);
        let answered = self.expression(&substitute.body);
        self.substituting.pop();
        self.aliases.pop();
        self.substituted_this.pop();
        answered.map(Some)
    }

    /// Remembers `const name = (params) => expression` for [`Self::substituted`]: an
    /// ARROW (its `this` and `arguments` are the caller's, as they are here).
    pub(super) fn remember_arrow(&mut self, name: Name, value: &Expr) {
        let crate::syntax::ExprKind::Function(function) = &value.kind else {
            return;
        };
        if !function.captures_this {
            return;
        }
        self.remember_local(name, function);
    }

    /// Remembers a function DECLARED at the top of this one that nothing writes --
    /// `names::resolve::Resolution::never_written` -- for [`Self::substituted`].
    ///
    /// Not an arrow, so its `arguments` is its own: a body reading that name is left a
    /// call, since substituted it would read the caller's. Its `this` needs no check,
    /// because [`substitutable`] admits no `this` at all.
    pub(super) fn remember_declared(&mut self, name: Name, function: &crate::syntax::Function) {
        let Some(binding) = self.resolution.binding_in(self.scope, name) else {
            return;
        };
        if !self.resolution.never_written(binding) {
            return;
        }
        if let Some(body) = single_expression(function) {
            let mut read = Vec::new();
            names_read(body, &mut read);
            if read.iter().any(|held| self.names.spelled(*held) == Some("arguments")) {
                return;
            }
        }
        self.remember_local(name, function);
    }

    /// `name` as `function`'s substitute, where its shape is one: plain parameters with
    /// no defaults or rest, not async or a generator, one expression that
    /// [`substitutable`] accepts.
    fn remember_local(&mut self, name: Name, function: &crate::syntax::Function) {
        if function.is_async || function.is_generator || function.rest_parameter.is_some() {
            return;
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        let mut defaults = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let crate::syntax::Pattern::Name(held) = parameter.target else {
                return;
            };
            // A DEFAULT reads the parameters before it and not its own or a later one,
            // which the language leaves in their temporal dead zone -- a read the
            // substitution would answer with a value instead of raising.
            if let Some(default) = &parameter.default {
                let mut read = Vec::new();
                names_read(default, &mut read);
                let later = function.parameters[parameters.len()..].iter().filter_map(|held| match held.target {
                    crate::syntax::Pattern::Name(name) => Some(name),
                    _ => None,
                });
                if !substitutable(default) || later.into_iter().any(|name| read.contains(&name)) {
                    return;
                }
            }
            parameters.push(held);
            defaults.push(parameter.default.clone());
        }
        let Some(body) = single_expression(function).cloned() else {
            return;
        };
        if !substitutable(&body) {
            return;
        }
        let Some(binding) = self.resolution.binding_in(self.scope, name) else {
            return;
        };
        self.local_arrows.insert(
            binding,
            (
                Substitute {
                    parameters,
                    defaults,
                    body,
                    counts_arguments: false,
                    reads_this: false,
                },
                self.scope,
            ),
        );
    }

    /// Whether every name `substitute`'s body reads, other than its parameters, resolves
    /// from here to the binding it resolved to where the arrow was written.
    fn same_names(&self, substitute: &Substitute, written: crate::names::resolve::ScopeId) -> bool {
        let mut read = Vec::new();
        names_read(&substitute.body, &mut read);
        for default in substitute.defaults.iter().flatten() {
            names_read(default, &mut read);
        }
        read.iter()
            .filter(|held| !substitute.parameters.contains(held))
            .all(|held| {
                self.resolution.binding_in(written, *held)
                    == self.resolution.binding_in(self.scope, *held)
            })
    }

    /// The local substitute an argument names, where it is a bare name of one: so
    /// `apply(inc, a)` with `apply` answering `g(x)` substitutes `inc` for `g(x)` too.
    ///
    /// Sound because what the name holds cannot change between the argument and the
    /// call: a local substitute is a `const` or a declaration nothing writes. A name that
    /// is itself a parameter being substituted passes on what its own argument named.
    fn local_substitute_named(&self, value: &Expr) -> Option<crate::names::resolve::BindingId> {
        let crate::syntax::ExprKind::Ident(name) = value.kind else {
            return None;
        };
        let binding = match self.substituted_name(name) {
            Some(_) => *self.aliases.last()?.get(&name)?,
            None => self.resolution.binding_in(self.scope, name)?,
        };
        self.local_arrows.contains_key(&binding).then_some(binding)
    }

    /// `this` inside a method body being substituted: the receiver's value.
    pub(super) fn substituted_receiver(&self) -> Option<ValueId> {
        self.substituted_this.last().copied().flatten()
    }

    /// A spelling the body being substituted binds, as the value it was bound to.
    pub(super) fn substituted_name(&self, name: Name) -> Option<ValueId> {
        self.substituting.last()?.1.get(&name).copied()
    }
}

/// Whether the lowering takes every node of `expr` with nothing numbered for it: no
/// function, class, literal of an object or array, template, `this` or `super`. A body
/// passing this can be lowered anywhere without turning a function the door takes into
/// one it declines.
pub fn substitutable(expr: &Expr) -> bool {
    substitutable_reading(expr, false)
}

/// [`substitutable`], also admitting `this` where the body is a method's.
pub fn substitutable_reading(expr: &Expr, this: bool) -> bool {
    use crate::syntax::ExprKind;
    let here = (this && matches!(expr.kind, ExprKind::This))
        || matches!(
        expr.kind,
        ExprKind::Literal(_)
            | ExprKind::Ident(_)
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Conditional { .. }
            | ExprKind::Member { .. }
            | ExprKind::Index { .. }
            | ExprKind::Call { .. }
            | ExprKind::Asserted { .. }
    ) || matches!(&expr.kind, ExprKind::Unary { op, .. } if *op != crate::syntax::UnaryOp::Delete);
    if !here {
        return false;
    }
    let mut every = true;
    crate::emit::capture::walk_expr(expr, &mut |child| match child {
        crate::emit::capture::Child::Expr(inner) => every &= substitutable_reading(inner, this),
        _ => every = false,
    });
    every
}

/// The one expression a function answers: a concise body, or a block that is only
/// `return expression;`.
fn single_expression(function: &crate::syntax::Function) -> Option<&Expr> {
    match &function.body {
        crate::syntax::FunctionBody::Expression(expr) => Some(expr),
        crate::syntax::FunctionBody::Block(statements) => match statements.as_slice() {
            [crate::syntax::Stmt {
                kind: crate::syntax::StmtKind::Return(Some(expr)),
                ..
            }] => Some(expr),
            _ => None,
        },
    }
}

/// Every identifier an expression reads.
fn names_read(expr: &Expr, into: &mut Vec<Name>) {
    if let crate::syntax::ExprKind::Ident(name) = expr.kind {
        into.push(name);
    }
    crate::emit::capture::walk_expr(expr, &mut |child| {
        if let crate::emit::capture::Child::Expr(inner) = child {
            names_read(inner, into);
        }
    });
}
