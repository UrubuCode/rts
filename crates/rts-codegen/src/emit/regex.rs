//! A regular expression literal.
//!
//! # Why this is three calls and not one instruction
//!
//! `/a+/g` names a pattern the compiler can read, and it would be tempting to
//! resolve it here — compile it, and emit a reference to the result. That is
//! wrong twice over.
//!
//! It is a **machine** question first: compiling a pattern allocates, and where
//! an allocation goes is rule 2's territory rather than this crate's. It is a
//! **language** question second, and that one is decisive: a literal evaluated
//! twice is two objects. Each has its own `lastIndex`, so two passes of a loop
//! that each make one must not share where the next search starts — and a
//! hoisted constant would make exactly that mistake, silently, in the one case
//! (`g`) where a program can tell.
//!
//! # Why the pattern crosses as a string
//!
//! It could cross as a number naming a table the compilation baked, the way a
//! string literal does. That table would serve the literal and have nothing to
//! say about `new RegExp(s)`, where the pattern is a value — so there would be
//! two ways to make a regular expression, and rule 3 asks for one. Two string
//! literals and a call is the shape both spellings reach.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{call, string_literal};
use super::{Ctx, EmitResult};
use crate::runtime::RuntimeOp;

/// Emits `/pattern/flags`.
pub(super) fn literal(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    pattern: &str,
    flags: &str,
) -> EmitResult<ValueId> {
    // Interned like any other string literal, which is what makes the pattern
    // text appear once in the program however many times the literal is
    // evaluated. What is not shared is the object the call below makes.
    let pattern = string_literal(builder, ctx, pattern)?;
    let flags = string_literal(builder, ctx, flags)?;
    Ok(call(builder, ctx, RuntimeOp::RegexNew, &[pattern, flags])?[0])
}
