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
use crate::runtime::RuntimeOp;
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

        // A call rather than `x * -1`. The multiply is right for a double and
        // wrong for a bigint: `-1` is a NUMBER, so the product is a mixed
        // operation the language refuses — which made every negative bigint
        // literal `NaN`, silently.
        UnaryOp::Negate => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            Ok(expr::call(builder, ctx, RuntimeOp::Negate, &[value])?[0])
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
        // `ToInt32` then complement. The truncation has its own rules for
        // infinities and for values past 2^31, which is why it is the
        // runtime.s and not an integer instruction emitted here.
        UnaryOp::BitNot => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            Ok(expr::call(builder, ctx, RuntimeOp::BitNot, &[value])?[0])
        }
        // Answers a string, and a string is a heap value this module cannot
        // yet materialise.
        // Answers a string, which the runtime makes — and which of the eight
        // words it is depends on a cell's header rather than on the tag, since
        // a function and an object are both references.
        //
        // `typeof` on an *unbound name* is the one read in the language that
        // does not throw, and that is not implemented: an unbound name is
        // refused before this is reached, because there is no global object for
        // it to be absent from.
        UnaryOp::TypeOf => {
            // The one read in the language that does not throw for a name
            // nothing declared, because it takes a REFERENCE rather than a
            // value. So a bare name here goes to the global object even when
            // reading it anywhere else would be refused — `typeof maybe` is how
            // a program asks whether something exists, and refusing it would
            // refuse the question itself.
            let value = match &operand.kind {
                ExprKind::Ident(name) if scope.lookup(*name).is_none() => {
                    super::globals::force_read(builder, ctx, *name)?
                }
                _ => emit_expr(builder, scope, ctx, operand)?,
            };
            Ok(expr::call(builder, ctx, RuntimeOp::TypeOf, &[value])?[0])
        }
        // Takes a reference rather than a value, and removing a property is an
        // operation the runtime does not define.
        // Takes a REFERENCE rather than a value, which is why it matches on the
        // operand's shape instead of emitting it: `delete o.x` addresses the
        // property, not what it holds.
        UnaryOp::Delete => {
            let (receiver, key) = match &operand.kind {
                ExprKind::Member {
                    object, property, ..
                } => {
                    let receiver = emit_expr(builder, scope, ctx, object)?;
                    // The name as a STRING, because the runtime resolves a
                    // computed key and a written one through the same path.
                    // Handing over the key number instead would be a second
                    // way to say the same thing.
                    let text = ctx.names.text(*property).to_owned();
                    let key = expr::string_literal(builder, ctx, &text)?;
                    (receiver, key)
                }
                ExprKind::Index { object, index, .. } => {
                    let receiver = emit_expr(builder, scope, ctx, object)?;
                    let key = emit_expr(builder, scope, ctx, index)?;
                    (receiver, key)
                }
                // `delete x` on a name is an early error in strict code and a
                // no-op answering false in sloppy. Neither is emitted: the
                // first is the checker's, and the second needs the global
                // object a bare name would be a property of.
                _ => return expr::gap("`delete` of anything but a property"),
            };
            let gone = expr::call(builder, ctx, RuntimeOp::DeleteProperty, &[receiver, key])?[0];
            Ok(builder.widen(gone))
        }
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
            let current = super::property::emit_read(builder, ctx, receiver, *property)?;
            let (before, after) = step_value(builder, ctx, current, step)?;
            super::property::emit_write(builder, ctx, receiver, *property, after)?;
            Ok(match position {
                UpdatePosition::Prefix => after,
                UpdatePosition::Postfix => before,
            })
        }

        ExprKind::Index { object, index, .. } => {
            // The receiver AND the key are evaluated once. `a[f()]++` calls `f`
            // a single time — a rewrite to `a[f()] = a[f()] + 1` calls it
            // twice, which is the same trap the named case records and one step
            // worse, because a computed key can have a side effect of its own.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            let key = emit_expr(builder, scope, ctx, index)?;
            let key = builder.widen(key);
            let current = expr::call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
            let (before, after) = step_value(builder, ctx, current, step)?;
            let stored = builder.widen(after);
            expr::call(
                builder,
                ctx,
                RuntimeOp::SetIndexed,
                &[receiver, key, stored],
            )?;
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
