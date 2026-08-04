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

use super::expr::{emit_condition, emit_expr, undefined};
use super::loops::{self, Loops};
use super::scope::Binding;
use super::UNPROVEN;
use super::{Ctx, EmitError, EmitResult, Scope};
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
                Some(expr) => emit_expr(builder, scope, ctx, expr)?,
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
                    return Err(EmitError::Unsupported {
                        construct: "a destructuring declaration",
                    });
                };
                let value = match &binding.value {
                    Some(expr) => emit_expr(builder, scope, ctx, expr)?,
                    // `let x;` is `undefined`. `const x;` is a syntax error and
                    // `var x;` is hoisted, and neither of those is decided here
                    // — the first is an early error and the second is a rule
                    // about where the declaration goes, not what it stores.
                    None => undefined(builder, ctx),
                };
                scope.declare(*name, value);
            }
            Ok(false)
        }

        StmtKind::Block(body) => {
            scope.enter();
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
        } => emit_if(builder, scope, ctx, loops, condition, then_branch, else_branch.as_deref()),

        // `debugger` with no debugger attached is specified to do nothing, and
        // "nothing" is the whole implementation rather than a gap.
        StmtKind::Debugger => Ok(false),

        // Everything that needs a second block. Named individually rather than
        // as "control flow" because they do not all need the same mechanism:
        // `if` needs a branch and a merge, a loop needs block parameters for
        // every local it rebinds, and `try` needs a protected region.
        StmtKind::While { condition, body } => {
            loops::emit_while(builder, scope, ctx, loops, condition, body)
        }
        StmtKind::DoWhile { body, condition } => {
            loops::emit_do_while(builder, scope, ctx, loops, body, condition)
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
        ),
        StmtKind::ForEach { .. } => gap("`for-in` or `for-of`"),
        StmtKind::Switch { .. } => gap("`switch`"),
        // A label is refused, so an unlabelled break is the only kind that
        // reaches here — and the innermost loop is where it goes.
        StmtKind::Break(None) => loops::emit_jump_out(builder, scope, loops, true),
        StmtKind::Continue(None) => loops::emit_jump_out(builder, scope, loops, false),
        StmtKind::Break(Some(_)) | StmtKind::Continue(Some(_)) => gap("a labelled jump"),
        StmtKind::Labelled { .. } => gap("a label"),
        StmtKind::Throw(_) => gap("`throw`"),
        StmtKind::Try { .. } => gap("`try`"),
        StmtKind::Function(_) => gap("a function declaration"),
        StmtKind::Class(_) => gap("a class declaration"),
        StmtKind::Using { .. } => gap("`using`"),
        StmtKind::With { .. } => gap("`with`"),
    }
}

/// A named gap.
fn gap(construct: &'static str) -> EmitResult<bool> {
    Err(EmitError::Unsupported { construct })
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
    let merged: Vec<usize> = match (then_terminated, else_terminated) {
        (false, false) => (0..after_then.len())
            .filter(|&position| after_then[position] != after_else[position])
            .collect(),
        // One arm cannot reach the join, so nothing merges: whatever the other
        // arm bound is the only definition that arrives.
        _ => Vec::new(),
    };
    let params: Vec<_> = merged
        .iter()
        .map(|_| builder.add_block_param(join, UNPROVEN))
        .collect();

    if !else_terminated {
        let args: Vec<_> = merged.iter().map(|&at| value_of(after_else[at])).collect();
        builder.jump(join, &args)?;
    }
    if !then_terminated {
        builder.switch_to(then_exit);
        let args: Vec<_> = merged.iter().map(|&at| value_of(after_then[at])).collect();
        builder.jump(join, &args)?;
    }

    // What each name means after the `if`: the merged ones are the join's
    // parameters, and the rest are whatever survived from the arm that reached
    // it.
    let mut after = if then_terminated { after_else } else { after_then };
    for (position, param) in merged.iter().zip(params) {
        after[*position] = Binding::Value(param);
    }
    scope.restore(&after);

    builder.switch_to(join);
    Ok(false)
}

/// The value behind a binding.
fn value_of(binding: Binding) -> rts_cranelift::ir::ValueId {
    match binding {
        Binding::Value(value) => value,
    }
}
