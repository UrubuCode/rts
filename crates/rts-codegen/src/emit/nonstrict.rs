//! The two things a NON-STRICT function does that a strict one does not.
//!
//! # Why this exists at all, when module code is always strict
//!
//! Because `Function(text)` and `eval(text)` are not module code. The text they
//! compile is a *script* body, and a script body with no `"use strict"`
//! directive is sloppy — so `Function("return this === globalThis")()` answers
//! `true` in every engine while every function in a `.ts` file answers `false`
//! for the same expression. This engine compiles that text through
//! `rts-host`'s `live.rs`, which is the only producer of a non-strict body
//! here, and [`crate::emit::Ctx::sloppy`] is how it says so.
//!
//! # Why the two live together
//!
//! They are one distinction with two observable consequences, and putting them
//! in two places is how one of them ends up applied where the other is not:
//!
//! - `this` is substituted when the call passed no receiver
//!   (`OrdinaryCallBindThis`), which is [`substitute_receiver`];
//! - `arguments.callee` names the running function, which is [`define_callee`].
//!   A STRICT function's arguments object has `callee` too — as an accessor
//!   pair that throws on read — and that is the part this does not do, because
//!   the poison accessor needs a cell shape this runtime has no producer for.
//!   Absent is the honest answer there: `typeof arguments.callee` is
//!   `"undefined"` in strict code here where a real engine throws, and that is
//!   a wrong answer for a program written to catch it and the right one for
//!   every program that never mentions it.
//!
//! Both are refused for an ARROW, which has neither its own `this` nor its own
//! `arguments`.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitResult, expr};
use crate::runtime::RuntimeOp;
use crate::syntax::Directive;

/// Whether a directive prologue turns strict mode on.
///
/// The prologue is the parser's — `parse::item::as_directive` collects it off
/// the front of the body and `Directive::is_use_strict` compares the RAW text,
/// which is what makes `"use strict"` an ordinary string statement rather
/// than a cleverly-spelled directive. Nothing is re-derived here: this is one
/// line over what the tree already carries, and the alternative — scanning the
/// leading statements — would have answered `false` always, because a directive
/// is not among them by the time emission sees the body.
pub(super) fn is_strict(directives: &[Directive]) -> bool {
    directives.iter().any(Directive::is_use_strict)
}

/// `OrdinaryCallBindThis` — the receiver, or the global object when there is
/// none.
///
/// Emitted once at the head of the body rather than at each `this`, because the
/// specification binds it when the function is ENTERED: two reads of `this` in
/// one activation are the same object, which a per-read call would also give
/// but would pay for every time.
pub(super) fn substitute_receiver(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
) -> EmitResult<ValueId> {
    Ok(expr::call(builder, ctx, RuntimeOp::SloppyThis, &[receiver])?[0])
}

/// `arguments.callee` — the function the arguments object was made for.
///
/// Non-enumerable, through `DefineMethod`, because that is what the attribute
/// says in every engine: `Object.keys(arguments)` answers the indices and
/// nothing else. An ordinary write would have put `callee` in `for`-`in`.
///
/// The value comes from [`RuntimeOp::RunningFunction`] and not from the name
/// the function was declared with, for the reason the self-binding above it in
/// `function.rs` gives: `const g = function f() {}` must answer the function
/// itself, and an anonymous function expression has no name to read at all.
pub(super) fn define_callee(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    arguments: ValueId,
) -> EmitResult<()> {
    let name = ctx.names.intern("callee");
    let key = super::property::key_constant(builder, ctx, name);
    let running = expr::call(builder, ctx, RuntimeOp::RunningFunction, &[])?[0];
    expr::call(builder, ctx, RuntimeOp::DefineMethod, &[arguments, key, running])?;
    Ok(())
}
