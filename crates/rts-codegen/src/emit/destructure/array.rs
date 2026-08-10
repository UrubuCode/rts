//! The array shape of a [`super::Pattern`]: stepping an iterator.
//!
//! Split out of `destructure/mod.rs` — over the codegen file-size ceiling
//! together — once this file grew from "index a materialised array" to the
//! full stepped protocol: getting the iterator, calling `next()` a bounded
//! number of times, gathering a rest, and closing on early abandonment. The
//! object shape stayed behind because it never grew past reading properties.

use rts_cranelift::fault::Position;
use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::super::loops::Loops;
use super::super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    ArrayPattern, BinaryOp, Binding as SyntaxBinding, BindingKind, Expr, ExprKind, Literal,
    Pattern, Spreadable, Stmt, StmtKind, UnaryOp,
};
use crate::values::Singleton;

use super::{ident, member_expr, place, plain_assign_stmt, Role};

/// `[a, b = 1, ...rest] = source` / `let [a, b = 1, ...rest] = source`.
///
/// Gets ONE iterator — [`get_pattern_iterator`] — and steps it, always. There
/// is exactly one merge in here (that call's own, a single `ValueId`, the
/// ordinary "two ways to compute the same thing" shape [`apply_default`]
/// already uses), and everything after it is a single unbranched path: no
/// second copy of "how a default fires" or "how a rest is gathered" for a
/// listwise source, and no second merge to keep agreeing with the first one —
/// see the module doc for why an EARLIER version of this function needed one
/// and why that was the wrong shape.
///
/// # What each shape reads through
///
/// [`array_pattern_stepwise`] always STEPS a real iterator now rather than
/// materialising the source up front: it calls `next()` exactly once per
/// position the pattern names, stops the moment `done` is true, and only
/// continues past the named positions for a rest element. That is the fix for
/// the bug this module used to have: `let [a, b] = iterable` called `next()`
/// until the source was exhausted, which is observable (an extra call is an
/// extra side effect) and fatal for a source that never reports `done` — an
/// infinite generator, legitimately. [`close_stmt`] calls `return()` on the
/// iterator when it is abandoned before exhaustion, after the named positions
/// when there is no rest — the one case this change closes; a default
/// initializer's evaluation throwing is a second case the specification calls
/// out and [`apply_default_stepwise`]'s own doc states as a named gap, with
/// why.
///
/// There is exactly one iterator, gotten by [`get_pattern_iterator`], never
/// two competing notions of "the source": a `string`, a `Map`, a `Set`, and a
/// typed array do NOT declare `Symbol.iterator` here —
/// `crates/rts-core-rwk/src/entry/collections/mod.rs` states why for the
/// collections, and the same is true of the other two — so for those,
/// [`get_pattern_iterator`] falls back to [`crate::runtime::RuntimeOp::Iterate`]'s
/// eager materialisation, WRAPPED in a real iterator object (`Iterator.from`)
/// so that everything past that point — defaults, rest, closing — is one
/// unbranched path rather than two copies of the same rule. That fallback
/// still walks the whole sequence up front, same as this module did before
/// this change, and that is correct exactly where it is taken: none of those
/// sources has a side effect to observe being asked for twice.
pub(super) fn array_pattern(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &ArrayPattern,
    source: ValueId,
    at: Position,
    depth: u32,
    role: Role,
) -> EmitResult<()> {
    let iterator = get_pattern_iterator(builder, scope, ctx, source, depth, at)?;
    array_pattern_stepwise(builder, scope, ctx, pattern, iterator, at, depth, role)
}

/// The iterator to step: `source`'s own, if it declares a callable
/// `Symbol.iterator` (which includes an array, since the array prototype
/// installs one), or — for a `string`, a `Map`, a `Set`, a typed array, and
/// anything else that does not (`entry/collections/mod.rs` states why for the
/// collections) — `Iterator.from(source)`, which is [`RuntimeOp::Iterate`]'s
/// own eager materialisation *wrapped* in a real iterator object.
///
/// That second path still walks the whole sequence up front, same as this
/// module did before this change — correct for exactly the sources it is
/// correct for, because none of them has a side effect to observe being asked
/// for twice. What changed is that `next()` on the WRAPPING object is what
/// gets called a bounded number of times afterward, uniformly with the first
/// path, rather than the array being indexed directly — so this file has
/// exactly one notion of "the source", not two.
///
/// A single merged value, built the same way [`apply_default`] merges one:
/// two blocks that each produce a `ValueId` and nothing else — neither
/// declares a name that needs to survive past its own block, so there is
/// nothing here for [`super::merge`] to reconcile beyond the value itself.
fn get_pattern_iterator(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    source: ValueId,
    depth: u32,
    at: Position,
) -> EmitResult<ValueId> {
    let symbol_global = ctx.names.intern("Symbol");
    let iterator_name = ctx.names.intern("iterator");
    let symbol_iterator = member_expr(ident(symbol_global, at), iterator_name, at);
    let key = super::super::expr::emit_expr(builder, scope, ctx, &symbol_iterator)?;
    let method = super::super::expr::call(builder, ctx, RuntimeOp::GetIndexed, &[source, key])?[0];
    let kind = super::super::expr::call(builder, ctx, RuntimeOp::TypeOf, &[method])?[0];
    let function_literal = super::super::expr::string_literal(builder, ctx, "function")?;
    let steppable = super::super::expr::call(
        builder,
        ctx,
        RuntimeOp::StrictEquals,
        &[kind, function_literal],
    )?[0];

    let own_block = builder.create_block();
    let wrapped_block = builder.create_block();
    let join = builder.create_block();
    let answer = builder.add_block_param(join, UNPROVEN);
    builder.branch(steppable, (own_block, &[]), (wrapped_block, &[]))?;

    builder.switch_to(own_block);
    let absent = super::super::expr::undefined(builder, ctx);
    let own_iter = super::super::expr::call(
        builder,
        ctx,
        RuntimeOp::Call,
        &[method, source, absent, absent, absent, absent],
    )?[0];
    let own_iter = builder.widen(own_iter);
    builder.jump(join, &[own_iter])?;

    builder.switch_to(wrapped_block);
    // Bracketed by its own scope, exactly as `assign_target` is and for the
    // same reason: this name exists to hand the source to `Iterator.from` as
    // an ordinary argument expression, and nothing past this block needs it.
    scope.enter();
    let src = ctx.names.intern(&format!("__rts_destructure_src_{depth}"));
    super::super::binding::declare(builder, scope, ctx, src, source)?;
    let iterator_global = ctx.names.intern("Iterator");
    let from_name = ctx.names.intern("from");
    let call_from = Expr {
        kind: ExprKind::Call {
            callee: Box::new(member_expr(ident(iterator_global, at), from_name, at)),
            arguments: vec![Spreadable::Single(ident(src, at))],
            optional: false,
        },
        at,
    };
    let wrapped_iter = super::super::expr::emit_expr(builder, scope, ctx, &call_from)?;
    let wrapped_iter = builder.widen(wrapped_iter);
    scope.leave();
    builder.jump(join, &[wrapped_iter])?;

    builder.switch_to(join);
    Ok(answer)
}

/// The stepped path: `iterator` is already the iterator (its `Symbol.iterator`
/// was already called), and this calls `next()` on it directly — at most once
/// per named position, and only past them for a rest.
///
/// `__rts_destructure_done_{depth}` is a plain `let` in the pattern's own
/// scope, the same trick every other synthetic temporary here uses: nothing a
/// program wrote can spell it, so nothing it writes can collide with it, and
/// ordinary `if`/`while`/`try` lowering can read and write it like any other
/// local because that is what it is.
fn array_pattern_stepwise(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &ArrayPattern,
    iterator: ValueId,
    at: Position,
    depth: u32,
    role: Role,
) -> EmitResult<()> {
    let iter = ctx.names.intern(&format!("__rts_destructure_iter_{depth}"));
    super::super::binding::declare(builder, scope, ctx, iter, iterator)?;

    let done = ctx.names.intern(&format!("__rts_destructure_done_{depth}"));
    let false_value = super::super::expr::emit_expr(builder, scope, ctx, &bool_literal(false, at))?;
    super::super::binding::declare(builder, scope, ctx, done, false_value)?;

    // `elements` holds a hole at the rest's own position too — the parser
    // maps `...tail` in `[head, ...tail]` to `None` there, the same
    // representation an ordinary hole gets, rather than shortening the
    // vector by one. That slot names no position to step for: the rest below
    // is what steps past it, and stepping here as well is exactly the
    // off-by-one this line guards against — verified by a fixture that lost
    // the element right after the last named one until this line excluded it.
    let named = match pattern.rest {
        Some(_) => &pattern.elements[..pattern.elements.len() - 1],
        None => &pattern.elements[..],
    };
    for (position, element) in named.iter().enumerate() {
        // A hole still steps — it already consumed a slot in the listwise
        // path, and stepping is what "consuming a slot" means here.
        let raw = step_iterator(builder, scope, ctx, iter, done, depth, position, at)?;
        let Some(element) = element else { continue };
        let value = apply_default_stepwise(
            builder,
            scope,
            ctx,
            raw,
            element.default.as_ref(),
            depth,
            position,
            at,
        )?;
        place(builder, scope, ctx, &element.pattern, value, at, depth + 1, role)?;
    }

    if let Some(rest) = &pattern.rest {
        // A rest takes the remainder, and only then is exhaustion correct —
        // the module doc's phrasing for exactly this case. No `return()`
        // needed: the loop below runs until `done` is true on its own.
        let gathered = gather_rest_stepwise(builder, scope, ctx, iter, done, depth, at)?;
        place(builder, scope, ctx, rest, gathered, at, depth + 1, role)?;
    } else {
        // No rest: the pattern named fewer elements than the source may have
        // had, so the iterator is abandoned early unless the last step
        // already reported `done`. `close_stmt` is itself conditioned on
        // `!done`, matching `IteratorClose`'s "only if not already done".
        let mut loops = Loops::default();
        let close = close_stmt(iter, done, ctx, at);
        super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &close)?;
    }

    Ok(())
}

/// One `next()`, guarded by `done`: does nothing and answers `undefined` once
/// the iterator already reported `done`, so a pattern naming more positions
/// than the source has elements does not call `next()` again after it did.
///
/// Built as ordinary statements — `if`, `let`, an assignment — rather than raw
/// IR blocks, which is this module's stated preference (see the module doc)
/// for exactly the reason it states: `if`'s own lowering already merges what
/// each arm did to a binding, so reusing it here is not writing that merge a
/// second time.
fn step_iterator(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    iter: Name,
    done: Name,
    depth: u32,
    position: usize,
    at: Position,
) -> EmitResult<ValueId> {
    let val = ctx.names.intern(&format!("__rts_destructure_val_{depth}_{position}"));
    let step = ctx.names.intern(&format!("__rts_destructure_step_{depth}_{position}"));
    let next_name = ctx.names.intern("next");
    let done_prop = ctx.names.intern("done");
    let value_prop = ctx.names.intern("value");

    let declare_val = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(val),
                value: None,
                claim: None,
            }],
        },
        at,
    };

    let call_next = Expr {
        kind: ExprKind::Call {
            callee: Box::new(member_expr(ident(iter, at), next_name, at)),
            arguments: vec![],
            optional: false,
        },
        at,
    };
    let declare_step = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Const,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(step),
                value: Some(call_next),
                claim: None,
            }],
        },
        at,
    };
    let assign_done = plain_assign_stmt(
        ident(done, at),
        member_expr(ident(step, at), done_prop, at),
        at,
    );
    let then_value = Expr {
        kind: ExprKind::Conditional {
            condition: Box::new(ident(done, at)),
            then_branch: Box::new(undefined_expr(at)),
            else_branch: Box::new(member_expr(ident(step, at), value_prop, at)),
        },
        at,
    };
    let assign_val_stepped = plain_assign_stmt(ident(val, at), then_value, at);
    let then_branch = Stmt {
        kind: StmtKind::Block(vec![declare_step, assign_done, assign_val_stepped]),
        at,
    };

    let already_done = Stmt {
        kind: StmtKind::Block(vec![plain_assign_stmt(ident(val, at), undefined_expr(at), at)]),
        at,
    };

    let not_done = Expr {
        kind: ExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(ident(done, at)),
        },
        at,
    };
    let if_stmt = Stmt {
        kind: StmtKind::If {
            condition: not_done,
            then_branch: Box::new(then_branch),
            else_branch: Some(Box::new(already_done)),
        },
        at,
    };

    let mut loops = Loops::default();
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &declare_val)?;
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &if_stmt)?;
    super::super::binding::read(builder, scope, ctx, val)
}

/// A default over a stepped source: the same rule [`apply_default`] states —
/// fires on `undefined` alone, never evaluated otherwise.
///
/// # A named gap: `[a = f()] = iterable` where `f` THROWS does not close
///
/// The specification calls `IteratorClose` on `iterable`'s iterator before the
/// throw leaves this pattern. Building that means catching the throw here,
/// which needs a real `try`/`catch` — and a synthetic one built at this point,
/// during emission, is unsound in this compiler: `capture.rs`'s
/// `assigned_under_protection` is what decides a name written inside a `try`
/// body needs heap storage rather than a register, so a handler reached by
/// unwinding still sees the write, and it runs ONCE, over the real parse tree,
/// before any of this module's synthetic statements exist. A `try` invented
/// here is invisible to it, so `out` below — written inside the very `try`
/// this gap describes, when it was tried — read back whatever it held BEFORE
/// the write on the fall-through path too, not only on the thrown one. Found
/// by a fixture that regressed silently: `[p = 5, q = 9] = [1]` answered `q =
/// undefined` with no exception in flight at all. Reverted rather than shipped
/// for that reason, and this is the same absence `foreach.rs`'s `for-of` and
/// `for_await.rs`'s loop both already state for their own `IteratorClose`
/// gaps, for adjacent reasons of their own. Closing it needs either teaching
/// `capture.rs` to anticipate a pattern default's synthetic `try`, or a
/// close-on-throw mechanism that does not write a name across one.
fn apply_default_stepwise(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    raw: ValueId,
    default: Option<&Expr>,
    depth: u32,
    position: usize,
    at: Position,
) -> EmitResult<ValueId> {
    let Some(default) = default else {
        return Ok(raw);
    };

    let raw_name = ctx.names.intern(&format!("__rts_destructure_raw_{depth}_{position}"));
    super::super::binding::declare(builder, scope, ctx, raw_name, raw)?;
    let out = ctx.names.intern(&format!("__rts_destructure_out_{depth}_{position}"));

    let declare_out = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(out),
                value: None,
                claim: None,
            }],
        },
        at,
    };

    let is_undefined = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictEqual,
            left: Box::new(ident(raw_name, at)),
            right: Box::new(undefined_expr(at)),
        },
        at,
    };
    let then_branch = Stmt {
        kind: StmtKind::Block(vec![plain_assign_stmt(ident(out, at), default.clone(), at)]),
        at,
    };
    let else_branch = Stmt {
        kind: StmtKind::Block(vec![plain_assign_stmt(ident(out, at), ident(raw_name, at), at)]),
        at,
    };
    let if_stmt = Stmt {
        kind: StmtKind::If {
            condition: is_undefined,
            then_branch: Box::new(then_branch),
            else_branch: Some(Box::new(else_branch)),
        },
        at,
    };

    let mut loops = Loops::default();
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &declare_out)?;
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &if_stmt)?;
    super::super::binding::read(builder, scope, ctx, out)
}

/// `...rest` of a STEPPED array pattern: `next()` called until `done`, each
/// non-final value appended to a fresh array.
///
/// `.push` rather than the raw `ArrayAppend` entry point, for the reason the
/// object shape's `object_rest` gives for building on an existing method
/// rather than a hand-written copy: it is already the correct answer to "grow
/// this by one", already exercised, and not a second way for an array to grow
/// that this module would own agreeing with the first.
fn gather_rest_stepwise(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    iter: Name,
    done: Name,
    depth: u32,
    at: Position,
) -> EmitResult<ValueId> {
    let rest = ctx.names.intern(&format!("__rts_destructure_rest_{depth}"));
    let step = ctx.names.intern(&format!("__rts_destructure_reststep_{depth}"));
    let next_name = ctx.names.intern("next");
    let done_prop = ctx.names.intern("done");
    let value_prop = ctx.names.intern("value");
    let push_name = ctx.names.intern("push");

    let empty_array = Expr {
        kind: ExprKind::Array { elements: vec![] },
        at,
    };
    let declare_rest = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Const,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(rest),
                value: Some(empty_array),
                claim: None,
            }],
        },
        at,
    };
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &declare_rest)?;

    let call_next = Expr {
        kind: ExprKind::Call {
            callee: Box::new(member_expr(ident(iter, at), next_name, at)),
            arguments: vec![],
            optional: false,
        },
        at,
    };
    let declare_step = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Const,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(step),
                value: Some(call_next),
                claim: None,
            }],
        },
        at,
    };
    let assign_done = plain_assign_stmt(
        ident(done, at),
        member_expr(ident(step, at), done_prop, at),
        at,
    );
    let push_value = Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Call {
                callee: Box::new(member_expr(ident(rest, at), push_name, at)),
                arguments: vec![Spreadable::Single(member_expr(ident(step, at), value_prop, at))],
                optional: false,
            },
            at,
        }),
        at,
    };
    let push_unless_done = Stmt {
        kind: StmtKind::If {
            condition: Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(ident(done, at)),
                },
                at,
            },
            then_branch: Box::new(push_value),
            else_branch: None,
        },
        at,
    };
    let body = Stmt {
        kind: StmtKind::Block(vec![declare_step, assign_done, push_unless_done]),
        at,
    };
    let not_done = Expr {
        kind: ExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(ident(done, at)),
        },
        at,
    };
    let while_stmt = Stmt {
        kind: StmtKind::While {
            condition: not_done,
            body: Box::new(body),
        },
        at,
    };
    let mut loops = Loops::default();
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &while_stmt)?;

    super::super::binding::read(builder, scope, ctx, rest)
}

/// `if (!done) { if (typeof iter.return === "function") { iter.return(); } }`
/// — `IteratorClose`, as far as this engine can express it: `return()` is
/// called only when the source has not already reported `done`, and only when
/// it exists and is callable at all — a plain iterator-like object with a bare
/// `next()` has none, and that is legal, not an error.
fn close_stmt(iter: Name, done: Name, ctx: &mut Ctx, at: Position) -> Stmt {
    let return_name = ctx.names.intern("return");
    let return_member = member_expr(ident(iter, at), return_name, at);
    let is_callable = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictEqual,
            left: Box::new(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::TypeOf,
                    operand: Box::new(return_member.clone()),
                },
                at,
            }),
            right: Box::new(Expr {
                kind: ExprKind::Literal(Literal::String("function".to_owned())),
                at,
            }),
        },
        at,
    };
    let call_return = Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Call {
                callee: Box::new(return_member),
                arguments: vec![],
                optional: false,
            },
            at,
        }),
        at,
    };
    let inner_if = Stmt {
        kind: StmtKind::If {
            condition: is_callable,
            then_branch: Box::new(call_return),
            else_branch: None,
        },
        at,
    };
    let not_done = Expr {
        kind: ExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(ident(done, at)),
        },
        at,
    };
    Stmt {
        kind: StmtKind::If {
            condition: not_done,
            then_branch: Box::new(inner_if),
            else_branch: None,
        },
        at,
    }
}

/// `undefined`, as a synthetic expression — distinct from `ident` because
/// nothing here binds the name `undefined`; it is the language's own
/// singleton literal.
fn undefined_expr(at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
        at,
    }
}

/// `true` or `false`, as a synthetic expression.
fn bool_literal(value: bool, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Boolean(value)),
        at,
    }
}
