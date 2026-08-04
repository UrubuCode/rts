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

use super::expr::{emit_expr, undefined};
use super::{EmitError, EmitResult, Scope};
use crate::syntax::{Pattern, Stmt, StmtKind};
use crate::values::ValueModel;

/// Emits a statement. Returns whether it terminated the block.
pub fn emit_stmt(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    model: &ValueModel,
    statement: &Stmt,
) -> EmitResult<bool> {
    match &statement.kind {
        // The value is discarded and the expression is still emitted: an
        // expression statement exists for its side effects, and dropping it
        // because nothing reads the result would drop those too.
        StmtKind::Expr(expr) => {
            emit_expr(builder, scope, model, expr)?;
            Ok(false)
        }

        StmtKind::Empty => Ok(false),

        StmtKind::Return(value) => {
            let result = match value {
                Some(expr) => emit_expr(builder, scope, model, expr)?,
                // `return;` yields `undefined`, not "no value". The signature
                // declares one return, and a JavaScript function always
                // produces something.
                None => undefined(builder, model),
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
                    Some(expr) => emit_expr(builder, scope, model, expr)?,
                    // `let x;` is `undefined`. `const x;` is a syntax error and
                    // `var x;` is hoisted, and neither of those is decided here
                    // — the first is an early error and the second is a rule
                    // about where the declaration goes, not what it stores.
                    None => undefined(builder, model),
                };
                scope.declare(*name, value);
            }
            Ok(false)
        }

        StmtKind::Block(body) => {
            scope.enter();
            let mut terminated = false;
            for inner in body {
                if emit_stmt(builder, scope, model, inner)? {
                    terminated = true;
                    break;
                }
            }
            scope.leave();
            Ok(terminated)
        }

        // `debugger` with no debugger attached is specified to do nothing, and
        // "nothing" is the whole implementation rather than a gap.
        StmtKind::Debugger => Ok(false),

        // Everything that needs a second block. Named individually rather than
        // as "control flow" because they do not all need the same mechanism:
        // `if` needs a branch and a merge, a loop needs block parameters for
        // every local it rebinds, and `try` needs a protected region.
        StmtKind::If { .. } => gap("`if`"),
        StmtKind::While { .. } => gap("`while`"),
        StmtKind::DoWhile { .. } => gap("`do`/`while`"),
        StmtKind::For { .. } => gap("`for`"),
        StmtKind::ForEach { .. } => gap("`for-in` or `for-of`"),
        StmtKind::Switch { .. } => gap("`switch`"),
        StmtKind::Break(_) => gap("`break`"),
        StmtKind::Continue(_) => gap("`continue`"),
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
