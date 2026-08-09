//! `?.` — optional chaining, and the short circuit that reaches past it.
//!
//! # Why this is not in `expr.rs`
//!
//! `ExprKind::Chain` is, like `?:`, `&&`, `||` and `??`, one of the forms that
//! may not evaluate part of itself and that creates blocks to say so — the same
//! reason `choice.rs` is its own file. It is not folded into `choice.rs`
//! because what it branches on is not a boolean or a nullish test of one
//! value: it is zero or more tests, one per optional link, threaded through a
//! walk of the member/index/call spine the chain wraps.
//!
//! # What `?.` means, precisely
//!
//! `a?.b.c` is not `(a?.b).c` with a crash swapped for `undefined` at one
//! link. It is `a === null || a === undefined ? undefined : a.b.c` — the
//! ENTIRE rest of the chain is skipped, not just the link that carries the
//! `?.`. `ExprKind::Chain` is the node that says how far the skip reaches: it
//! wraps the whole spine, and every optional link inside it exits straight to
//! the chain's own join rather than to whatever would ordinarily come next.
//!
//! `0` and `""` do not short-circuit — only `null` and `undefined` do, which
//! is exactly what [`super::choice::branch_on_nullish`] tests and nothing
//! else. Reused rather than reimplemented: `??` already needed "is this value
//! one of the two nullish singletons", and a second copy here is how that
//! test would drift from `??`'s.
//!
//! # Evaluated once
//!
//! `a?.b.c` reads `a` exactly once, whichever way it turns out: the walk below
//! evaluates the object of a link before deciding whether that link's `?.`
//! short-circuits, and the result is threaded downward rather than the object
//! expression being re-emitted after the test.

use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};

use super::expr::{self, emit_expr};
use super::{Ctx, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind};

/// Emits the whole of an optional chain, `a?.b.c`, `a?.[k]`, `f?.()` and every
/// mix of them.
///
/// One join for the whole chain, not one per link: every short-circuiting
/// link jumps straight here with `undefined`, and the spine that reaches the
/// end jumps here with whatever it produced. That is `a?.b.c`'s "the rest of
/// the chain is skipped" made concrete — a join per link would instead thread
/// `undefined` *through* `.c`, which is a crash if `.c` is a call and the
/// wrong value if it is a property read on `undefined`.
pub fn emit_chain(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    inner: &Expr,
) -> EmitResult<ValueId> {
    let join = builder.create_block();
    let result = builder.add_block_param(join, super::UNPROVEN);

    let value = walk(builder, scope, ctx, join, inner)?;
    let value = expr::tagged(builder, value);
    builder.jump(join, &[value])?;

    builder.switch_to(join);
    Ok(result)
}

/// Walks one node of the spine a `Chain` wraps, short-circuiting to `join`
/// with `undefined` at any link written `?.`.
///
/// Recurses into the object/callee position for `Member`, `Index` and `Call`
/// — the three link kinds `optional` can mark — because the object of an
/// inner link may itself be an optional link: `a?.b.c` has `a?.b` as the
/// object of the outer `.c`. Anything else is the base of the chain, an
/// ordinary expression with no link of its own, and is handed to
/// [`emit_expr`].
fn walk(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    join: BlockId,
    expr: &Expr,
) -> EmitResult<ValueId> {
    match &expr.kind {
        ExprKind::Member {
            object,
            property,
            optional,
        } => {
            // The same redirect `expr.rs`'s plain `Member` arm takes: an object
            // the escape analysis replaced has no object to read `object` FROM
            // at all — reading it as a receiver here is exactly the bug this
            // guards, since a flattened local was never declared under its own
            // name. `field_of` already refuses when `optional` is set, so this
            // costs nothing on the ordinary optional-chain path.
            if let Some(field) = super::escape::field_of(ctx, object, *property, *optional) {
                return super::binding::read(builder, scope, ctx, field);
            }
            let receiver = walk(builder, scope, ctx, join, object)?;
            let receiver = maybe_short_circuit(builder, ctx, join, receiver, *optional)?;
            super::property::emit_read(builder, ctx, receiver, *property)
        }
        // `o?.[k]` and `o[k]` inside a chain. The literal-key fast path
        // `expr.rs`'s `Index` arm takes is not reused here: that lookup is a
        // private helper of `expr.rs`, kept that way because it is only ever
        // reached from the two sites in that file that already own the rule
        // for what a canonical array index is. Reimplementing the same test a
        // third time, in a third file, is the drift rule 3 exists to prevent
        // — so a computed key inside a chain always takes the general path,
        // `GetIndexed`, which is correct and merely not the 150x-faster one.
        ExprKind::Index {
            object,
            index,
            optional,
        } => {
            let receiver = walk(builder, scope, ctx, join, object)?;
            let receiver = maybe_short_circuit(builder, ctx, join, receiver, *optional)?;
            let key = emit_expr(builder, scope, ctx, index)?;
            Ok(expr::call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0])
        }
        ExprKind::Call {
            callee,
            arguments,
            optional,
        } => {
            let (receiver, function) = walk_callee(builder, scope, ctx, join, callee)?;
            let function = maybe_short_circuit(builder, ctx, join, function, *optional)?;
            super::call::emit_call_with(builder, scope, ctx, function, receiver, arguments)
        }
        _ => emit_expr(builder, scope, ctx, expr),
    }
}

/// The callee half of [`walk`]'s `Call` arm: what is called, and what it is
/// called ON.
///
/// The mirror of `call.rs`'s `callee_and_receiver`, kept apart rather than
/// reused because that one's answer for an optional link is to refuse —
/// correctly, for a call outside any chain, since nothing there can express a
/// short circuit reaching past the call. Inside a chain it can, and it is
/// exactly the same "receiver once, then decide" shape, so this walks the
/// object through [`walk`] instead of `emit_expr` — the object may itself be
/// an optional link — and threads the receiver through [`maybe_short_circuit`]
/// instead of erroring.
fn walk_callee(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    join: BlockId,
    callee: &Expr,
) -> EmitResult<(ValueId, ValueId)> {
    let pair = match &callee.kind {
        ExprKind::Member {
            object,
            property,
            optional,
        } => {
            // Same redirect as `walk`'s own `Member` arm. A method call still
            // kills the candidate in `escape.rs`'s own scan (the receiver
            // escapes as `this`), so this is defensive rather than reachable
            // today — but it is the same fact stated once rather than assumed
            // twice, which is what a second Member-reading site owes.
            if let Some(field) = super::escape::field_of(ctx, object, *property, *optional) {
                let value = super::binding::read(builder, scope, ctx, field)?;
                let undefined = expr::undefined(builder, ctx);
                return Ok((undefined, value));
            }
            let receiver = walk(builder, scope, ctx, join, object)?;
            let receiver = maybe_short_circuit(builder, ctx, join, receiver, *optional)?;
            let function = super::property::emit_read(builder, ctx, receiver, *property)?;
            (receiver, function)
        }
        ExprKind::Index {
            object,
            index,
            optional,
        } => {
            let receiver = walk(builder, scope, ctx, join, object)?;
            let receiver = maybe_short_circuit(builder, ctx, join, receiver, *optional)?;
            let key = emit_expr(builder, scope, ctx, index)?;
            let function = expr::call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
            (receiver, function)
        }
        _ => {
            let function = walk(builder, scope, ctx, join, callee)?;
            let undefined = expr::undefined(builder, ctx);
            (undefined, function)
        }
    };
    Ok(pair)
}

/// Tests a link's object for `null`/`undefined` when the link was written
/// `?.`, exiting straight to the chain's join with `undefined` when it is.
///
/// A plain link (`optional` false) costs nothing: the value passes through
/// unchanged and no block is created. `0` and `""` do not take the exit —
/// [`super::choice::branch_on_nullish`] tests exactly the two singletons the
/// language names, nothing wider.
fn maybe_short_circuit(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    join: BlockId,
    value: ValueId,
    optional: bool,
) -> EmitResult<ValueId> {
    if !optional {
        return Ok(value);
    }

    let nullish = builder.create_block();
    let present = builder.create_block();
    super::choice::branch_on_nullish(builder, ctx, value, nullish, present)?;

    builder.switch_to(nullish);
    let undefined = expr::undefined(builder, ctx);
    builder.jump(join, &[undefined])?;

    builder.switch_to(present);
    Ok(value)
}
