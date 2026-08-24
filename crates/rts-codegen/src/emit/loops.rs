//! Loops, and the machinery everything that merges more than two paths needs.
//!
//! # What `switch.rs` and `foreach.rs` take from here
//!
//! A frame so `break` can reach an exit, a block parameter per name the
//! statement assigns, and the truncation to a shared prefix that makes a
//! position mean the same binding in two snapshots. Those three were private
//! and are now `pub(super)`, which is the cost of the split — and the split
//! happened because this file passed the thousand-line ceiling rule 8 sets,
//! not because the mechanism stopped being one.
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
use crate::syntax::{AssignTarget, Expr, ExprKind, ForInit, Stmt, StmtKind};

/// A loop being emitted, for `break` and `continue` to reach.
pub struct Frame {
    /// The label it was written with, if any.
    ///
    /// `break outer` names a frame rather than taking the innermost one, so the
    /// label is recorded where the target is. Two frames sharing a label is an
    /// early error and therefore not this module's to reject.
    pub label: Option<Name>,
    /// Where `continue` goes.
    ///
    /// Not always the header: in a `for`, `continue` runs the update first, and
    /// in a `do`/`while` it jumps to the condition rather than to the top. So
    /// the block is recorded rather than derived.
    ///
    /// `None` for a labelled statement that is not a loop. `outer: { … }` can
    /// be broken out of and cannot be continued, and a `continue` written
    /// inside one belongs to the nearest enclosing **loop** — which is why the
    /// search skips such a frame rather than stopping at it.
    pub continue_to: Option<BlockId>,
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
    /// The frame a jump reaches.
    ///
    /// Unlabelled, it is the innermost frame that can take the jump — which for
    /// `continue` skips a labelled block, because one cannot be continued and
    /// the `continue` belongs to the loop around it.
    ///
    /// Labelled, it is the frame carrying that label, wherever it sits.
    pub fn target(&self, label: Option<Name>, breaking: bool) -> Option<&Frame> {
        self.at(label, breaking).map(|(_, frame)| frame)
    }

    /// The same, with HOW MANY frames enclose it.
    ///
    /// Read by a jump to decide which `finally` bodies are on its way out: one
    /// entered inside the loop being left runs, one wrapped around that loop
    /// does not. See [`super::Ctx::finally_jumps`].
    pub(super) fn at(&self, label: Option<Name>, breaking: bool) -> Option<(usize, &Frame)> {
        self.frames
            .iter()
            .enumerate()
            .rev()
            .find(|(_, frame)| match label {
                Some(wanted) => frame.label == Some(wanted),
                None => breaking || frame.continue_to.is_some(),
            })
    }

    /// How many loops are enclosing right now.
    pub(super) fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Runs `body` with a frame pushed.
    pub(super) fn inside<T>(&mut self, frame: Frame, body: impl FnOnce(&mut Self) -> T) -> T {
        self.frames.push(frame);
        let result = body(self);
        self.frames.pop();
        result
    }
}

/// The arguments a jump into a merged block carries.
pub(super) fn merged_args(snapshot: &[Binding], merged: &[usize]) -> Vec<ValueId> {
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
    label: Option<Name>,
) -> EmitResult<bool> {
    let header = builder.create_block();
    // The CONDITION assigns too, and leaving it out is not a corner:
    // `while ((n -= 1) > 0)` writes `n` in the header and nowhere else, so with
    // no parameter for it the header re-entered with the value from BEFORE the
    // loop and the test never changed — a hang, not a wrong answer.
    let (merged, depth) = plan_including(scope, body, &assigned_in_expr_names(condition));
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
    // A `while` head declares nothing, so nothing is copied in or out — but a
    // `let` the BODY declares is still a new binding on every pass, and a
    // closure made in one must not see the next one's value. Same mechanism,
    // one of its two sets empty.
    let (copied, resident) = per_iteration_names(scope, None, body);
    let outer_environment = open_iteration(builder, scope, ctx, &copied, &resident)?;
    let frame = Frame {
        label,
        continue_to: Some(header),
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| emit_stmt(builder, scope, ctx, loops, body))?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(header, &merged_args(&leaving, &merged))?;
    }
    // Nothing is emitted here, which is why it is safe after the jump and safe
    // for a `continue` to have skipped it: with no head names the close is a
    // scope pop and no instruction at all.
    close_iteration(builder, scope, ctx, &copied, &resident, outer_environment)?;

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
    label: Option<Name>,
) -> EmitResult<bool> {
    let top = builder.create_block();
    let test = builder.create_block();
    let exit = builder.create_block();
    // Including what the CONDITION assigns, for the reason `emit_while` states:
    // `do { … } while ((n -= 1) > 0)` writes `n` in the test block alone, and a
    // name with no parameter re-enters the loop holding what it held before it.
    let (merged, depth) = plan_including(scope, body, &assigned_in_expr_names(condition));
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
        label,
        continue_to: Some(test),
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    // Same as `while`: no head to copy, but a `let` the body declares is a new
    // binding on every pass.
    let (copied, resident) = per_iteration_names(scope, None, body);
    let outer_environment = open_iteration(builder, scope, ctx, &copied, &resident)?;
    let terminated = loops.inside(frame, |loops| emit_stmt(builder, scope, ctx, loops, body))?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(test, &merged_args(&leaving, &merged))?;
    }
    close_iteration(builder, scope, ctx, &copied, &resident, outer_environment)?;

    builder.switch_to(test);
    settle(scope, &entering, &merged, &at_test_params);
    let cond = super::expr::emit_condition(builder, scope, ctx, condition)?;
    // AFTER the condition, not before it. `do { … } while ((n -= 1) > 0)`
    // writes `n` while the test is being emitted, and a snapshot taken first
    // hands the back edge the value from before the write — so every pass
    // re-entered with the same number and the loop never ended. `emit_while`
    // already takes its snapshot in this order; this one did not, and the two
    // differing was the whole of the bug.
    let at_test = scope.snapshot();
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
    label: Option<Name>,
) -> EmitResult<bool> {
    // The header owns a scope: `for (let i = …)` introduces `i` outside the
    // body and it does not survive the loop.
    scope.enter();
    let result = emit_for_inner(builder, scope, ctx, loops, init, test, update, body, label);
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
    label: Option<Name>,
) -> EmitResult<bool> {
    match init {
        Some(ForInit::Declare { kind, bindings }) => {
            // `var i` in `for (var i = 0; …)` was already hoisted to the
            // function's own scope by `function::hoist_vars` — this loop
            // header does not own it the way `for (let i = …)` owns `i`, so
            // reaching this line WRITES the hoisted binding rather than
            // introducing a new one in the header's layer, which is what kept
            // `i` alive only until `scope.leave()` at the loop's own exit.
            let is_var = *kind == crate::syntax::BindingKind::Var;
            for binding in bindings {
                let Some(expr) = &binding.value else {
                    if is_var {
                        // `for (var i; …)` — nothing to write; the hoisted
                        // `undefined` (or whatever an earlier `var i` already
                        // set) stands.
                        continue;
                    }
                    let value = super::expr::undefined(builder, ctx);
                    super::destructure::declare(builder, scope, ctx, &binding.target, value, body.at)?;
                    continue;
                };
                let value = super::expr::emit_expr(builder, scope, ctx, expr)?;
                // `Pattern::Name` and a destructuring header both go through
                // one lowering now — `destructure::declare`/`assign` is the
                // fast path for the plain case too, since a name is the leaf
                // that introduces (or writes) itself.
                if is_var {
                    super::destructure::assign(builder, scope, ctx, &binding.target, value, body.at)?;
                } else {
                    super::destructure::declare(builder, scope, ctx, &binding.target, value, body.at)?;
                }
            }
        }
        Some(ForInit::Expr(expr)) => {
            super::expr::emit_expr(builder, scope, ctx, expr)?;
        }
        None => {}
    }

    let header = builder.create_block();
    // The update writes the same names the body does, so both are asked.
    // The body, the update, AND THE TEST. All three run once per pass and all
    // three can write, so a name any of them assigns needs a block parameter to
    // carry it across the back edge.
    //
    // The test was missing, and it is not an exotic place to write:
    // `for (let i = 0, c; c = s.charAt(i++); )` is what the base64 decoder every
    // obfuscator emits looks like, and it is ordinary JavaScript. Without a
    // parameter for `i`, every pass re-read the header's value — so `charAt`
    // answered the first character forever and the loop never ended. A HANG,
    // found by a generated program where the whole hand-written corpus had
    // missed it.
    let mut merged = assigned_positions(scope, body);
    if let Some(update) = update {
        merged.extend(assigned_in_expr_positions(scope, update));
    }
    if let Some(test) = test {
        merged.extend(assigned_in_expr_positions(scope, test));
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
    // Every pass gets its own copy of a `let` head name that something closes
    // over. Opened HERE rather than before the header, because the copy has to
    // read what this pass's test just saw — see [`open_iteration`].
    let (copied, resident) = per_iteration_names(scope, init, body);
    let outer_environment = open_iteration(builder, scope, ctx, &copied, &resident)?;
    // `continue` runs the update, so it targets a block of its own rather than
    // the header — jumping straight to the header would skip `i++`.
    let stepping = builder.create_block();
    let step_params = add_params(builder, stepping, &merged, &entering);
    let frame = Frame {
        label,
        continue_to: Some(stepping),
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| emit_stmt(builder, scope, ctx, loops, body))?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(stepping, &merged_args(&leaving, &merged))?;
    }

    builder.switch_to(stepping);
    settle(scope, &at_header, &merged, &step_params);
    // The pass's values go back out BEFORE the update runs, which is what makes
    // `i++` step the counter the body may have already moved — see
    // [`close_iteration`].
    close_iteration(builder, scope, ctx, &copied, &resident, outer_environment)?;
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
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    breaking: bool,
    label: Option<Name>,
) -> EmitResult<bool> {
    // Every `finally` entered inside the loop being left runs first, innermost
    // out — the order the language states, and the reason the bodies are
    // carried rather than routed to one block the way a `return` is: a jump's
    // destination is known HERE and not at the `try`.
    if let Some((index, _)) = loops.at(label, breaking) {
        let owed: Vec<Vec<Stmt>> = ctx
            .finally_jumps
            .iter()
            .filter(|(_, depth)| *depth > index)
            .map(|(body, _)| body.clone())
            .collect();
        for body in owed.into_iter().rev() {
            // The stack is TAKEN while a body is emitted: a `break` written
            // inside a `finally` belongs to a loop outside it, and leaving the
            // entry in place would make that body owe itself.
            let held = std::mem::take(&mut ctx.finally_jumps);
            scope.enter();
            let mut terminated = false;
            for statement in &body {
                if emit_stmt(builder, scope, ctx, loops, statement)? {
                    terminated = true;
                    break;
                }
            }
            scope.leave();
            ctx.finally_jumps = held;
            if terminated {
                // The `finally` completed abruptly, which REPLACES the jump
                // that was on its way out.
                return Ok(true);
            }
        }
    }
    let Some(frame) = loops.target(label, breaking) else {
        // Not a gap: `break` outside a loop, and a label naming nothing, are
        // both syntax errors, and this module is not the checker. Refused
        // rather than emitted because there is nothing to emit.
        return Err(EmitError::Unsupported {
            construct: match (breaking, label.is_some()) {
                (true, false) => "`break` outside a loop",
                (false, false) => "`continue` outside a loop",
                (true, true) => "`break` naming a label that is not here",
                (false, true) => "`continue` naming a label that is not a loop",
            },
        });
    };
    let here = scope.snapshot();
    // Only the prefix the target knows about. A binding declared inside the
    // body has no position in the target block's parameters, because the target
    // is outside the block that declared it.
    let visible = &here[..frame.depth];
    let target = if breaking {
        frame.break_to
    } else {
        // `target` already refused a frame with nothing to continue to, so an
        // unlabelled `continue` reached a loop and a labelled one reached the
        // loop it named.
        let Some(block) = frame.continue_to else {
            return Err(EmitError::Unsupported {
                construct: "`continue` naming a label that is not a loop",
            });
        };
        block
    };
    builder.jump(target, &merged_args(visible, &frame.merged))?;
    Ok(true)
}

/// Emits a labelled statement that is not a loop.
///
/// `outer: { … break outer; … }` leaves the block. It needs the same machinery
/// a loop's exit does — a block with a parameter per name the body assigns, and
/// a jump from every `break` — but none of the rest: there is no back edge, so
/// nothing carries values *into* it and there is no header.
pub fn emit_labelled_block(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    label: Name,
    body: &Stmt,
) -> EmitResult<bool> {
    let (merged, depth) = plan(scope, body);
    let entering = scope.snapshot();
    let exit = builder.create_block();
    let params = add_params(builder, exit, &merged, &entering);

    let frame = Frame {
        label: Some(label),
        // Nothing to continue to. A `continue` written inside belongs to the
        // loop around this, which is what `Loops::target` skipping this frame
        // means.
        continue_to: None,
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    let terminated = loops.inside(frame, |loops| emit_stmt(builder, scope, ctx, loops, body))?;

    if !terminated {
        let leaving = scope.snapshot();
        builder.jump(exit, &merged_args(&leaving[..depth], &merged))?;
    }

    builder.switch_to(exit);
    settle(scope, &entering, &merged, &params);
    // Control reaches the statement after it whether the body ran to the end or
    // broke out, so the label never terminates the block it sits in — unlike
    // the body, which may.
    Ok(false)
}

/// Decides which bindings a loop must carry, and how deep the environment is.
pub(super) fn plan(scope: &Scope, body: &Stmt) -> (Vec<usize>, usize) {
    (assigned_positions(scope, body), scope.snapshot().len())
}

/// The same, plus names the caller knows must be carried although nothing
/// assigns them.
///
/// A `switch` needs it and a loop does not: `case 0: let x = 5;` followed by
/// `case 1: use(x)` is ONE binding shared by two blocks, and a body that only
/// ever declares `x` assigns it nowhere the syntactic walk can see. Without the
/// name in the carried set the second clause reads the seed the switch wrote
/// before the tests rather than what the first clause put there.
pub(super) fn plan_including(scope: &Scope, body: &Stmt, also: &[Name]) -> (Vec<usize>, usize) {
    let mut names = Vec::new();
    assigned_in_stmt(body, &mut names);
    names.extend_from_slice(also);
    (positions_of(scope, &names), scope.snapshot().len())
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

/// What a pass's own environment holds, and which of it arrives from the last.
///
/// Two sets, because they are answers to two different questions.
///
/// **Carried** are the head's `let`/`const` names that something closes over.
/// They arrive with a value — the one this pass's test just saw — and they
/// leave with whatever the body made of them, which is what lets `i++` step a
/// counter the body already moved. `for (var i = …)` contributes none: it is
/// ONE binding for the whole loop, and that is the entire difference between
/// the two spellings.
///
/// **Resident** adds what the BODY declares and something closes over. Those
/// need no copy — a `let` inside a block is a new binding on every pass by
/// definition, so arriving empty is arriving correct — but they must be bound
/// at zero hops here, because [`super::binding::declare`] writes a captured
/// name into whatever environment is current and a read that resolved one hop
/// further would look in the function's for something written in the pass's.
/// That mismatch is not hypothetical; it is what this pair was rewritten to
/// fix.
///
/// Over-approximates in one direction on purpose, the way [`super::capture`]
/// does: a name declared in a NESTED loop's body counts for this loop too. The
/// cost is one object allocated per pass that nothing needed. The cost of the
/// other direction is two closures sharing a variable, which is a wrong
/// program.
///
/// Both are empty when the function has no environment, which is not a special
/// case but the same fact from the other side: a function that builds none
/// captures nothing, so `is_captured` is false for every name in it.
fn per_iteration_names(
    scope: &Scope,
    init: Option<&ForInit>,
    body: &Stmt,
) -> (Vec<Name>, Vec<Name>) {
    let mut carried = Vec::new();
    if let Some(ForInit::Declare { kind, bindings }) = init
        && *kind != crate::syntax::BindingKind::Var
    {
        for binding in bindings {
            binding.target.bound_names(&mut carried);
        }
        carried.retain(|name| scope.is_captured(*name));
    }

    // `var` is not in this set even when the body spells one, and does not need
    // to be: a `var` was hoisted to the function's scope, so reaching its line
    // is a WRITE to a binding that already exists — which resolves through the
    // chain to where it was hoisted, one hop further out, rather than through
    // the declaration path that would put it here.
    let mut declared = std::collections::BTreeSet::new();
    super::capture::declared_by_statement(body, &mut declared);
    let mut resident = carried.clone();
    resident.extend(
        declared
            .into_iter()
            .filter(|name| scope.is_captured(*name) && !carried.contains(name)),
    );
    (carried, resident)
}

/// Opens the environment this pass's closures will capture.
///
/// # Why the copy is in and out rather than a chain the loop carries
///
/// The language's `CreatePerIterationEnvironment` makes a new record, copies
/// the bindings into it, and runs the increment against the NEXT pass's record.
/// This makes a new record, copies in, runs the body, and copies back out
/// before the increment — and the two are indistinguishable to a program.
///
/// They are indistinguishable because nothing but these names lives in the
/// record: its `__rts_outer` is the same object on every pass, so a name
/// resolved through it reaches the same storage either way, and the only
/// observable difference between two records is which values the copies put in
/// them. Carrying the record itself across the back edge as a block parameter
/// would match the specification's wording more closely and would mean teaching
/// [`add_params`] about a binding that is not a name — a machine question asked
/// in the language layer, which rule 2 refuses.
///
/// Returns the environment that was in force. `None` when there was nothing to
/// do, which [`close_iteration`] reads the same way.
fn open_iteration(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    copied: &[Name],
    resident: &[Name],
) -> EmitResult<Option<ValueId>> {
    if resident.is_empty() {
        return Ok(None);
    }
    // Read BEFORE the layer exists, so these come from the environment the
    // header wrote — reading after would read the fresh object's own empty
    // slots, which is `undefined` dressed as a counter.
    let incoming = copied
        .iter()
        .map(|name| super::binding::read(builder, scope, ctx, *name))
        .collect::<EmitResult<Vec<_>>>()?;
    let previous = super::binding::push_environment(builder, scope, ctx, resident)?
        .expect("resident is non-empty, checked above");
    for (name, value) in copied.iter().zip(incoming) {
        super::binding::write(builder, scope, ctx, *name, value)?;
    }
    Ok(previous)
}

/// Closes it, leaving this pass's values where the update will find them.
///
/// The order is the semantics: read while the pass's record is still the one in
/// force, THEN leave, then write. Reversed, both halves would name the same
/// object and the copy would be a no-op that looks like a copy.
fn close_iteration(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    copied: &[Name],
    resident: &[Name],
    previous: Option<ValueId>,
) -> EmitResult<()> {
    if resident.is_empty() {
        return Ok(());
    }
    let outgoing = copied
        .iter()
        .map(|name| super::binding::read(builder, scope, ctx, *name))
        .collect::<EmitResult<Vec<_>>>()?;
    scope.leave_environment(previous);
    for (name, value) in copied.iter().zip(outgoing) {
        super::binding::write(builder, scope, ctx, *name, value)?;
    }
    Ok(())
}

/// Turns names into positions, dropping any the loop cannot see.
///
/// A name assigned in the body but declared in it too has no position out here,
/// and correctly so: it is a different binding on every pass and nothing
/// outside the body can refer to it.
fn positions_of(scope: &Scope, names: &[Name]) -> Vec<usize> {
    let mut positions: Vec<usize> = names
        .iter()
        // A **captured** name carries nothing across the back edge. It lives in
        // an environment object, so every pass reads and writes the same heap
        // slot and there is no second definition to merge — which is exactly
        // what `Binding::value` says when it refuses to answer for one.
        //
        // Without this filter an ordinary program crashed:
        //
        // ```js
        // function f() {
        //   let n = 0;
        //   function g() { return n; }   // captures n
        //   while (n < 3) { n = n + 1; } // and the loop assigns it
        // }
        // ```
        //
        // `assigned_positions` is syntactic — it reads the tree and does not
        // know where a name lives — so it offered the loop a position whose
        // binding has no value, and the panic said "which cannot happen".
        // It could: the two facts were established in different places and
        // nothing compared them until here.
        .filter(|name| !matches!(scope.lookup(**name), Some(Binding::InEnvironment { .. })))
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
pub(super) fn add_params(
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
pub(super) fn settle(scope: &mut Scope, base: &[Binding], merged: &[usize], params: &[ValueId]) {
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
pub(super) fn assigned_in_stmt(statement: &Stmt, into: &mut Vec<Name>) {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => assigned_in_expr(expr, into),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                assigned_in_expr(expr, into);
            }
        }
        // The names it introduces count as written, not only what its
        // initialisers assign: a `var` inside a protected region is visible
        // after it, and a handler that read the value from before the
        // declaration would read one the program never had.
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                binding.target.bound_names(into);
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
        StmtKind::ForEach { subject, body, .. } => {
            into.extend(assigned_in_expr_names(subject));
            assigned_in_stmt(body, into);
        }
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            // The discriminant too: `switch (x = 1)` assigns, and a name missing
            // from this list has no parameter at the exit — so the value after
            // the switch would be whatever reached it on one arbitrary path.
            into.extend(assigned_in_expr_names(discriminant));
            for clause in clauses {
                if let Some(test) = &clause.test {
                    into.extend(assigned_in_expr_names(test));
                }
                for statement in &clause.body {
                    assigned_in_stmt(statement, into);
                }
            }
        }
        // The remaining statements are refused by the emitter, so a name they
        // write cannot reach a loop this module built. When one stops being
        // refused it is added here, and the test below is what says so.
        _ => {}
    }
}


/// Collects the names an expression writes.
///
/// # Why the shape of the tree comes from `capture` and is not written here
///
/// It WAS written here, as a second full match over `ExprKind`, and it was
/// missing `New` — so a write that happened only inside a constructor's
/// arguments was invisible to this analysis. `while (…) { new F(y = i); }` gave
/// `y` no block parameter, the header restored it to its pre-loop value on every
/// pass, and every write to it was discarded. The program compiled and ran
/// wrong, which is the outcome this whole module exists to prevent.
///
/// [`super::capture::walk_expr`] already describes what is inside an expression,
/// exhaustively, and its own comment names this exact failure: *"Two copies of
/// this match is how a node comes to be walked by one analysis and silently
/// skipped by the other."* That warning was written for two traversals and there
/// were four. This is one of them removed.
///
/// A nested function is deliberately not descended into: a name it assigns is
/// captured, which is a cell rather than a block parameter, and the environment
/// is what carries it.
fn assigned_in_expr(expr: &Expr, into: &mut Vec<Name>) {
    match &expr.kind {
        ExprKind::Assign { target, .. } => match target {
            AssignTarget::Place(place) => {
                if let ExprKind::Ident(name) = &place.kind {
                    into.push(*name);
                }
            }
            // A destructuring assignment writes names too, and this arm did not
            // exist. Nothing noticed while `for (x of xs)` over an EXISTING
            // binding was refused; the day that was allowed, the desugaring
            // started emitting `x = ks[i]` as a pattern assignment, this
            // collector did not see `x`, the loop carried no position for it,
            // and the writes were thrown away at the back edge. `for (p of
            // ["a","b"]) {}` then left `p` at its old value — a WRONG ANSWER
            // where the refusal had been, which is the worse direction.
            AssignTarget::Pattern(pattern) => pattern.bound_names(into),
        },
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind {
                into.push(*name);
            }
        }
        _ => {}
    }
    super::capture::walk_expr(expr, &mut |child| {
        if let super::capture::Child::Expr(inner) = child {
            assigned_in_expr(inner, into);
        }
    });
}

/// The names an expression assigns, as names rather than positions.
///
/// [`assigned_in_expr_positions`] answers the same question against a scope;
/// this one is for a caller collecting into a list it will resolve later.
fn assigned_in_expr_names(expr: &Expr) -> Vec<Name> {
    let mut names = Vec::new();
    assigned_in_expr(expr, &mut names);
    names
}
