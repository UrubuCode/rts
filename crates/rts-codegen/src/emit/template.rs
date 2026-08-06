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
/// # The divergence, named
///
/// The specification caches the strings object **per call site**, so a tag that
/// uses it as a map key sees the same object on every pass. This builds a fresh
/// one per evaluation. Caching it needs somewhere per-site to keep a value, which
/// is what an inline cache cell is and it holds two words of machine data rather
/// than a reference the collector must see. Recorded rather than approximated:
/// the wrong version is one that looks identical until a program memoises.
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

    let cooked_texts: Vec<Option<&str>> = parts
        .iter()
        .map(|part| part.cooked.as_deref())
        .collect();
    let strings = string_array(builder, ctx, &cooked_texts)?;
    let raw_texts: Vec<Option<&str>> = parts.iter().map(|part| Some(part.raw.as_str())).collect();
    let raw = string_array(builder, ctx, &raw_texts)?;
    // Interned like any other property name, so `strings.raw` written in a tag
    // reaches the same key number this write used — one numbering, which is the
    // agreement `Names` exists to hold.
    let name = ctx.names.intern("raw");
    let key = super::property::key_constant(builder, ctx, name);
    super::expr::call(builder, ctx, RuntimeOp::SetProperty, &[strings, key, raw])?;

    let mut values = Vec::with_capacity(1 + expressions.len());
    values.push(strings);
    for expression in expressions {
        values.push(emit_expr(builder, scope, ctx, expression)?);
    }
    super::call::issue(builder, ctx, function, receiver, &values)
}

/// An array of string values, with `undefined` where there is no text.
///
/// A hole would be wrong: the specification puts `undefined` at a cooked
/// position whose escapes were invalid, and the array's length is what tells the
/// tag how many pieces there were.
fn string_array(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    texts: &[Option<&str>],
) -> EmitResult<ValueId> {
    let length = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(texts.len() as u64),
    });
    let length = builder.use_const(length);
    let array = super::expr::call(builder, ctx, RuntimeOp::ArrayNew, &[length])?[0];
    for (position, text) in texts.iter().enumerate() {
        let value = match text {
            Some(text) => string_literal(builder, ctx, text)?,
            None => super::expr::undefined(builder, ctx),
        };
        let at = super::expr::number_constant(builder, position as f64);
        super::expr::call(builder, ctx, RuntimeOp::SetIndexed, &[array, at, value])?;
    }
    Ok(array)
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
