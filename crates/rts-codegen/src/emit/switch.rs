//! `switch` — the one construct that merges more paths than it has arms.
//!
//! Its own file because `loops.rs` reached the thousand-line ceiling rule 8
//! sets, and this is what had been added to it. It still uses that module's
//! machinery — a frame so `break` can reach the exit, and the block parameters
//! for names the statement assigns — because a switch needs exactly what a
//! loop's exit needs and nothing else in this crate does.

use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};

use super::loops::{Frame, Loops, add_params, merged_args, settle};
use super::stmt::emit_stmt;
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{Expr, Stmt};

/// Emits `switch`.
///
/// # Why this lives beside the loops
///
/// A switch is not a loop and it needs everything a loop's exit needs: a block
/// with a parameter per name the statement assigns, a frame so `break` can
/// reach it, and the truncation to a shared prefix that makes a position mean
/// the same binding in two snapshots. Putting it elsewhere would mean exporting
/// four private helpers to a module that uses nothing else here.
///
/// # The shape, and why every body block takes parameters
///
/// ```text
///   test₀ ──match──▶ body₀ ─fall─▶ body₁ ─fall─▶ … ─▶ exit
///     │                 ▲             ▲               ▲
///    miss               └── test₁ ────┘               │
///     ▼                                            break
///   test₁ … ──all missed──▶ default, or exit
/// ```
///
/// `body₁` has two predecessors that are nowhere near each other: the test that
/// matched it, and `body₀` falling through. Their environments differ, so the
/// body needs block parameters — the same answer `if` reaches by comparing two
/// snapshots, except that here there is no pair to compare. Every body gets the
/// same parameter list, over the names the whole statement assigns, and each
/// predecessor passes its own values.
///
/// # Why the bodies are created before anything branches
///
/// A jump checks its argument count against the target's parameters, so every
/// parameter has to exist before the first branch. That is the same ordering
/// `add_params` records for loops, and getting it wrong there produced
/// `ArgumentCount { expected: 0, found: 1 }` on every loop.
///
/// # `default` is matched last and runs where it sits
///
/// It is not a fallback appended to the end. `switch (9) { default: a(); case
/// 1: b(); }` runs both, because control enters at `default` and falls through
/// — so the clause keeps its position among the bodies and only the *test*
/// chain treats it differently, by leaving it out and jumping there when
/// nothing matched.
pub fn emit_switch(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    statement: &Stmt,
    discriminant: &Expr,
    clauses: &[crate::syntax::SwitchClause],
) -> EmitResult<bool> {
    // The discriminant is evaluated in the ENCLOSING scope, before the block the
    // clauses share exists — `switch (x) { case 0: let x = 1; }` reads the outer
    // `x`, not the dead zone of the inner one.
    let subject = super::expr::emit_expr(builder, scope, ctx, discriminant)?;

    // ONE block scope for the whole statement, not one per clause. The clauses
    // are not nested scopes: `case 0: let n = 1;` declares a binding `case 1:`
    // can read and assign, which is what fall-through means — and emitting each
    // clause inside `enter`/`leave` of its own made that name `Unbound` at
    // compile time.
    scope.enter();
    // Seeded before anything branches, because a block PARAMETER is what carries
    // a binding from one clause into the next and a parameter list has to exist
    // before the first jump. A name declared partway through clause zero would
    // sit past `depth` and be truncated out of every argument list.
    // A function declared in ANY clause is bound before the statement runs, over
    // the whole switch block and not the clause it sits in — `case 0: return
    // f(); case 1: function f() {}` calls it, which is what a block hoisting its
    // declarations means and what the per-clause scope made impossible.
    for clause in clauses {
        super::function::hoist(builder, scope, ctx, &clause.body)?;
    }
    let mut lexical = Vec::new();
    for clause in clauses {
        lexical.extend(super::binding::lexical_names(&clause.body));
    }
    let seed = super::expr::undefined(builder, ctx);
    for name in &lexical {
        super::binding::declare(builder, scope, ctx, *name, seed)?;
    }
    // And the dead zone re-armed on top of the seed: the seed is bookkeeping,
    // not a declaration, so a read before the clause that declares the name is
    // still the `ReferenceError` the language gives it. `binding::declare`
    // clears the pending entry, which is why this comes after the loop.
    scope.expect_lexical(&lexical);

    let (merged, depth) = super::loops::plan_including(scope, statement, &lexical);
    let entering = scope.snapshot();
    let exit = builder.create_block();
    let exit_params = add_params(builder, exit, &merged, &entering);

    let bodies: Vec<BlockId> = clauses.iter().map(|_| builder.create_block()).collect();
    let body_params: Vec<Vec<ValueId>> = bodies
        .iter()
        .map(|&block| add_params(builder, block, &merged, &entering))
        .collect();

    // The test chain. Each comparison is `===`, so `case "1"` does not match
    // `1` and `case NaN` matches nothing — including `NaN`.
    let mut fell_through = None;
    for (position, clause) in clauses.iter().enumerate() {
        let Some(test) = &clause.test else {
            continue;
        };
        if let Some(previous) = fell_through.take() {
            builder.switch_to(previous);
        }
        let candidate = super::expr::emit_expr(builder, scope, ctx, test)?;
        // A PROVEN boolean, which is what a branch takes — so this is the one
        // comparison in the emitter that does not widen it back into a value
        // afterwards. Whether it costs a call or a single instruction is
        // `strict_equals_proof`'s question and not this file's: a switch over
        // numbers should not have to know that `===` on two doubles is IEEE
        // equality, and it is the second place that knew it that made this a
        // call at every label.
        let matched = super::expr::strict_equals_proof(builder, ctx, subject, candidate)?;
        let next = builder.create_block();
        let here = scope.snapshot();
        builder.branch(
            matched,
            (bodies[position], &merged_args(&here[..depth], &merged)),
            (next, &[]),
        )?;
        fell_through = Some(next);
    }

    // Nothing matched. `default` where it sits, and past the whole statement
    // where there is none.
    if let Some(last) = fell_through.take() {
        builder.switch_to(last);
    }
    let unmatched = scope.snapshot();
    let target = clauses
        .iter()
        .position(|clause| clause.test.is_none())
        .map_or(exit, |at| bodies[at]);
    builder.jump(target, &merged_args(&unmatched[..depth], &merged))?;

    // The bodies, in source order, each falling into the next.
    let frame = Frame {
        label: None,
        // A switch is not a loop: `continue` inside one belongs to whatever
        // loop encloses it, which is what a `None` here arranges.
        continue_to: None,
        break_to: exit,
        depth,
        merged: merged.clone(),
    };
    loops.inside(frame, |loops| -> EmitResult<()> {
        for (position, clause) in clauses.iter().enumerate() {
            builder.switch_to(bodies[position]);
            settle(scope, &entering, &merged, &body_params[position]);
            let mut terminated = false;
            for inner in &clause.body {
                if emit_stmt(builder, scope, ctx, loops, inner)? {
                    terminated = true;
                    break;
                }
            }
            if !terminated {
                let leaving = scope.snapshot();
                let next = bodies.get(position + 1).copied().unwrap_or(exit);
                builder.jump(next, &merged_args(&leaving[..depth], &merged))?;
            }
        }
        Ok(())
    })?;

    builder.switch_to(exit);
    settle(scope, &entering, &merged, &exit_params);
    // After `settle`, which reads positions the block scope's own bindings are
    // counted in: leaving first would renumber the snapshot the exit parameters
    // were built against.
    scope.leave();
    // Control reaches whatever follows: a switch with no matching clause and no
    // `default` runs nothing at all.
    Ok(false)
}
