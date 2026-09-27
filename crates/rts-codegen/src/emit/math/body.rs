//! The members and the proof that admits each. See the module header.

use rts_cranelift::ir::{FloatOp, FuncBuilder, NumOp, ValueId};
use rts_cranelift::repr::Repr;

use super::super::expr::{emit_expr, number_constant, tagged};
use super::super::{Ctx, EmitResult, Scope};
use crate::runtime::{RuntimeOp, math_direct};
use crate::syntax::{Expr, ExprKind, Spreadable};

/// Whether `object` is the language's own `Math`, here: the program never
/// disturbs it, and nothing in scope shadows the name.
fn is_math(scope: &Scope, ctx: &Ctx, object: &Expr) -> bool {
    if !ctx.math_primordial {
        return false;
    }
    let ExprKind::Ident(name) = &object.kind else {
        return false;
    };
    ctx.names.text(*name) == "Math" && scope.lookup(*name).is_none()
}

/// `Math.PI` and its seven siblings, as the number itself.
///
/// The same proof the calls rest on, and it is what makes a read of `Math.PI`
/// in a loop cost nothing rather than a cached property read: under it the
/// property is a constant the language fixed, and a constant is emitted as one.
/// Answers the value; the caller makes the machine constant, so this stays a
/// table of the language's numbers and nothing else.
pub(in super::super) fn constant(scope: &Scope, ctx: &Ctx, object: &Expr, property: crate::names::Name) -> Option<f64> {
    if !is_math(scope, ctx, object) {
        return None;
    }
    constant_named(ctx.names.text(property))
}

/// The value a constant member holds, by name — the one table both emitters
/// read, so `Math.PI` is the same bits whichever stage compiles the read.
pub(crate) fn constant_named(property: &str) -> Option<f64> {
    Some(match property {
        "PI" => std::f64::consts::PI,
        "E" => std::f64::consts::E,
        "LN2" => std::f64::consts::LN_2,
        "LN10" => std::f64::consts::LN_10,
        "LOG2E" => std::f64::consts::LOG2_E,
        "LOG10E" => std::f64::consts::LOG10_E,
        "SQRT2" => std::f64::consts::SQRT_2,
        "SQRT1_2" => std::f64::consts::FRAC_1_SQRT_2,
        _ => return None,
    })
}

/// How a member is answered when its operands are proven doubles.
enum Shape {
    /// One machine instruction over one operand.
    Unary(FloatOp),
    /// One machine instruction over two operands.
    Binary(NumOp),
    /// A sequence of instructions stated once in `sequence.rs`.
    Round,
    /// See [`Shape::Round`].
    Sign,
    /// See [`Shape::Round`].
    Imul,
    /// See [`Shape::Round`].
    Clz32,
    /// The operand itself: `Math.max(x)` is `ToNumber(x)`, and a proven double
    /// is already that.
    Identity,
    /// A library call by number, operand and answer unboxed.
    Direct1(i64),
    /// See [`Shape::Direct1`].
    Direct2(i64),
}

/// Which shape a member is, over how many written arguments — or `None` for a
/// member this does not decide, which stays an ordinary call.
fn shape_of(name: &str, written: usize) -> Option<Shape> {
    Some(match (name, written) {
        ("sqrt", 1) => Shape::Unary(FloatOp::Sqrt),
        ("floor", 1) => Shape::Unary(FloatOp::Floor),
        ("ceil", 1) => Shape::Unary(FloatOp::Ceil),
        ("trunc", 1) => Shape::Unary(FloatOp::Trunc),
        ("abs", 1) => Shape::Unary(FloatOp::Abs),
        ("fround", 1) => Shape::Unary(FloatOp::RoundToSingle),
        ("min", 2) => Shape::Binary(NumOp::Min),
        ("max", 2) => Shape::Binary(NumOp::Max),
        ("min" | "max", 1) => Shape::Identity,
        ("round", 1) => Shape::Round,
        ("sign", 1) => Shape::Sign,
        ("imul", 2) => Shape::Imul,
        ("clz32", 1) => Shape::Clz32,
        (_, 1) => Shape::Direct1(math_direct::unary_index(name)?),
        (_, 2) => Shape::Direct2(math_direct::binary_index(name)?),
        _ => return None,
    })
}

/// `Math.f(…)` as the instruction the hardware has, the sequence the language
/// defines over those instructions, or the library function reached directly.
///
/// # Three conditions, and each one is a proof rather than a guess
///
/// The program must not disturb `Math` anywhere — `primordial::untouched`,
/// computed over the whole tree before anything was emitted. No enclosing scope
/// may bind the name, which the scope answers exactly. And every operand must
/// ALREADY be a proven double: a guard here would be correct too, but the
/// operand of a square root in a loop is proven by the type pass in the case
/// that matters, and emitting a guard for the rest would cost a branch to
/// discover what the call would have found anyway.
///
/// # What a refusal does with the operands it emitted
///
/// Finishes the call over them. Answering `None` here handed the ordinary path
/// the tree, which emitted the arguments again, so `Math.floor(f())` called `f`
/// twice whenever `f`'s answer was not proven a double — a counter in `f` read
/// 2 on the binary before this was fixed. The member read comes after the
/// arguments where the language reads it first, which is observable only if
/// reading `Math.floor` has an effect, and the proof above is exactly that it
/// has none.
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
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return Ok(None);
    };
    if !is_math(scope, ctx, object) {
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
        return Ok(Some(tagged(builder, drawn)));
    }
    let Some(shape) = shape_of(ctx.names.text(*property), arguments.len()) else {
        return Ok(None);
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
            Shape::Round => super::sequence::round(builder, values[0])?,
            Shape::Sign => super::sequence::sign(builder, values[0])?,
            Shape::Imul => super::sequence::imul(builder, values[0], values[1])?,
            Shape::Clz32 => super::sequence::clz32(builder, values[0])?,
            Shape::Identity => values[0],
            Shape::Direct1(which) => {
                let which = selector(builder, which);
                super::super::expr::call(builder, ctx, RuntimeOp::MathDirect1, &[which, values[0]])?[0]
            }
            Shape::Direct2(which) => {
                let which = selector(builder, which);
                super::super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::MathDirect2,
                    &[which, values[0], values[1]],
                )?[0]
            }
        };
        return Ok(Some(tagged(builder, answered)));
    }
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

/// The number a library member goes by, as the machine word the door takes.
fn selector(builder: &mut FuncBuilder, which: i64) -> ValueId {
    let id = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(which as u64),
    });
    builder.use_const(id)
}

/// Keeps `number_constant` reachable from here for the constant fold, so the
/// caller in `expr.rs` and this module agree on how a fixed double is made.
pub(in super::super) fn fixed(builder: &mut FuncBuilder, value: f64) -> ValueId {
    number_constant(builder, value)
}
