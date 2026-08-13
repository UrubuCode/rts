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

use rts_cranelift::ir::{ConstDecl, ScalarBits};
use rts_cranelift::repr::Repr;

use super::expr::{emit_binary, emit_expr, gap, string_literal};
use crate::runtime::RuntimeOp;
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

    // An EMPTY literal part is not joined. `` `${a}${b}` `` has three parts and
    // two of them are empty, and each one was a crossing into `__rts_add` plus
    // an intermediate string, to append nothing. The commonest interpolation
    // there is — a value straight after another value — paid two of them.
    //
    // Not a general fold of empty operands: this is the one place that KNOWS a
    // part is empty at compile time, because it has the text. Teaching `add`
    // to check would put the test on every concatenation in the program to
    // serve the one that could have been decided here.
    let mut joined = string_literal(builder, ctx, cooked(first)?)?;
    // One part after each expression, which is the invariant the tree records
    // as "always one more than `expressions`". Zipping is what reads it.
    for (expression, part) in expressions.iter().zip(rest) {
        let value = emit_expr(builder, scope, ctx, expression)?;
        joined = emit_binary(builder, ctx, BinaryOp::Add, joined, value)?;
        let text = cooked(part)?;
        if text.is_empty() {
            continue;
        }
        let text = string_literal(builder, ctx, text)?;
        joined = emit_binary(builder, ctx, BinaryOp::Add, joined, text)?;
    }
    Ok(joined)
}

/// Emits `` tag`a${b}c` ``.
///
/// # Why this is not a call of the template's value
///
/// The tag receives the **pieces**, not the joined string: an array of the
/// cooked texts, carrying a `raw` property holding the same pieces as written.
/// That is the whole point of the form — `String.raw` reads `raw` and answers
/// text with the escapes unresolved, which no amount of looking at the cooked
/// result can recover.
///
/// # Why a part with invalid escapes is `undefined` here and refused there
///
/// `` tag`\unicode` `` is legal: the cooked text is absent and the tag reads
/// `raw` instead. An untagged template with the same escape is a syntax error.
/// So [`emit_template`] refuses what this one has to allow, and the difference is
/// the language's rather than an inconsistency.
///
/// # Why the pieces are declared rather than emitted
///
/// The specification gives a site ONE strings object for the life of the
/// program, so a tag using it as a map key sees the same object on every pass.
/// Building it here would produce a fresh one per evaluation.
///
/// Every piece is a compile-time constant, so the site is **declared** — a list
/// of literal positions travelling with the program, exactly as the literals
/// themselves do — and the runtime materialises it once, on first ask. That also
/// removes the building from the path a tagged template in a loop takes.
///
/// The rejected alternative was a per-site cache cell holding the object: an
/// inline cache cell holds two words of machine data the collector does not
/// scan, and a reference kept there would be one it cannot see.
pub fn emit_tagged_template(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    tag: &Expr,
    parts: &[TemplatePart],
    expressions: &[Expr],
) -> EmitResult<ValueId> {
    // The tag first, and through the same function an ordinary call uses, so
    // `` o.tag`x` `` passes `o` as its receiver.
    let (receiver, function) = super::call::callee_and_receiver(builder, scope, ctx, tag)?;

    // Two positions per piece — the cooked text then the raw one — because the
    // site crosses to the runtime as numbers. A piece whose escapes are invalid
    // has no cooked text and carries the sentinel, which is legal here and a
    // syntax error in an untagged template.
    let mut pieces = Vec::with_capacity(parts.len() * 2);
    for part in parts {
        pieces.push(match &part.cooked {
            Some(text) => ctx.literal(text),
            None => super::NO_COOKED,
        });
        pieces.push(ctx.literal(&part.raw));
    }
    // An integer rather than a tagged value, for the reason a property key is
    // one: it is not a value the program could compute, it is which site.
    let which = ctx.template(pieces);
    let which = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(which)),
    });
    let which = builder.use_const(which);
    let strings = super::expr::call(builder, ctx, RuntimeOp::TemplateStrings, &[which])?[0];

    let mut values = Vec::with_capacity(1 + expressions.len());
    values.push(strings);
    for expression in expressions {
        values.push(emit_expr(builder, scope, ctx, expression)?);
    }
    super::call::issue(builder, ctx, function, receiver, &values)
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
