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
        //
        // That argument is about the MULTIPLY, and it was taken to mean this
        // operator has no proven form at all — `proven.rs` says so in as many
        // words, and `sign = -sign` in a loop paid a runtime call every pass
        // because of it. A sign flip over a value already proven to be a double
        // is neither a multiply nor reachable by a bigint: `Repr::F64` is the
        // proof that the operand is not one. So the instruction is emitted
        // there and the call stays for everything else.
        UnaryOp::Negate => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            if builder.repr_of(value) == rts_cranelift::repr::Repr::F64 {
                let flipped = builder.float_unary(rts_cranelift::ir::FloatOp::Neg, value)?;
                return Ok(flipped);
            }
            Ok(expr::call(builder, ctx, RuntimeOp::Negate, &[value])?[0])
        }

        // Unary plus is not an identity in the generic domain: `+"3"` is `3`.
        // Once the operand is already a proven number, ToNumber has no
        // observable work left. F64 returns unchanged (which preserves -0);
        // I32 widens directly to the numeric representation. Both remove the
        // multiply-by-one from the carried chain while preserving the generic
        // path for strings, objects and other values.
        UnaryOp::Plus => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            match builder.repr_of(value) {
                rts_cranelift::repr::Repr::F64 => return Ok(value),
                rts_cranelift::repr::Repr::I32 => return Ok(builder.to_f64(value)?),
                _ => {}
            }
            let one = expr::number_constant(builder, 1.0);
            expr::emit_binary(builder, ctx, BinaryOp::Mul, value, one)
        }

        // `~` is `ToInt32` then complement.
        //
        // # The reason this was a call is gone, and it said so twice
        //
        // The comment here read "`ToInt32` is a truncation with its own rules
        // for infinities and for values past 2^31 — a conversion the runtime
        // does not define", and then said it again in a second, truncated
        // paragraph ("which is why it is the runtime.s and not an integer
        // instruction emitted here"). A rule written twice is a rule that will
        // be written differently, and here it was also written STALE: the
        // premise stopped being true when `Inst::ToInt32` arrived, which is the
        // same instruction that made `&`, `|` and `^` reachable. Its own
        // documentation carries why the code generator's conversions could not
        // be used and this one could.
        //
        // # Why `^ -1` and not a complement instruction
        //
        // `BitOp` has no `Not`, and it does not need one: complementing every
        // bit of a two's-complement integer IS exclusive-or with all ones, on
        // every width and every target. Adding a variant for it would be a
        // second spelling of an operation the machine already performs — and
        // `-1` as an `I32` is exactly all ones.
        //
        // Measured before this, `analytic.ts`: `int not` 9.99 ns against 6.75
        // for `int and`, `int or` and `int xor` — the same conversion pair
        // around a call instead of around an instruction.
        //
        // The call stays for anything not proven, which is what `emit_binary`'s
        // guarded path does for the binary bitwise operators and what rule 5
        // asks for: what cannot be proven becomes generic, visibly.
        UnaryOp::BitNot => {
            let value = emit_expr(builder, scope, ctx, operand)?;
            if builder.repr_of(value) == rts_cranelift::repr::Repr::F64 {
                let bits = builder.to_int32(value)?;
                let ones = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                    repr: rts_cranelift::repr::Repr::I32,
                    bits: rts_cranelift::ir::ScalarBits(u32::MAX as u64),
                });
                let ones = builder.use_const(ones);
                let flipped = builder.bitwise(rts_cranelift::ir::BitOp::Xor, bits, ones)?;
                return Ok(builder.to_f64(flipped)?);
            }
            Ok(expr::call(builder, ctx, RuntimeOp::BitNot, &[value])?[0])
        }
        // Answers a string, which the runtime makes — and which of the eight
        // words it is depends on a cell's header rather than on the tag, since
        // a function and an object are both references.
        UnaryOp::TypeOf => {
            // The one read in the language that does not throw for a name
            // nothing declared, because it takes a REFERENCE rather than a
            // value. So a bare name here goes to the global object even when
            // reading it anywhere else would be refused — `typeof maybe` is how
            // a program asks whether something exists, and refusing it would
            // refuse the question itself.
            let value = typeof_operand(builder, scope, ctx, operand)?;
            Ok(expr::call(builder, ctx, RuntimeOp::TypeOf, &[value])?[0])
        }
        // Takes a reference rather than a value, and removing a property is an
        // operation the runtime does not define.
        // Takes a REFERENCE rather than a value, which is why it matches on the
        // operand's shape instead of emitting it: `delete o.x` addresses the
        // property, not what it holds.
        UnaryOp::Delete => {
            // `delete a?.b` — the chain short-circuits to TRUE rather than to
            // `undefined`, because a chain that never reached a property did
            // not fail to delete one. Its own join for that reason: the value
            // that leaves it is a boolean, not the value of a read.
            //
            // The link's own `?.` is what is tested. A nullish object DEEPER in
            // the chain still reaches the delete, which is the narrower claim
            // and the one this handles: `delete a?.b.c` with a nullish `a` is
            // the shape left over, and it is written here rather than
            // discovered.
            if let ExprKind::Chain(inner) = &operand.kind
                && let ExprKind::Member { object, property, optional: true } = &inner.kind
            {
                let text = ctx.names.text(*property).to_owned();
                let key = expr::string_literal(builder, ctx, &text)?;
                return delete_optional(builder, scope, ctx, object, key);
            }
            if let ExprKind::Chain(inner) = &operand.kind
                && let ExprKind::Index { object, index, optional: true } = &inner.kind
            {
                let key = emit_expr(builder, scope, ctx, index)?;
                return delete_optional(builder, scope, ctx, object, key);
            }
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
                // `delete <anything else>` is not a property removal at all:
                // the specification evaluates the operand and answers TRUE,
                // because there was no reference to remove. `delete (1 + 1)` is
                // legal JavaScript and this REFUSED THE WHOLE PROGRAM for it —
                // a compile error for an expression the language defines.
                //
                // `delete x` on a name is the one shape that answers false: a
                // declared binding cannot be deleted. Strict code makes it an
                // early error, which is the checker's to raise.
                _ => {
                    let bound = matches!(&operand.kind, ExprKind::Ident(name)
                        if scope.lookup(*name).is_some());
                    emit_expr(builder, scope, ctx, operand)?;
                    return Ok(expr::boolean_constant(builder, !bound));
                }
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

/// `delete a?.b` and `delete a?.[k]` — the delete a nullish object skips.
///
/// Its own join, and the value that leaves it is a BOOLEAN: a chain that never
/// reached a property answers `true`, because it did not fail to remove one.
/// That is the whole difference from an ordinary optional read, whose join
/// carries `undefined`, and it is why this does not reuse `optional::walk`.
fn delete_optional(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    object: &Expr,
    key: ValueId,
) -> EmitResult<ValueId> {
    let receiver = emit_expr(builder, scope, ctx, object)?;
    let join = builder.create_block();
    let answer = builder.add_block_param(join, super::UNPROVEN);
    let nullish = builder.create_block();
    let present = builder.create_block();
    super::choice::branch_on_nullish(builder, ctx, receiver, nullish, present)?;

    builder.switch_to(nullish);
    let skipped = expr::boolean_constant(builder, true);
    builder.jump(join, &[skipped])?;

    builder.switch_to(present);
    let gone = expr::call(builder, ctx, RuntimeOp::DeleteProperty, &[receiver, key])?[0];
    let gone = builder.widen(gone);
    builder.jump(join, &[gone])?;

    builder.switch_to(join);
    Ok(answer)
}

/// The value `typeof` is applied to, with every rule that operand has.
///
/// Shared with `expr::typeof_equals_literal`, which fuses
/// `typeof x === "string"` into ONE crossing and must reach the operand the
/// same way. Emitting it there instead would have been a second statement of
/// the exemptions below, and the second statement is where `typeof maybe`
/// starts throwing for a name the first one lets through.
pub(super) fn typeof_operand(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    operand: &Expr,
) -> EmitResult<ValueId> {
    Ok(match &operand.kind {
        // `NaN`, `Infinity` and `undefined` are unbound as far as
        // `scope` is concerned — they are the emitter's own constants,
        // not a declared binding — so this arm must offer them the
        // same way `binding::read` does before falling to the global
        // object. Skipping straight to `force_read` answered
        // `typeof NaN` from a property named `"NaN"` that no global
        // object holds, i.e. `undefined`.
        // `typeof` is exempt from the error an UNDECLARED name raises,
        // and is NOT exempt from the temporal dead zone: the name is
        // declared, so there is no reference to take — which is why
        // this arm asks before the exemption below applies.
        ExprKind::Ident(name) if scope.in_dead_zone(*name) => {
            return super::binding::read(builder, scope, ctx, *name);
        }
        ExprKind::Ident(name) if scope.lookup(*name).is_none() => {
            match super::binding::predefined(builder, ctx, *name) {
                Some(value) => value,
                None => super::globals::force_read(builder, ctx, *name)?,
            }
        }
        _ => emit_expr(builder, scope, ctx, operand)?,
    })
}
