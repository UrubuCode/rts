//! `yield*` — producing everything another iterable produces.
//!
//! It is not a suspension. `yield x` parks once; `yield* xs` parks once per
//! element, so what it compiles to is a LOOP whose body is an ordinary `yield`.
//! That is why it was refused separately from `yield` after generators were
//! built, and why it is emitted here rather than beside the single suspension.
//!
//! # Two loops, and the question that picks between them
//!
//! The specification's `yield*` drives the inner ITERATOR: it calls `next(v)`
//! with whatever was sent into the outer generator, yields what comes back, and
//! when the inner iterator answers `done` the `.value` of that last step is what
//! the `yield*` expression evaluates to. Both of those are observable and both
//! were missing while this iterated a materialised list: a value sent in was
//! dropped, and `const r = yield* inner()` answered `undefined` where the
//! delegate's `return` value was the point.
//!
//! So the protocol is what is driven — WHEN there is one. `source.next` being a
//! function is the question asked, at run time, because it is the only part of
//! the protocol that is reachable from here: `Symbol.iterator` is a symbol key
//! and this layer has no way to spell one, and every object this engine hands to
//! a `yield*` that needs stepping (a generator, a list iterator) IS its own
//! iterator and carries `next` directly.
//!
//! Anything else — an array, a string, a `Map` — goes through
//! [`RuntimeOp::Iterate`], which is what `for`-`of` uses, so the two agree about
//! what iterating something means. That path keeps the two limits it always had,
//! and they are the same limit: the whole thing is materialised first, so
//! `yield*` over an endless one does not finish, and there is no return value to
//! capture because what came back is the elements. Neither is a second answer to
//! the stepping question — it is the case where nothing is steppable.
//!
//! The rejected alternative was a new runtime operation answering "the iterator
//! of this", which is the right end state and is a change to the operation set
//! rather than to `yield*`: `for`-`of`, spread and this would all move to it
//! together, and moving one of them alone is how the two would come to disagree.

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
    let source = super::expr::as_value(builder, produced);

    let stepwise = builder.create_block();
    let listwise = builder.create_block();
    let done = builder.create_block();
    // What the whole expression evaluates to arrives as a block parameter: the
    // two paths answer it from different places — the last step's `.value` and
    // `undefined` — and a parameter is how one expression has one value without
    // either path knowing the other exists.
    let answer = builder.add_block_param(done, UNPROVEN);
    // Declared here rather than inside [`emit_protocol`], because the branch
    // below carries an argument for it and a block's parameters have to exist
    // before anything jumps to it — the builder checks the count at the jump.
    let sent = builder.add_block_param(stepwise, UNPROVEN);

    let named = ctx.names.intern("next");
    let key = key_constant(builder, ctx, named);
    let step = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[source, key])?[0];
    // `typeof` and not truthiness. An object carrying a `next` that is not
    // callable is not an iterator, and calling it would fail where iterating it
    // is what the program asked for.
    let kind = super::expr::call(builder, ctx, RuntimeOp::TypeOf, &[step])?[0];
    let callable = super::expr::string_literal(builder, ctx, "function")?;
    let steppable = super::expr::call(builder, ctx, RuntimeOp::StrictEquals, &[kind, callable])?[0];
    let first = super::expr::undefined(builder, ctx);
    builder.branch(steppable, (stepwise, &[first]), (listwise, &[]))?;

    emit_protocol(builder, ctx, source, step, stepwise, sent, done)?;
    emit_list(builder, ctx, source, listwise, done)?;

    builder.switch_to(done);
    Ok(answer)
}

/// The iterator protocol, stepped: `next(sent)` until it answers `done`.
///
/// The block parameter is the value to send in, which is `undefined` the first
/// time and thereafter whatever the resumption of the outer generator delivered.
/// That forwarding is the whole point of this path — `g.next(v)` on the outer
/// generator has to reach the inner one's `next` — and it is why the loop is
/// emitted with block parameters rather than a local: this builds machine code
/// directly, and there is no binding for the analysis that gives loop variables
/// their parameters to see.
fn emit_protocol(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    source: rts_cranelift::ir::ValueId,
    step: rts_cranelift::ir::ValueId,
    head: rts_cranelift::ir::BlockId,
    sent: rts_cranelift::ir::ValueId,
    done: rts_cranelift::ir::BlockId,
) -> EmitResult<()> {
    let body = builder.create_block();
    let yielded = builder.add_block_param(body, UNPROVEN);

    builder.switch_to(head);
    let absent = super::expr::undefined(builder, ctx);
    // `next` is called with the inner iterator as the receiver, which is what
    // makes a delegated generator advance its own frame rather than someone
    // else's.
    let answered = super::expr::call(
        builder,
        ctx,
        RuntimeOp::Call,
        &[step, source, sent, absent, absent, absent],
    )?[0];
    let named = ctx.names.intern("value");
    let key = key_constant(builder, ctx, named);
    let value = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[answered, key])?[0];
    let named = ctx.names.intern("done");
    let key = key_constant(builder, ctx, named);
    let finished = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[answered, key])?[0];
    let finished = super::expr::to_boolean(builder, ctx, finished)?;
    // The FINISHED step's `.value` is what `yield*` evaluates to, and it is not
    // yielded: `{ value: 9, done: true }` from the inner generator's `return 9`
    // is the answer to the expression, not an element of the sequence.
    builder.branch(finished, (done, &[value]), (body, &[value]))?;

    builder.switch_to(body);
    super::expr::call(builder, ctx, RuntimeOp::GeneratorYield, &[yielded])?;
    // The result of the suspension is what the resumption delivered, and it goes
    // straight back into the inner iterator's `next`.
    let received = builder.suspend();
    builder.jump(head, &[received])?;
    Ok(())
}

/// Everything else: what iteration answers, yielded element by element.
///
/// The elements are all of them, built before the first is yielded, because
/// `Iterate` answers a list. What that costs and why it is not fixed here is in
/// the module's opening note.
fn emit_list(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    source: rts_cranelift::ir::ValueId,
    entry: rts_cranelift::ir::BlockId,
    done: rts_cranelift::ir::BlockId,
) -> EmitResult<()> {
    let head = builder.create_block();
    let body = builder.create_block();
    let index = builder.add_block_param(head, UNPROVEN);

    builder.switch_to(entry);
    let values = super::expr::call(builder, ctx, RuntimeOp::Iterate, &[source])?[0];
    let named = ctx.names.intern("length");
    let key = key_constant(builder, ctx, named);
    let count = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[values, key])?[0];
    let first = super::expr::number_constant(builder, 0.0);
    let first = builder.widen(first);
    builder.jump(head, &[first])?;

    builder.switch_to(head);
    let more = super::expr::call(builder, ctx, RuntimeOp::Less, &[index, count])?[0];
    // An iterable that is not an iterator has no return value, so the expression
    // answers `undefined` — which is what it answers for an array in a real
    // engine too, `[1,2]` having nothing to return.
    let absent = super::expr::undefined(builder, ctx);
    builder.branch(more, (body, &[]), (done, &[absent]))?;

    builder.switch_to(body);
    let element = super::expr::call(builder, ctx, RuntimeOp::GetIndexed, &[values, index])?[0];
    super::expr::call(builder, ctx, RuntimeOp::GeneratorYield, &[element])?;
    // Dropped, and that IS the forwarding this path does not do: a materialised
    // list has no `next` to hand the value to.
    builder.suspend();
    let one = super::expr::number_constant(builder, 1.0);
    let one = builder.widen(one);
    let stepped = super::expr::call(builder, ctx, RuntimeOp::Add, &[index, one])?[0];
    let stepped = super::expr::as_value(builder, stepped);
    builder.jump(head, &[stepped])?;
    Ok(())
}
