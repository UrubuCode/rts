//! `for (k in o)` — walking an object's keys — and `for (x of xs)` — stepping a
//! sequence.
//!
//! Its own file for the same reason `switch.rs` is: `loops.rs` had reached the
//! thousand-line ceiling and this is part of what pushed it there.
//!
//! # Why one loop walks an array AND steps an iterator
//!
//! `for-in` reduces to an indexed walk over the keys, and so did `for-of` over
//! [`crate::runtime::RuntimeOp::Iterate`]'s materialised array. That reduction
//! is wrong for `for-of` in three ways at once, all of them measured against
//! Bun on `tests/cross-runtime/`: a `break` never calls `return()` on the
//! iterator, a `Map` or `Set` mutated by the body is walked as it was BEFORE
//! the body ran, and a source that never reports `done` is drained forever
//! instead of once per pass.
//!
//! The obvious repair — step the protocol always — costs a `{ value, done }`
//! ALLOCATION per element of every `for-of` in the program, including the
//! overwhelmingly common `for (const x of anArray)`, which is exactly what the
//! materialised walk exists to avoid. So the two live in one loop and the
//! source picks:
//!
//! ```text
//! for (let i = 0; ; i++) {
//!   let e;
//!   if (i < len)            e = ks[i];          // the array walk, unchanged
//!   else if (it === undefined) break;           // …and its end
//!   else { const s = it.next();                 // the protocol, one step
//!          if (s.done) { it = undefined; break; }
//!          e = s.value; }
//!   let x = e;
//!   body
//! }
//! if (it !== undefined) close(it);              // IteratorClose, on `break`
//! ```
//!
//! Exactly one of the two arms is ever live for a given source: a stepped
//! source gets `ks` empty (`len` is 0, so the first arm never fires) and an
//! array-walked one gets `it` undefined. The array path therefore emits and
//! executes what it did before — the same `ks[i]`, the same proven index, the
//! same hoisted run — and pays one predicted branch it was already paying as
//! the loop test.
//!
//! # What picks, and why it is not `Array.isArray`
//!
//! The source is STEPPED unless walking a copy of it is indistinguishable from
//! stepping it, which is true for exactly two primordials: an array's
//! `Symbol.iterator` and a string's. So the question asked is "is this value's
//! `Symbol.iterator` the one a fresh array would use?" — an identity comparison
//! against `[]`'s own method, which needs no global name and cannot be broken
//! by a program that rebinds `Array`. A program that replaces
//! `Array.prototype[Symbol.iterator]` moves both sides of that comparison at
//! once and keeps the array walk, which is the behaviour this loop had before
//! and not a new divergence.
//!
//! `typeof src === "string"` is the second half, rather than a third identity
//! comparison, because a string primitive is not an object and reading a method
//! off one to compare it allocates a wrapper for nothing.
//!
//! # What `IteratorClose` here covers, and what it still does not
//!
//! A `break` leaves by falling out of the loop, and the statement after it
//! closes the iterator. A `throw` leaves through a protected REGION opened
//! around the loop, whose handler closes and re-raises.
//!
//! The region is built out of blocks rather than expanded as a `try` statement,
//! and that is not a style choice — the `try` was written and measured first. It
//! closed the iterator and it also made `for (const x of [1,2,3]) s += x` answer
//! `s === 0`, because `protect::emit_try` discards the protected span's SSA
//! bindings at its join and is sound only because `capture.rs` has already
//! forced every name assigned under protection into memory. That analysis reads
//! the parse tree, before any statement this file invents exists.
//!
//! The experiment that says so is one flip: under the `try`, the same loop with
//! a `try` the PROGRAM wrote inside its body answered 6, and so did one whose
//! accumulator any other `try` in scope had touched. The region machinery was
//! never the problem. `destructure/array.rs::open_close_region` met the same
//! wall first and took the same way round it.
//!
//! A `return` out of the body, and a `break` or `continue` to a label on an
//! ENCLOSING loop, leave without unwinding and without falling out of this
//! loop — so neither the handler nor the statement after it is reached, and
//! this file used to name all three as uncovered. Each is now routed through
//! the mechanism the language already has for "run this on the way out": the
//! `return` through a block on `Ctx::finally_returns`, the two jumps through a
//! body on `Ctx::finally_jumps`. Neither is a third mechanism, which is the
//! point — a `try`/`finally` written inside the body and this loop's close now
//! run in one order, decided in one place.
//!
//! A label on THIS loop was covered already — `break outer` from a nested loop
//! still leaves the outer one by its own exit — and so is `continue`, which
//! never leaves at all. Those two are worth naming because "a labelled break"
//! reads like one case and is two with different answers.
//!
//! What is still NOT covered: a `return` inside a `for await`, whose close must
//! be awaited — `for_await.rs` builds its own loop and none of this reaches it.

use rts_cranelift::fault::Position;
use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::close::{
    always, assign_stmt, close_iterator_stmt, ident, member_expr, still_open, text_expr,
    undefined_expr,
};
use super::loops::{Loops, emit_for};
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal, LogicalOp, Pattern, Stmt, UnaryOp};

/// Emits `for (k in o)`.
///
/// # Why this is built as a tree and emitted as an ordinary `for`
///
/// This crate refuses desugaring where it loses a fact — `a += b` is not
/// rewritten to `a = a + b`, because the rewrite evaluates the target twice.
/// Here nothing is lost: the keys are an array, and walking an array by index
/// **is** what `for-in` reduces to once the enumeration itself is a value.
///
/// What the expansion buys is everything the loop already gets right and would
/// otherwise be written a second time: `break`, `continue`, a label on the
/// loop, the block parameters for names the body assigns, and a fresh binding
/// per pass so a closure made in the body captures that pass's key.
///
/// ```text
/// for (let k in o)  ──▶  for (let i = 0, ks = keys(o); i < ks.length; i++) {
///   body                     let k = ks[i];
///                            body
///                          }
/// ```
///
/// # The names it introduces
///
/// Spelled so a program cannot collide with them, and they are ordinary
/// bindings rather than a side channel — which is what lets the existing
/// analysis see the index being assigned and give it a block parameter without
/// being told.
pub fn emit_for_each(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    loops: &mut Loops,
    statement: &Stmt,
    target: &crate::syntax::ForEachTarget,
    subject: &Expr,
    body: &Stmt,
    label: Option<Name>,
    over: crate::runtime::RuntimeOp,
) -> EmitResult<bool> {
    use crate::syntax::{
        AssignOp, AssignTarget, Binding as SyntaxBinding, BindingKind, ForEachTarget, StmtKind,
    };

    // `for (x of xs)` / `for (x in o)` where `x` already exists: no fresh
    // binding, the same place is written every pass. Unlike `Declare`, this
    // does not need a block of its own — a shared place written repeatedly
    // is exactly what the program asked for, not a bug to route around.
    let pattern = match target {
        ForEachTarget::Declare {
            target: pattern, ..
        } => pattern,
        ForEachTarget::Assign(pattern) => pattern,
        ForEachTarget::Dispose { .. } => {
            return super::expr::gap("`using` in a for-head, which needs `Symbol.dispose`");
        }
    };
    // `for (var k of xs)` DECLARES and still is not fresh: `var` was hoisted to
    // the function's scope, so every pass writes the one binding and a closure
    // made in any pass sees the last value. That is the language, and it stopped
    // being free the day a `let` head got a per-iteration environment — this
    // expansion spelled every head `let`, so a `var` head silently acquired the
    // fresh binding `let` had just been given. Measured as `for (var k of xs)`
    // answering `1,2,3` where every other runtime answers `3,3,3`.
    let fresh_binding = matches!(
        target,
        ForEachTarget::Declare {
            kind: BindingKind::Let | BindingKind::Const,
            ..
        }
    );
    let at = statement.at;
    let index = ctx.names.intern("__rts_in_index");
    let keys = ctx.names.intern("__rts_in_keys");
    let name = |of: Name| ident(of, at);
    let number = |value: f64| Expr {
        kind: ExprKind::Literal(crate::syntax::Literal::Number(value)),
        at,
    };

    // `for-of` steps a real iterator when the source asks to be stepped;
    // `for-in` never does, because an object's keys are a list by definition
    // and nothing observes them being read.
    let stepping = matches!(over, RuntimeOp::Iterate);
    let iterator = ctx.names.intern("__rts_of_it");

    // The keys are emitted HERE rather than as a node in the expansion,
    // because there is no name a program could write that means "the runtime
    // operation". Bound in a scope of its own so the loop below sees an
    // ordinary binding and needs to know nothing about where it came from.
    scope.enter();
    let subject_value = super::expr::emit_expr(builder, scope, ctx, subject)?;
    let enumerated = match stepping {
        false => super::expr::call(builder, ctx, over, &[subject_value])?[0],
        true => open_sequence(builder, scope, ctx, subject_value, iterator, at)?,
    };
    super::binding::declare(builder, scope, ctx, keys, enumerated)?;
    // The subject itself, bound so the guard below can ask about it once per
    // pass. Only for `for`-`in`: nothing in the stepped path needs it.
    let subject_name = ctx.names.intern("__rts_in_src");
    if !stepping {
        super::binding::declare(builder, scope, ctx, subject_name, subject_value)?;
    }
    // A second name for the same iterator, written ONCE and never again — what
    // the cleanup handler below reads.
    //
    // It cannot read `__rts_of_it`, and the reason is SSA rather than taste:
    // exhaustion assigns `it = undefined` inside the loop, so that binding has a
    // block parameter and its value at any point is whichever edge arrived
    // there. The handler is reached from a THROW rather than from an edge, so
    // there is no such value for it to see. A binding the loop never writes has
    // one definition, which dominates every block created after it — the same
    // property `destructure/array.rs`'s handler relies on for `iter`.
    //
    // Never cleared is also what makes the handler's close unconditional and
    // correct: it runs only on a throw, and a throw means the loop did not reach
    // exhaustion, so the iterator is open by construction.
    let closing = ctx.names.intern("__rts_of_close");
    if stepping {
        let held = super::binding::read(builder, scope, ctx, iterator)?;
        super::binding::declare(builder, scope, ctx, closing, held)?;
    }

    let init = crate::syntax::ForInit::Declare {
        kind: BindingKind::Let,
        bindings: vec![SyntaxBinding {
            target: Pattern::Name(index),
            value: Some(number(0.0)),
            claim: None,
        }],
    };

    // The bound, read ONCE.
    //
    // `ks` is the array `iterate`/`own_keys` just answered, and that operation
    // COPIES — deliberately, and `rts-core`'s `entry::iterate` says why: "the
    // loop walks what it is given and a body that pushes to the original must
    // not walk its own additions forever". So its length cannot change while
    // this loop runs, and re-reading it per pass was asking a question whose
    // answer this compiler already fixed.
    //
    // It was a `Member` node in the test, which the ordinary emitter has no
    // choice but to lower as a cached property read plus a miss path — per
    // element, forever. Hoisting it is not an optimisation of the read; it
    // removes the read.
    let length = ctx.names.intern("__rts_in_len");
    // `enumerated` is always a fresh internal array: `EnumerateKeys` returns
    // one directly, while the `for-of` join selects either an empty array or
    // the array materialised by `Iterate`. Its length is therefore a compiler
    // fact, not a user property read, and the scalar entry preserves F64 for
    // the loop header.
    let bound = super::expr::call(
        builder,
        ctx,
        crate::runtime::RuntimeOp::ArrayLength,
        &[enumerated],
    )?[0];
    super::binding::declare(builder, scope, ctx, length, bound)?;

    let test = Expr {
        kind: ExprKind::Binary {
            op: crate::syntax::BinaryOp::Less,
            left: Box::new(name(index)),
            right: Box::new(name(length)),
        },
        at,
    };

    let update = Expr {
        kind: ExprKind::Update {
            op: crate::syntax::UpdateOp::Increment,
            position: crate::syntax::UpdatePosition::Postfix,
            target: Box::new(name(index)),
        },
        at,
    };

    // `let k = ks[i];` in front of the body, in a block of its own so the
    // binding is fresh every pass. `target: pattern.clone()` rather than
    // requiring a plain name: `Declare`'s own lowering already knows how to
    // destructure, through `destructure::declare`, so a `for-of` over a
    // pattern is this expansion with nothing extra — the same reasoning that
    // makes `for-in` an ordinary `for` in the first place.
    // `ks[i]` as the operation that asks NOTHING, rather than as an ordinary
    // computed read.
    //
    // `Index` lowers to `GetIndexed`, which earns its cost honestly for a
    // program's own `o[k]`: is the receiver a proxy, is the key a canonical
    // index, is the receiver a typed array, a string, or an object with a
    // property under that name. Every one of those is a question the caller
    // could not have answered.
    //
    // This caller answered all of them by CONSTRUCTION, and that is the whole
    // of what compiling a program before it runs buys: `ks` is the copy
    // `Iterate` just made — a fresh array nothing can name, so not a proxy, not
    // a view, not a string — and `i` is the counter minted above, starting at
    // zero and stopping at the length read off that same array. So the proof
    // happens once, here, instead of per element forever.
    //
    // The tree still says `ks[i]`, because that is what it means. What changes
    // is that the emitter is TOLD the pair is proven — `Ctx::prove_element_read`
    // — and `expr.rs` reads that when it lowers an `Index`. Inventing a node
    // for it would put a construct in the tree no program can write, and
    // spelling it as a call to a name would invent a binding nothing declares.
    let indexed = Expr {
        kind: ExprKind::Index {
            object: Box::new(name(keys)),
            index: Box::new(name(index)),
            optional: false,
        },
        at,
    };
    // What the pattern is bound FROM. The walked form reads the array
    // directly; the stepped form reads a local, because one of its two arms
    // did not read the array at all.
    let held = ctx.names.intern("__rts_of_elem");
    let element = match stepping {
        false => indexed.clone(),
        true => name(held),
    };
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
        // `pattern` here is `Assign`'s: an arbitrary existing place, not a
        // fresh name. `AssignTarget::Pattern` with `AssignOp::Plain` is the
        // one target the language lets a pattern occupy this way, and it is
        // what `for ([a, b] of pairs)` over an existing `a`/`b` needs too.
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
    // The walked form's whole pass is "bind, then run the body"; the stepped
    // form puts the two arms that decide WHERE the element came from in front
    // of it. Both end in the same two statements, which is what keeps the
    // per-pass binding rule stated once.
    let mut passes = match stepping {
        false => Vec::new(),
        true => fetch_element(ctx, at, held, iterator, &test, indexed),
    };
    passes.push(bind);
    // A key DELETED during a `for`-`in` must not be visited, and the keys are a
    // snapshot — collected once, because a cursor into a shape is a mechanism
    // this engine does not have. The snapshot is right in one direction already:
    // a key ADDED during the loop need not be visited, and is not. This guard
    // fixes the other direction: the key is visited if it is still reachable,
    // own or inherited.
    //
    // `BinaryOp::ForInHas` and NOT `BinaryOp::In`, which is what it was. The
    // operator refuses a receiver that is not an object and this loop converts
    // one — `for (const k in "ab")` enumerates two keys — so writing the guard
    // as the operator made a `for`-`in` over a string raise the operator's
    // `TypeError`. That variant's own doc has the rest.
    //
    // Only for `for`-`in`. A `for`-`of` steps a real iterator now, so nothing
    // there is a snapshot to go stale.
    //
    // It costs one `HasProperty` per pass, on a construct that was already the
    // slow path — and the alternative is visiting a property the program has
    // deleted, which is a read of something that is not there.
    match stepping {
        true => passes.push(body.clone()),
        false => passes.push(Stmt {
            kind: StmtKind::If {
                condition: Expr {
                    kind: ExprKind::Binary {
                        op: BinaryOp::ForInHas,
                        left: Box::new(key_expression(pattern, at)),
                        right: Box::new(name(subject_name)),
                    },
                    at,
                },
                then_branch: Box::new(body.clone()),
                else_branch: None,
            },
            at,
        }),
    }
    let inner = Stmt {
        kind: StmtKind::Block(passes),
        at,
    };

    // Both names hold numbers, and this compiler MINTED both: the index starts
    // at a numeric literal here and is only ever incremented by the `update`
    // above, and the bound is an array's `length`. `proven::analyse` cannot see
    // either, because it read the program's tree and these nodes were built
    // after it — so the counter travelled `Tagged` and was guarded on every
    // pass, against a bound that was guarded too.
    //
    // Asserted rather than derived, which is sound for exactly this case and
    // for no other: a name a program could write would be a claim about the
    // program. See `Numeric::prove_minted`.
    //
    // SAVED and restored rather than forgotten, because two nested `for-of`s
    // share the spelling: an inner loop that simply forgot would take the proof
    // out from under the outer one, whose block parameters are already `F64` —
    // and the next store into the outer counter would widen to `Tagged` against
    // them. That is `ImplicitNarrowing`, and it is what a nested `for-in` with
    // `break outer` reported before this was written as a save.
    let index_was_proven = ctx.holds_number(index);
    let length_was_proven = ctx.holds_number(length);
    ctx.prove_minted(index);
    ctx.prove_minted(length);
    // And that `ks[i]` is an element of a proven array at a proven index — the
    // one place in the compiler that can say so. Saved and restored for the
    // same reason the two above are: nested loops share the spelling.
    let outer_element = ctx.prove_element_read(Some((keys, index)));
    // The RUN is deliberately NOT hoisted, and the paragraph that stood here
    // described hoisting it: `ElementsBase` answered where the elements start,
    // `ArrayLength` how many there were, and each element became a bounded load
    // instead of a crossing.
    //
    // It read words the collector had already given away. The address belongs to
    // the `Vec` behind the copy `Iterate` makes, and that copy is a cell like
    // any other — reachable, while this loop runs, from exactly one place: the
    // tagged reference this desugaring binds to `ks`. Hoisting the address
    // REPLACED the last read of that reference, so the copy became unreachable
    // at the top of the first pass; a body that allocated enough collected it,
    // and the loop went on reading a run that by then belonged to something
    // else. Measured on this tree: `for-of` over 90 objects with an allocating
    // body handed back 83 wrong elements, and the same loop over 200 with a body
    // allocating arrays of the same length SEGFAULTED.
    //
    // The machine layer's rule 8 is the one this broke, and it names the
    // mechanism exactly: "root sets are derived from LIVENESS — there is no
    // entry point through which a client could report its own set, because a
    // discipline that must hold at every allocation in every program will not
    // hold." An address is not a reference, liveness cannot see one, and the
    // hoist turned the second into the first.
    //
    // # Why this is not repaired here, and what would repair it
    //
    // Nothing this layer can emit keeps the copy alive. A read of `ks` whose
    // result is unused does not survive to the machine, and there is no way to
    // spell "this value is live across that call" in a layer whose rule 2
    // forbids deciding what is live at a collection. That is exactly the shape
    // rule 2 names: a capability is missing below and the fix belongs below.
    //
    // The capability is a bounded load that takes the run's OWNER as an operand
    // it keeps live — precise stack maps, or an operand the lowering genuinely
    // uses. Until one exists, `ElementAt` is the form, and it is a call per
    // element rather than a load. That is the cost, stated plainly: this is
    // slower, and it is right.
    //
    // # Why the trigger is not what refuses it
    //
    // The defect surfaced only for a loop at MODULE TOP LEVEL whose array no
    // closure captured; the same loop inside a function, and the same loop in a
    // file whose assertions captured the array, answered correctly throughout.
    // Two release binaries built hours apart disagreed about which of 90
    // elements came back wrong. A condition that decides correctness by where a
    // binding happened to be allocated is not a condition to narrow — it is one
    // to remove.
    //
    // The stepped form has NO header test: the two arms above decide when the
    // sequence ended, and each of them says so with a `break`. Leaving the test
    // in the header as well would ask `i < len` twice per pass and end the loop
    // before the iterator was ever stepped, `len` being zero for a source that
    // is stepped rather than walked.
    let header_test = match stepping {
        false => Some(&test),
        true => None,
    };
    // `IteratorClose` on a THROW, as a region built directly rather than as a
    // synthetic `try` statement.
    //
    // The `try` was written first and measured: it closed the iterator, and it
    // also made `for (const x of [1,2,3]) s += x` answer `s === 0`. The cause is
    // not the region — a `try` a PROGRAM writes in the same position works — it
    // is that `protect::emit_try` discards the protected span's SSA bindings at
    // its join, which is sound only because `capture::assigned_under_protection`
    // has already forced those names into memory, and that analysis reads the
    // parse tree before any synthetic statement exists. Proven by flipping one
    // thing: under the patch, the same loop with a `try` the program wrote
    // inside its body answered 6, and so did one whose accumulator any other
    // `try` in scope had touched.
    //
    // This region has no join to restore at. Its handler RE-RAISES, so the block
    // after it is reached from the normal path alone and the bindings the loop
    // made dominate it — `destructure/array.rs::open_close_region` is the same
    // shape, for the same reason, and hit the same defect first.
    // `IteratorClose` on a `return` out of the BODY, and on a `break`/`continue`
    // naming a loop that encloses this one. Both leave without unwinding and
    // without falling out of this loop, so neither the region's handler nor the
    // statement after the loop is reached — which is what the module doc listed
    // as "still not covered", measured against Bun.
    //
    // Each reuses the mechanism the language already has for "run this on the
    // way out", rather than inventing a third: a `return` is routed through a
    // block exactly as `try`/`finally` routes one, and a jump past this loop
    // collects owed bodies exactly as a `finally` entered inside it does. The
    // alternative — wrapping the loop in a synthetic `try`/`finally` — is the
    // one the module doc records as measured and rejected: `emit_try` discards
    // the protected span's SSA bindings at its join.
    //
    // Created BEFORE the region below is opened, so it belongs to whatever
    // encloses this loop: a `return()` that throws while closing then lands in
    // the caller's handler rather than in this loop's, which would be the close
    // being closed. `protect.rs` creates its own returning block in the same
    // position for the same reason.
    let returning = match stepping {
        false => None,
        true => {
            let block = builder.create_block();
            // The parameter exists at CREATION: a jump checks its argument
            // count against the target's parameters, so one added later is a
            // refusal at every `return` that already jumped.
            let held = builder.add_block_param(block, rts_cranelift::repr::Repr::Tagged);
            Some((block, held))
        }
    };
    let closing_region = match stepping {
        false => None,
        true => {
            let after = builder.create_block();
            let protected = builder.create_block();
            builder.jump(protected, &[])?;
            builder.switch_to(protected);
            let handler = builder.create_block();
            builder.open_region(
                vec![rts_cranelift::unwind::Handler {
                    tag: super::protect::JS_THROW,
                    block: handler,
                }],
                None,
            );
            Some((after, handler))
        }
    };
    // Owed to any jump that leaves PAST this loop, recorded at the depth this
    // loop's own frame will occupy — `emit_for` pushes it. `emit_jump_out` runs
    // every owed body whose depth is greater than the target frame's index, so
    // a `break outer` runs this close and a `break`/`continue` of this loop does
    // not: those two leave by the loop's own exit, where the statement below
    // closes it once.
    if stepping {
        let close = close_iterator_stmt(ctx, at, iterator, still_open(iterator, at), false);
        ctx.finally_jumps.push((vec![close], loops.depth()));
    }
    if let Some((block, _)) = returning {
        ctx.finally_returns.push(block);
    }
    let result = emit_for(
        builder,
        scope,
        ctx,
        loops,
        Some(&init),
        header_test,
        Some(&update),
        &inner,
        label,
    );
    if returning.is_some() {
        ctx.finally_returns.pop();
    }
    if stepping {
        ctx.finally_jumps.pop();
    }
    ctx.prove_element_read(outer_element);
    if !index_was_proven {
        ctx.forget_minted(index);
    }
    if !length_was_proven {
        ctx.forget_minted(length);
    }
    // A THROW is the region's, and it is closed here — see the block that opened
    // it, above the loop, for why it is a region and not a synthetic `try`.
    if let Some((after, handler)) = closing_region {
        builder.close_region();
        let reaches = matches!(result, Ok(false));
        // The snapshot is taken AFTER the loop and restored at `after`, which is
        // the opposite of what `protect::emit_try` does and the whole reason
        // this shape is sound: what the normal path carries forward is what the
        // loop produced, not what preceded it.
        let normal = scope.snapshot();
        if reaches {
            builder.jump(after, &[])?;
        }
        // The thrown value arrives as the handler's parameter — the machine's
        // discipline for it, the same as every other handler here.
        let thrown = builder.add_block_param(handler, rts_cranelift::repr::Repr::Tagged);
        builder.switch_to(handler);
        // Unconditional: the handler is reached only by a throw, and a throw
        // means the loop never reached exhaustion, so the iterator is open. The
        // name it reads is the write-once alias, for the SSA reason recorded
        // where that alias is declared.
        let close = close_iterator_stmt(ctx, at, closing, always(at), false);
        let terminated =
            super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &close)?;
        if !terminated {
            builder.throw(super::protect::JS_THROW, thrown);
        }
        // The copy a `return` written in the body reaches. It closes the
        // iterator and then leaves — by returning, or, when this loop is itself
        // inside a `finally` or another stepped loop, by handing the value to
        // the next block out, so the closes and the `finally` bodies run from
        // the inside out. `protect.rs` ends its own returning copy the same way,
        // and the two have to agree because they share the stack.
        //
        // It reads the write-once alias for the same SSA reason the handler
        // does, and closes unconditionally for the same reason: a `return` out
        // of the body means the loop did not reach exhaustion, so the iterator
        // is open by construction.
        if let Some((block, held)) = returning {
            builder.switch_to(block);
            let close = close_iterator_stmt(ctx, at, closing, always(at), false);
            let left = super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &close)?;
            if !left {
                match ctx.finally_returns.last().copied() {
                    Some(outer) => builder.jump(outer, &[held])?,
                    None => builder.ret(&[held]),
                }
            }
        }
        builder.switch_to(after);
        scope.restore(&normal);
    }
    // `IteratorClose` on a `break`, which leaves by falling out of the loop
    // rather than through the region: `it` still holds the iterator, because
    // only exhaustion clears it.
    if stepping && matches!(result, Ok(false)) {
        let close = close_iterator_stmt(ctx, at, iterator, still_open(iterator, at), false);
        super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &close)?;
    }
    scope.leave();
    result
}

/// The array a `for-of` walks, and — declared as `iterator` — what to step once
/// it runs out.
///
/// Exactly one of the two is real for a given source, and the module doc says
/// which and why. What this function owns is the QUESTION: it evaluates the
/// source once, reads its `Symbol.iterator` once, and answers with an array in
/// both cases so that everything downstream — the length read, the proven index,
/// the hoisted run — sees what it has always seen.
fn open_sequence(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    source: ValueId,
    iterator: Name,
    at: Position,
) -> EmitResult<ValueId> {
    let source_name = ctx.names.intern("__rts_of_src");
    super::binding::declare(builder, scope, ctx, source_name, source)?;

    // `"@@iterator"` rather than the global `Symbol.iterator`, which is what
    // `for_await.rs` and `destructure/array.rs` spell. Two reasons, and the
    // second is why this one differs from them: a `for-of` is in almost every
    // program, so resolving a GLOBAL name for it would let `const Symbol = 1`
    // anywhere in scope break every loop under it; and the reserved `@@` space
    // is already how this crate names a symbol key — `class.rs` emits
    // `[Symbol.iterator]() {}` in a class body under exactly this name.
    let symbol_iterator = ctx.names.intern("@@iterator");
    let key = super::property::key_constant(builder, ctx, symbol_iterator);
    let method = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[source, key])?[0];
    let method_name = ctx.names.intern("__rts_of_m");
    super::binding::declare(builder, scope, ctx, method_name, method)?;

    // The array a fresh `[]` would be stepped by. Made here rather than read off
    // a global, so that no name a program can rebind decides which arm a loop
    // takes — see the module doc.
    let none = super::expr::count_constant(builder, 0);
    let empty = super::expr::call(builder, ctx, RuntimeOp::ArrayNew, &[none])?[0];
    let key = super::property::key_constant(builder, ctx, symbol_iterator);
    let walked = super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[empty, key])?[0];
    let walked_name = ctx.names.intern("__rts_of_walked");
    super::binding::declare(builder, scope, ctx, walked_name, walked)?;

    // Whether a STRING may be walked as its list: `TextWalk` answers the list where
    // the string's iterator would step exactly it, and `undefined` where a program
    // changed that protocol -- `rts_core::entry::text_walk`. `typeof src ===
    // "string"` asked this too little, and a replaced `String.prototype
    // [Symbol.iterator]` was never called.
    let text = super::expr::call(builder, ctx, RuntimeOp::TextWalk, &[source])?[0];
    let text_name = ctx.names.intern("__rts_of_text");
    super::binding::declare(builder, scope, ctx, text_name, text)?;

    let steppable = steppable_expr(at, text_name, method_name, walked_name);
    let asked = super::expr::emit_expr(builder, scope, ctx, &steppable)?;
    let asked = super::expr::to_boolean(builder, ctx, asked)?;

    let stepped = builder.create_block();
    let listed = builder.create_block();
    let join = builder.create_block();
    let held = builder.add_block_param(join, UNPROVEN);
    let elements = builder.add_block_param(join, UNPROVEN);
    builder.branch(asked, (stepped, &[]), (listed, &[]))?;

    builder.switch_to(stepped);
    let absent = super::expr::undefined(builder, ctx);
    // `src[Symbol.iterator]()` takes no arguments, and takes the SOURCE as its
    // receiver — which is what makes a class method reached through the
    // prototype chain see its own instance.
    let written = super::expr::count_constant(builder, 0);
    // A call the COMPILER wrote: `[Symbol.iterator]()` has no source spelling,
    // so there is no literal to name, and the operand says so.
    let unnamed = super::expr::name_constant(builder, None);
    let it = super::expr::call(
        builder,
        ctx,
        RuntimeOp::Call,
        &[
            method, source, written, unnamed, absent, absent, absent, absent,
        ],
    )?[0];
    let it = builder.widen(it);
    // The walk gets nothing to walk: `len` is 0, so the arm that reads the array
    // never fires and the array itself is the one already allocated above.
    let empty = builder.widen(empty);
    builder.jump(join, &[it, empty])?;

    builder.switch_to(listed);
    let absent = super::expr::undefined(builder, ctx);
    let materialised = super::expr::call(builder, ctx, RuntimeOp::Iterate, &[source])?[0];
    let materialised = builder.widen(materialised);
    builder.jump(join, &[absent, materialised])?;

    builder.switch_to(join);
    super::binding::declare(builder, scope, ctx, iterator, held)?;
    Ok(elements)
}

/// `m !== walked && text === void 0 && typeof m === "function"` — is this
/// source one that has to be STEPPED rather than copied? `text` is what
/// `TextWalk` answered for it: a list only for a string whose protocol is the
/// primordial one.
///
/// The identity comparison is FIRST, and the order is the only thing about this
/// expression that is a performance decision rather than a semantic one: none of
/// the three has a side effect, so `&&` may hold them in any order, and an array
/// — by a wide margin the most common source — fails the first and evaluates
/// neither of the other two.
fn steppable_expr(at: Position, text: Name, method: Name, walked: Name) -> Expr {
    let is_function = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictEqual,
            left: Box::new(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::TypeOf,
                    operand: Box::new(ident(method, at)),
                },
                at,
            }),
            right: Box::new(text_expr("function", at)),
        },
        at,
    };
    let not_an_array = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictNotEqual,
            left: Box::new(ident(method, at)),
            right: Box::new(ident(walked, at)),
        },
        at,
    };
    let not_text = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictEqual,
            left: Box::new(ident(text, at)),
            right: Box::new(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Void,
                    operand: Box::new(Expr {
                        kind: ExprKind::Literal(crate::syntax::Literal::Number(0.0)),
                        at,
                    }),
                },
                at,
            }),
        },
        at,
    };
    both(both(not_an_array, not_text, at), is_function, at)
}

/// `left && right`.
fn both(left: Expr, right: Expr, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Logical {
            op: LogicalOp::And,
            left: Box::new(left),
            right: Box::new(right),
        },
        at,
    }
}

/// The two arms that put one element in `held`, and the `break` that ends the
/// loop when neither can.
///
/// Written as ordinary statements — `let`, `if`, an assignment, `break` — for
/// the reason `destructure/array.rs` gives for the same choice: `if`'s own
/// lowering already merges what each arm did to a binding, and building the
/// merge here would be that rule written a second time.
fn fetch_element(
    ctx: &mut Ctx,
    at: Position,
    held: Name,
    iterator: Name,
    within_bounds: &Expr,
    indexed: Expr,
) -> Vec<Stmt> {
    use crate::syntax::{Binding as SyntaxBinding, BindingKind, StmtKind};

    let step = ctx.names.intern("__rts_of_step");
    let next_name = ctx.names.intern("next");
    let done_name = ctx.names.intern("done");
    let value_name = ctx.names.intern("value");

    let declare_held = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(held),
                value: None,
                claim: None,
            }],
        },
        at,
    };

    // `const s = it.next(); if (s.done) { it = undefined; break; } e = s.value;`
    let declare_step = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Const,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(step),
                // Wrapped in the operator no program can write, which answers
                // the record unchanged and raises the `TypeError`
                // `IteratorNext` raises for a record that is not an object.
                // Without it `s.done` reads `undefined` off a primitive,
                // `undefined` is never true, and the loop does not end — a
                // HANG rather than a wrong answer, which is why the check is
                // here and not left to the reads below.
                value: Some(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::IteratorResult,
                        operand: Box::new(Expr {
                            kind: ExprKind::Call {
                                callee: Box::new(member_expr(ident(iterator, at), next_name, at)),
                                arguments: Vec::new(),
                                optional: false,
                            },
                            at,
                        }),
                    },
                    at,
                }),
                claim: None,
            }],
        },
        at,
    };
    let finish = Stmt {
        kind: StmtKind::If {
            condition: member_expr(ident(step, at), done_name, at),
            then_branch: Box::new(Stmt {
                kind: StmtKind::Block(vec![
                    assign_stmt(ident(iterator, at), undefined_expr(at), at),
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
    let take_value = assign_stmt(
        ident(held, at),
        member_expr(ident(step, at), value_name, at),
        at,
    );
    let from_iterator = Stmt {
        kind: StmtKind::Block(vec![declare_step, finish, take_value]),
        at,
    };

    // `else if (it === undefined) break;` — the walked form's own end, and the
    // only place a `for-of` over an array leaves the loop.
    let exhausted = Stmt {
        kind: StmtKind::If {
            condition: Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::StrictEqual,
                    left: Box::new(ident(iterator, at)),
                    right: Box::new(undefined_expr(at)),
                },
                at,
            },
            then_branch: Box::new(Stmt {
                kind: StmtKind::Break(None),
                at,
            }),
            else_branch: Some(Box::new(from_iterator)),
        },
        at,
    };

    let choose = Stmt {
        kind: StmtKind::If {
            condition: within_bounds.clone(),
            then_branch: Box::new(Stmt {
                kind: StmtKind::Block(vec![assign_stmt(ident(held, at), indexed, at)]),
                at,
            }),
            else_branch: Some(Box::new(exhausted)),
        },
        at,
    };

    vec![declare_held, choose]
}

/// The key a `for`-`in` head just bound, as an expression.
///
/// A plain name in the head is the only shape a `for`-`in` key can take that
/// this guard can ask about — a destructuring head over a KEY is legal grammar
/// and no program writes it, so it is left unguarded rather than approximated.
fn key_expression(pattern: &Pattern, at: Position) -> Expr {
    match pattern {
        Pattern::Name(name) => ident(*name, at),
        // Nothing to name, so the guard is `true` and the snapshot stands.
        _ => Expr {
            kind: ExprKind::Literal(Literal::Boolean(true)),
            at,
        },
    }
}
