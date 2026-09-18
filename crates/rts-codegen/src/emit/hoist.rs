//! Function declarations, bound before the body that declares them runs.
//!
//! Split out of `function.rs`, which was far over the crate's 1000-line
//! ceiling, when the pickle needed the entry body to register its named
//! functions right after they are hoisted — `serde_names` is that half.

use rts_cranelift::ir::FuncBuilder;

use super::{Ctx, EmitResult, Scope};
use super::{binding, expr};
use crate::syntax::{Stmt, StmtKind};

/// Binds every function declared directly in a body, before the body runs.
///
/// # Why this is not just emitting the declaration where it was written
///
/// `function f() { return f(); }` reads `f` inside `f`, and mutual recursion
/// reads the second function before the first has been written. Both are
/// ordinary JavaScript, and both are the reason declarations are hoisted rather
/// than evaluated in order.
///
/// Hoisting is done per block rather than to the top of the function, which is
/// not quite what the specification says for `var`-like function scoping. It is
/// what makes the cases above work, and the difference shows only for a
/// declaration inside a nested block referenced before that block — named here
/// so the gap is a sentence rather than a surprise.
pub fn hoist(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
) -> EmitResult<()> {
    // Two passes, and the first one is the whole point: every name is bound
    // before any body is emitted, so a closure made in the second pass can
    // already see the ones declared after it.
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let placeholder = expr::undefined(builder, ctx);
            binding::declare(builder, scope, ctx, name, placeholder)?;
        }
    }
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let closure = super::function::emit_closure_declared(builder, scope, ctx, function)?;
            binding::write(builder, scope, ctx, name, closure)?;
        }
    }
    Ok(())
}
