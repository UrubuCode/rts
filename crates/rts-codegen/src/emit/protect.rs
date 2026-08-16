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

/// Whether a `finally` body can finish by *not* falling off its end.
///
/// # Why this changes how the whole construct is emitted
///
/// A `finally` is normally a CLEANUP: one copy of the body reached from every
/// path that unwinds through it, ending by handing control back to whatever is
/// unwinding. That shape has no way out other than back into the unwind — which
/// is what makes it a cleanup and what `Terminator::CleanupDone` says
/// structurally.
///
/// `try { return "t" } finally { return "f" }` needs the opposite. The language
/// says an abrupt completion in the `finally` REPLACES the pending one: the
/// return happens, the unwind is abandoned, and `"f"` is the answer. A `return`
/// inside a cleanup copy is a terminator with no successor, and the machine's
/// verifier rejects exactly that — `CleanupDoesNotEnd`, because leaving a copy
/// through a path the unwind knows nothing about is a frame nobody finishes.
///
/// So a `finally` that can complete abruptly is not emitted as a cleanup at all.
/// It becomes a catch-all HANDLER, which is an ordinary block: a `return` in one
/// is a return, and re-raising when the body falls off its end is what puts the
/// pending throw back. See [`emit_try`].
///
/// Over-approximates on purpose, and the direction is the safe one: a `break`
/// belonging to a loop written INSIDE the `finally` counts here although it
/// never leaves the body. The cost is the handler shape for a body that did not
/// need it, and the handler shape is correct for both. Under-approximating is a
/// program the verifier refuses.
///
/// Does not descend into a nested function or class, for the reason
/// [`super::suspends`] gives about the same boundary: a `return` written inside
/// one leaves THAT function.
fn leaves_abruptly(body: &[Stmt]) -> bool {
    // A `yield` or an `await` counts, and the reason is one step downstream: a
    // suspending body is rewritten by `frame::resumable_form`, which turns each
    // `Suspend` into a RETURN with a resume label. So `finally { yield "fin" }`
    // is a terminator with no successor in the cleanup copy, exactly as a
    // written `return` is — the verifier says `CleanupDoesNotEnd` about both
    // and is right about both.
    //
    // Asked through [`super::suspends`] rather than by matching `yield` here,
    // because that module already answers "does this body park its own frame"
    // and answering it a second time is where the two would come to disagree.
    super::suspends::body_suspends(body) || body.iter().any(statement_leaves_abruptly)
}

fn statement_leaves_abruptly(statement: &Stmt) -> bool {
    use crate::syntax::StmtKind;
    match &statement.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break(_) | StmtKind::Continue(_) => {
            return true;
        }
        StmtKind::Function(_) | StmtKind::Class(_) => return false,
        _ => {}
    }
    let mut found = false;
    super::capture::walk_stmt(statement, &mut |child| {
        if let super::capture::StmtChild::Stmt(inner) = child
            && statement_leaves_abruptly(inner)
        {
            found = true;
        }
    });
    if found {
        return true;
    }
    // `walk_stmt` hands a `catch` over as its own child rather than as a
    // statement, so a `return` written in one would be missed by the arm above.
    let mut in_handler = false;
    super::capture::walk_stmt(statement, &mut |child| {
        if let super::capture::StmtChild::Catch(catch) = child
            && catch.body.iter().any(statement_leaves_abruptly)
        {
            in_handler = true;
        }
    });
    in_handler
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
    // A `finally` that can complete abruptly takes the handler shape instead —
    // see [`leaves_abruptly`]. Its block is created HERE, before any region is
    // opened, so it belongs to whatever encloses this `try`: a throw from
    // inside the `finally` then lands in the enclosing handler rather than in
    // this one, which would be the `finally` catching itself.
    let abrupt = finally.is_some_and(leaves_abruptly);
    let unwind_block = abrupt.then(|| builder.create_block());
    // Where a `return` written inside the protected span goes. Unprotected, so
    // returning FROM it does not re-enter this region's own cleanup — the
    // `finally` runs once, here, rather than once here and once on the way out.
    let returning = finally.map(|_| {
        let block = builder.create_unprotected_block();
        // The parameter exists at CREATION, not where the block is filled in.
        // A jump checks its argument count against the target's parameters, so
        // a parameter added later is an `ArgumentCount` refusal at every
        // `return` that already jumped — the same order `loops::add_params`
        // records for the same reason.
        let held = builder.add_block_param(block, rts_cranelift::repr::Repr::Tagged);
        (block, held)
    });
    let cleanup_block = finally
        .filter(|_| !abrupt)
        .map(|_| builder.create_unprotected_block());
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
        // A cleanup for the ordinary shape, a catch-all HANDLER for the abrupt
        // one. Same region, same nesting, different way of leaving it — which
        // is the whole difference the two shapes have.
        let handlers = unwind_block
            .map(|block| {
                vec![Handler {
                    tag: JS_THROW,
                    block,
                }]
            })
            .unwrap_or_default();
        builder.open_region(handlers, cleanup_block);
        // The same block a written `return` reaches, told to the region as
        // well — because one `return` in the protected span is never written
        // anywhere: `g.return(v)` injects it AT a parked `yield`, after this
        // emitter has stopped. Routing it to a second block would give
        // `try { yield 1 } finally { yield 99 }` two `finally` copies that
        // disagree about which one holds the pending value.
        if let Some((block, _)) = returning {
            builder.set_region_return(block);
        }
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
    // Pushed for the BODY and the handler alike: a `return` written in a
    // `catch` owes the `finally` exactly as one in the `try` does.
    if let Some((block, _)) = returning {
        ctx.finally_returns.push(block);
    }
    if let Some(body) = finally {
        ctx.finally_jumps.push((body.to_vec(), loops.depth()));
    }
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
        // A CAPTURED parameter needs its own environment, not just its own
        // lexical layer: a closure reaches a captured name through the
        // function's environment keyed by spelling, so `catch (e)` inside a
        // function that also has an `e` would otherwise write the outer slot
        // and the shadow would be invisible to every closure. Measured on
        // `fn-meta/claude-closure-from-try-finally.ts`, which read
        // `thrown_e:thrown_e` where every other runtime reads
        // `thrown_e:outer_e`.
        let layer = match &catch.binding {
            Some(crate::syntax::Pattern::Name(name)) if scope.is_captured(*name) => {
                super::binding::push_environment(builder, scope, ctx, &[*name])?
            }
            _ => None,
        };
        if let Some(pattern) = &catch.binding {
            bind_caught(builder, scope, ctx, pattern, thrown)?;
        }
        let handler_terminated = emit_block(builder, scope, ctx, loops, &catch.body)?;
        if let Some(previous) = layer {
            scope.leave_environment(previous);
        }
        scope.leave();
        if !handler_terminated {
            reaches_join = true;
            builder.jump(join, &[])?;
        }
    }

    if outer {
        builder.close_region();
    }

    // Popped once the protected span AND its handler are behind us: from here
    // on, a `return` in one of the `finally` copies below belongs to whatever
    // encloses this `try`, not to this one. Without the pop, the copy that runs
    // the `finally` would jump to itself.
    if returning.is_some() {
        ctx.finally_returns.pop();
    }
    if finally.is_some() {
        ctx.finally_jumps.pop();
    }

    // The copy a `return` inside the protected span reaches. It runs the
    // `finally` and then leaves — by RETURNING, or, when this `try` is itself
    // inside one, by handing the value to the next block out so that nested
    // `finally` blocks run from the inside out.
    if let (Some((block, held)), Some(finally)) = (returning, finally) {
        builder.switch_to(block);
        scope.restore(&before);
        scope.enter();
        let terminated = emit_block(builder, scope, ctx, loops, finally)?;
        scope.leave();
        // A `finally` that completes ABRUPTLY replaces the pending return, so
        // nothing is emitted after one that terminated — its own `return` has
        // already been routed by the same mechanism.
        if !terminated {
            match ctx.finally_returns.last().copied() {
                Some(outer) => builder.jump(outer, &[held])?,
                None => builder.ret(&[held]),
            }
        }
    }

    if let (Some(block), Some(finally)) = (unwind_block, finally) {
        // The abrupt shape's unwinding copy. An ordinary handler block, so a
        // `return` in it is a return and a `break` reaches the loop it names.
        //
        // Falling off the end re-raises what was caught, which is the language:
        // a `finally` that completes NORMALLY leaves the pending completion
        // alone, and here the pending completion is a throw. Completing
        // abruptly discards it, which is why nothing is emitted after a body
        // that terminated.
        let thrown = builder.add_block_param(block, rts_cranelift::repr::Repr::Tagged);
        builder.switch_to(block);
        scope.restore(&before);
        scope.enter();
        let terminated = emit_block(builder, scope, ctx, loops, finally)?;
        scope.leave();
        if !terminated {
            builder.throw(JS_THROW, thrown);
        }
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
    // A `try`, a `catch` and a `finally` body are each their own lexical scope,
    // so each has its own temporal dead zone. Armed here rather than at the six
    // call sites for the reason this function exists at all: they differ in
    // where control goes afterwards, never in what the body means. The caller
    // has already opened the layer this lands in and closes it after.
    let lexical = super::binding::lexical_names(body);
    scope.expect_lexical(&lexical);
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
