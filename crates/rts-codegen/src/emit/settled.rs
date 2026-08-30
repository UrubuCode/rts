//! Comparisons the emitter settles without asking the runtime.
//!
//! One rule, two members so far: **where an operand is a constant this emitter
//! itself materialised, the path that assumes nothing is known about the
//! operands is already wrong.** `emit_binary_inner` speculates that both sides
//! of a comparison are doubles — two guards, an instruction, a runtime call as
//! the slow path — which is a good bet against two unknown values and no bet at
//! all against a constant `undefined`, whose guard fails on every pass by
//! construction.
//!
//! They live here rather than in `expr.rs` because that file is 2 300 lines
//! against a 1 000-line ceiling, and the workspace rule is that new code lands
//! in a small focused module rather than being appended to one already over.
//!
//! Each is decided at the layer that can see the evidence, and the two differ:
//! [`singleton_equality`] asks about the emitted VALUE, because "is this a
//! constant `undefined`" survives into the value and a shadowed `undefined`
//! answers itself correctly there; [`typeof_equals_literal`] asks about the
//! TREE, because "the left operand is a `typeof` application" is a fact about
//! the expression and is gone once it is a value.
//!
//! **Expect more members.** The two named and not yet done are `x === true` —
//! a boolean constant is a unique word too, but the machine's `IsSingleton`
//! takes a singleton number, so it needs a capability rather than an emitter
//! change — and the whole of `docs/codegen/entry-tax.md` part three.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{boolean_constant, call, count_constant};
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::runtime::RuntimeOp;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal, UnaryOp};
use crate::values::Singleton;

/// `a === b` where one side is a singleton the emitter itself wrote down.
///
/// # Why this is not an optimisation of strict equality
///
/// It is the observation that `x === undefined` asks a different question from
/// `x === y`. Strict equality between two unknown values has to reach the
/// runtime because two strings are `===` when their TEXT is, which reads the
/// heap. Against `undefined` or `null` there is no text and no heap: a
/// singleton has exactly one encoding, so the answer is whether one machine
/// word equals another, which is what `FuncBuilder::is_singleton` is.
///
/// # What it cost to not do this
///
/// Every defaulted parameter, and every `x === undefined` and `x !== null` a
/// program writes. `emit_binary_inner` speculates that both operands are
/// doubles, so the emitted shape was two guards, an instruction nobody reaches,
/// and a call to `StrictEquals` — and the guard against a constant `undefined`
/// fails on EVERY pass by construction, because the constant is not a double
/// and never will be. `bench/analytic.ts` read a defaulted parameter at
/// 24.5 ns against 8 for the same call without one.
///
/// # Why the operand and not the syntax
///
/// `undefined` is an identifier in JavaScript and a program may shadow it.
/// Asked here, that case needs no thought: a shadowed `undefined` emits a scope
/// read rather than a constant, and `is_constant_singleton` answers false for
/// it. Asked of the tree, it would have needed the scope and would have been
/// wrong the first time someone wrote `let undefined = 1`. It also catches the
/// comparisons no program wrote — `bind_parameters` synthesises one per
/// defaulted parameter — which a syntax test would have had to be told about.
pub(super) fn singleton_equality(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<Option<ValueId>> {
    let negated = match op {
        BinaryOp::StrictEqual => false,
        BinaryOp::StrictNotEqual => true,
        _ => return Ok(None),
    };
    for which in Singleton::ALL {
        let id = ctx.model.singleton(*which);
        // The other side is the one under test, so the constant side is
        // whichever of the two the emitter can settle. `undefined === x` is the
        // same question as `x === undefined` and is written by real code.
        let tested = if builder.is_constant_singleton(b, id) {
            a
        } else if builder.is_constant_singleton(a, id) {
            b
        } else {
            continue;
        };
        // Nothing proven is a singleton — a proved double or boolean cannot be
        // one — so the comparison has a constant answer and the machine refuses
        // the question rather than emitting a test that is always false. That
        // refusal is what surfaces the case here instead of leaving it emitted.
        if builder.repr_of(tested) != UNPROVEN {
            return Ok(Some(boolean_constant(builder, negated)));
        }
        let is = builder.is_singleton(tested, id)?;
        return Ok(Some(if negated {
            super::choice::from_bool(builder, is, true)?
        } else {
            builder.widen(is)
        }));
    }
    Ok(None)
}

/// `typeof x === "string"` as ONE crossing instead of three.
///
/// # What it replaces
///
/// Spelled out, the construct emitted `TypeOf` to build a string, `StringConst`
/// to build the other one, and `StrictEquals` to compare their TEXT — three
/// runtime crossings, each with the throw check a crossing implies, to settle a
/// question decided by a tag and a cell header. Measured 2026-08-29 on
/// `--release`: the bare `typeof` was 8.3 ns and `typeof x === "string"` 24.0,
/// so the comparison cost nearly twice what the operation did.
///
/// `RuntimeOp::TypeOfIs` builds no string and answers a PROVEN boolean, which a
/// branch takes with no guard — and it is on `raising::CANNOT_RAISE`, so the
/// throw check goes as well. `TypeOf` is not on that list for one stated
/// reason: it allocates its answer. This does not, which is what it was added
/// for.
///
/// # Why the tree and not the values
///
/// The opposite of [`singleton_equality`], and for the opposite reason. What
/// has to be recognised here is that the left operand is a `typeof`
/// APPLICATION, which is a fact about the expression and is gone by the time it
/// is a value: a `ValueId` holding `"string"` cannot say whether it came from
/// `typeof` or from a string a program computed.
///
/// # What it refuses
///
/// A right side that is not a plain string literal. `typeof x === y` compares
/// two strings for real and stays a call. The operand goes through
/// `unary::typeof_operand`, so an undeclared name keeps `typeof`'s exemption
/// from the reference error and the dead zone still raises — stated there once
/// rather than restated here, because the second statement is where
/// `typeof maybe` starts throwing.
pub(super) fn typeof_equals_literal(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
) -> EmitResult<Option<ValueId>> {
    let negated = match op {
        BinaryOp::StrictEqual => false,
        BinaryOp::StrictNotEqual => true,
        _ => return Ok(None),
    };
    // Either order. `"string" === typeof x` is the same question, is written by
    // real code, and costs one arm.
    let (applied, spelled) = match (&left.kind, &right.kind) {
        (
            ExprKind::Unary {
                op: UnaryOp::TypeOf,
                operand,
            },
            ExprKind::Literal(Literal::String(text)),
        ) => (operand, text),
        (
            ExprKind::Literal(Literal::String(text)),
            ExprKind::Unary {
                op: UnaryOp::TypeOf,
                operand,
            },
        ) => (operand, text),
        _ => return Ok(None),
    };
    let value = super::unary::typeof_operand(builder, scope, ctx, applied)?;
    // The literal's own index, minted from the same table `StringConst` reads,
    // because that numbering is an agreement the compiler and the runtime
    // already have. A number naming one of the nine `typeof` answers would be a
    // second numbering of them, which is the drift `TypeName` exists to prevent
    // one level down.
    let which = ctx.literal_units(spelled.units());
    let index = count_constant(builder, which as usize);
    let is = call(builder, ctx, RuntimeOp::TypeOfIs, &[value, index])?[0];
    Ok(Some(if negated {
        super::choice::from_bool(builder, is, true)?
    } else {
        builder.widen(is)
    }))
}

/// `x == null` and `x != null` — the one loose equality that coerces nothing.
///
/// # Why this is not an optimisation of `==`
///
/// It is the observation that the specification's `IsLooselyEqual` reaches its
/// coercions only after two arms that `null` and `undefined` take first. With
/// one side either of them, `x == null` is true **exactly** when `x` is `null`
/// or `undefined`: same-type falls through to strict equality, the two
/// cross-arms cover the other one of the pair, and none of the Number/String,
/// Boolean, BigInt or ToPrimitive arms names `null` or `undefined` at all. So
/// the answer is reached without converting anything and without running any
/// user code — which no other spelling of `==` can say.
///
/// That is the same question `??` and `?.` ask, so it is
/// `choice::branch_on_nullish` in a value's clothing rather than a new rule.
///
/// # What it cost to not do this
///
/// `x == null` is one of the most written idioms in JavaScript and emitted the
/// worst shape in this file: the double speculation's two guards, of which the
/// one on the constant `null` fails on every pass by construction; a full
/// crossing to `__rts_loose_equals`; and the THROW CHECK that crossing implies,
/// because `==` in general runs `ToPrimitive` and `ToPrimitive` runs user code.
/// Every part of that is paid for a conversion this arm does not perform.
///
/// # The one exotic object that would make this wrong, and why it cannot exist
///
/// `document.all` is specified to be loosely equal to `null` while being an
/// object — the `[[IsHTMLDDA]]` slot, which exists so that a 1990s feature test
/// keeps working. Nothing in this engine can create one: it is a host object a
/// browser DOM provides, and `rts-dom` is a rendering engine that publishes no
/// such thing. If one ever arrives, this function is where it breaks, which is
/// why it is named here rather than left for someone to rediscover.
pub(super) fn loose_null_equality(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<Option<ValueId>> {
    let negated = match op {
        BinaryOp::LooseEqual => false,
        BinaryOp::LooseNotEqual => true,
        _ => return Ok(None),
    };
    for which in Singleton::ALL {
        let id = ctx.model.singleton(*which);
        let tested = if builder.is_constant_singleton(b, id) {
            a
        } else if builder.is_constant_singleton(a, id) {
            b
        } else {
            continue;
        };
        // Nothing proven is nullish, so the answer is constant. The machine
        // refuses `IsSingleton` for a proven operand rather than answering a
        // constant `false`, which is what surfaces this case here instead of
        // leaving it emitted and never taken.
        if builder.repr_of(tested) != UNPROVEN {
            return Ok(Some(boolean_constant(builder, negated)));
        }
        return Ok(Some(super::choice::nullish_value(
            builder, ctx, tested, negated,
        )?));
    }
    Ok(None)
}
