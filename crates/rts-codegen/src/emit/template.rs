//! Template literals.
//!
//! # Why this needs no operation of its own
//!
//! A template is concatenation, and `+` already concatenates. The pieces are
//! folded left to right starting from the **first literal piece**, which is
//! what makes the fold correct rather than merely plausible: `+` decides
//! between adding and concatenating from its operands, so a fold that began
//! with the first *substitution* would compute `` `${1}${2}` `` as `3`.
//! Beginning with a string — even the empty one — makes every step a
//! concatenation, which is what a template means.
//!
//! # Its own module rather than more of `expr.rs`
//!
//! That file was at 975 lines when this was appended to it, and rule 8 says a
//! file approaching the ceiling is split rather than grown. This is the piece
//! that had just been added, so it is the piece that moves.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{emit_binary, emit_expr, gap, string_literal};
use super::{Ctx, EmitError, EmitResult, Scope};
use crate::syntax::{BinaryOp, Expr, TemplatePart};

/// Emits a template literal.
///
/// # Why this needs no operation of its own
///
/// A template is concatenation, and `+` already concatenates. The pieces are
/// folded left to right starting from the **first literal piece**, which is
/// what makes the fold correct rather than merely plausible: `+` decides
/// between adding and concatenating from its operands, so a fold that began
/// with the first *substitution* would compute `` `${1}${2}` `` as `3`.
/// Beginning with a string — even the empty one — makes every step a
/// concatenation, which is what a template means.
///
/// # Why `cooked` and not `raw`
///
/// `raw` is the text as written, escapes and all; `cooked` is what the value
/// is. `` `\n` `` is one code unit, not two characters. A part with no cooked
/// text is one whose escapes are invalid, which is legal only in a **tagged**
/// template — where the tag can still read `raw` — so it is refused here rather
/// than approximated with the raw text.
pub fn emit_template(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    parts: &[TemplatePart],
    expressions: &[Expr],
) -> EmitResult<ValueId> {
    let Some((first, rest)) = parts.split_first() else {
        // The tree states that there is always one more part than expression,
        // so an empty list is a malformed tree rather than a program.
        return gap("a template with no literal parts");
    };

    let mut joined = string_literal(builder, ctx, cooked(first)?)?;
    // One part after each expression, which is the invariant the tree records
    // as "always one more than `expressions`". Zipping is what reads it.
    for (expression, part) in expressions.iter().zip(rest) {
        let value = emit_expr(builder, scope, ctx, expression)?;
        joined = emit_binary(builder, ctx, BinaryOp::Add, joined, value)?;
        let text = string_literal(builder, ctx, cooked(part)?)?;
        joined = emit_binary(builder, ctx, BinaryOp::Add, joined, text)?;
    }
    Ok(joined)
}

/// The value a template part stands for.
fn cooked(part: &TemplatePart) -> EmitResult<&str> {
    match &part.cooked {
        Some(text) => Ok(text),
        // Legal only in a tagged template, where the tag reads `raw` instead.
        None => Err(EmitError::Unsupported {
            construct: "a template part whose escapes are invalid",
        }),
    }
}
