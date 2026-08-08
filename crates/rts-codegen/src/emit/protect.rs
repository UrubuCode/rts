//! `throw`, `try`, and the cleanup a `using` owes.
//!
//! # Why one module for three constructs
//!
//! They are one mechanism seen from three angles. A `try` declares a protected
//! region; a `throw` leaves through whatever region it is in; a `using` declares
//! a region whose cleanup is a disposal. Splitting them would put the region
//! tree's shape in three places that have to agree about it.
//!
//! # What the machine decides, and what is decided here
//!
//! Everything about *where* a throw lands is the machine's: the region tree, the
//! cleanup chain, the handler search. This layer says which spans are protected
//! and what a handler contains — which is the language part, and the only part.
//!
//! # The one tag, and why there is only one
//!
//! JavaScript has a single kind of throw. Anything can be thrown and every
//! `catch` catches everything, so a language that has one thing to say needs one
//! tag to say it with. The machine supports more because other languages need
//! more, and using one here is not leaving a feature unused — it is declining to
//! invent a distinction the language does not have.
//!
//! # The boundary that moved
//!
//! A `try` whose body contains a call USED to be refused by name. The machine
//! computes where a throw lands from the region tree of the function it is *in*,
//! which is complete for handlers in that function and silent about a caller's —
//! so a throw inside a callee ran past the `catch` and ended the program, and
//! compiling that would have been a `catch` that reads correctly and never runs.
//!
//! What moved it is not an unwinder. A throw leaves ONE frame — the runtime
//! records it and the machine returns instead of trapping — and every call site
//! asks whether the frame below left by throwing. Asking is `expr::call`'s
//! `check_for_throw`, and what it does when the answer is yes is re-raise, which
//! puts the value straight back into the region tree this module already builds.
//! So the handler search is unchanged; only the reach is.
//!
//! # Why `using` is not here yet, and what it is waiting on
//!
//! Not the cleanup. A cleanup is a piece rather than a block now, which is what
//! `finally` was waiting on and what `using` was waiting on with it — the region
//! and the disposal-on-every-exit are exactly the shape this module already
//! emits.
//!
//! It is waiting on the *disposal*. `using x = e` calls `x[Symbol.dispose]()`,
//! and this engine has no `Symbol`, so there is no way to name the method. A
//! region with no call in its cleanup would scope correctly and dispose
//! nothing, which reads as working — the one failure a resource construct must
//! not have. So it is refused by name, and the name says which half is missing.

use rts_cranelift::ir::FuncBuilder;
use rts_cranelift::unwind::{Handler, Tag};

use super::stmt::emit_stmt;
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{Catch, Expr, Stmt};

/// What a JavaScript `throw` is tagged with.
///
/// One value, named rather than written as `Tag(1)` at each of the three places
/// that mention it — a throw, a handler, and the cleanup plan all have to agree,
/// and three literals is three chances to disagree.
pub const JS_THROW: Tag = Tag(1);

/// Emits `throw e`.
///
/// Always terminates the block: nothing after a `throw` in the same statement
/// list is reachable, and the machine's verifier rejects a second terminator.
pub fn emit_throw(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    value: &Expr,
) -> EmitResult<bool> {
    let produced = super::expr::emit_expr(builder, scope, ctx, value)?;
    let payload = super::expr::as_value(builder, produced);
    builder.throw(JS_THROW, payload);
    Ok(true)
}

/// Emits `try { … } catch (e) { … } finally { … }`.
pub fn emit_try(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut super::loops::Loops,
    body: &[Stmt],
    catch: Option<&Catch>,
    finally: Option<&[Stmt]>,
) -> EmitResult<bool> {
    // Both outside every region, and for opposite reasons. A cleanup inside its
    // own region would run itself; the continuation inside it would run the
    // cleanup a second time on the way out.
    let cleanup_block = finally.map(|_| builder.create_unprotected_block());
    let join = builder.create_unprotected_block();

    let protected = builder.create_block();
    builder.jump(protected, &[])?;
    builder.switch_to(protected);

    // Two regions, and which encloses which is the semantics. `finally` runs
    // after `catch`, and also runs when the *handler* throws — so the handler
    // has to sit inside the cleanup's region and outside its own. One region
    // could not say that.
    //
    // Opened after switching, because opening puts the block being built into
    // the region, and every block anything nested creates until it closes. A
    // nested `if` inside a `try` does not have to know it is inside one.
    let outer = finally.is_some();
    if outer {
        builder.open_region(Vec::new(), cleanup_block);
    }

    // Created while the cleanup's region is open, so a throw from inside the
    // handler unwinds through the `finally` — which is the whole reason the two
    // regions are not one.
    let handler_block = catch.map(|_| builder.create_block());
    let handlers = handler_block
        .map(|block| {
            vec![Handler {
                tag: JS_THROW,
                block,
            }]
        })
        .unwrap_or_default();
    builder.open_region(handlers, None);

    let before = scope.snapshot();
    scope.enter();
    let body_terminated = emit_block(builder, scope, ctx, loops, body)?;
    scope.leave();
    builder.close_region();

    // The handler starts from the environment as it was *before* the body,
    // which is sound only because everything the body assigns lives in memory
    // by now — `capture::assigned_under_protection` put it there. What the
    // snapshot carries is the SSA values the body could not have changed.
    let mut reaches_join = !body_terminated;
    if !body_terminated {
        builder.jump(join, &[])?;
    }

    if let (Some(block), Some(catch)) = (handler_block, catch) {
        // The thrown value arrives as the block's first parameter, which is the
        // machine's discipline for it: a handler that had to find the value
        // somewhere else would be reading a side channel that outlives the
        // frame it belongs to.
        let thrown = builder.add_block_param(block, rts_cranelift::repr::Repr::Tagged);
        builder.switch_to(block);
        scope.restore(&before);
        // The binding belongs to the handler alone — `catch (e)` introduces `e`
        // for the handler body and nowhere else.
        scope.enter();
        if let Some(pattern) = &catch.binding {
            bind_caught(builder, scope, ctx, pattern, thrown)?;
        }
        let handler_terminated = emit_block(builder, scope, ctx, loops, &catch.body)?;
        scope.leave();
        if !handler_terminated {
            reaches_join = true;
            builder.jump(join, &[])?;
        }
    }

    if outer {
        builder.close_region();
    }

    if let (Some(block), Some(finally)) = (cleanup_block, finally) {
        // The unwinding copy. It ends by handing control back to whatever is
        // unwinding rather than by jumping anywhere: a cleanup has no way back,
        // because it is entered from every path that unwinds through it and
        // those paths have nothing in common to return to.
        builder.switch_to(block);
        scope.restore(&before);
        scope.enter();
        // The same statements the normal path gets, emitted where a
        // `CleanupDone` rather than a jump will end them.
        // The throw check is suppressed for the length of this body; see
        // `Ctx::in_cleanup`. Saved and restored rather than set and cleared,
        // because a `try` inside a `finally` would otherwise re-enable it half
        // way through the outer one.
        let outer = ctx.in_cleanup;
        ctx.in_cleanup = true;
        let terminated = emit_block(builder, scope, ctx, loops, finally)?;
        ctx.in_cleanup = outer;
        scope.leave();
        if !terminated {
            builder.cleanup_done();
        }
    }

    builder.switch_to(join);
    scope.restore(&before);

    if !reaches_join {
        // Every arm left. The join still had to exist, because the arms were
        // emitted before anyone could know that — so it is given a terminator
        // nothing reaches rather than left unterminated, and the caller is told
        // control does not continue.
        builder.trap(rts_cranelift::ir::TrapCode::Unreachable);
        return Ok(true);
    }

    // The normal-path copy, emitted from the tree a second time rather than
    // shared with the unwinding one. The unwinding copy cannot be jumped to and
    // back from — that is what `cleanup_done` makes structural — so one body
    // reached two ways is not available, and two emissions of one tree is what
    // "runs on every path out" costs.
    if let Some(finally) = finally {
        scope.enter();
        let terminated = emit_block(builder, scope, ctx, loops, finally)?;
        scope.leave();
        if terminated {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Emits the statements of a protected span, stopping at a terminator.
fn emit_block(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut super::loops::Loops,
    body: &[Stmt],
) -> EmitResult<bool> {
    for statement in body {
        if emit_stmt(builder, scope, ctx, loops, statement)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Binds what was caught.
fn bind_caught(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &crate::syntax::Pattern,
    thrown: rts_cranelift::ir::ValueId,
) -> EmitResult<()> {
    match pattern {
        crate::syntax::Pattern::Name(name) => {
            super::binding::declare(builder, scope, ctx, *name, thrown)
        }
        // A destructuring catch binding is the destructuring emitter's job, and
        // that does not exist yet. Named here rather than silently binding
        // nothing, which would make `catch ({ message })` a `message` that is
        // always `undefined`.
        _ => super::expr::gap("a destructuring `catch` binding").map(|_: bool| ()),
    }
}
