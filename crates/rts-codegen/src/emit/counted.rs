//! Recognising `for (let i = 0; i < a.length; i++)` over an array, and proving
//! its body cannot move the run underneath it.
//!
//! # Why this exists beside `foreach.rs` rather than inside it
//!
//! `foreach.rs` desugars `for-of`, and what makes ITS hoist safe is a fact about
//! the desugaring: the array walked is the copy `iterate` just made, no program
//! can name it, and the loop only reads. A counted loop over an array a program
//! wrote has none of that. The safety has to be established here, from the shape
//! of the loop, or not at all.
//!
//! `loops.rs` is 980 lines against this crate's 1000-line ceiling, so this lands
//! in a module of its own rather than being appended to it (rule 5).
//!
//! # What the recogniser admits, and why it is this narrow
//!
//! Exactly `for (let i = 0; i < a.length; i++)` where `a` and `i` are plain
//! names, `i` is declared by the head, and nothing in the body can reach `a` to
//! change it. Anything else keeps the ordinary path.
//!
//! The narrowness is the point. A hoisted base is an ADDRESS into a `Vec`, and
//! `rts-core`'s `elements_base` states the obligation: it is good only for as
//! long as nothing grows that array. `for-of` discharges it with a copy; here it
//! is discharged by refusing every body that could do the growing. A predicate
//! that admitted one it should not would be a load from freed memory, which is
//! why this errs toward refusing and says so at each arm.
//!
//! # What it does NOT prove, and what pays for that at run time
//!
//! That `a` is an array at all, and that the element found is not a hole.
//! Neither is knowable here — this crate has no type pass — so both are compares
//! in the emitted read rather than claims made here. See
//! `expr.rs`'s guarded element read.

use rts_cranelift::ir::{CmpOp, ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

use super::{Ctx, EmitResult, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    AssignTarget, BinaryOp, Expr, ExprKind, ForInit, Literal, Pattern, Stmt, StmtKind, UpdateOp,
    UpdatePosition,
};

/// The array and the index of a counted walk this loop is, when it is one.
///
/// Answers `None` for every loop that is not exactly the shape above, and for
/// every one whose body could reach the array. A caller may treat `Some` as
/// "the base and count read before this loop stay valid for its whole run".
pub(super) fn array_walk(
    init: Option<&ForInit>,
    test: Option<&Expr>,
    update: Option<&Expr>,
    body: &Stmt,
    length_key: Name,
) -> Option<(Name, Name)> {
    let index = index_from_zero(init?)?;
    let array = tested_against_length(test?, index, length_key)?;
    if !increments(update?, index) {
        return None;
    }
    // The index must not be written anywhere but the update, or the walk is not
    // a walk: a body that assigns `i` decides its own order and the hoisted
    // count stops bounding what it reads.
    if writes_name(body, index) {
        return None;
    }
    if may_reach(body, array) {
        return None;
    }
    Some((array, index))
}

/// `let i = 0`, and the name it declared.
fn index_from_zero(init: &ForInit) -> Option<Name> {
    let ForInit::Declare { bindings, .. } = init else {
        // `for (i = 0; …)` assigns a binding declared outside, which may be
        // captured by a closure made before the loop and written through it.
        // Refused rather than analysed.
        return None;
    };
    let [binding] = bindings.as_slice() else {
        return None;
    };
    let Pattern::Name(index) = binding.target else {
        return None;
    };
    match binding.value.as_ref()?.kind {
        ExprKind::Literal(Literal::Number(start)) if start == 0.0 => Some(index),
        _ => None,
    }
}

/// `i < a.length`, and the `a` it names.
fn tested_against_length(test: &Expr, index: Name, length_key: Name) -> Option<Name> {
    let ExprKind::Binary {
        op: BinaryOp::Less,
        left,
        right,
    } = &test.kind
    else {
        return None;
    };
    if !is_name(left, index) {
        return None;
    }
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &right.kind
    else {
        return None;
    };
    // The property must be `length`, compared against the interned name the
    // caller holds — spelling it here would be a second place deciding what the
    // bound of an array is, and `i < a.size` would hoist a run it never named.
    if *property != length_key {
        return None;
    }
    let ExprKind::Ident(array) = object.kind else {
        return None;
    };
    Some(array)
}

/// `i++` or `++i`, and nothing else.
fn increments(update: &Expr, index: Name) -> bool {
    // Either spelling: the loop reads `i` before the update in both, so which
    // value the expression yields is not observed here.
    matches!(
        &update.kind,
        ExprKind::Update {
            op: UpdateOp::Increment,
            position: UpdatePosition::Prefix | UpdatePosition::Postfix,
            target,
        } if is_name(target, index)
    )
}

fn is_name(expr: &Expr, name: Name) -> bool {
    matches!(expr.kind, ExprKind::Ident(held) if held == name)
}

/// Whether a body assigns to a name anywhere inside it.
fn writes_name(body: &Stmt, name: Name) -> bool {
    let mut found = false;
    let unwalked = walk_stmt(body, &mut |expr| {
        found |= match &expr.kind {
            ExprKind::Assign { target, .. } => match target {
                AssignTarget::Place(place) => is_name(place, name),
                AssignTarget::Pattern(_) => true,
            },
            ExprKind::Update { target, .. } => is_name(target, name),
            _ => false,
        };
    });
    found || unwalked
}

/// Whether anything in this body could reach `array` to change what it holds.
///
/// Conservative in one direction on purpose: it answers `true` for anything it
/// cannot rule out. The cost of a false `true` is the ordinary path, which is
/// what the program had before; the cost of a false `false` is a read from a
/// buffer that has been reallocated, which is not a wrong answer but a wrong
/// ADDRESS — the failure that reproduces rarely and explains nothing.
fn may_reach(body: &Stmt, array: Name) -> bool {
    let mut reached = false;
    let unwalked = walk_stmt(body, &mut |expr| {
        if reached {
            return;
        }
        reached = match &expr.kind {
            // Any call at all. A callee could have captured the array before the
            // loop, and this crate cannot see what a callee does — `inline.rs`
            // makes the same judgement for the same reason: no deoptimiser, so
            // the answer has to be a fact about the whole program.
            ExprKind::Call { .. } | ExprKind::New { .. } | ExprKind::TaggedTemplate { .. } => true,
            // A function made here can be called here, and it can name the
            // array. Refused without looking inside it.
            ExprKind::Function(_) | ExprKind::Class(_) => true,
            // `a = …`, `a[i] = …`, `a.x = …`, `a++`. Only writes THROUGH the
            // array matter, but a write to the name itself matters too: it would
            // leave the hoisted base pointing at the previous array's storage.
            ExprKind::Assign { target, .. } => assign_touches(target, array),
            ExprKind::Update { target, .. } => touches(target, array),
            // `delete a[i]` leaves a hole, which the read handles — but it goes
            // through the same store, so it is refused with the rest.
            ExprKind::Unary { operand, .. } => touches(operand, array),
            _ => false,
        };
    });
    reached || unwalked
}

/// Whether an assignment target names the array or reaches through it.
fn assign_touches(target: &AssignTarget, array: Name) -> bool {
    match target {
        AssignTarget::Place(place) => touches(place, array),
        // A pattern can bind the array's name, which is a write to it.
        AssignTarget::Pattern(_) => true,
    }
}

/// Whether an expression names the array as the thing being acted on.
fn touches(expr: &Expr, array: Name) -> bool {
    match &expr.kind {
        ExprKind::Ident(held) => *held == array,
        ExprKind::Member { object, .. } | ExprKind::Index { object, .. } => touches(object, array),
        _ => false,
    }
}

/// Visits every expression in a statement, including nested statements.
///
/// Written out rather than reusing `suspends.rs`'s walk: that one answers a
/// different question and short-circuits on the arms it cares about, so sharing
/// would tie two predicates to one traversal and make a change to either
/// silently change the other.
/// Answers whether it met a statement kind it does not walk. A caller must treat
/// `true` as "this body could do anything": walking a `try`, a `switch` or a
/// nested loop would be more arms for a case that is rare inside the loops this
/// admits, and one arm written wrongly is the failure the whole predicate exists
/// to avoid.
fn walk_stmt(statement: &Stmt, visit: &mut impl FnMut(&Expr)) -> bool {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => {
            walk_expr(expr, visit);
            false
        }
        StmtKind::Return(held) => {
            if let Some(expr) = held {
                walk_expr(expr, visit);
            }
            false
        }
        StmtKind::Block(statements) => statements
            .iter()
            .fold(false, |seen, held| walk_stmt(held, visit) | seen),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, visit);
            let seen = walk_stmt(then_branch, visit);
            match else_branch {
                Some(held) => walk_stmt(held, visit) | seen,
                None => seen,
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr(condition, visit);
            walk_stmt(body, visit)
        }
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Some(value) = &binding.value {
                    walk_expr(value, visit);
                }
            }
            false
        }
        // Everything else — a nested `for`, a `try`, a `switch`, a labelled
        // statement, a `for-of` — is REFUSED rather than walked, by the caller
        // treating an unrecognised statement as reaching. Walking them would be
        // more code for a case that is rare inside the loops this admits, and
        // getting one arm wrong is the failure this predicate exists to avoid.
        _ => true,
    }
}



fn walk_expr(expr: &Expr, visit: &mut impl FnMut(&Expr)) {
    visit(expr);
    match &expr.kind {
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, visit),
        ExprKind::Update { target, .. } => walk_expr(target, visit),
        ExprKind::Member { object, .. } => walk_expr(object, visit),
        ExprKind::Index { object, index, .. } => {
            walk_expr(object, visit);
            walk_expr(index, visit);
        }
        ExprKind::Assign { target, value, .. } => {
            if let AssignTarget::Place(held) = target {
                walk_expr(held, visit);
            }
            walk_expr(value, visit);
        }
        _ => {}
    }
}

/// The read the recognition above enables: a bounded load where the array and
/// the index hold up, and the ordinary computed read where they do not.
///
/// # What is compared, and why each compare is here rather than proven
///
/// ```text
///   0 <= i < count ── no ──────────────┐
///          │ yes                       │
///   ElementLoad ── is it a hole? ── yes ┼─▶ undefined
///          │ no                        │
///          └────────▶ join(value) ◀── GetIndexed
/// ```
///
/// **The range.** `count` is what the RUN holds; the loop's own test uses the
/// `length` PROPERTY, and for anything that is not an array those disagree —
/// `elements_count` answers 0 for a string while `"abcd".length` is 4. Without
/// this compare a string walk would ask for a load past the end, and
/// `ElementLoad` traps rather than answering absent. With it, a string takes the
/// slow arm on every pass and is exactly as correct as before.
///
/// It also covers the index the loop's generic bound could let past `i32`: a
/// double above 2^31 wraps negative through `to_int32`, so the lower half of the
/// comparison is not decoration.
///
/// **The hole.** `ElementLoad` hands back the raw word, and a hole is a distinct
/// singleton that every read is supposed to put through `array::visible` — sixty
/// sites, and what leaks if one forgets is a word that corresponds to no
/// JavaScript value. Reading one answers `undefined`, so this converts rather
/// than falling back.
///
/// # What is NOT compared, because it does not need to be
///
/// Whether the receiver is an array. It is not asked: a non-array has a run of
/// zero, so the range compare sends it down the slow arm, and one compare
/// answers what a type test would have.
pub(super) fn checked_element_read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
    key: ValueId,
    position: ValueId,
    base: ValueId,
    count: ValueId,
) -> EmitResult<ValueId> {
    let index = builder.to_int32(position)?;
    let zero = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I32,
        bits: ScalarBits(0),
    });
    let zero = builder.use_const(zero);
    let not_negative = builder.compare(CmpOp::Ge, index, zero)?;

    let below_block = builder.create_block();
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    // Two branches rather than one conjunction: `bitwise` is for proven
    // integers and refuses a `Bool` operand, which is machine rule 10 doing its
    // job — a boolean is not a number here. Both are predicted the same way in a
    // loop that walks forward.
    builder.branch(not_negative, (below_block, &[]), (slow, &[]))?;

    builder.switch_to(below_block);
    let below = builder.compare(CmpOp::Lt, index, count)?;
    builder.branch(below, (fast, &[]), (slow, &[]))?;

    builder.switch_to(fast);
    let held = builder.element_load(base, index, count)?;
    // Through `is_singleton`, which is the generic-only counterpart `compare`
    // points at: `compare` refuses two generic operands (rule 10), and this is
    // the one question the encoding answers about a generic value without
    // reading anything. The number still comes from the model.
    let is_hole = builder.is_singleton(held, ctx.model.hole())?;
    let absent = builder.create_block();
    let present = builder.create_block();
    builder.branch(is_hole, (absent, &[]), (present, &[]))?;

    builder.switch_to(absent);
    let undefined = super::expr::undefined(builder, ctx);
    builder.jump(join, &[undefined])?;

    builder.switch_to(present);
    builder.jump(join, &[held])?;

    builder.switch_to(slow);
    let answered = super::expr::call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}

/// Reads the run once for a loop this recognises, and installs it for the body.
///
/// Answers what was installed BEFORE, so the caller restores it after the body.
/// Nested loops need that for the reason `Ctx::prove_element_read` states: an
/// inner walk that simply forgot would leave the outer body reading the inner
/// array's storage with the inner array's bound.
///
/// # Why two crossings here rather than one
///
/// [`RuntimeOp::ElementsBase`] and [`RuntimeOp::ElementsCount`] are separate
/// entry points because one `extern "C"` call returns one value, and the two
/// answer different things — an ADDRESS and a BOUND. They are paid once per
/// loop against one crossing per element, and the recogniser only admits a body
/// that cannot call anything, so it does not admit a loop entered for two
/// elements and abandoned.
#[allow(clippy::type_complexity)]
pub(super) fn hoist_walk(
    builder: &mut FuncBuilder,
    scope: &mut super::Scope,
    ctx: &mut Ctx,
    init: Option<&ForInit>,
    test: Option<&Expr>,
    update: Option<&Expr>,
    body: &Stmt,
) -> EmitResult<(Option<(Name, Name)>, Option<(ValueId, ValueId)>)> {
    let length_key = ctx.names.intern("length");
    let Some((array, index)) = array_walk(init, test, update, body, length_key) else {
        // Nothing installed, and the previous pair still cleared: a loop this
        // does not recognise must not inherit an enclosing loop's run, or its
        // reads would be bounded by another array.
        return Ok(ctx.check_element_read(None, None));
    };

    let subject = super::expr::emit_expr(
        builder,
        scope,
        ctx,
        &Expr {
            kind: ExprKind::Ident(array),
            at: body.at,
        },
    )?;
    let base = super::expr::call(builder, ctx, RuntimeOp::ElementsBase, &[subject])?[0];
    let count = super::expr::call(builder, ctx, RuntimeOp::ElementsCount, &[subject])?[0];
    Ok(ctx.check_element_read(Some((array, index)), Some((base, count))))
}
