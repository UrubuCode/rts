//! Calling something.
//!
//! # What a call site decides, and what it must not
//!
//! It decides two things the callee cannot: what `this` is, and how many
//! arguments were written. Everything else — whether the callee is code at all,
//! which code, and what happens when it is not — belongs to the runtime, and
//! this module deliberately does not look.
//!
//! # `this` comes from the *spelling*, not from the value
//!
//! `f(a)` and `o.f(a)` pass the same arguments to the same function and differ
//! only in the receiver, and which one was written is a syntactic fact this is
//! the last place that still knows. `o.f()` is the only form that has a
//! receiver; everything else passes `undefined`.
//!
//! That is why a member call is not "emit the callee, then call it": the
//! receiver has to be kept, and it has to be evaluated **once**. `f().g()` calls
//! `f` a single time, and an implementation that emitted the object expression
//! for the read and again for the receiver would call it twice — invisibly, for
//! every receiver without a side effect.
//!
//! # Why the arity is padded here
//!
//! Because the callee has a fixed shape and the call does not know which
//! function it will reach. A call passing three arguments to a function that
//! declared one is ordinary JavaScript, so the missing slots are filled with
//! `undefined` at the site rather than being the callee's problem — the callee
//! cannot have a problem, since its parameters exist whether or not anything
//! was passed.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{self, emit_expr};
use super::{Ctx, EmitError, EmitResult, Scope};
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{Expr, ExprKind, Spreadable};

/// Emits a call.
pub fn emit_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    // The receiver and the callee, in the order the language evaluates them:
    // the member expression first, then the arguments.
    let (receiver, function) = match &callee.kind {
        ExprKind::Member {
            object,
            property,
            optional,
        } => {
            if *optional {
                return Err(EmitError::Unsupported {
                    construct: "an optional call",
                });
            }
            let receiver = emit_expr(builder, scope, ctx, object)?;
            let function = expr::emit_read(builder, ctx, receiver, *property)?;
            (receiver, function)
        }
        _ => {
            // A plain call has no receiver, and `undefined` is what the
            // specification passes in strict code. Sloppy mode substitutes the
            // global object, which does not exist here — named as the reason
            // rather than left as a coincidence, because the day globals arrive
            // this line is one of the two that has to change.
            let function = emit_expr(builder, scope, ctx, callee)?;
            let undefined = expr::undefined(builder, ctx);
            (undefined, function)
        }
    };

    emit_call_with(builder, scope, ctx, function, receiver, arguments)
}

/// Emits a call whose callee and receiver are already values.
///
/// Split out for `super(…)`, which has both and no member expression to derive
/// them from: the callee comes from the class environment and the receiver is
/// the `this` the constructor was handed. Everything after that — the arity
/// check, the order, the padding — is the same rule, and a second copy of it is
/// where a `super` call would come to pad differently from a method call.
pub fn emit_call_with(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    receiver: ValueId,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    // Past what the convention carries, the arguments go in an array and the
    // runtime holds it for the activation. The common call is unchanged and
    // allocates nothing — which is the whole reason this is a second operation
    // rather than the only one.
    // A spread takes this path whatever the written count is, because how many
    // values it contributes is not known while compiling — one written
    // argument may become none or nine.
    if arguments.len() > ARGUMENT_SLOTS || has_spread(arguments) {
        let vector = emit_argument_vector(builder, scope, ctx, arguments)?;
        return Ok(expr::call(
            builder,
            ctx,
            RuntimeOp::CallWithArgs,
            &[function, receiver, vector],
        )?[0]);
    }

    let mut passed = Vec::with_capacity(2 + ARGUMENT_SLOTS);
    passed.push(function);
    passed.push(receiver);
    for argument in arguments {
        let Spreadable::Single(value) = argument else {
            return Err(EmitError::Unsupported {
                construct: "a spread argument",
            });
        };
        passed.push(emit_expr(builder, scope, ctx, value)?);
    }
    // Padded after the written arguments were evaluated, so the padding cannot
    // get between two of them and change the order side effects happen in.
    while passed.len() < 2 + ARGUMENT_SLOTS {
        let undefined = expr::undefined(builder, ctx);
        passed.push(undefined);
    }

    Ok(expr::call(builder, ctx, RuntimeOp::Call, &passed)?[0])
}

/// Emits `new f(…)`.
///
/// # Why this is not a call with a flag
///
/// It does not pass a receiver — it *makes* one, from the callee's `prototype`
/// — and it answers the object rather than what the callee returned, unless the
/// callee returned an object of its own. Three differences, none of which a
/// flag on a call site could express without the call site knowing all of them.
///
/// So the arguments are padded exactly as a call's are, and everything else is
/// the runtime's.
pub fn emit_construct(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    let function = emit_expr(builder, scope, ctx, callee)?;
    emit_construction(builder, scope, ctx, function, arguments, RuntimeOp::Construct)
}

/// The same, for a callee that is already a value.
///
/// `super(…)` needs it: the parent comes from the class environment rather than
/// from an expression, and it reaches a **different** runtime operation —
/// `super()` must not establish a new `new.target`, because the object the chain
/// builds has to inherit from the prototype of the class `new` actually named.
/// Everything else — the arity, the order, the padding — is one rule, and a
/// second copy is where the two would come to pad differently.
pub fn emit_super_construct(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    emit_construction(builder, scope, ctx, function, arguments, RuntimeOp::SuperConstruct)
}

/// The written arguments, as an ordinary array.
///
/// An array literal rather than a second kind of storage: the elements are
/// evaluated in source order and written at their own indices, which is what
/// `[a, b, c]` already does — so a call with six arguments and an array literal
/// of six elements produce the same thing, and there is one answer to what a
/// sequence of values is.
pub(super) fn emit_argument_vector(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    // Started empty and appended to, rather than sized once and written at
    // fixed indices: a spread contributes a count nothing knows while
    // compiling, so every index after one is unknown too.
    let length = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(0),
    });
    let length = builder.use_const(length);
    let array = expr::call(builder, ctx, RuntimeOp::ArrayNew, &[length])?[0];
    for argument in arguments {
        let (value, op) = match argument {
            Spreadable::Single(value) => (value, RuntimeOp::ArrayAppend),
            Spreadable::Spread(value) => (value, RuntimeOp::ArrayAppendAll),
        };
        let value = emit_expr(builder, scope, ctx, value)?;
        expr::call(builder, ctx, op, &[array, value])?;
    }
    Ok(array)
}

/// The shared body, differing only in which operation is dialled.
fn emit_construction(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    arguments: &[Spreadable],
    op: RuntimeOp,
) -> EmitResult<ValueId> {
    // Past what the convention carries, the same trade the call side makes.
    // `super()` is deliberately excluded: it must not establish a new
    // `new.target`, and a vector operation that did would put the object on the
    // wrong prototype — so forwarding more than four through `super()` is a
    // named gap rather than a silently different answer.
    if arguments.len() > ARGUMENT_SLOTS || has_spread(arguments) {
        if op == RuntimeOp::SuperConstruct {
            return Err(EmitError::Unsupported {
                construct: "`super()` with more than four arguments or a spread",
            });
        }
        let vector = emit_argument_vector(builder, scope, ctx, arguments)?;
        return Ok(expr::call(
            builder,
            ctx,
            RuntimeOp::ConstructWithArgs,
            &[function, vector],
        )?[0]);
    }

    let mut passed = Vec::with_capacity(1 + ARGUMENT_SLOTS);
    passed.push(function);
    for argument in arguments {
        let Spreadable::Single(value) = argument else {
            return Err(EmitError::Unsupported {
                construct: "a spread argument",
            });
        };
        passed.push(emit_expr(builder, scope, ctx, value)?);
    }
    while passed.len() < 1 + ARGUMENT_SLOTS {
        let undefined = expr::undefined(builder, ctx);
        passed.push(undefined);
    }
    Ok(expr::call(builder, ctx, op, &passed)?[0])
}

/// Whether any argument is a spread.
///
/// Asked before the arity is, because a spread makes the arity unknowable: one
/// written argument may contribute none or nine, so the padded path cannot be
/// taken however few were spelled.
fn has_spread(arguments: &[Spreadable]) -> bool {
    arguments
        .iter()
        .any(|argument| matches!(argument, Spreadable::Spread(_)))
}
