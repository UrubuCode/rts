//! What a free name means inside a `with` body.
//!
//! # The one construct whose scope chain is a value
//!
//! Every other binding in this engine is resolved while compiling: a name is a
//! register, a slot in an environment, or a property of the global object, and
//! which of the three it is never changes. `with (o) { x }` breaks that — `x` is
//! `o.x` when `o` has the property and the lexical `x` when it does not, and `o`
//! is a value nobody can see until the program runs.
//!
//! So this module emits the decision instead of taking it: one test per object
//! on the stack, innermost first, and the lexical answer as the last link. The
//! test is [`RuntimeOp::WithHas`] rather than `in`, because
//! `Symbol.unscopables` blocks names the object does have — that rule lives in
//! the runtime, in one place, for the reason its entry states.
//!
//! # Why both paths write to memory, and what would happen if they did not
//!
//! A body containing a `with` forces every binding it declares into its
//! environment — `emit/function.rs` does it, by the same rule and in the same
//! place that a body mentioning `eval` does. That is not an optimisation
//! detail, it is what makes this module possible at all.
//!
//! Without it the lexical branch of a WRITE would take
//! [`Binding::Value`](super::scope::Binding::Value)'s path and REBIND the name
//! to a fresh SSA value, while the object branch would store a property and
//! rebind nothing. The two paths would then leave the scope disagreeing about
//! what the binding is — which is exactly what `Binding::value()` says cannot
//! happen — and merging that disagreement means a block parameter per name per
//! `with`, decided from a branch nothing lexical can predict.
//!
//! With every binding in memory, both branches are stores. Nothing about the
//! scope differs at the join, and the only thing to merge is the VALUE the
//! expression produced, which is one block parameter.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::scope::Binding;
use super::{Ctx, EmitResult, Scope, UNPROVEN, expr};
use crate::names::Name;
use crate::runtime::RuntimeOp;

/// Reads a name against the `with` objects in force, then lexically.
pub(super) fn read(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<ValueId> {
    Ok(read_callee(builder, scope, ctx, name)?.1)
}

/// The same read, with the RECEIVER the name was found on.
///
/// # Why a call needs this and every other read does not
///
/// A reference has a base, and for a name a `with` resolved the base is the
/// OBJECT — so `with (a) { join("-") }` calls `a.join` with `a` as its `this`,
/// exactly as `a.join("-")` does. Only a call ever looks at the base, which is
/// why [`read`] discards it rather than every caller carrying one.
///
/// Getting this wrong is not a missing feature but a wrong answer that runs:
/// the function is found, called with `undefined`, and reads its own `this` as
/// something else entirely. The lexical link answers `undefined`, which is what
/// a plain call passes.
pub(super) fn read_callee(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<(ValueId, ValueId)> {
    let join = builder.create_block();
    let receiver = builder.add_block_param(join, UNPROVEN);
    let result = builder.add_block_param(join, UNPROVEN);
    for object in objects(ctx) {
        let (found, absent) = test(builder, ctx, object, name)?;
        builder.switch_to(found);
        let value = super::property::emit_read(builder, ctx, object, name)?;
        let value = expr::as_value(builder, value);
        builder.jump(join, &[object, value])?;
        builder.switch_to(absent);
    }
    let value = super::binding::lexical_read(builder, scope, ctx, name)?;
    let value = expr::as_value(builder, value);
    let nothing = expr::undefined(builder, ctx);
    builder.jump(join, &[nothing, value])?;
    builder.switch_to(join);
    Ok((receiver, result))
}

/// Writes a name against the `with` objects in force, then lexically.
///
/// A `with` scope is where an assignment goes when the object has the name —
/// `with (o) { x = 1 }` sets `o.x` and leaves the outer `x` alone — which is
/// the same lookup the read performs, so it is the same chain rather than a
/// second one.
pub(super) fn write(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    name: Name,
    value: ValueId,
) -> EmitResult<ValueId> {
    // The module's own precondition, checked rather than assumed. A name still
    // held in a register would make the two paths disagree about the binding —
    // see the module documentation — and the failure would be a program that
    // runs and assigns the wrong thing. `emit/function.rs` puts every binding
    // of a body containing a `with` in memory, so reaching this is a defect
    // here rather than anything a program can write; it is refused by name
    // instead of emitted, which is what the rest of this crate does with a case
    // it cannot answer.
    if matches!(scope.lookup(name), Some(Binding::Value(_))) {
        return expr::gap("`with` over a binding that stayed in a register");
    }
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);
    let value = expr::as_value(builder, value);
    for object in objects(ctx) {
        let (found, absent) = test(builder, ctx, object, name)?;
        builder.switch_to(found);
        super::property::emit_write(builder, ctx, object, name, value)?;
        builder.jump(join, &[value])?;
        builder.switch_to(absent);
    }
    super::binding::lexical_write(builder, scope, ctx, name, value)?;
    builder.jump(join, &[value])?;
    builder.switch_to(join);
    Ok(result)
}

/// The objects to test, innermost first.
///
/// Copied out of the context because emitting the test needs `ctx` mutably and
/// the list is three pointers at most. Reversed here rather than at each caller:
/// `with (a) with (b) x` asks `b` before `a`, and a chain built the other way
/// round would read the outer object's property while the inner one shadows it.
fn objects(ctx: &Ctx) -> Vec<ValueId> {
    ctx.with_objects.iter().rev().copied().collect()
}

/// Emits one link: the test, and the two blocks it chooses between.
///
/// Neither is switched to, because the caller emits into both — the access into
/// the first and the next link of the chain into the second — and a function
/// that left emission in one of them would make the caller's order of writing
/// them load-bearing. That is the defect the first draft had: the second link's
/// test landed inside the first link's `found` block, so `with (a) with (b) x`
/// asked `a` only when `b` had already answered.
fn test(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: ValueId,
    name: Name,
) -> EmitResult<(rts_cranelift::ir::BlockId, rts_cranelift::ir::BlockId)> {
    // The key as TEXT, because the entry takes a value and interns it — the
    // same shape `o[k]` has. The named property access on the other side takes
    // the number the compiler resolved, which is why the two spellings appear
    // together here and are not the same argument.
    let spelling = ctx.names.text(name).to_owned();
    let key = expr::string_literal(builder, ctx, &spelling)?;
    let has = expr::call(builder, ctx, RuntimeOp::WithHas, &[object, key])?[0];
    let found = builder.create_block();
    let absent = builder.create_block();
    builder.branch(has, (found, &[]), (absent, &[]))?;
    Ok((found, absent))
}
