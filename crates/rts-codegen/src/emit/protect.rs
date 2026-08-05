//! `throw`, `try`, and the cleanup a `using` owes.
//!
//! # Why one module for three constructs
//!
//! They are one mechanism seen from three angles. A `try` declares a protected
//! region; a `throw` leaves through whatever region it is in; a `using` declares
//! a region whose cleanup is a disposal. Splitting them would put the region
//! tree's shape in three places that have to agree about it.
//!
//! # What the machine decides, and what is decided here
//!
//! Everything about *where* a throw lands is the machine's: the region tree, the
//! cleanup chain, the handler search. This layer says which spans are protected
//! and what a handler contains — which is the language part, and the only part.
//!
//! # The one tag, and why there is only one
//!
//! JavaScript has a single kind of throw. Anything can be thrown and every
//! `catch` catches everything, so a language that has one thing to say needs one
//! tag to say it with. The machine supports more because other languages need
//! more, and using one here is not leaving a feature unused — it is declining to
//! invent a distinction the language does not have.
//!
//! # The boundary, stated
//!
//! A `try` whose body contains a call is refused by name. The machine computes
//! where a throw lands from the region tree of the function it is *in*, which is
//! complete for handlers in that function and silent about a caller's — so a
//! throw inside the callee would run past this `catch` and end the program.
//! Compiling that would be a `catch` that is written, reads correctly, and never
//! runs, which is worse than not compiling it at all.

use rts_cranelift::ir::FuncBuilder;
use rts_cranelift::unwind::{Handler, Tag};

use super::stmt::emit_stmt;
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{Catch, Expr, ExprKind, Property, Stmt, StmtKind};

/// What a JavaScript `throw` is tagged with.
///
/// One value, named rather than written as `Tag(1)` at each of the three places
/// that mention it — a throw, a handler, and the cleanup plan all have to agree,
/// and three literals is three chances to disagree.
pub const JS_THROW: Tag = Tag(1);

/// Emits `throw e`.
///
/// Always terminates the block: nothing after a `throw` in the same statement
/// list is reachable, and the machine's verifier rejects a second terminator.
pub fn emit_throw(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    value: &Expr,
) -> EmitResult<bool> {
    let produced = super::expr::emit_expr(builder, scope, ctx, value)?;
    let payload = super::expr::as_value(builder, produced);
    builder.throw(JS_THROW, payload);
    Ok(true)
}

/// Emits `try { … } catch (e) { … } finally { … }`.
pub fn emit_try(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut super::loops::Loops,
    body: &[Stmt],
    catch: Option<&Catch>,
    finally: Option<&[Stmt]>,
) -> EmitResult<bool> {
    if let Some(reason) = calls_something(body) {
        return super::expr::gap(reason);
    }
    if finally.is_some() {
        // The machine models cleanup as one block ending in `CleanupDone`, and
        // that is not an oversight -- a cleanup is *copied* into every path
        // that unwinds through it, which is only sound while it has one exit.
        // A `finally` body is arbitrary statements, and arbitrary statements
        // need arbitrary blocks: `x + y` alone emits a fast path and a slow
        // one. Copying a subgraph rather than a block is the capability that
        // is missing, and it is missing below.
        return super::expr::gap("`finally`");
    }

    // Declared before anything is emitted into it, because a block is placed in
    // a region and a region cannot be named before it exists.
    let handler_block = catch.map(|_| builder.create_block());

    let handlers = handler_block
        .map(|block| {
            vec![Handler {
                tag: JS_THROW,
                block,
            }]
        })
        .unwrap_or_default();
    let protected = builder.create_block();
    builder.jump(protected, &[])?;
    builder.switch_to(protected);

    // Opened *after* switching, because opening puts the block being built into
    // the region — and every block anything nested creates until it closes. A
    // nested `if` inside a `try` does not have to know it is inside one, which
    // is the machine deriving membership rather than offering a call to forget.
    // No cleanup block: `finally` is refused above, and this is the only
    // construct that would supply one.
    builder.open_region(handlers, None);

    let before = scope.snapshot();
    scope.enter();
    let body_terminated = emit_block(builder, scope, ctx, loops, body)?;
    scope.leave();
    builder.close_region();

    // The handler starts from the environment as it was *before* the body,
    // which is sound only because everything the body assigns lives in memory
    // by now — `capture::assigned_under_protection` put it there. What the
    // snapshot carries is the SSA values the body could not have changed.
    // Created only if something reaches it. `try { throw 1 } catch (e) { return
    // e }` leaves through both arms, and a join block nothing enters is a block
    // with no terminator — which the verifier rejects, and rightly.
    let mut join = None;
    if !body_terminated {
        let block = builder.create_block();
        join = Some(block);
        builder.jump(block, &[])?;
    }

    if let (Some(block), Some(catch)) = (handler_block, catch) {
        // The thrown value arrives as the block's first parameter, which is the
        // machine's discipline for it: a handler that had to find the value
        // somewhere else would be reading a side channel that outlives the
        // frame it belongs to.
        let thrown = builder.add_block_param(block, rts_cranelift::repr::Repr::Tagged);
        builder.switch_to(block);
        scope.restore(&before);
        // The binding belongs to the handler alone -- `catch (e)` introduces
        // `e` for the handler body and nowhere else.
        scope.enter();
        if let Some(pattern) = &catch.binding {
            bind_caught(builder, scope, ctx, pattern, thrown)?;
        }
        let handler_terminated = emit_block(builder, scope, ctx, loops, &catch.body)?;
        scope.leave();
        if !handler_terminated {
            let block = *join.get_or_insert_with(|| builder.create_block());
            builder.jump(block, &[])?;
        }
    }

    let Some(join) = join else {
        // Every arm left. Nothing follows the `try`, and saying so is what stops
        // the caller emitting into a block that has already ended.
        return Ok(true);
    };

    builder.switch_to(join);
    scope.restore(&before);
    Ok(false)
}

/// Emits the statements of a protected span, stopping at a terminator.
fn emit_block(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut super::loops::Loops,
    body: &[Stmt],
) -> EmitResult<bool> {
    for statement in body {
        if emit_stmt(builder, scope, ctx, loops, statement)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Binds what was caught.
fn bind_caught(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &crate::syntax::Pattern,
    thrown: rts_cranelift::ir::ValueId,
) -> EmitResult<()> {
    match pattern {
        crate::syntax::Pattern::Name(name) => {
            super::binding::declare(builder, scope, ctx, *name, thrown)
        }
        // A destructuring catch binding is the destructuring emitter's job, and
        // that does not exist yet. Named here rather than silently binding
        // nothing, which would make `catch ({ message })` a `message` that is
        // always `undefined`.
        _ => super::expr::gap("a destructuring `catch` binding").map(|_: bool| ()),
    }
}

/// Whether a protected body contains a call, and what to say if it does.
///
/// The reason this exists is in the module doc: a throw inside a callee runs
/// past a handler in the caller, because the machine plans a throw from the
/// region tree of the function containing it. Until a throw can cross a frame,
/// a `catch` around a call is a `catch` that never runs.
fn calls_something(body: &[Stmt]) -> Option<&'static str> {
    let mut found = false;
    for statement in body {
        walk_stmt(statement, &mut found);
    }
    found.then_some("a `try` whose body contains a call")
}

fn walk_stmt(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => walk_expr(expr, found),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                walk_expr(expr, found);
            }
        }
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                if let Some(value) = &binding.value {
                    walk_expr(value, found);
                }
            }
        }
        StmtKind::Block(body) => body.iter().for_each(|inner| walk_stmt(inner, found)),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, found);
            walk_stmt(then_branch, found);
            if let Some(otherwise) = else_branch {
                walk_stmt(otherwise, found);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            walk_expr(condition, found);
            walk_stmt(body, found);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            match init {
                Some(crate::syntax::ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
                        if let Some(value) = &binding.value {
                            walk_expr(value, found);
                        }
                    }
                }
                Some(crate::syntax::ForInit::Expr(expr)) => walk_expr(expr, found),
                None => {}
            }
            if let Some(test) = test {
                walk_expr(test, found);
            }
            if let Some(update) = update {
                walk_expr(update, found);
            }
            walk_stmt(body, found);
        }
        StmtKind::ForEach { subject, body, .. } => {
            walk_expr(subject, found);
            walk_stmt(body, found);
        }
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            walk_expr(discriminant, found);
            for clause in clauses {
                if let Some(test) = &clause.test {
                    walk_expr(test, found);
                }
                clause.body.iter().for_each(|inner| walk_stmt(inner, found));
            }
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => walk_stmt(body, found),
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            body.iter().for_each(|inner| walk_stmt(inner, found));
            if let Some(catch) = catch {
                catch.body.iter().for_each(|inner| walk_stmt(inner, found));
            }
            if let Some(finally) = finally {
                finally.iter().for_each(|inner| walk_stmt(inner, found));
            }
        }
        // A function written here is not a function called here. Its body is
        // reached only through a call, which this walk finds where it is made.
        StmtKind::Function(_)
        | StmtKind::Class(_)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Debugger
        | StmtKind::Empty => {}
    }
}

fn walk_expr(expression: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    match &expression.kind {
        ExprKind::Call { .. }
        | ExprKind::New { .. }
        | ExprKind::SuperCall { .. }
        | ExprKind::TaggedTemplate { .. }
        | ExprKind::ImportCall { .. } => *found = true,

        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            walk_expr(left, found);
            walk_expr(right, found);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, found),
        ExprKind::Update { target, .. } => walk_expr(target, found),
        ExprKind::Await(inner) | ExprKind::Chain(inner) => walk_expr(inner, found),
        ExprKind::Yield { value, .. } => {
            if let Some(value) = value {
                walk_expr(value, found);
            }
        }
        ExprKind::Member { object, .. } => walk_expr(object, found),
        ExprKind::Index { object, index, .. } => {
            walk_expr(object, found);
            walk_expr(index, found);
        }
        ExprKind::Template { expressions, .. } => {
            expressions.iter().for_each(|e| walk_expr(e, found));
        }
        ExprKind::Object { properties } => {
            for property in properties {
                match property {
                    Property::Value { value, .. } => walk_expr(value, found),
                    Property::Spread(value) | Property::Prototype(value) => walk_expr(value, found),
                    // A method written here is not one called here.
                    Property::Method { .. } | Property::Getter { .. } | Property::Setter { .. } => {
                    }
                }
            }
        }
        ExprKind::Array { elements } => {
            for element in elements.iter().flatten() {
                match element {
                    crate::syntax::Spreadable::Single(value)
                    | crate::syntax::Spreadable::Spread(value) => walk_expr(value, found),
                }
            }
        }
        ExprKind::Assign { target, value, .. } => {
            if let crate::syntax::AssignTarget::Place(place) = target {
                walk_expr(place, found);
            }
            walk_expr(value, found);
        }
        ExprKind::Sequence { operands } => operands.iter().for_each(|e| walk_expr(e, found)),
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, found);
            walk_expr(then_branch, found);
            walk_expr(else_branch, found);
        }
        ExprKind::Asserted { value, .. } => walk_expr(value, found),
        ExprKind::SuperMember { property } => {
            if let crate::syntax::PropertyKey::Computed(key) = &**property {
                walk_expr(key, found);
            }
        }

        ExprKind::Literal(_)
        | ExprKind::Ident(_)
        | ExprKind::This
        | ExprKind::NewTarget
        | ExprKind::ImportMeta
        | ExprKind::PrivateName(_)
        | ExprKind::Function(_)
        | ExprKind::Class(_) => {}
    }
}
