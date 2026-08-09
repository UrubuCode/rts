//! `yield*` — producing everything another iterable produces.
//!
//! It is not a suspension. `yield x` parks once; `yield* xs` parks once per
//! element, so what it compiles to is a LOOP whose body is an ordinary `yield`.
//! That is why it was refused separately from `yield` after generators were
//! built, and why it is emitted here rather than beside the single suspension.
//!
//! # What this does not do, and it is visible rather than silent
//!
//! The specification's `yield*` forwards `next`, `throw` and `return` to the
//! inner iterator, so a value sent into the OUTER generator reaches the inner
//! one, and an inner `return()` runs the inner generator's cleanup.
//!
//! This forwards none of them: it takes what iteration produces and yields the
//! elements. `RuntimeOp::Iterate` is what `for`-`of` already uses, so the two
//! agree about what iterating something means — including that it is answered
//! as a whole, which makes `yield*` over an endless iterator a program that does
//! not finish rather than one that yields forever.
//!
//! Both limits are the SAME limit, held in one place: the day `Iterate` becomes
//! a stepwise protocol, `for`-`of` and this move together. A second, stepwise
//! iteration written here for `yield*` alone would be the second answer to one
//! question that this repository keeps paying for.
//!
//! The value `yield*` evaluates to is the inner iterator's RETURN value, which
//! this cannot see for the same reason: what came back is the elements, and a
//! generator's return value is not one of them. `undefined` is what it answers,
//! which is what it is for every iterable that returns nothing — an array
//! among them.

use rts_cranelift::ir::FuncBuilder;

use super::property::key_constant;
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::runtime::RuntimeOp;
use crate::syntax::Expr;

/// Emits `yield* subject`, and answers what the expression produces.
pub(super) fn emit_delegated(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    subject: &Expr,
) -> EmitResult<rts_cranelift::ir::ValueId> {
    let produced = super::expr::emit_expr(builder, scope, ctx, subject)?;
    let iterable = super::expr::as_value(builder, produced);
    let values = super::expr::call(builder, ctx, RuntimeOp::Iterate, &[iterable])?[0];

    let length = ctx.names.intern("length");
    let key = key_constant(builder, ctx, length);
    let count = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[values, key])?[0];

    // The index is a block parameter rather than a mutable local, because this
    // emits machine code directly: there is no binding for the analysis that
    // gives loop variables their parameters to see.
    let head = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    let index = builder.add_block_param(head, UNPROVEN);

    let first = super::expr::number_constant(builder, 0.0);
    let first = builder.widen(first);
    builder.jump(head, &[first])?;

    builder.switch_to(head);
    let more = super::expr::call(builder, ctx, RuntimeOp::Less, &[index, count])?[0];
    builder.branch(more, (body, &[]), (done, &[]))?;

    builder.switch_to(body);
    let element = super::expr::call(builder, ctx, RuntimeOp::GetIndexed, &[values, index])?[0];
    super::expr::call(builder, ctx, RuntimeOp::GeneratorYield, &[element])?;
    // The result of the suspension is what the resumption delivered. It is
    // dropped, and that IS the forwarding this does not do — recorded here
    // beside the drop rather than only in the module's opening note.
    builder.suspend();
    let one = super::expr::number_constant(builder, 1.0);
    let one = builder.widen(one);
    let stepped = super::expr::call(builder, ctx, RuntimeOp::Add, &[index, one])?[0];
    let stepped = super::expr::as_value(builder, stepped);
    builder.jump(head, &[stepped])?;

    builder.switch_to(done);
    Ok(super::expr::undefined(builder, ctx))
}
