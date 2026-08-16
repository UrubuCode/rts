//! Expressions that choose a path, and the merge that turns one back into a
//! value.
//!
//! # Why these are not in `expr.rs`
//!
//! Every other expression emits into the block it started in. These four —
//! `?:`, `&&`, `||`, `??` — create blocks, and one of them is *specified* not to
//! evaluate part of itself. `a && b()` where `a` is falsy must not call `b`, so
//! a version that emitted both operands and then chose between the results
//! would be a different language rather than a slower one.
//!
//! # Why the merge here is not the one `stmt.rs` does for `if`
//!
//! They look like one rule and are not. A statement's arm can fail to reach the
//! join — `if (c) return 1; else return 2;` is a whole function — so that merge
//! is mostly about the cases where there is nothing to merge. An operand of an
//! expression always reaches the join, because nothing an expression can hold
//! terminates a block: `return` is a statement, and `throw` is a gap. So this
//! one always has two live paths and always carries a value, and writing it as
//! the general case would mean every caller here handling three outcomes that
//! cannot happen.
//!
//! If `throw` arrives as an expression-position construct, that stops being true
//! and the two collapse into one function. Named here so the day it does, the
//! reason this was ever two is on record.

use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};

use super::expr::{self, emit_condition, emit_expr};
use super::scope::Binding;
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::syntax::{Expr, LogicalOp};
use crate::values::Singleton;

/// One path through a two-way expression, as it was left.
///
/// `exit` is where the path *ended*, which is not where it began as soon as
/// anything nests: `c ? (d ? 1 : 2) : 3` leaves its first arm in a block the
/// inner conditional created. Jumping from the block the arm started in would
/// append a second terminator to a block whose branch is already there — the
/// same defect `emit_if` records having hit.
struct Path {
    /// The block the path ended in.
    exit: BlockId,
    /// What every name in scope meant there.
    bindings: Vec<Binding>,
    /// What the expression produced along it.
    value: ValueId,
}

/// Merges two paths and yields the value the expression produced.
///
/// The join takes the value as its first parameter and one more for each name
/// the two paths disagree about. A name neither touched has one definition and
/// gets nothing — the same reasoning `emit_if` states, and for the same reason:
/// a parameter for a name that did not change is a correct program moving a
/// value through a register for no purpose.
///
/// The value parameter is `UNPROVEN` unconditionally. Two paths can produce two
/// different representations — `c ? 1 : o.x` is a double on one and a tagged
/// value on the other — and there is no representation that is both, so the
/// widening happens on each path where it can still be defended.
fn merge(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    first: Path,
    second: Path,
) -> EmitResult<ValueId> {
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    let merged = super::merge::disagreements(&first.bindings, &second.bindings);
    let params = super::merge::parameters(builder, join, &merged, &first.bindings);

    for path in [&first, &second] {
        builder.switch_to(path.exit);
        // Widened in the path's own block, not at the join: a widening is an
        // instruction, and it has to sit where the value it widens is defined.
        let value = builder.widen(path.value);
        let mut args = vec![value];
        args.extend(merged.iter().map(|&at| path.bindings[at].value()));
        builder.jump(join, &args)?;
    }

    super::merge::settle(scope, first.bindings, &merged, params);

    builder.switch_to(join);
    Ok(result)
}

/// Emits `c ? a : b`.
pub fn emit_conditional(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    condition: &Expr,
    then_branch: &Expr,
    else_branch: &Expr,
) -> EmitResult<ValueId> {
    let cond = emit_condition(builder, scope, ctx, condition)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let before = scope.snapshot();
    builder.branch(cond, (then_block, &[]), (else_block, &[]))?;

    builder.switch_to(then_block);
    let value = emit_expr(builder, scope, ctx, then_branch)?;
    let taken = Path {
        exit: builder.current(),
        bindings: scope.snapshot(),
        value,
    };

    // The second arm starts from the environment the first one started in.
    // Without this, `c ? (x = 1) : x` would read what the other arm produced.
    scope.restore(&before);
    builder.switch_to(else_block);
    let value = emit_expr(builder, scope, ctx, else_branch)?;
    let untaken = Path {
        exit: builder.current(),
        bindings: scope.snapshot(),
        value,
    };

    merge(builder, scope, taken, untaken)
}

/// Emits `&&`, `||` or `??`.
///
/// # What the left operand is, and is not
///
/// The value, not the test. `0 || "x"` is `"x"` and `1 || "x"` is `1` — never
/// `true`. So the short-circuit path carries the operand itself, and the
/// boolean the branch consumed is discarded. That is the difference between
/// these and the relational operators, which produce a boolean because that is
/// what they mean.
pub fn emit_logical(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: LogicalOp,
    left: &Expr,
    right: &Expr,
) -> EmitResult<ValueId> {
    let decided = emit_expr(builder, scope, ctx, left)?;
    emit_logical_from(builder, scope, ctx, op, decided, right)
}

/// The same operator, over a left side that has already been emitted.
///
/// `x &&= v` is this: the left operand is a binding that was read rather than
/// an expression to evaluate, and reading it again would be a second evaluation
/// of a target the language reads once. Split out rather than duplicated so the
/// short-circuit and the merge have one implementation — the whole content of
/// the operator is *when the right side is skipped*, and two copies of that is
/// two chances to skip it in different cases.
pub fn emit_logical_from(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: LogicalOp,
    decided: ValueId,
    right: &Expr,
) -> EmitResult<ValueId> {
    emit_logical_write(builder, scope, ctx, op, decided, right, |_, _, _, value| {
        Ok(value)
    })
}

/// The same short-circuit as [`emit_logical_from`], with a write performed
/// only on the path that evaluated the right side.
///
/// What `obj.x ||= v` needs and a plain local does not: when the left side
/// already decided, the specification performs **no write at all** — not a
/// write of the value the left side held. That is observable through a
/// setter and through a frozen object, so the write has to sit inside the
/// branch that ran, rather than after the merge where both paths reach it.
/// `write` receives the newly evaluated right-hand value and answers what the
/// expression produced, which lets the property-write entry point (whatever
/// it is for the target in hand) report back the value actually stored.
pub fn emit_logical_write(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    op: LogicalOp,
    decided: ValueId,
    right: &Expr,
    write: impl FnOnce(&mut FuncBuilder, &mut Scope, &mut Ctx, ValueId) -> EmitResult<ValueId>,
) -> EmitResult<ValueId> {
    let before = scope.snapshot();

    let evaluate = builder.create_block();
    let short = builder.create_block();
    match op {
        LogicalOp::And => {
            let cond = expr::to_boolean(builder, ctx, decided)?;
            builder.branch(cond, (evaluate, &[]), (short, &[]))?;
        }
        LogicalOp::Or => {
            let cond = expr::to_boolean(builder, ctx, decided)?;
            builder.branch(cond, (short, &[]), (evaluate, &[]))?;
        }
        // Not `||` with a different threshold: `0 ?? 1` is `0`. Truthiness is
        // never asked, which is the whole reason the operator was added.
        LogicalOp::Coalesce => branch_on_nullish(builder, ctx, decided, evaluate, short)?,
    }

    builder.switch_to(evaluate);
    let value = emit_expr(builder, scope, ctx, right)?;
    let written = write(builder, scope, ctx, value)?;
    let evaluated = Path {
        exit: builder.current(),
        bindings: scope.snapshot(),
        value: written,
    };

    scope.restore(&before);
    builder.switch_to(short);
    let skipped = Path {
        exit: short,
        bindings: before,
        value: decided,
    };

    merge(builder, scope, evaluated, skipped)
}

/// Branches on whether a value is `null` or `undefined`.
///
/// # Why two comparisons and not one test
///
/// Because nullish is TWO singletons and the machine answers about one at a
/// time. Which two they are is this crate's knowledge and not the machine's,
/// so a single `is_nullish` operation down there would be a language concept in
/// the layer that is not allowed one. The second comparison is only reached
/// when the first said no, which is where the cost sits: a value that IS
/// undefined pays one.
///
/// # Why neither is a call any more
///
/// It used to be two calls to strict equality, and the comment here argued that
/// nothing else could distinguish the singletons — true at the time, and it
/// cost every `?.`, every `??` and every defaulted parameter two calls with the
/// throw check each implies.
///
/// `FuncBuilder::is_singleton` is the machine's answer to exactly that, and it
/// is not a `to_nullish` entry point smuggled in: it names no language concept,
/// it takes a number the machine itself issued, and it is one comparison
/// against a constant word because a singleton has exactly one encoding.
pub(super) fn branch_on_nullish(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    value: ValueId,
    nullish: BlockId,
    present: BlockId,
) -> EmitResult<()> {
    // Nothing proven is a singleton — a proved double, boolean or reference
    // cannot be either of them — so the test has one answer and the branch has
    // one destination. The machine refuses the question for a proven operand
    // rather than answering a constant, which is what surfaces this case here
    // instead of leaving it emitted and always false.
    if builder.repr_of(value) != UNPROVEN {
        builder.jump(present, &[])?;
        return Ok(());
    }

    let undefined = ctx.model.singleton(Singleton::Undefined);
    let is_undefined = builder.is_singleton(value, undefined)?;

    let against_null = builder.create_block();
    builder.branch(is_undefined, (nullish, &[]), (against_null, &[]))?;

    builder.switch_to(against_null);
    let null = ctx.model.singleton(Singleton::Null);
    let is_null = builder.is_singleton(value, null)?;
    builder.branch(is_null, (nullish, &[]), (present, &[]))?;
    Ok(())
}

/// Turns a proven boolean into the JavaScript value it stands for, optionally
/// negated.
///
/// # Why a branch and not an instruction
///
/// Negation of a boolean is arithmetic on a representation the machine's
/// numeric instructions do not take, and asking for a `Bool`-typed constant to
/// compare against would be deciding how a boolean is held — a machine
/// question, and rule 2 says this crate does not answer one. A branch asks for
/// nothing: it consumes exactly what the machine already promised `branch`
/// takes, and each side names the value it produces.
///
/// Used by `!x` and by `a !== b`, which is what makes it worth a function: the
/// second is the first applied to strict equality, and writing it twice is how
/// `!==` ends up meaning something subtly different from `!(a === b)`.
pub fn from_bool(builder: &mut FuncBuilder, cond: ValueId, negated: bool) -> EmitResult<ValueId> {
    let when_true = builder.create_block();
    let when_false = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    builder.branch(cond, (when_true, &[]), (when_false, &[]))?;

    builder.switch_to(when_true);
    let answer = expr::boolean_constant(builder, !negated);
    builder.jump(join, &[answer])?;

    builder.switch_to(when_false);
    let answer = expr::boolean_constant(builder, negated);
    builder.jump(join, &[answer])?;

    builder.switch_to(join);
    Ok(result)
}
