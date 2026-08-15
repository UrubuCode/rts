//! `for await (x of xs)` — stepping the async iterator protocol.
//!
//! # Why this is not `foreach.rs`'s expansion with an `await` bolted on
//!
//! `foreach.rs`'s `for-of` reduces to an indexed loop over an array the
//! runtime materialises **up front**, through [`crate::runtime::RuntimeOp::Iterate`]
//! — `rts-core::entry::iterate` drains `[Symbol.iterator]()` to
//! exhaustion before the loop body runs once. `for await` cannot reuse that:
//! stepping an async iterator means calling `.next()` and awaiting its
//! result, one pass at a time, interleaved with the body — collecting every
//! step first would run every `.next()` before the body sees the first
//! `value`, which is a different program.
//!
//! So this is a **second** desugaring rather than a variation on the first,
//! built entirely from constructs `emit` already knows how to lower —
//! member/computed access, a call, `await`, `if`/`break` — through the same
//! [`super::loops::emit_for`] header/body/exit machinery `foreach.rs` uses.
//! No new `RuntimeOp` and no new entry point: the async iterator protocol is
//! ordinary property lookup and an ordinary call, which the language layer
//! already lowers, and `await` already suspends the frame and re-raises a
//! rejection — see `emit/expr.rs`'s `ExprKind::Await` arm.
//!
//! ```text
//! for await (const x of xs)  ──▶  {
//!   let __rts_af_src = xs;
//!   let __rts_af_iter = __rts_af_src[Symbol.asyncIterator]();
//!   for (;;) {
//!     let __rts_af_step = await __rts_af_iter.next();
//!     if (__rts_af_step.done) break;
//!     let x = __rts_af_step.value;
//!     body
//!   }
//! }
//! ```
//!
//! # What is deliberately absent
//!
//! **The fallback to a wrapped sync iterator** — the specification steps
//! `[Symbol.iterator]()` and wraps it when an object has no
//! `[Symbol.asyncIterator]`. Not built: an object with neither answers
//! `undefined` for the computed read, and calling `undefined()` is an
//! ordinary `TypeError` through the same "not a function" path every other
//! uncallable callee takes — a visible failure, not a silently different
//! program, which is the same trade `foreach.rs`'s missing `IteratorClose`
//! makes.
//!
//! **`IteratorClose` past a `break`** — a `return` out of this loop, or a
//! `throw` through it, still does not call `.return()` on the iterator. A
//! `break` now does: the loop clears `__rts_af_iter` when the iterator reports
//! `done` and nothing else does, so a binding still holding one after the loop
//! IS an abandonment. `foreach.rs`'s `for-of` closes on exactly the same rule
//! and states the same remainder — the paths this misses leave without reaching
//! the statement after the loop at all, which needs a `try`/`finally`-shaped
//! region rather than a statement.
//!
//! The call is AWAITED, which the synchronous form has no equivalent of and is
//! what the specification's `AsyncIteratorClose` says: an async `return()` that
//! suspends must finish before whatever follows the loop runs.

use rts_cranelift::ir::FuncBuilder;

use super::{Ctx, EmitResult, Scope};
use super::loops::Loops;
use crate::names::Name;
use crate::syntax::{Expr, ExprKind, Pattern, Stmt};

/// Emits `for await (target of subject) body`.
///
/// Mirrors [`super::foreach::emit_for_each`]'s signature deliberately, so the
/// two call sites in `stmt.rs` — the plain form and the labelled one — read
/// as the same shape choosing a different sequence, which is what they are.
pub fn emit_for_await(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    target: &crate::syntax::ForEachTarget,
    subject: &Expr,
    body: &Stmt,
    label: Option<Name>,
) -> EmitResult<bool> {
    use crate::syntax::{
        AssignOp, AssignTarget, Binding as SyntaxBinding, BindingKind, ForEachTarget, StmtKind,
    };

    let pattern = match target {
        ForEachTarget::Declare {
            target: pattern, ..
        } => pattern,
        ForEachTarget::Assign(pattern) => pattern,
        ForEachTarget::Dispose { .. } => {
            return super::expr::gap("`await using` in a `for await`-head, which needs `Symbol.asyncDispose`");
        }
    };
    let fresh_binding = matches!(target, ForEachTarget::Declare { .. });
    if !fresh_binding {
        // `for await (existingName of xs)` — an existing binding written every
        // pass, exactly as `foreach.rs`'s `for (existingName of xs)` writes
        // one. That form's write goes through `AssignTarget::Pattern`, and
        // `loops.rs`'s `assigned_in_expr` (the walk that decides which names
        // need a block parameter to survive the loop) only recognises
        // `AssignTarget::Place` — so the write happens on every pass and is
        // read correctly INSIDE the loop, but the loop's own exit merges the
        // header's stale value over it, and the name outside the loop reads
        // whatever it held before the loop ran. Verified with a debug fixture:
        // `pass:0 pass:1 pass:2` print correctly from inside the body,
        // `final:2` prints `final:-1`.
        //
        // That is a pre-existing gap in `loops.rs`, shared with the
        // synchronous `for-of` (untested there because no fixture reads the
        // assigned name after the loop) — outside this change's file area to
        // fix, and refused here rather than shipped silently wrong.
        return super::expr::gap(
            "`for await (existingBinding of …)` — the assigned name does not \
             survive the loop; see `loops.rs::assigned_in_expr`, which only \
             recognises `AssignTarget::Place`",
        );
    }
    let at = statement_pos(subject, body);

    let name = |of: Name| Expr {
        kind: ExprKind::Ident(of),
        at,
    };
    let member = |object: Expr, property: Name| Expr {
        kind: ExprKind::Member {
            object: Box::new(object),
            property,
            optional: false,
        },
        at,
    };
    let index = |object: Expr, key: Expr| Expr {
        kind: ExprKind::Index {
            object: Box::new(object),
            index: Box::new(key),
            optional: false,
        },
        at,
    };
    let call = |callee: Expr| Expr {
        kind: ExprKind::Call {
            callee: Box::new(callee),
            arguments: Vec::new(),
            optional: false,
        },
        at,
    };

    let src = ctx.names.intern("__rts_af_src");
    let iter = ctx.names.intern("__rts_af_iter");
    let step = ctx.names.intern("__rts_af_step");
    let symbol_global = ctx.names.intern("Symbol");
    let async_iterator_name = ctx.names.intern("asyncIterator");
    let next_name = ctx.names.intern("next");
    let done_name = ctx.names.intern("done");
    let value_name = ctx.names.intern("value");

    // `__rts_af_src` and `__rts_af_iter` live outside the loop, exactly as
    // `foreach.rs`'s `keys` binding does — evaluated once, not re-derived
    // every pass.
    scope.enter();
    let evaluated_subject = super::expr::emit_expr(builder, scope, ctx, subject)?;
    super::binding::declare(builder, scope, ctx, src, evaluated_subject)?;

    // `__rts_af_src[Symbol.asyncIterator]()`, called with `__rts_af_src` as
    // the receiver — the `Index` callee shape `call.rs`'s
    // `callee_and_receiver` already binds correctly, the same path
    // `arr["push"](1)` takes.
    let async_iterator_key = member(name(symbol_global), async_iterator_name);
    let get_iterator = call(index(name(src), async_iterator_key));
    let iterator_value = super::expr::emit_expr(builder, scope, ctx, &get_iterator)?;
    super::binding::declare(builder, scope, ctx, iter, iterator_value)?;

    let step_call = Expr {
        kind: ExprKind::Await(Box::new(call(member(name(iter), next_name)))),
        at,
    };

    let init_step = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(step),
                value: Some(step_call),
                claim: None,
            }],
        },
        at,
    };

    // Exhaustion CLEARS the iterator, which is what makes the binding itself
    // the record of whether the loop abandoned one: only this path assigns it,
    // so anything else that leaves the loop leaves it holding the iterator.
    let exit_when_done = Stmt {
        kind: StmtKind::If {
            condition: member(name(step), done_name),
            then_branch: Box::new(Stmt {
                kind: StmtKind::Block(vec![
                    Stmt {
                        kind: StmtKind::Expr(Expr {
                            kind: ExprKind::Assign {
                                target: AssignTarget::Place(Box::new(name(iter))),
                                value: Box::new(Expr {
                                    kind: ExprKind::Literal(
                                        crate::syntax::Literal::Singleton(
                                            crate::values::Singleton::Undefined,
                                        ),
                                    ),
                                    at,
                                }),
                                op: AssignOp::Plain,
                            },
                            at,
                        }),
                        at,
                    },
                    Stmt {
                        kind: StmtKind::Break(None),
                        at,
                    },
                ]),
                at,
            }),
            else_branch: None,
        },
        at,
    };

    let element = member(name(step), value_name);
    let bind = if fresh_binding {
        Stmt {
            kind: StmtKind::Declare {
                kind: BindingKind::Let,
                bindings: vec![SyntaxBinding {
                    target: pattern.clone(),
                    value: Some(element),
                    claim: None,
                }],
            },
            at,
        }
    } else {
        Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Assign {
                    target: AssignTarget::Pattern(pattern.clone()),
                    value: Box::new(element),
                    op: AssignOp::Plain,
                },
                at,
            }),
            at,
        }
    };

    let inner = Stmt {
        kind: StmtKind::Block(vec![init_step, exit_when_done, bind, body.clone()]),
        at,
    };

    let result = super::loops::emit_for(builder, scope, ctx, loops, None, None, None, &inner, label);
    // `AsyncIteratorClose`, on the one exit that reaches here — see the module
    // doc for which exits do not. The rule itself is `foreach.rs`'s, called
    // rather than copied: the synchronous loop and this one closing an iterator
    // differently is exactly what one shared statement makes unrepresentable.
    if matches!(result, Ok(false)) {
        let guard = super::foreach::still_open(iter, at);
        let close = super::foreach::close_iterator_stmt(ctx, at, iter, guard, true);
        super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &close)?;
    }
    scope.leave();
    result
}

/// A source position for every node this expansion invents.
///
/// Neither `subject` nor `body` alone is right for every synthesised node —
/// `foreach.rs` uses the enclosing statement's; this takes `subject`'s, which
/// is the earliest position of the two and close enough for a diagnostic
/// pointing at generated code the program never wrote.
fn statement_pos(subject: &Expr, _body: &Stmt) -> rts_cranelift::Position {
    subject.at
}
