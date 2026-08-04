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

    if arguments.len() > ARGUMENT_SLOTS {
        return Err(EmitError::Unsupported {
            construct: "a call with more than four arguments",
        });
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
