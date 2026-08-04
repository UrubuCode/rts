//! Loops, and the thing about them that `if` did not have to solve.
//!
//! # A loop header is a join whose second predecessor does not exist yet
//!
//! `if` merges two paths that are both finished by the time the join is built,
//! so the parameters can be decided by comparing what each arm produced. A loop
//! cannot do that:
//!
//! ```text
//!     ┌──────────────┐
//!     │ header       │◀────────┐   ← needs its parameters HERE
//!     │  cond?       │         │
//!     └───┬──────┬───┘         │
//!         │      │             │
//!       body   exit        back edge  ← which are only known HERE
//!         └─────────────────────┘
//! ```
//!
//! The header's parameters must exist before the body is emitted, because the
//! body reads the values they carry — and what the back edge passes is not known
//! until the body has been emitted.
//!
//! # Two ways out, and why this takes the second
//!
//! **Give every live local a parameter.** Correct, and it makes every loop pay
//! for every variable in scope whether or not the loop touches it. The machine
//! would have to prove them away afterwards, which is the same trap as the
//! stack-slot-per-local that `scope.rs` refuses.
//!
//! **Ask the tree which names the loop assigns.** A syntactic question with a
//! syntactic answer: walk the body, collect the targets of assignments and
//! `++`/`--`. Cheap, needs no emission, and it is exact for the case that
//! matters — a name the body never writes cannot differ between passes.
//!
//! It over-approximates in one direction only, which is the safe one: an
//! assignment inside a branch that never runs still counts, so a name gets a
//! parameter it did not need. It never *under*-approximates, because a write
//! that would need one is a write the walk sees.
//!
//! # `break` and `continue` merge through the same mechanism
//!
//! A `continue` is a back edge and a `break` is an extra predecessor of the exit
//! block, so both carry the same set of names. The exit block therefore takes
//! the same parameters as the header, and the loop's false path passes the
//! header's own.

use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};

use super::scope::Binding;
use super::stmt::emit_stmt;
use super::{Ctx, EmitError, EmitResult, Scope};
use crate::names::Name;
use crate::syntax::{
    AssignTarget, Expr, ExprKind, ForInit, Pattern, Property, PropertyKey, Spreadable, Stmt, StmtKind,
};

/// A loop being emitted, for `break` and `continue` to reach.
pub struct Frame {
    /// Where `continue` goes.
    ///
    /// Not always the header: in a `for`, `continue` runs the update first, and
    /// in a `do`/`while` it jumps to the condition rather than to the top. So
    /// the block is recorded rather than derived.
    pub continue_to: BlockId,
    /// Where `break` goes.
    pub break_to: BlockId,
    /// How many bindings were in scope when the loop started.
    ///
    /// A `break` inside a nested block has a longer environment than the loop
    /// header does, and only the shared prefix can be merged — the rest are
    /// bindings the exit block has no name for. Truncating to this length is
    /// what makes a position in one snapshot mean the same binding as the same
    /// position in another.
    pub depth: usize,
    /// Which of those positions the loop merges.
    pub merged: Vec<usize>,
}

/// The loops enclosing the statement being emitted, innermost last.
#[derive(Default)]
pub struct Loops {
    frames: Vec<Frame>,
}

impl Loops {
    /// The innermost loop, if there is one.
    pub fn innermost(&self) -> Option<&Frame> {
        self.frames.last()
    }

    /// Runs `body` with a frame pushed.
    fn inside<T>(&mut self, frame: Frame, body: impl FnOnce(&mut Self) -> T) -> T {
        self.frames.push(frame);
        let result = body(self);
        self.frames.pop();
        result
    }
}

/// The arguments a jump into a merged block carries.
fn merged_args(snapshot: &[Binding], merged: &[usize]) -> Vec<ValueId> {
    merged.iter().map(|&at| snapshot[at].value()).collect()
}

/// Emits `while`.
pub fn emit_while(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    condition: &Expr,
    body: &Stmt,
) -> EmitResult<bool> {
    let header = builder.create_block();
    let (merged, depth) = plan(scope, body);
    let entering = scope.snapshot();
    let carried = add_params(builder, header, &merged, &entering);

    // Into the header with whatever the names mean now. The header's parameters
    // are what they mean *inside* the loop, which is not the same thing on the
    // second pass — and that difference is the entire reason they exist.
    builder.jump(header, &merged_args(&entering, &merged))?;

    builder.switch_to(header);
    settle(scope, &entering, &merged, &carried);

    let cond = super::expr::emit_condition(builder, scope, ctx, condition)?;
    let inside = builder.create_block();
    let exit = builder.create_block();
    let params = add_params(builder, exit, &merged, &entering);

    let at_header = scope.snapshot();
    builder.branch(
        cond,
        (inside, &[]),
        (exit, &merged_args(&at_header, &merged)),
    )?;

    builder.switch_to(inside);
    let frame = Frame {
        continue_to: header,
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| {
        emit_stmt(builder, scope, ctx, loops, body)
    })?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(header, &merged_args(&leaving, &merged))?;
    }

    builder.switch_to(exit);
    settle(scope, &at_header, &merged, &params);
    // A `while` always has a path around it: the condition can be false on the
    // first pass. `while (true)` is not an exception here — nothing in this
    // module reads the condition's value, and deciding that a loop never exits
    // is a question for whatever proves things about values.
    Ok(false)
}

/// Emits `do`/`while`.
pub fn emit_do_while(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    body: &Stmt,
    condition: &Expr,
) -> EmitResult<bool> {
    let top = builder.create_block();
    let test = builder.create_block();
    let exit = builder.create_block();
    let (merged, depth) = plan(scope, body);
    let entering = scope.snapshot();
    let at_top = add_params(builder, top, &merged, &entering);
    let params = add_params(builder, exit, &merged, &entering);
    let at_test_params = add_params(builder, test, &merged, &entering);

    builder.jump(top, &merged_args(&entering, &merged))?;

    builder.switch_to(top);
    settle(scope, &entering, &merged, &at_top);

    // `continue` in a `do`/`while` reaches the CONDITION, not the top. The tree
    // says so where the node is declared, and getting it wrong produces a loop
    // that runs its body twice per pass for programs that use `continue`.
    let frame = Frame {
        continue_to: test,
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| {
        emit_stmt(builder, scope, ctx, loops, body)
    })?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(test, &merged_args(&leaving, &merged))?;
    }

    builder.switch_to(test);
    settle(scope, &entering, &merged, &at_test_params);
    let at_test = scope.snapshot();
    let cond = super::expr::emit_condition(builder, scope, ctx, condition)?;
    builder.branch(
        cond,
        (top, &merged_args(&at_test, &merged)),
        (exit, &merged_args(&at_test, &merged)),
    )?;

    builder.switch_to(exit);
    settle(scope, &at_test, &merged, &params);
    Ok(false)
}

/// Emits a three-part `for`.
pub fn emit_for(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    init: Option<&ForInit>,
    test: Option<&Expr>,
    update: Option<&Expr>,
    body: &Stmt,
) -> EmitResult<bool> {
    // The header owns a scope: `for (let i = …)` introduces `i` outside the
    // body and it does not survive the loop.
    scope.enter();
    let result = emit_for_inner(builder, scope, ctx, loops, init, test, update, body);
    scope.leave();
    result
}

#[allow(clippy::too_many_arguments)]
fn emit_for_inner(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    init: Option<&ForInit>,
    test: Option<&Expr>,
    update: Option<&Expr>,
    body: &Stmt,
) -> EmitResult<bool> {
    match init {
        Some(ForInit::Declare { bindings, .. }) => {
            for binding in bindings {
                let Pattern::Name(name) = &binding.target else {
                    return Err(EmitError::Unsupported {
                        construct: "a destructuring `for` header",
                    });
                };
                let value = match &binding.value {
                    Some(expr) => super::expr::emit_expr(builder, scope, ctx, expr)?,
                    None => super::expr::undefined(builder, ctx),
                };
                super::binding::declare(builder, scope, ctx, *name, value)?;
            }
        }
        Some(ForInit::Expr(expr)) => {
            super::expr::emit_expr(builder, scope, ctx, expr)?;
        }
        None => {}
    }

    let header = builder.create_block();
    // The update writes the same names the body does, so both are asked.
    let mut merged = assigned_positions(scope, body);
    if let Some(update) = update {
        merged.extend(assigned_in_expr_positions(scope, update));
    }
    merged.sort_unstable();
    merged.dedup();
    let depth = scope.snapshot().len();

    let entering = scope.snapshot();
    let carried = add_params(builder, header, &merged, &entering);
    builder.jump(header, &merged_args(&entering, &merged))?;
    builder.switch_to(header);
    settle(scope, &entering, &merged, &carried);

    let inside = builder.create_block();
    let exit = builder.create_block();
    let params = add_params(builder, exit, &merged, &entering);

    let at_header = scope.snapshot();
    match test {
        // An absent test is `true`, not "no branch": the exit block still needs
        // a predecessor, and `for (;;) break;` reaches it.
        Some(test) => {
            let cond = super::expr::emit_condition(builder, scope, ctx, test)?;
            builder.branch(
                cond,
                (inside, &[]),
                (exit, &merged_args(&at_header, &merged)),
            )?;
        }
        None => builder.jump(inside, &[])?,
    }

    builder.switch_to(inside);
    // `continue` runs the update, so it targets a block of its own rather than
    // the header — jumping straight to the header would skip `i++`.
    let stepping = builder.create_block();
    let step_params = add_params(builder, stepping, &merged, &entering);
    let frame = Frame {
        continue_to: stepping,
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| {
        emit_stmt(builder, scope, ctx, loops, body)
    })?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(stepping, &merged_args(&leaving, &merged))?;
    }

    builder.switch_to(stepping);
    settle(scope, &at_header, &merged, &step_params);
    if let Some(update) = update {
        super::expr::emit_expr(builder, scope, ctx, update)?;
    }
    let after_update = scope.snapshot();
    builder.jump(header, &merged_args(&after_update, &merged))?;

    builder.switch_to(exit);
    settle(scope, &at_header, &merged, &params);
    Ok(false)
}

/// Emits `break` or `continue`.
pub fn emit_jump_out(
    builder: &mut FuncBuilder,
    scope: &Scope,
    loops: &Loops,
    breaking: bool,
) -> EmitResult<bool> {
    let Some(frame) = loops.innermost() else {
        // Not a gap: `break` outside a loop is a syntax error, and this module
        // is not the checker. It is refused rather than emitted because there
        // is nothing to emit.
        return Err(EmitError::Unsupported {
            construct: if breaking {
                "`break` outside a loop"
            } else {
                "`continue` outside a loop"
            },
        });
    };
    let here = scope.snapshot();
    // Only the prefix the loop knows about. A binding declared inside the body
    // has no position in the target block's parameters, because the target is
    // outside the block that declared it.
    let visible = &here[..frame.depth];
    let target = if breaking {
        frame.break_to
    } else {
        frame.continue_to
    };
    builder.jump(target, &merged_args(visible, &frame.merged))?;
    Ok(true)
}

/// Decides which bindings a loop must carry, and how deep the environment is.
fn plan(scope: &Scope, body: &Stmt) -> (Vec<usize>, usize) {
    (assigned_positions(scope, body), scope.snapshot().len())
}

/// Where the names a statement assigns sit in the current environment.
fn assigned_positions(scope: &Scope, body: &Stmt) -> Vec<usize> {
    let mut names = Vec::new();
    assigned_in_stmt(body, &mut names);
    positions_of(scope, &names)
}

/// The same, for the `for` header's update expression.
fn assigned_in_expr_positions(scope: &Scope, expr: &Expr) -> Vec<usize> {
    let mut names = Vec::new();
    assigned_in_expr(expr, &mut names);
    positions_of(scope, &names)
}

/// Turns names into positions, dropping any the loop cannot see.
///
/// A name assigned in the body but declared in it too has no position out here,
/// and correctly so: it is a different binding on every pass and nothing
/// outside the body can refer to it.
fn positions_of(scope: &Scope, names: &[Name]) -> Vec<usize> {
    let mut positions: Vec<usize> = names
        .iter()
        .filter_map(|name| scope.position_of(*name))
        .collect();
    positions.sort_unstable();
    positions.dedup();
    positions
}

/// Gives a block one parameter per merged binding.
///
/// Separate from [`settle`], and it has to be: a jump checks its argument count
/// against the target's parameters, so every parameter must exist before
/// anything jumps there. The first version of this added them while switching
/// to the header — after the entry jump — and every loop failed with
/// `ArgumentCount { expected: 0, found: 1 }`.
fn add_params(
    builder: &mut FuncBuilder,
    block: BlockId,
    merged: &[usize],
    incoming: &[Binding],
) -> Vec<ValueId> {
    merged
        .iter()
        .map(|&position| {
            // The representation of what ARRIVES, not `Tagged`. A parameter
            // declared generic would widen every proven value passed to it —
            // the builder inserts that silently, which is correct and is
            // exactly how a loop lost everything the type pass proved about its
            // counter.
            //
            // Sound because a name the analysis proved numeric is numeric on
            // every path into this block, so every predecessor passes the same
            // representation. A name it did not prove is tagged at every store,
            // so those agree too.
            let repr = builder.repr_of(incoming[position].value());
            builder.add_block_param(block, repr)
        })
        .collect()
}

/// Points the environment at parameters that already exist.
fn settle(scope: &mut Scope, base: &[Binding], merged: &[usize], params: &[ValueId]) {
    let mut snapshot = scope.snapshot();
    snapshot[..base.len()].copy_from_slice(base);
    for (&position, &param) in merged.iter().zip(params) {
        snapshot[position] = Binding::Value(param);
    }
    scope.restore(&snapshot);
}

/// Collects the names a statement writes.
///
/// Deliberately syntactic and deliberately over-approximate: an assignment in a
/// branch that never runs still counts. The direction of the error is the safe
/// one — a name gets a parameter it did not need, rather than needing one it did
/// not get.
fn assigned_in_stmt(statement: &Stmt, into: &mut Vec<Name>) {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => assigned_in_expr(expr, into),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                assigned_in_expr(expr, into);
            }
        }
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Some(expr) = &binding.value {
                    assigned_in_expr(expr, into);
                }
            }
        }
        StmtKind::Block(body) => body.iter().for_each(|inner| assigned_in_stmt(inner, into)),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            assigned_in_expr(condition, into);
            assigned_in_stmt(then_branch, into);
            if let Some(otherwise) = else_branch {
                assigned_in_stmt(otherwise, into);
            }
        }
        StmtKind::While { condition, body } => {
            assigned_in_expr(condition, into);
            assigned_in_stmt(body, into);
        }
        StmtKind::DoWhile { body, condition } => {
            assigned_in_stmt(body, into);
            assigned_in_expr(condition, into);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            match init {
                Some(ForInit::Expr(expr)) => assigned_in_expr(expr, into),
                Some(ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
                        if let Some(expr) = &binding.value {
                            assigned_in_expr(expr, into);
                        }
                    }
                }
                None => {}
            }
            if let Some(test) = test {
                assigned_in_expr(test, into);
            }
            if let Some(update) = update {
                assigned_in_expr(update, into);
            }
            assigned_in_stmt(body, into);
        }
        StmtKind::Labelled { body, .. } => assigned_in_stmt(body, into),
        // The remaining statements are refused by the emitter, so a name they
        // write cannot reach a loop this module built. When one stops being
        // refused it is added here, and the test below is what says so.
        _ => {}
    }
}

/// Collects the names an expression writes.
fn assigned_in_expr(expr: &Expr, into: &mut Vec<Name>) {
    match &expr.kind {
        ExprKind::Assign { target, value, .. } => {
            if let AssignTarget::Place(place) = target
                && let ExprKind::Ident(name) = &place.kind
            {
                into.push(*name);
            }
            assigned_in_expr(value, into);
        }
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind {
                into.push(*name);
            }
        }
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            assigned_in_expr(left, into);
            assigned_in_expr(right, into);
        }
        ExprKind::Unary { operand, .. } => assigned_in_expr(operand, into),
        ExprKind::Sequence { operands } => {
            operands.iter().for_each(|one| assigned_in_expr(one, into))
        }
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            assigned_in_expr(condition, into);
            assigned_in_expr(then_branch, into);
            assigned_in_expr(else_branch, into);
        }
        ExprKind::Call { callee, arguments, .. } => {
            assigned_in_expr(callee, into);
            for argument in arguments {
                match argument {
                    Spreadable::Single(expr) | Spreadable::Spread(expr) => {
                        assigned_in_expr(expr, into)
                    }
                }
            }
        }
        ExprKind::Member { object, .. } => assigned_in_expr(object, into),
        ExprKind::Index { object, index, .. } => {
            assigned_in_expr(object, into);
            assigned_in_expr(index, into);
        }
        ExprKind::Array { elements } => {
            for element in elements.iter().flatten() {
                match element {
                    Spreadable::Single(expr) | Spreadable::Spread(expr) => {
                        assigned_in_expr(expr, into)
                    }
                }
            }
        }
        ExprKind::Object { properties } => {
            for property in properties {
                match property {
                    Property::Value { key, value, .. } => {
                        if let PropertyKey::Computed(key) = key {
                            assigned_in_expr(key, into);
                        }
                        assigned_in_expr(value, into);
                    }
                    Property::Spread(expr) | Property::Prototype(expr) => {
                        assigned_in_expr(expr, into)
                    }
                    _ => {}
                }
            }
        }
        // The literal pieces hold no expressions; only what is substituted
        // between them can write anything.
        ExprKind::Template { expressions, .. } => {
            expressions.iter().for_each(|expr| assigned_in_expr(expr, into))
        }
        ExprKind::Await(inner) | ExprKind::Chain(inner) => assigned_in_expr(inner, into),
        // A function body is a different frame. A name it assigns is captured,
        // which is a cell rather than a block parameter — and the emitter
        // refuses closures, so nothing reaches here that would need it.
        _ => {}
    }
}
