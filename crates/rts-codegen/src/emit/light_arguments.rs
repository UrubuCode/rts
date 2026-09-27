//! `arguments.length` and `arguments[i]` without the object.
//!
//! A function that mentions `arguments` built an array-like object on EVERY
//! call — a wide cell, one property per argument, `length`, `Symbol.iterator`
//! and `Symbol.toStringTag`, each a shape transition — for what is nearly
//! always one read of `length` or of one index. `bench/analytic.ts` put
//! `arguments.length` at 1 518 ns a call (release, 2026-09-27) against 4 for
//! the call itself; caching the keys the object is stamped with took it to 950
//! and no further, because the object is the cost.
//!
//! Where every mention of the name in a body is the object of `.length` or of
//! `[e]`, and only as something READ, the object is never observable: nothing
//! can compare it, pass it, spread it, write to it or read another member of it.
//! So the body is emitted with no binding for the name, and those two reads
//! become entries over the activation's own four slots — `ArgumentsCount` and
//! `ArgumentSlot`, which answer what the object's `length` and index would
//! have, from the same count the runtime uses to build it.
//!
//! # What is refused, and why each
//!
//! - A mention anywhere else — `arguments` passed, returned, spread, aliased,
//!   `arguments.callee`, `arguments.foo`: the object is observable there.
//! - A mention as a WRITE target — `arguments[0] = v`, `arguments.length = 0`,
//!   `arguments[0]++`, `delete arguments[0]`: a write needs the object.
//! - A mention inside an ARROW written in the body: the arrow sees this
//!   function's `arguments` through a captured binding, and there would be no
//!   binding. A nested ordinary function has its own and does not count.
//!
//! The index of `arguments[e]` is any expression: the entry receives its value
//! and answers from a slot only for a number that names one; anything else —
//! `arguments["length"]`, a string, a symbol — builds the object and reads it,
//! so the answer is the language's for every key the program can spell.
//!
//! Shared by both emitters: the running one decides here and reads the slots
//! it was handed; the MIR stage asks the same question in `lower/gather.rs` and
//! answers its reads in `lower/mod.rs`. One classifier, so the two cannot admit
//! different bodies.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::expr::{emit_expr, tagged};
use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{AssignTarget, Expr, ExprKind, Function, FunctionBody, Stmt, UnaryOp};

/// Whether every mention of `arguments` in `body` is a read of `.length` or of
/// `[e]`, and nothing else — see the module header for what that excludes.
pub(crate) fn measured_only(body: &[Stmt], arguments: Name, length: Name) -> bool {
    let mut walk = Walk {
        arguments,
        length,
        fine: true,
    };
    for statement in body {
        walk.statement(statement);
    }
    walk.fine
}

struct Walk {
    arguments: Name,
    length: Name,
    fine: bool,
}

impl Walk {
    fn statement(&mut self, statement: &Stmt) {
        if !self.fine {
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
            StmtChild::Catch(clause) => clause.body.iter().for_each(|held| self.statement(held)),
            StmtChild::Function(inner) => self.function(inner),
            // A class body's methods have their own `arguments`, but its field
            // initialisers and heritage could mention this one. Rare enough in a
            // body that also reads `arguments` to refuse rather than walk.
            StmtChild::Class(_) => self.fine = false,
        });
    }

    /// A nested ORDINARY function has its own `arguments`; an arrow reads this
    /// one's and is refused.
    fn function(&mut self, function: &Function) {
        if !function.captures_this {
            return;
        }
        let mentioned = match &function.body {
            FunctionBody::Block(body) => super::capture::mentions(body, self.arguments),
            FunctionBody::Expression(value) => {
                let mut found = false;
                mentions_in(value, self.arguments, &mut found);
                found
            }
        };
        if mentioned {
            self.fine = false;
        }
    }

    fn is_arguments(&self, value: &Expr) -> bool {
        matches!(&unasserted(value).kind, ExprKind::Ident(name) if *name == self.arguments)
    }

    fn expression(&mut self, value: &Expr) {
        if !self.fine {
            return;
        }
        match &value.kind {
            ExprKind::Ident(name) if *name == self.arguments => {
                self.fine = false;
            }
            ExprKind::Member {
                object, property, ..
            } if self.is_arguments(object) => {
                if *property != self.length {
                    self.fine = false;
                }
            }
            ExprKind::Index { object, index, .. } if self.is_arguments(object) => {
                self.expression(index);
            }
            // A WRITE through the name, in any spelling: the object is needed.
            ExprKind::Assign { target, value: written, .. } => {
                match target {
                    AssignTarget::Place(place) => {
                        if self.reaches(place) {
                            self.fine = false;
                        }
                        self.expression(place);
                    }
                    AssignTarget::Pattern(_) => {
                        if self.reaches(value) {
                            self.fine = false;
                        }
                    }
                }
                self.expression(written);
            }
            ExprKind::Update { target, .. } => {
                if self.reaches(target) {
                    self.fine = false;
                }
                self.expression(target);
            }
            ExprKind::Unary {
                op: UnaryOp::Delete,
                operand,
            } => {
                if self.reaches(operand) {
                    self.fine = false;
                }
                self.expression(operand);
            }
            _ => walk_expr(value, &mut |child| match child {
                Child::Expr(inner) => self.expression(inner),
                Child::Function(inner) => self.function(inner),
                Child::Class(_) => self.fine = false,
            }),
        }
    }

    /// Whether `arguments` is mentioned anywhere under `value`, arrows included.
    fn reaches(&self, value: &Expr) -> bool {
        let mut found = false;
        mentions_in(value, self.arguments, &mut found);
        found
    }
}

fn mentions_in(value: &Expr, wanted: Name, found: &mut bool) {
    if *found {
        return;
    }
    if matches!(&value.kind, ExprKind::Ident(name) if *name == wanted) {
        *found = true;
        return;
    }
    walk_expr(value, &mut |child| match child {
        Child::Expr(inner) => mentions_in(inner, wanted, found),
        Child::Function(inner) => {
            if inner.captures_this {
                match &inner.body {
                    FunctionBody::Block(body) => *found |= super::capture::mentions(body, wanted),
                    FunctionBody::Expression(inner) => mentions_in(inner, wanted, found),
                }
            }
        }
        Child::Class(_) => *found = true,
    });
}

/// Whether `object` is this activation's `arguments`, read light: the body was
/// admitted and set the slots, and the body binds nothing under the name.
///
/// NOT `scope.lookup`: an enclosing function that binds `arguments` for the
/// arrows inside it leaves the name visible from a function declared in one of
/// them, and this function's own `arguments` shadows that — which is why the
/// admitted body is emitted with the flag set and every other body with it
/// cleared, in `emit/function.rs`.
fn is_light(ctx: &Ctx, object: &Expr) -> Option<[ValueId; 4]> {
    let slots = ctx.light_arguments?;
    let ExprKind::Ident(name) = &unasserted(object).kind else {
        return None;
    };
    (ctx.names.find("arguments") == Some(*name)).then_some(slots)
}

/// `value` with its type assertions removed: `(arguments as any)[i]` is how
/// every TypeScript body spells the read, and an assertion is erased — it names
/// the same value. Without this the classifier admitted `arguments.length`
/// alone and the fixture's index reads still built the object, at 885 ns.
pub(crate) fn unasserted(value: &Expr) -> &Expr {
    match &value.kind {
        ExprKind::Asserted { value: inner, .. } => unasserted(inner),
        _ => value,
    }
}

/// `arguments.length`, where the body reads it light.
pub(super) fn length(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: &Expr,
    property: Name,
) -> EmitResult<Option<ValueId>> {
    let Some(slots) = is_light(ctx, object) else {
        return Ok(None);
    };
    if ctx.names.find("length") != Some(property) {
        return Ok(None);
    }
    Ok(Some(super::expr::call(builder, ctx, RuntimeOp::ArgumentsCount, &slots)?[0]))
}

/// `arguments[e]`, where the body reads it light.
pub(super) fn at(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    object: &Expr,
    index: &Expr,
) -> EmitResult<Option<ValueId>> {
    let Some(slots) = is_light(ctx, object) else {
        return Ok(None);
    };
    let index = emit_expr(builder, scope, ctx, index)?;
    let index = tagged(builder, index);
    let mut operands = slots.to_vec();
    operands.push(index);
    Ok(Some(super::expr::call(builder, ctx, RuntimeOp::ArgumentSlot, &operands)?[0]))
}
