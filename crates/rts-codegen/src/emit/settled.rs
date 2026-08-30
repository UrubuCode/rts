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
use rts_cranelift::repr::Repr;

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
    let Some(is) = singleton_equality_proof(builder, ctx, a, b)? else {
        return Ok(None);
    };
    let proof = super::choice::negated_proof(builder, is, negated)?;
    Ok(Some(builder.widen(proof)))
}

/// The same question, answered as a PROOF rather than as a JavaScript value.
///
/// Two callers want it and they want different things back. An expression wants
/// a value, so [`singleton_equality`] widens; a `switch` label wants the proof a
/// branch takes, and widening it there would be undone immediately.
///
/// Extracted rather than copied, because `switch`'s test chain reaching for its
/// own answer to "is this `=== null`" is exactly the second statement that made
/// the chain a call at every label in the first place — `switch.rs`'s own
/// comment says so about `===` on two doubles.
pub(super) fn singleton_equality_proof(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    a: ValueId,
    b: ValueId,
) -> EmitResult<Option<ValueId>> {
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
            return Ok(Some(builder.bool_constant(false)));
        }
        return Ok(Some(builder.is_singleton(tested, id)?));
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
    // THREE OF THE NINE ANSWERS ARE DECIDED BY THE TAG, with no heap access at
    // all, so they need no crossing. See [`tag_decidable`].
    if let Some(proof) = tag_decidable(builder, ctx, value, spelled)? {
        let proof = super::choice::negated_proof(builder, proof, negated)?;
        return Ok(Some(builder.widen(proof)));
    }
    // The literal's own index, minted from the same table `StringConst` reads,
    // because that numbering is an agreement the compiler and the runtime
    // already have. A number naming one of the nine `typeof` answers would be a
    // second numbering of them, which is the drift `TypeName` exists to prevent
    // one level down.
    let which = ctx.literal_units(spelled.units());
    let index = count_constant(builder, which as usize);
    let is = call(builder, ctx, RuntimeOp::TypeOfIs, &[value, index])?[0];
    let proof = super::choice::negated_proof(builder, is, negated)?;
    Ok(Some(builder.widen(proof)))
}

/// `typeof <already-emitted operand> === "<name>"`, for a name the EMITTER
/// itself wrote rather than one a program spelled.
///
/// The desugarings ask this: `for`-`of`, `yield*` and array destructuring all
/// need to know whether a `next` or a `Symbol.iterator` they just read is
/// callable, and each wrote it out as three crossings — `TypeOf` to build a
/// string, `StringConst` to build `"function"`, and `StrictEquals` to compare
/// their text.
///
/// That is the shape `RuntimeOp::TypeOfIs` was added to replace, and they could
/// not reach it: `settled::typeof_equals_literal` recognises a BINARY
/// EXPRESSION, and a desugaring has no expression — it has two values. This is
/// the same question asked of the values.
///
/// The name goes through `ctx.literal`, the same table a written literal is
/// interned into, so `"function"` emitted here and `"function"` written by a
/// program are one string and one index.
pub(super) fn typeof_is_named(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    value: ValueId,
    name: &str,
) -> EmitResult<ValueId> {
    let which = ctx.literal(name);
    let index = count_constant(builder, which as usize);
    Ok(call(builder, ctx, RuntimeOp::TypeOfIs, &[value, index])?[0])
}

/// `typeof <already-emitted operand> === "…"`, as a proven boolean.
///
/// The value-level half of [`typeof_equals_literal`], for the caller that has
/// the operand already and no binary expression to recognise: a `switch` label.
/// `switch (typeof x) { case "string": }` asks exactly what
/// `typeof x === "string"` asks, and asked it as a text comparison because the
/// settlement was reachable only from `emit_binary_inner`.
///
/// `None` when the name is not one of the nine, so the caller falls back to the
/// ordinary chain and a label like `case "wrong"` still answers false the long
/// way.
pub(super) fn typeof_is_proof(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    value: ValueId,
    spelled: &crate::syntax::Text,
) -> EmitResult<Option<ValueId>> {
    if let Some(proof) = tag_decidable(builder, ctx, value, spelled)? {
        return Ok(Some(proof));
    }
    // No filter on WHICH name: `TypeOfIs` compares the literal against the one
    // the value has and answers false for anything else, so `case "wrong"` is
    // correct without this layer holding a second copy of the nine names — the
    // drift `TypeName` exists to prevent.
    let which = ctx.literal_units(spelled.units());
    let index = count_constant(builder, which as usize);
    Ok(Some(call(builder, ctx, RuntimeOp::TypeOfIs, &[value, index])?[0]))
}

/// `typeof v === "…"` for the three names the TAG decides, as a proven boolean.
///
/// # Which three, and why not the other six
///
/// `number`, `boolean` and `undefined` are readable from the encoding alone. The
/// machine already tests exactly these: `lower/value.rs`'s `test` answers a
/// double by asking whether the word is outside the encoded quadrant, a boolean
/// by `has_tag(TAG_BOOL)`, and `IsSingleton` compares one word.
///
/// `string`, `object` and `function` all arrive as `TAG_REFERENCE` and are told
/// apart by the CELL HEADER, which is the heap — so they stay a crossing, and
/// that is a fact about the representation rather than a gap.
///
/// `symbol` and `bigint` ARE tag-decidable and are deliberately left out. Their
/// tag numbers are runtime values (`context.kinds.symbol`), so the emitter would
/// need a compile-time agreement asserted in `rts-host` — the shape of work the
/// singleton numbering is — for two spellings a corpus census found 6 and 8
/// times against 45 for `number`.
///
/// # Why a proven operand falls through
///
/// A value the emitter already proved is not a question: `typeof x` for a proven
/// double is `"number"` and the machine refuses to guard what it has narrowed.
/// Answering the constant here would be right and would also be the emitter
/// deciding a language question from a machine fact in a second place — the
/// existing call is correct for it and costs nothing anybody measured.
///
/// Measured 2026-08-30, release, min of 9 over 10 M iterations, against a floor
/// of 3.70 and `x === undefined` — already a tag test — at 2.00:
/// `typeof x === "undefined"` 11.40, `"number"` 12.70, `"boolean"` 13.90.
fn tag_decidable(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    value: ValueId,
    spelled: &crate::syntax::Text,
) -> EmitResult<Option<ValueId>> {
    if builder.repr_of(value) != UNPROVEN {
        return Ok(None);
    }
    // A lone surrogate is a legal string and names none of the nine, so the
    // absence is the answer rather than a case to handle.
    let Some(name) = spelled.as_rust() else {
        return Ok(None);
    };
    let name = name.as_str();
    match name {
        // One instruction: a singleton has exactly one encoding.
        "undefined" => {
            let id = ctx.model.singleton(Singleton::Undefined);
            Ok(Some(builder.is_singleton(value, id)?))
        }
        "boolean" => Ok(Some(has_any_repr(builder, value, &[Repr::Bool])?)),
        // A number is a double OR a small integer, and the encoding keeps them
        // apart — so it is the one name that needs two tests. The second is
        // asked only where the first failed.
        "number" => Ok(Some(has_any_repr(
            builder,
            value,
            &[Repr::F64, Repr::I32],
        )?)),
        _ => Ok(None),
    }
}

/// Whether a generic value carries ANY of these representations, as a proven
/// boolean.
///
/// A guard is a TERMINATOR — it hands the narrowed value to its success block as
/// a parameter, which is what makes narrowing unrepresentable without a test —
/// so asking the question as a VALUE costs a branch and a join rather than one
/// instruction. Two blocks and a constant against a crossing.
///
/// The list exists for `number` alone: a number is a double OR a small integer
/// and the encoding keeps them apart, so it is the one name of the three that
/// needs two tests. They are CHAINED on failure rather than both computed and
/// merged — the second is asked only where the first did not hold, which is what
/// a short circuit means and what a merge would have thrown away.
fn has_any_repr(
    builder: &mut FuncBuilder,
    value: ValueId,
    reprs: &[Repr],
) -> EmitResult<ValueId> {
    let join = builder.create_block();
    let answer = builder.add_block_param(join, Repr::Bool);
    let no = builder.create_block();

    for (at, repr) in reprs.iter().enumerate() {
        let yes = builder.create_block();
        builder.add_block_param(yes, *repr);
        let fail = match at + 1 == reprs.len() {
            true => no,
            false => builder.create_block(),
        };
        builder.guard(value, *repr, (yes, &[]), (fail, &[]))?;
        builder.switch_to(yes);
        let held = builder.bool_constant(true);
        builder.jump(join, &[held])?;
        builder.switch_to(fail);
    }

    let held = builder.bool_constant(false);
    builder.jump(join, &[held])?;
    builder.switch_to(join);
    Ok(answer)
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
