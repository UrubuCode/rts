//! The array shape of a [`super::Pattern`]: stepping an iterator.
//!
//! Split out of `destructure/mod.rs` — over the codegen file-size ceiling
//! together — once this file grew from "index a materialised array" to the
//! full stepped protocol: getting the iterator, calling `next()` a bounded
//! number of times, gathering a rest, and closing on early abandonment. The
//! object shape stayed behind because it never grew past reading properties.

use rts_cranelift::fault::Position;
use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};
use rts_cranelift::repr::Repr;
use rts_cranelift::unwind::Handler;

use super::super::loops::Loops;
use super::super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    ArrayPattern, BinaryOp, Binding as SyntaxBinding, BindingKind, Element, Expr, ExprKind,
    Literal, Pattern, Spreadable, Stmt, StmtKind, UnaryOp,
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
/// when there is no rest. An element's initialization leaving ABRUPTLY — a
/// throwing default, a throwing nested pattern — is the second case the
/// specification calls out, and [`open_close_region`] closes it from one place
/// rather than adding a second rule for the default alone.
///
/// There is exactly one iterator, gotten by [`get_pattern_iterator`], never
/// two competing notions of "the source": a `string`, a `Map`, a `Set`, and a
/// typed array do NOT declare `Symbol.iterator` here —
/// `crates/rts-core/src/entry/collections/mod.rs` states why for the
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
    let direct = direct_candidate(builder, scope, ctx, pattern, source, depth)?;
    let iterator = iterator_unless_direct(builder, scope, ctx, source, depth, at, direct)?;
    array_pattern_stepwise(builder, scope, ctx, pattern, iterator, at, depth, role, direct)
}

/// The source, and the answer to "may this pattern read it by index" — or
/// `None` for a pattern this does not yet cover.
///
/// # Which patterns, and why the line is here
///
/// A rest and an element that can throw are excluded, and both for the same
/// reason rather than two: each is a place the ITERATOR is named again after the
/// positions are stepped. A rest gathers the remainder by stepping until `done`,
/// and a throwing element opens a region whose handler calls `return()` on the
/// iterator — and on this path there is no iterator, because not making one is
/// the whole saving. Covering them means giving each a second arm of its own,
/// which is the duplication of "how a default fires" and "how a rest is
/// gathered" that this module's header refuses. They keep stepping, exactly as
/// before, and what is left is `[x, y, z]` and `[a, , c]` — the shape almost
/// every program writes.
///
/// # Why the question goes to the runtime whole
///
/// Four facts have to hold at once and every one of them is state a program can
/// change; `rts_core::entry::pattern` states them and why they are asked
/// together. `emit/foreach.rs` asks its own version out of emitted operations,
/// and that is the shape NOT taken here: it costs a `Symbol.iterator` read, an
/// `ArrayNew` and an identity comparison — 203 ns measured, 66 of them an array
/// allocated only to read a method off it — which a loop amortises and a
/// destructuring pays in full. It also cannot ask three of the four.
fn direct_candidate(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &ArrayPattern,
    source: ValueId,
    depth: u32,
) -> EmitResult<Option<Direct>> {
    let covered = pattern.rest.is_none()
        && pattern
            .elements
            .iter()
            .flatten()
            .all(|element| !can_throw(element));
    if !covered {
        return Ok(None);
    }

    let held = ctx.names.intern(&format!("__rts_destructure_src_{depth}"));
    super::super::binding::declare(builder, scope, ctx, held, source)?;
    let asked =
        super::super::expr::call(builder, ctx, RuntimeOp::ArrayPatternDirect, &[source])?[0];
    let flag = ctx.names.intern(&format!("__rts_destructure_direct_{depth}"));
    super::super::binding::declare(builder, scope, ctx, flag, asked)?;
    Ok(Some(Direct { flag, held }))
}

/// A pattern that may read its source by index, and the two names that say so.
#[derive(Clone, Copy)]
struct Direct {
    /// Holds the runtime's answer.
    flag: Name,
    /// Holds the source, so an indexed read can name it.
    held: Name,
}

/// The iterator to step, or `undefined` when the source is read by index.
///
/// The prologue is what this skips, and skipping it is most of the point: it is
/// ten emitted calls — the method read, `typeof`, a string constant, an identity
/// comparison, and the invocation — for a source that is about to be read
/// directly anyway. A merge by hand rather than an `if`, because what it joins
/// is one `ValueId` and neither arm declares a name that outlives it, which is
/// the same shape and the same argument [`get_pattern_iterator`] already carries
/// for its own two arms.
fn iterator_unless_direct(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    source: ValueId,
    depth: u32,
    at: Position,
    direct: Option<Direct>,
) -> EmitResult<ValueId> {
    let Some(direct) = direct else {
        return get_pattern_iterator(builder, scope, ctx, source, depth, at);
    };

    let asked = super::super::binding::read(builder, scope, ctx, direct.flag)?;
    let asked = super::super::expr::to_boolean(builder, ctx, asked)?;
    let skipped = builder.create_block();
    let stepped = builder.create_block();
    let join = builder.create_block();
    let answer = builder.add_block_param(join, UNPROVEN);
    builder.branch(asked, (skipped, &[]), (stepped, &[]))?;

    builder.switch_to(skipped);
    let absent = super::super::expr::undefined(builder, ctx);
    builder.jump(join, &[absent])?;

    builder.switch_to(stepped);
    let stepping = get_pattern_iterator(builder, scope, ctx, source, depth, at)?;
    builder.jump(join, &[stepping])?;

    builder.switch_to(join);
    Ok(answer)
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
    // The key directly, never the global name `Symbol`. Reading `Symbol` here
    // made a LOCAL binding of that name decide whether a destructuring worked:
    // `function f(arr) { const Symbol = null; const [x, y] = arr; }` threw
    // `Cannot read properties of null` where node and bun both answer, because
    // the emitter resolved the shadowing binding instead of the well-known
    // symbol. `emit/foreach.rs` states the same rule for the same reason and
    // spells it the same way — the reserved `@@` space is how this crate names
    // a symbol key, and `class.rs` emits `[Symbol.iterator]() {}` under it.
    let symbol_iterator = ctx.names.intern("@@iterator");
    let key = super::super::property::key_constant(builder, ctx, symbol_iterator);
    let method = super::super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[source, key])?[0];
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
    // `it[Symbol.iterator]()` takes no arguments.
    let written = super::super::expr::count_constant(builder, 0);
    // A call the COMPILER wrote: no source spelling, so no literal to name.
    let unnamed = super::super::expr::name_constant(builder, None);
    let own_iter = super::super::expr::call(
        builder,
        ctx,
        RuntimeOp::Call,
        &[
            method,
            source,
            written,
            unnamed,
            absent,
            absent,
            absent,
            absent,
        ],
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
    direct: Option<Direct>,
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
        let raw = step_iterator(builder, scope, ctx, iter, done, depth, position, at, direct)?;
        let Some(element) = element else { continue };
        // `BindingInitialization` of ONE element — the default and the target
        // together — is what the specification protects: a throw from either
        // closes the iterator, and a throw from `next()` does not (the
        // iterator has already failed, and `IteratorClose` on it is exactly
        // what the specification declines to do). So the region starts AFTER
        // `step_iterator` and not before it.
        let region = can_throw(element).then(|| open_close_region(builder));
        let region = match region {
            Some(region) => Some(region?),
            None => None,
        };
        let value = apply_default_stepwise(
            builder,
            scope,
            ctx,
            raw,
            element.default.as_ref(),
            &element.pattern,
            depth,
            position,
            at,
        )?;
        place(builder, scope, ctx, &element.pattern, value, at, depth + 1, role)?;
        if let Some(region) = region {
            close_close_region(builder, scope, ctx, region, iter, done, at)?;
        }
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
        // Not on the indexed path, and this is a correctness guard rather than a
        // saving: there is no iterator there, so `iter` holds `undefined` and
        // `typeof iter.return` would read a property of it and throw.
        //
        // Skipping it is also what the specification does. `IteratorClose` looks
        // `return` up on the iterator and returns without calling anything when
        // it is absent, and `%ArrayIteratorPrototype%` has none — it carries
        // `next` and a string tag. `rts_core::entry::pattern` refuses the
        // indexed path outright if a program has added one, so the case where
        // this would have mattered never reaches here.
        let close = match direct {
            None => close,
            Some(direct) => Stmt {
                kind: StmtKind::If {
                    condition: Expr {
                        kind: ExprKind::Unary {
                            op: UnaryOp::Not,
                            operand: Box::new(ident(direct.flag, at)),
                        },
                        at,
                    },
                    then_branch: Box::new(close),
                    else_branch: None,
                },
                at,
            },
        };
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
    direct: Option<Direct>,
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

    // The indexed arm, when the runtime licensed one. An `if` and not a second
    // merge written by hand, for the reason the module doc gives for the `if`
    // above it: the lowering of `if` already reconciles what each arm did to a
    // binding, and both arms here write exactly `val` and `done`.
    //
    // Exactly one arm is live for a given source, so nothing downstream — the
    // default, the target, the close — has to know which was taken.
    let step = match direct {
        None => if_stmt,
        Some(direct) => Stmt {
            kind: StmtKind::If {
                condition: ident(direct.flag, at),
                then_branch: Box::new(indexed_step(direct.held, val, position, at)),
                else_branch: Some(Box::new(if_stmt)),
            },
            at,
        },
    };

    let mut loops = Loops::default();
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &declare_val)?;
    super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &step)?;
    super::super::binding::read(builder, scope, ctx, val)
}

/// One position, read straight out of the source: `val = src[position]`.
///
/// # Why there is no length test, and why that is not a shortcut
///
/// An indexed read past the end already answers `undefined`, and so does a
/// hole — which is exactly what the stepping arm answers for both, because the
/// primordial cursor reads `elements[i]` through the same visibility rule and
/// `val = done ? undefined : step.value` collapses to the same thing. So the
/// comparison a first version wrote here was computing a value that could not
/// change the answer, and it was not free: `position >= src.length` emitted a
/// property read AND `__rts_greater_equal`, because neither side is proven.
/// Counted in `rts ir`, that is two of the three crossings this arm was paying
/// per position.
///
/// # Why `done` is not written either
///
/// Because on this arm nothing reads it, and that is a fact about
/// [`direct_candidate`]'s gate rather than about this function. Every reader was
/// checked: `close_close_region` runs only for an element where
/// [`can_throw`] holds, `gather_rest_stepwise` only for a pattern with a rest,
/// and both are what the gate excludes; the `!done` guard and the `done ?`
/// ternary are the STEPPING arm's own; and [`close_stmt`] is conditioned on the
/// flag, not on `done`, precisely so that it does not depend on this.
///
/// Widening the gate means giving `done` back before anything else — it is the
/// one thing here that a wider scope would silently need.
fn indexed_step(held: Name, val: Name, position: usize, at: Position) -> Stmt {
    let element = Expr {
        kind: ExprKind::Index {
            object: Box::new(ident(held, at)),
            index: Box::new(Expr {
                kind: ExprKind::Literal(Literal::Number(position as f64)),
                at,
            }),
            optional: false,
        },
        at,
    };
    plain_assign_stmt(ident(val, at), element, at)
}

/// Whether initializing this element can leave abruptly, and therefore needs
/// the closing region [`open_close_region`] opens.
///
/// A bare name with no default cannot: binding a name is a store, and a store
/// throws nothing. Everything else can — a default is an arbitrary expression,
/// a nested pattern destructures again (`Symbol.iterator` lookups, more
/// `next()` calls), and a member target evaluates an object that may be
/// `null`. Asked rather than always wrapping because the region is not free
/// and `let [a, b] = xs` is the shape almost every program writes.
fn can_throw(element: &Element) -> bool {
    element.default.is_some() || !matches!(element.pattern, Pattern::Name(_))
}

/// Opens a protected region whose handler will close the iterator.
///
/// # Why raw blocks rather than a synthetic `try` statement
///
/// A `try` invented during emission is unsound here, and this module used to
/// state that as the reason `[a = f()] = it` did not close when `f` threw.
/// `capture.rs`'s `assigned_under_protection` is what decides that a name
/// written inside a `try` needs heap storage, and it runs ONCE over the real
/// parse tree, before any synthetic statement exists — so `protect::emit_try`'s
/// `scope.restore` at the join threw away every SSA binding the body had made,
/// and `[p = 5, q = 9] = [1]` answered `q = undefined` with nothing thrown at
/// all.
///
/// The region built here has no such join. Its handler RE-RAISES, so the block
/// after it is reached from the normal path alone — the bindings the body made
/// dominate it exactly as an `if`'s do, and nothing has to be restored. The
/// handler itself reads only `iter` and `done`, and neither is rebound inside
/// the region — so it needs no snapshot of its own, and taking one would have
/// been wrong anyway: the body DECLARES names (the element's own target among
/// them), so the scope it leaves is longer than the one it started in and
/// `Scope::restore` refuses that by construction.
fn open_close_region(builder: &mut FuncBuilder) -> EmitResult<CloseRegion> {
    let after = builder.create_unprotected_block();
    let protected = builder.create_block();
    builder.jump(protected, &[])?;
    builder.switch_to(protected);
    let handler = builder.create_block();
    builder.open_region(
        vec![Handler {
            tag: super::super::protect::JS_THROW,
            block: handler,
        }],
        None,
    );
    Ok(CloseRegion { after, handler })
}

/// What [`open_close_region`] opened, so [`close_close_region`] can finish it.
struct CloseRegion {
    after: BlockId,
    handler: BlockId,
}

/// Closes the region: the normal path leaves for `after`, and the handler
/// calls `return()` on the iterator and re-raises what it caught.
fn close_close_region(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    region: CloseRegion,
    iter: Name,
    done: Name,
    at: Position,
) -> EmitResult<()> {
    builder.close_region();
    let normal = scope.snapshot();
    builder.jump(region.after, &[])?;

    // The thrown value arrives as the handler's parameter — the machine's
    // discipline for it, same as `protect::emit_try`'s handler.
    let thrown = builder.add_block_param(region.handler, Repr::Tagged);
    builder.switch_to(region.handler);
    let close = close_stmt(iter, done, ctx, at);
    let mut loops = Loops::default();
    let terminated = super::super::stmt::emit_stmt(builder, scope, ctx, &mut loops, &close)?;
    if !terminated {
        builder.throw(super::super::protect::JS_THROW, thrown);
    }

    builder.switch_to(region.after);
    scope.restore(&normal);
    Ok(())
}

/// A default over a stepped source: the same rule [`apply_default`] states —
/// fires on `undefined` alone, never evaluated otherwise.
///
/// A throw from the default's evaluation closes the iterator, which is
/// [`open_close_region`]'s doing rather than this function's: the region wraps
/// the default AND the target it binds, because the specification protects
/// `BindingInitialization` as a whole and not one half of it.
fn apply_default_stepwise(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    raw: ValueId,
    default: Option<&Expr>,
    bound: &Pattern,
    depth: u32,
    position: usize,
    at: Position,
) -> EmitResult<ValueId> {
    let Some(default) = default else {
        return Ok(raw);
    };
    // NamedEvaluation, for the same reason the object path states it: a
    // function arriving through `const [f = () => {}] = []` is named `f`.
    if let Pattern::Name(name) = bound
        && super::super::stmt::anonymous_definition(default)
    {
        ctx.lend_name(*name);
    }

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
                kind: ExprKind::Literal(Literal::String("function".into())),
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
