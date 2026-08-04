//! Operators with one operand, and the two that also write back.
//!
//! # Why `-x` is emitted as `x * -1`
//!
//! Because that is what it means, not because it is convenient. Unary minus is
//! `ToNumeric` of the operand followed by negation, and multiplication is
//! `ToNumeric` of both followed by a product — with a literal on the right,
//! whose conversion has no operand to observe and no side effect to order,
//! those are the same sequence of observable steps.
//!
//! The alternative considered and rejected was `0 - x`, which is wrong: `-(0)`
//! is `-0` and `0 - 0` is `+0`. The two are distinguishable — `1 / -0` is
//! `-Infinity` — so it is a real difference and not a pedantic one.
//!
//! What this buys is rule 3. `*` already decides between an instruction and a
//! call from what was proved about its operands, already emits the guard when
//! nothing was, and already knows which runtime symbol the slow path dials.
//! Restating any of that here would be a second definition of numeric
//! coercion, and the second one is the one that goes stale.
//!
//! # Why `++` is `- -1` and not `+ 1`
//!
//! `+` may concatenate. `x++` where `x` is `"5"` is `6`, and `"5" + 1` is
//! `"51"`. Subtraction has no such second meaning, so subtracting negative one
//! is the increment — and on two proven doubles it lowers to exactly the
//! instruction the spelling suggests.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::choice;
use super::expr::{self, emit_condition, emit_expr};
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{BinaryOp, Expr, ExprKind, UnaryOp, UpdateOp, UpdatePosition};

/// Emits a unary operator.
pub fn emit_unary(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: UnaryOp,
    operand: &Expr,
) -> EmitResult<ValueId> {
    match op {
        // `!` is truthiness answered backwards, and truthiness is the one
        // conversion that already had to exist for a branch. So this is that
        // call and a branch, with nothing new asked of the runtime.
        UnaryOp::Not => {
            let cond = emit_condition(builder, scope, ctx, operand)?;
            choice::from_bool(builder, cond, true)
        }

        // The operand is still emitted. `void f()` calls `f`; the operator
        // discards the result, not the evaluation.
        UnaryOp::Void => {
            emit_expr(builder, scope, ctx, operand)?;
            Ok(expr::undefined(builder, ctx))
        }

        UnaryOp::Negate => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            let minus_one = expr::number_constant(builder, -1.0);
            expr::emit_binary(builder, ctx, BinaryOp::Mul, value, minus_one)
        }

        // Unary plus is not an identity: `+"3"` is `3`. It is `ToNumber`
        // spelled short, and multiplying by one is that conversion with a
        // product the identity of which leaves every case — `-0`, `NaN`,
        // infinities — where it started.
        UnaryOp::Plus => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            let one = expr::number_constant(builder, 1.0);
            expr::emit_binary(builder, ctx, BinaryOp::Mul, value, one)
        }

        // `~` is `ToInt32` then complement, and `ToInt32` is a truncation with
        // its own rules for infinities and for values past 2^31 — a conversion
        // the runtime does not define. Emitting an integer complement of a
        // double would be wrong for every operand that is not already a small
        // integer, and wrong silently.
        UnaryOp::BitNot => expr::gap("`~`"),
        // Answers a string, and a string is a heap value this module cannot
        // yet materialise.
        UnaryOp::TypeOf => expr::gap("`typeof`"),
        // Takes a reference rather than a value, and removing a property is an
        // operation the runtime does not define.
        UnaryOp::Delete => expr::gap("`delete`"),
    }
}

/// Emits `++` or `--`.
///
/// # What the expression produces
///
/// Both spellings read the target, coerce it through `ToNumeric`, and write the
/// stepped value back. What differs is only which of the two the expression
/// yields — and the old one is the *coerced* old one, so `let s = "5"; s++`
/// leaves `s` at `6` and evaluates to `5` rather than to `"5"`.
pub fn emit_update(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: UpdateOp,
    position: UpdatePosition,
    target: &Expr,
) -> EmitResult<ValueId> {
    // Subtracted rather than added, so that neither spelling can concatenate.
    let step = match op {
        UpdateOp::Increment => -1.0,
        UpdateOp::Decrement => 1.0,
    };

    match &target.kind {
        ExprKind::Ident(name) => {
            let current = super::binding::read(builder, scope, ctx, *name)?;
            let (before, after) = step_value(builder, ctx, current, step)?;
            super::binding::write(builder, scope, ctx, *name, after)?;
            Ok(match position {
                UpdatePosition::Prefix => after,
                UpdatePosition::Postfix => before,
            })
        }

        ExprKind::Member {
            object, property, ..
        } => {
            // The receiver is evaluated ONCE. `f().x++` calls `f` once, and
            // re-emitting the object for the write is the rewrite that breaks
            // it — invisibly, for every receiver without a side effect.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            let current = expr::emit_read(builder, ctx, receiver, *property)?;
            let (before, after) = step_value(builder, ctx, current, step)?;
            expr::emit_write(builder, ctx, receiver, *property, after)?;
            Ok(match position {
                UpdatePosition::Prefix => after,
                UpdatePosition::Postfix => before,
            })
        }

        _ => expr::gap("`++` or `--` on anything but a local or a property"),
    }
}

/// The coerced old value and the stepped new one.
///
/// Both are needed whichever spelling was written, because the write always
/// takes the new value and the postfix result is the coerced old one. Returning
/// the pair rather than branching on the position keeps that fact in one place.
fn step_value(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    current: ValueId,
    step: f64,
) -> EmitResult<(ValueId, ValueId)> {
    // `ToNumeric`, spelled as the multiplication that already implements it.
    let one = expr::number_constant(builder, 1.0);
    let before = expr::emit_binary(builder, ctx, BinaryOp::Mul, current, one)?;
    let step = expr::number_constant(builder, step);
    let after = expr::emit_binary(builder, ctx, BinaryOp::Sub, before, step)?;
    Ok((before, after))
}
