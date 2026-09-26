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
        // A NAME THE BODY BEING SUBSTITUTED BINDS is that body's parameter, whatever
        // else the spelling names; and one already being substituted is a cycle.
        if self.substituted_name(name).is_some()
            || self.substituting.iter().any(|(held, _)| *held == name)
        {
            return Ok(None);
        }
        let local = self.resolution.binding_in(self.scope, name);
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

    /// Remembers `const name = (params) => expression` for [`Self::substituted`]: an
    /// ARROW (its `this` and `arguments` are the caller's, as they are here), plain
    /// parameters with no defaults or rest, one expression that
    /// [`substitutable`] accepts.
    pub(super) fn remember_arrow(&mut self, name: Name, value: &Expr) {
        let crate::syntax::ExprKind::Function(function) = &value.kind else {
            return;
        };
        if !function.captures_this
            || function.is_async
            || function.is_generator
            || function.rest_parameter.is_some()
        {
            return;
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let crate::syntax::Pattern::Name(held) = parameter.target else {
                return;
            };
            if parameter.default.is_some() {
                return;
            }
            parameters.push(held);
        }
        let body = match &function.body {
            crate::syntax::FunctionBody::Expression(expr) => expr.as_ref().clone(),
            crate::syntax::FunctionBody::Block(statements) => match statements.as_slice() {
                [crate::syntax::Stmt {
                    kind: crate::syntax::StmtKind::Return(Some(expr)),
                    ..
                }] => expr.clone(),
                _ => return,
            },
        };
        if !substitutable(&body) {
            return;
        }
        let Some(binding) = self.resolution.binding_in(self.scope, name) else {
            return;
        };
        self.local_arrows
            .insert(binding, (Substitute { parameters, body }, self.scope));
    }

    /// Whether every name `substitute`'s body reads, other than its parameters, resolves
    /// from here to the binding it resolved to where the arrow was written.
    fn same_names(&self, substitute: &Substitute, written: crate::names::resolve::ScopeId) -> bool {
        let mut read = Vec::new();
        names_read(&substitute.body, &mut read);
        read.iter()
            .filter(|held| !substitute.parameters.contains(held))
            .all(|held| {
                self.resolution.binding_in(written, *held)
                    == self.resolution.binding_in(self.scope, *held)
            })
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
    use crate::syntax::ExprKind;
    let here = matches!(
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
        crate::emit::capture::Child::Expr(inner) => every &= substitutable(inner),
        _ => every = false,
    });
    every
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
