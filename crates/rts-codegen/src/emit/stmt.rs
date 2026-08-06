//! Statements, in the machine's representation.
//!
//! # The return value: "did this terminate the block?"
//!
//! Every emitter here returns a `bool` saying whether control can still reach
//! the next statement. It is not a convenience: the machine's verifier rejects a
//! block with two terminators, so `return 1; return 2;` — which is legal
//! JavaScript, the second being unreachable — has to stop emitting rather than
//! emit both.
//!
//! Answering it with a `bool` rather than by inspecting the block afterwards
//! keeps it a property of what was emitted rather than of what happens to be
//! there, which matters as soon as control flow arrives and a statement can
//! terminate through a path that is not its own last instruction.

use rts_cranelift::ir::FuncBuilder;

use super::expr::{emit_condition, emit_expr, stored, undefined};
use super::loops::{self, Loops};
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{Pattern, Stmt, StmtKind};

/// Emits a statement. Returns whether it terminated the block.
pub fn emit_stmt(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    statement: &Stmt,
) -> EmitResult<bool> {
    match &statement.kind {
        // The value is discarded and the expression is still emitted: an
        // expression statement exists for its side effects, and dropping it
        // because nothing reads the result would drop those too.
        StmtKind::Expr(expr) => {
            emit_expr(builder, scope, ctx, expr)?;
            Ok(false)
        }

        StmtKind::Empty => Ok(false),

        StmtKind::Return(value) => {
            let result = match value {
                // A function's signature declares a tagged return, because a
                // caller cannot know what it will get back. Whatever was proved
                // inside stops being useful at the boundary.
                Some(expr) => {
                    let produced = emit_expr(builder, scope, ctx, expr)?;
                    super::expr::as_value(builder, produced)
                }
                // `return;` yields `undefined`, not "no value". The signature
                // declares one return, and a JavaScript function always
                // produces something.
                None => undefined(builder, ctx),
            };
            builder.ret(&[result]);
            Ok(true)
        }

        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                let Pattern::Name(name) = &binding.target else {
                    // A destructuring declaration. Nothing here is a
                    // flattening candidate — `escape.rs` only ever makes one
                    // of a `Pattern::Name` target — so there is no fast path
                    // to try before falling to the general lowering.
                    let value = match &binding.value {
                        Some(expr) => emit_expr(builder, scope, ctx, expr)?,
                        // A pattern with nothing to destructure is a syntax
                        // error the checker owns, not a case this reaches.
                        None => undefined(builder, ctx),
                    };
                    super::destructure::declare(
                        builder,
                        scope,
                        ctx,
                        &binding.target,
                        value,
                        statement.at,
                    )?;
                    continue;
                };
                // An object literal whose local provably never leaves this
                // function is not built: each property becomes a binding of its
                // own, in source order, and the allocation and its two calls per
                // property go away. `escape.rs` says what "provably" means and
                // what it refuses.
                if super::escape::declare_flattened(
                    builder,
                    scope,
                    ctx,
                    *name,
                    binding.value.as_ref(),
                )? {
                    continue;
                }
                let value = match &binding.value {
                    Some(expr) => {
                        let produced = emit_expr(builder, scope, ctx, expr)?;
                        stored(builder, ctx, *name, produced)
                    }
                    // `let x;` is `undefined`. `const x;` is a syntax error and
                    // `var x;` is hoisted, and neither of those is decided here
                    // — the first is an early error and the second is a rule
                    // about where the declaration goes, not what it stores.
                    None => undefined(builder, ctx),
                };
                super::binding::declare(builder, scope, ctx, *name, value)?;
            }
            Ok(false)
        }

        StmtKind::Block(body) => {
            scope.enter();
            // Declarations in a block are bound before the block runs, so a
            // closure written above one can still reach it — the same reason
            // a function body hoists.
            super::function::hoist(builder, scope, ctx, body)?;
            let mut terminated = false;
            for inner in body {
                if emit_stmt(builder, scope, ctx, loops, inner)? {
                    terminated = true;
                    break;
                }
            }
            scope.leave();
            Ok(terminated)
        }

        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => emit_if(
            builder,
            scope,
            ctx,
            loops,
            condition,
            then_branch,
            else_branch.as_deref(),
        ),

        // `debugger` with no debugger attached is specified to do nothing, and
        // "nothing" is the whole implementation rather than a gap.
        StmtKind::Debugger => Ok(false),

        // Everything that needs a second block. Named individually rather than
        // as "control flow" because they do not all need the same mechanism:
        // `if` needs a branch and a merge, a loop needs block parameters for
        // every local it rebinds, and `try` needs a protected region.
        StmtKind::While { condition, body } => {
            loops::emit_while(builder, scope, ctx, loops, condition, body, None)
        }
        StmtKind::DoWhile { body, condition } => {
            loops::emit_do_while(builder, scope, ctx, loops, body, condition, None)
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => loops::emit_for(
            builder,
            scope,
            ctx,
            loops,
            init.as_ref(),
            test.as_ref(),
            update.as_ref(),
            body,
            None,
        ),
        StmtKind::ForEach {
            source,
            target,
            subject,
            body,
        } => match source {
            crate::syntax::ForEachSource::In => super::foreach::emit_for_each(
                builder,
                scope,
                ctx,
                loops,
                statement,
                target,
                subject,
                body,
                None,
                crate::runtime::RuntimeOp::OwnKeys,
            ),
            // `for-of` is the same expansion over a different sequence: the
            // keys become the elements, and everything the loop already gets
            // right is got right once.
            crate::syntax::ForEachSource::Of => super::foreach::emit_for_each(
                builder,
                scope,
                ctx,
                loops,
                statement,
                target,
                subject,
                body,
                None,
                crate::runtime::RuntimeOp::Iterate,
            ),
            crate::syntax::ForEachSource::AwaitOf => super::expr::gap("`for await`"),
        },
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            super::switch::emit_switch(builder, scope, ctx, loops, statement, discriminant, clauses)
        }
        // Both spellings go through one path. An unlabelled jump takes the
        // innermost frame that can accept it; a labelled one takes the frame
        // carrying that name.
        StmtKind::Break(label) => loops::emit_jump_out(builder, scope, loops, true, *label),
        StmtKind::Continue(label) => loops::emit_jump_out(builder, scope, loops, false, *label),
        StmtKind::Labelled { label, body } => {
            emit_labelled(builder, scope, ctx, loops, *label, body)
        }
        StmtKind::Throw(value) => super::protect::emit_throw(builder, scope, ctx, value),
        StmtKind::Try {
            body,
            catch,
            finally,
        } => super::protect::emit_try(
            builder,
            scope,
            ctx,
            loops,
            body,
            catch.as_ref(),
            finally.as_deref(),
        ),
        // Already bound and already emitted, by the hoisting pass that ran
        // before this body did. Emitting it here as well would make a second
        // closure over the same code and rebind the name to it, which is
        // wasteful and — for a name something else already captured — wrong.
        StmtKind::Function(_) => Ok(false),
        StmtKind::Class(class) => {
            // A declaration binds its name; an expression does not. That is the
            // only difference between the two, which is why one function emits
            // both and this arm is three lines rather than a second lowering.
            let value = super::class::emit_class(builder, scope, ctx, class)?;
            if let Some(name) = class.name {
                super::binding::declare(builder, scope, ctx, name, value)?;
            }
            Ok(false)
        }
        // The cleanup a `using` needs exists now -- a cleanup is a piece rather
        // than a block, which is what `finally` was waiting on and this was
        // waiting on with it. What is still missing is the other half: disposal
        // is `x[Symbol.dispose]()`, and there is no `Symbol` in this engine, so
        // there is no way to name the method to call.
        //
        // Emitting the region without the call would be a `using` that scopes
        // correctly and disposes nothing -- which reads as working and is the
        // one failure a resource construct must not have.
        StmtKind::Using { .. } => super::expr::gap("`using`, which needs `Symbol.dispose`"),
        StmtKind::With { .. } => super::expr::gap("`with`"),
    }
}

/// Emits `if`.
///
/// # The part that is actually the work
///
/// Not the branch. The branch is three calls. The work is that a local assigned
/// in one arm and not the other has **two definitions reaching its use**, and
/// the IR is in SSA form, so there is no such thing as "the variable" to write
/// twice. The machine's answer is a block parameter, and finding which names
/// need one means comparing the environment along both paths.
///
/// ```text
///     if (c) { x = 1 } else { x = 2 }
///     use(x)                              ← which x?
/// ```
///
/// The join block takes one parameter per name the two arms disagree about, and
/// each arm passes its own value. That is the whole mechanism, and it is the
/// same one loops will need — a loop is a join whose second predecessor has not
/// been emitted yet.
///
/// # Why the join block is created late
///
/// A block with no predecessors and no terminator is malformed, and both arms
/// ending in `return` is ordinary JavaScript. Creating the join only when
/// something can reach it means that case emits nothing rather than emitting an
/// unreachable block for a verifier to complain about.
fn emit_if(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    condition: &crate::syntax::Expr,
    then_branch: &Stmt,
    else_branch: Option<&Stmt>,
) -> EmitResult<bool> {
    let cond = emit_condition(builder, scope, ctx, condition)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let before = scope.snapshot();
    builder.branch(cond, (then_block, &[]), (else_block, &[]))?;

    builder.switch_to(then_block);
    let then_terminated = emit_stmt(builder, scope, ctx, loops, then_branch)?;
    let after_then = scope.snapshot();
    // Where the arm ENDED, which is not where it started as soon as anything
    // nests inside it. Asking the builder rather than assuming `then_block` is
    // the difference between a nested `if` working and a second terminator
    // being appended to a block that already has one.
    let then_exit = builder.current();

    // The second arm starts from the environment the first one started in, not
    // from what it left. Without this, `if (c) { x = 1 } else { y = x }` would
    // read the value the other branch produced.
    scope.restore(&before);
    builder.switch_to(else_block);
    let else_terminated = match else_branch {
        Some(statement) => emit_stmt(builder, scope, ctx, loops, statement)?,
        None => false,
    };
    let after_else = scope.snapshot();

    if then_terminated && else_terminated {
        // Nothing follows. `if (c) return 1; else return 2;` is a whole
        // function, and a join here would be a block nothing jumps to.
        return Ok(true);
    }

    let join = builder.create_block();

    // Only names the two arms disagree about need a parameter. A name neither
    // touched has one definition, and giving it a parameter anyway would be a
    // correct program that moves a value through a register for no reason.
    // The one thing a statement's merge has that an expression's does not: an
    // arm that cannot reach the join. Nothing merges then, because whatever the
    // other arm bound is the only definition that arrives.
    let merged = match (then_terminated, else_terminated) {
        (false, false) => super::merge::disagreements(&after_then, &after_else),
        _ => Vec::new(),
    };
    let params = super::merge::parameters(builder, join, &merged, &after_then);

    if !else_terminated {
        let args: Vec<_> = merged.iter().map(|&at| after_else[at].value()).collect();
        builder.jump(join, &args)?;
    }
    if !then_terminated {
        builder.switch_to(then_exit);
        let args: Vec<_> = merged.iter().map(|&at| after_then[at].value()).collect();
        builder.jump(join, &args)?;
    }

    // Started from the arm that REACHED the join, which is the other one when
    // an arm returned — the reason `settle` takes the environment rather than
    // choosing one.
    let after = if then_terminated {
        after_else
    } else {
        after_then
    };
    super::merge::settle(scope, after, &merged, params);

    builder.switch_to(join);
    Ok(false)
}

/// Emits a labelled statement.
///
/// # Why the label reaches the loop rather than wrapping it
///
/// `outer: while (c) { … continue outer; … }` continues the loop. If the label
/// were a block *around* the loop, `continue outer` would have nothing to
/// continue — the wrapper is not a loop — and the jump would either be refused
/// or, worse, reach the wrapper's exit and turn a `continue` into a `break`.
///
/// So a label on a loop is handed to the loop, which records it on the frame it
/// was going to push anyway. Only a label on something that is *not* a loop
/// needs a frame of its own, and that one can be broken out of and not
/// continued.
fn emit_labelled(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    label: crate::names::Name,
    body: &Stmt,
) -> EmitResult<bool> {
    match &body.kind {
        StmtKind::While { condition, body } => {
            loops::emit_while(builder, scope, ctx, loops, condition, body, Some(label))
        }
        StmtKind::DoWhile { body, condition } => {
            loops::emit_do_while(builder, scope, ctx, loops, body, condition, Some(label))
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => loops::emit_for(
            builder,
            scope,
            ctx,
            loops,
            init.as_ref(),
            test.as_ref(),
            update.as_ref(),
            body,
            Some(label),
        ),
        StmtKind::ForEach {
            source,
            target,
            subject,
            body: inner,
        } => {
            // A label reaches both spellings, and which sequence is walked is
            // the only difference between them.
            let over = match source {
                crate::syntax::ForEachSource::In => crate::runtime::RuntimeOp::OwnKeys,
                crate::syntax::ForEachSource::Of => crate::runtime::RuntimeOp::Iterate,
                crate::syntax::ForEachSource::AwaitOf => {
                    return super::expr::gap("a labelled `for await`");
                }
            };
            super::foreach::emit_for_each(
                builder,
                scope,
                ctx,
                loops,
                body,
                target,
                subject,
                inner,
                Some(label),
                over,
            )
        }
        _ => loops::emit_labelled_block(builder, scope, ctx, loops, label, body),
    }
}
