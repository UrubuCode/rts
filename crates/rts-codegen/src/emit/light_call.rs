//! Which functions may be called without the runtime's argument bookkeeping.
//!
//! # What "light" buys
//!
//! A call through `RuntimeOp::Call` costs 23 to 27 ns, and 7 to 9 of it is
//! bookkeeping for the callee's sake: two argument stacks pushed and popped, so
//! that it can find what it was passed beyond its four slots and how many it
//! was passed. For a callee that reads none of that, the door skips it —
//! `rts-core`'s `entry/light_call.rs` has the measurement and the two designs
//! that were built before this one and lost.
//!
//! # The rule, and the reason for each clause
//!
//! The door's two argument stacks exist so that a callee can find what it was
//! passed beyond its four slots, and how many it was passed. A function is
//! light when nothing in it asks either question:
//!
//! - no `arguments`, in any form: the object is built from that record, and the
//!   light reads of `emit/light_arguments.rs` ask it for the count;
//! - no rest parameter, which is gathered from it;
//! - at most [`ARGUMENT_SLOTS`] parameters, since a fifth is read from it;
//! - no `new.target`, which is answered by comparing activation depths the
//!   ordinary door and `construct` keep in step;
//! - no `eval`, which can spell any of the above at run time;
//! - not `async` and not a generator: those are wrappers around a parked frame
//!   and the door is what knows it;
//! - strict: a non-strict function substitutes the global object for a missing
//!   receiver, and that substitution is argued for the door's call.
//!
//! An arrow written inside is walked with the function, because it reads the
//! enclosing `arguments` and `new.target`; an ordinary function inside has its
//! own of both and is judged when IT is emitted.
//!
//! What the compiler cannot see is decided at run time by the door: that
//! the value called is a callable at all, and that it is not a class
//! constructor reached without `new`.
//!
//! # How the answer travels
//!
//! In the code. `Ctx::light_functions` holds the machine id of every function
//! found light, and a closure over one is made by `RuntimeOp::ClosureNewLight`
//! where any other is made by `ClosureNew` — the runtime records the address
//! as the closure is born. No table carries it, so the executable-memory and
//! object-file destinations cannot disagree about it.

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::names::Name;
use crate::runtime::ARGUMENT_SLOTS;
use crate::syntax::{Expr, ExprKind, Function, FunctionBody, Stmt};

/// Whether `function` may be called through the light door — see the module
/// header for every clause.
pub(super) fn is_light(function: &Function, sloppy: bool, arguments: Name, eval: Name) -> bool {
    if sloppy
        || function.is_async
        || function.is_generator
        || function.rest_parameter.is_some()
        || function.parameters.len() > ARGUMENT_SLOTS
    {
        return false;
    }
    let mut probe = Probe {
        arguments,
        eval,
        refused: false,
    };
    for parameter in &function.parameters {
        if let Some(value) = &parameter.default {
            probe.expression(value);
        }
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                probe.statement(statement);
            }
        }
        FunctionBody::Expression(value) => probe.expression(value),
    }
    !probe.refused
}

struct Probe {
    arguments: Name,
    eval: Name,
    refused: bool,
}

impl Probe {
    fn statement(&mut self, statement: &Stmt) {
        if self.refused {
            return;
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner),
            StmtChild::Expr(value) => self.expression(value),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    self.expression(value);
                }
            }
            StmtChild::Catch(clause) => {
                for inner in &clause.body {
                    self.statement(inner);
                }
            }
            StmtChild::Function(inner) => self.function(inner),
            // A class's methods have their own `arguments` and `new.target`;
            // its field initialisers and heritage may read this function's.
            // Refused rather than walked: a class declared inside a function
            // that is called often enough to matter is rare.
            StmtChild::Class(_) => self.refused = true,
        });
    }

    fn expression(&mut self, value: &Expr) {
        if self.refused {
            return;
        }
        match &value.kind {
            ExprKind::NewTarget => {
                self.refused = true;
                return;
            }
            ExprKind::Ident(name) if *name == self.arguments || *name == self.eval => {
                self.refused = true;
                return;
            }
            _ => {}
        }
        walk_expr(value, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(inner) => self.function(inner),
            Child::Class(_) => self.refused = true,
        });
    }

    /// An arrow reads this function's `arguments` and `new.target`; an ordinary
    /// function has its own.
    fn function(&mut self, function: &Function) {
        if !function.captures_this {
            return;
        }
        for parameter in &function.parameters {
            if let Some(value) = &parameter.default {
                self.expression(value);
            }
        }
        match &function.body {
            FunctionBody::Block(body) => {
                for statement in body {
                    self.statement(statement);
                }
            }
            FunctionBody::Expression(value) => self.expression(value),
        }
    }
}
