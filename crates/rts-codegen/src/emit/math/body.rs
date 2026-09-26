//! The seven members and the proof that admits each. See the module header.

use rts_cranelift::ir::{FloatOp, FuncBuilder, NumOp, ValueId};
use rts_cranelift::repr::Repr;

use super::super::expr::emit_expr;
use super::super::{Ctx, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, Spreadable};

/// `Math.sqrt(x)`, its four unary siblings, and `Math.min`/`Math.max` over two
/// operands, as the instruction the hardware has.
///
/// # Three conditions, and each one is a proof rather than a guess
///
/// The program must not disturb `Math` anywhere — `primordial::untouched`,
/// computed over the whole tree before anything was emitted. No enclosing scope
/// may bind the name, which the scope answers exactly. And the argument must
/// ALREADY be a proven double: a guard here would be correct too, but the
/// operand of a square root in a loop is proven by the type pass in the case
/// that matters, and emitting a guard for the rest would cost a branch to
/// discover what the call would have found anyway.
///
/// Answers `None` for anything else, and the ordinary call follows.
///
/// # Why the language decides this and not the machine
///
/// `Inst::FloatUnary` knows nothing about `Math` — rule 2 of the machine's own
/// README, no source-language knowledge there. Which name means a square root
/// is a fact about JavaScript, so it is decided here, in the crate that is
/// allowed to know.
pub(in super::super) fn emit(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    if !ctx.math_primordial {
        return Ok(None);
    }
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return Ok(None);
    };
    let ExprKind::Ident(name) = &object.kind else {
        return Ok(None);
    };
    if ctx.names.text(*name) != "Math" || scope.lookup(*name).is_some() {
        return Ok(None);
    }
    // `Math.random()` takes no argument and answers a double, so it needs no
    // proven operand — only the same whole-program proof. Not an instruction:
    // there is no opcode for a generator. What it skips is the PATH — the
    // property read through the chain cache and the generic call machinery —
    // which is where its 40 ns were, since the generator itself is a
    // thread-local xorshift.
    if ctx.names.text(*property) == "random" && arguments.is_empty() {
        let drawn = super::super::expr::call(builder, ctx, RuntimeOp::MathRandom, &[])?[0];
        return Ok(Some(super::super::expr::tagged(builder, drawn)));
    }
    // Which instruction, and over how many operands. `Math.min(a, b)` and
    // `Math.max(a, b)` are the machine's `fmin`/`fmax`, whose NaN and
    // signed-zero rules are the language's: `Math.min(0, -0)` is `-0`,
    // `Math.max(-0, 0)` is `+0`, a NaN operand answers NaN. Exactly two written
    // arguments — `Math.max(x)` is `x` with no comparison to lower, and a third
    // operand has no instruction — so the other arities stay a call.
    let shape = match (ctx.names.text(*property), arguments) {
        ("sqrt", [_]) => Shape::Unary(FloatOp::Sqrt),
        ("floor", [_]) => Shape::Unary(FloatOp::Floor),
        ("ceil", [_]) => Shape::Unary(FloatOp::Ceil),
        ("trunc", [_]) => Shape::Unary(FloatOp::Trunc),
        ("abs", [_]) => Shape::Unary(FloatOp::Abs),
        ("min", [_, _]) => Shape::Binary(NumOp::Min),
        ("max", [_, _]) => Shape::Binary(NumOp::Max),
        _ => return Ok(None),
    };
    let mut values = Vec::with_capacity(2);
    for argument in arguments {
        let Spreadable::Single(argument) = argument else {
            return Ok(None);
        };
        values.push(emit_expr(builder, scope, ctx, argument)?);
    }
    if values.iter().all(|&value| builder.repr_of(value) == Repr::F64) {
        let answered = match shape {
            Shape::Unary(op) => builder.float_unary(op, values[0])?,
            Shape::Binary(op) => builder.arith(op, values[0], values[1])?,
        };
        return Ok(Some(super::super::expr::tagged(builder, answered)));
    }
    // Refused — and the arguments are ALREADY EMITTED. Answering `None` here
    // handed the ordinary path the tree, which emitted them again, so
    // `Math.floor(f())` called `f` twice whenever `f`'s answer was not proven a
    // double. Measured on the binary before this: a counter in `f` read 2. So
    // the call is finished here, over the values in hand. The member read comes
    // after the arguments where the language reads it first, which is
    // observable only if reading `Math.floor` has an effect — and the proof
    // above is exactly that it has none.
    let (receiver, function) = super::super::call::callee_and_receiver(builder, scope, ctx, callee)?;
    let name = super::super::call::callee_spelling(ctx, callee);
    Ok(Some(super::super::call::issue_as(
        builder,
        ctx,
        function,
        receiver,
        &values,
        name,
        RuntimeOp::Call,
    )?))
}

/// The two instruction shapes a `Math` member lowers to.
enum Shape {
    /// One proven double in, one out.
    Unary(FloatOp),
    /// Two proven doubles in, one out.
    Binary(NumOp),
}
