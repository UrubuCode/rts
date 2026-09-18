//! `return f(…)` as a proper tail call — where the language says it is one.
//!
//! # What the language says
//!
//! ECMAScript 2015 §15.10 (`IsInTailPosition`, `PrepareForTailCall`): a call
//! in tail position of STRICT code runs after the caller's execution context
//! is discarded, so tail recursion runs in constant stack. JavaScriptCore
//! implements it — Bun answers `loop(500000)` for
//! `function loop(n) { return n <= 0 ? 0 : loop(n - 1) }` — and V8 does not,
//! so Node throws `RangeError: Maximum call stack size exceeded` for the same
//! program. The standard is the ruler here, and Bun is the runtime that meets
//! it.
//!
//! # How, in one sentence
//!
//! The call becomes `RuntimeOp::TailCall`, which records it; the function then
//! returns, and the runtime door that called it makes the recorded call after
//! this frame is gone. `rts_core::entry::tail_call` has the mechanism and why
//! it is not the machine's `return_call`.
//!
//! # Where it is refused, and why each refusal is the language's and not a gap
//!
//! Nothing may run between the record and the return, because whatever runs
//! there would run BEFORE the callee instead of after it. So every clause
//! below names work that happens after a return:
//!
//! - **sloppy code** — the specification makes it strict-only, because a
//!   sloppy function's `arguments` and `caller` could observe the frame;
//! - **`async` bodies and generators** — `IsInTailPosition` excludes them by
//!   name: their return settles a promise or finishes an iterator, and their
//!   body is resumed by something other than the door that settles a record;
//! - **a derived constructor** — its return is checked against the `this`
//!   `super()` made, which is work after the return;
//! - **a protected region or a `finally`** — the handler, the `finally`, the
//!   iterator close a `for`-`of` owes and a `using` disposal all run on the
//!   way out;
//! - **`with`** — the callee's name resolves through an object at run time,
//!   and the scope it was resolved in is part of the frame being discarded.
//!
//! A refused call is an ordinary call. That is never wrong — only deeper.
//!
//! Recognised: `return f(…)`, `return f(…) as T`, and either arm of
//! `return c ? f(…) : g(…)` — the conditional is how an accumulator loop is
//! usually written, and it is taken as the `if` it means. NOT recognised, and
//! could be: the right of `a && f()` and `a || f()`, and a comma's last
//! operand. The specification makes each a tail position; each is an ordinary
//! call here.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitResult, Scope};
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{Expr, ExprKind, Function, Spreadable, Stmt, StmtKind};

/// Whether a body of this function may contain a tail call at all.
///
/// Answered once per body, where the function's kind is known; the questions
/// that change INSIDE a body — a region opened, a `finally` entered — are
/// asked at the `return` by [`emit_return_call`].
pub(super) fn permitted(function: &Function, derived_constructor: bool) -> bool {
    !function.is_async && !function.is_generator && !derived_constructor
}

/// Emits `returned` as a tail call when it is one, and answers `None` —
/// having emitted NOTHING — when it is not, so the caller emits the ordinary
/// return instead.
pub(super) fn emit_return_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    returned: &Expr,
) -> EmitResult<Option<ValueId>> {
    if !in_tail_position(builder, ctx) {
        return Ok(None);
    }
    emit_tail_call(builder, scope, ctx, returned)
}

/// `return c ? a : b` as `if (c) return a; else return b;`, when an arm is a
/// call and the `return` is in tail position — so each arm reaches
/// [`emit_return_call`] on its own. The two are the same program: the
/// condition is read once, for truthiness, and the arm it picks is returned.
pub(super) fn conditional_return(
    builder: &FuncBuilder,
    ctx: &Ctx,
    returned: &Expr,
    at: rts_cranelift::fault::Position,
) -> Option<Stmt> {
    let ExprKind::Conditional {
        condition,
        then_branch,
        else_branch,
    } = &returned.kind
    else {
        return None;
    };
    let is_call = |arm: &Expr| matches!(arm.kind, ExprKind::Call { .. });
    if !in_tail_position(builder, ctx) || !(is_call(then_branch) || is_call(else_branch)) {
        return None;
    }
    let returning = |arm: &Expr| Box::new(Stmt::new(StmtKind::Return(Some(arm.clone())), at));
    let kind = StmtKind::If {
        condition: (**condition).clone(),
        then_branch: returning(then_branch),
        else_branch: Some(returning(else_branch)),
    };
    Some(Stmt::new(kind, at))
}

/// What a `return` here owes nothing to: see the module's list of refusals.
fn in_tail_position(builder: &FuncBuilder, ctx: &Ctx) -> bool {
    ctx.tail_calls
        && !ctx.sloppy
        && ctx.with_objects.is_empty()
        && ctx.finally_returns.is_empty()
        && builder.innermost_open_region().is_none()
}

/// An answer of a body `inline.rs` substitutes, which is in tail position when
/// the substituted call was.
///
/// # Why a substitution has to carry this at all
///
/// Substituting `g` into `return g(n)` replaces the one tail call with g's
/// body — and when that body ends in `return f(n - 1)`, emitting its answer as
/// an ordinary call turns a chain of tail calls into real recursion. The
/// mutual pair `even`/`odd` is the shape: `odd` is substituted into `even`, so
/// `even` calls `even` and the stack grows by one frame every two levels. A
/// million-level `odd(1000001)` overflowed that way while the direct
/// `loop(1000000)` beside it ran flat.
///
/// Nothing runs between such an answer and the return the substitution sits
/// in: the answer is widened, jumps to the join, and the join's value is what
/// [`emit_return_call`] returns — so the record is still made last.
pub(super) fn emit_answer(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    answer: &Expr,
    tail: bool,
) -> EmitResult<ValueId> {
    if tail && let Some(recorded) = emit_tail_call(builder, scope, ctx, answer)? {
        return Ok(recorded);
    }
    super::expr::emit_expr(builder, scope, ctx, answer)
}

/// `returned` through `RuntimeOp::TailCall` when it is a call the record can
/// hold, emitting nothing and answering `None` when it is not. The caller has
/// already established that the position is a tail position.
fn emit_tail_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    returned: &Expr,
) -> EmitResult<Option<ValueId>> {
    // `return f() as T` is the same call: a type assertion emits nothing.
    let mut called = returned;
    while let ExprKind::Asserted { value, .. } = &called.kind {
        called = value.as_ref();
    }
    let ExprKind::Call {
        callee,
        arguments,
        optional: false,
    } = &called.kind
    else {
        return Ok(None);
    };
    // A class field initialiser is carried as a call node and is not one.
    if super::class::field_initialiser(called).is_some() {
        return Ok(None);
    }
    // The record holds what the convention carries. A spread or a fifth
    // argument travels in a vector the door pushes for ONE activation, and a
    // record that outlived it would hand the callee a popped vector.
    let fits = arguments.len() <= ARGUMENT_SLOTS
        && arguments
            .iter()
            .all(|argument| matches!(argument, Spreadable::Single(_)));
    if !fits {
        return Ok(None);
    }
    // Widened because the call may not become one: an inlined body or a
    // `Math` operation answers whatever it proved, and a return is tagged.
    let produced =
        super::call::emit_call_as(builder, scope, ctx, callee, arguments, RuntimeOp::TailCall)?;
    Ok(Some(super::expr::as_value(builder, produced)))
}
