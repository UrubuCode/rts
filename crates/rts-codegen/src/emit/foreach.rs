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
//! # What `IteratorClose` here does NOT cover
//!
//! A `break`, and nothing else. A `return` out of the body, a `throw` through
//! it, or a `break` to a LABEL on an enclosing loop all leave by a path that
//! never reaches the statement after this loop, so `return()` is not called.
//! Covering those needs a `try`/`finally`-shaped region around the loop, and
//! `destructure/array.rs`'s `apply_default_stepwise` records what a synthetic
//! `try` built during emission costs here: `capture.rs` decided which names
//! need heap storage before any of these statements existed.

use rts_cranelift::fault::Position;
use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::loops::{Loops, emit_for};
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal, LogicalOp, Pattern, Stmt, UnaryOp};
use crate::values::Singleton;

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
    let length_key = ctx.names.intern("length");
    let bound = super::property::emit_read(builder, ctx, enumerated, length_key)?;
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
    // fixes the other direction, and `k in o` is exactly the question — the key
    // is visited if it is still reachable, own or inherited.
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
                        op: BinaryOp::In,
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
    // And the RUN itself, asked once and held for the loop: where the elements
    // start, and how many there are. With those, each element is a bounded load
    // instead of a crossing.
    //
    // Refused for a body that PARKS. `frame::resumable_form` rewrites a
    // suspending function around every suspension, so a value defined here and
    // read after a `yield` is not the value it was — the same reason
    // `function.rs` withholds the throw-flag address from such a body. Those
    // loops keep `ElementAt`, which is a call and therefore survives the
    // rewrite.
    //
    // The base is an ADDRESS INTO A `Vec`, and it is stable only because this
    // array is the copy `Iterate` made: no program can name it, and this loop
    // only reads. `array::elements_base` states that contract from the other
    // side, and this is the caller it names.
    //
    // And refused a second time when the bound is not a PROVEN double. The
    // count is the machine's bound for a load, and `to_int32` takes one — a
    // `length` read that stayed generic has no such proof, and asking anyway is
    // `WrongDomain` at emission, which refuses the whole program rather than
    // this loop. Rule 5 of this crate's README is the shape: what cannot be
    // proven becomes generic, visibly. Here that means keeping `ElementAt`.
    // # Why the bound's representation is no longer the question
    //
    // It was `builder.repr_of(bound) == Repr::F64`, and that could never hold:
    // `bound` is what `property::emit_read` answered for `length`, which is
    // `emit::UNPROVEN` — `Repr::Tagged` — always and by construction. So no run
    // was ever hoisted, `ElementsBase` was never called, and `ElementLoad` was
    // never emitted. Verified with `rts ir` over the twelve `bench/*.ts` and
    // every `array_*`/`for_of*` test: **59 files, zero hoists**, 2026-08-23.
    //
    // The fix is not to narrow the bound. Narrowing is only reachable through a
    // guard (machine rule 11), and a guard's failure path would need a second
    // copy of the whole loop body — against a throw check that already accounts
    // for ~32% of every instruction emitted.
    //
    // So the count is ASKED FOR in the form the instruction takes:
    // `ElementsCount` answers a proven `I32`. Machine rule 10 from the other
    // side — an operation does not take both a proven and a generic operand, so
    // the way to get a proven one is a separate operation. The loop's own test
    // keeps using the generic `bound`; the two are read from one array before
    // the loop and cannot disagree.
    let hoistable = !super::suspends::body_suspends(std::slice::from_ref(body));
    let outer_run = match hoistable {
        false => ctx.set_element_run(None),
        true => {
            let base = super::expr::call(builder, ctx, crate::runtime::RuntimeOp::ElementsBase, &[enumerated])?[0];
            let count = super::expr::call(builder, ctx, crate::runtime::RuntimeOp::ElementsCount, &[enumerated])?[0];
            ctx.set_element_run(Some((base, count)))
        }
    };
    // The stepped form has NO header test: the two arms above decide when the
    // sequence ended, and each of them says so with a `break`. Leaving the test
    // in the header as well would ask `i < len` twice per pass and end the loop
    // before the iterator was ever stepped, `len` being zero for a source that
    // is stepped rather than walked.
    let header_test = match stepping {
        false => Some(&test),
        true => None,
    };
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
    // The run is put BACK, and it is a pair with the line above rather than
    // tidiness: a `for`-`of` inside a `for`-`of` replaces the outer loop's base
    // and count, and without this the outer body would go on reading from the
    // INNER array's storage with the inner array's bound.
    //
    // Latent rather than observable today, and said that way on purpose. The
    // only proven element read a body has is the one the desugaring emits at
    // its top, before any nested loop runs — so nothing reaches the stale pair
    // yet. That is a property of what `is_proven_element` currently admits, not
    // a property of this loop, and the failure it would become is a load from
    // the wrong array with no crash to announce it.
    ctx.set_element_run(outer_run);
    ctx.prove_element_read(outer_element);
    if !index_was_proven {
        ctx.forget_minted(index);
    }
    if !length_was_proven {
        ctx.forget_minted(length);
    }
    // `IteratorClose`, and the reason it can be a plain statement after the loop
    // rather than a cleanup region: the only early exit that reaches here is a
    // `break` out of this loop, and a `break` leaves `it` holding the iterator
    // because only exhaustion clears it. The module doc lists what that misses.
    if stepping && matches!(result, Ok(false)) {
        let close = close_iterator_stmt(ctx, at, iterator, still_open(iterator, at), false);
        super::stmt::emit_stmt(builder, scope, ctx, &mut Loops::default(), &close)?;
    }
    scope.leave();
    result
}

/// `it !== undefined` — the iterator was abandoned rather than exhausted.
///
/// The flag IS the binding: exhaustion is the one thing that clears it, so
/// nothing else has to be kept agreeing with it.
pub(super) fn still_open(iterator: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::StrictNotEqual,
            left: Box::new(ident(iterator, at)),
            right: Box::new(undefined_expr(at)),
        },
        at,
    }
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

    let steppable = steppable_expr(at, source_name, method_name, walked_name);
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
    let it = super::expr::call(
        builder,
        ctx,
        RuntimeOp::Call,
        &[method, source, written, absent, absent, absent, absent],
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

/// `m !== walked && typeof src !== "string" && typeof m === "function"` — is
/// this source one that has to be STEPPED rather than copied?
///
/// The identity comparison is FIRST, and the order is the only thing about this
/// expression that is a performance decision rather than a semantic one: none of
/// the three has a side effect, so `&&` may hold them in any order, and an array
/// — by a wide margin the most common source — fails the first and evaluates
/// neither of the other two.
fn steppable_expr(at: Position, source: Name, method: Name, walked: Name) -> Expr {
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
            op: BinaryOp::StrictNotEqual,
            left: Box::new(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::TypeOf,
                    operand: Box::new(ident(source, at)),
                },
                at,
            }),
            right: Box::new(text_expr("string", at)),
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
                value: Some(Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(member_expr(ident(iterator, at), next_name, at)),
                        arguments: Vec::new(),
                        optional: false,
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

/// `if (<guard>) { if (typeof it.return === "function") { it.return(); } }` —
/// `IteratorClose`, as far as this engine expresses it.
///
/// One home for three callers — this loop, `for_await.rs`, and
/// `destructure/array.rs` — because the rule has three parts a second copy
/// would get differently: `return()` is called only when the source has not
/// already reported `done`, only when it exists and is callable (a plain
/// iterator-like object with a bare `next()` has none, and that is legal), and
/// `for await` must AWAIT what it answers, which is what makes an async
/// `return()` that suspends finish before the loop's caller carries on.
pub(super) fn close_iterator_stmt(
    ctx: &mut Ctx,
    at: Position,
    iterator: Name,
    guard: Expr,
    awaited: bool,
) -> Stmt {
    use crate::syntax::StmtKind;

    let return_name = ctx.names.intern("return");
    let return_member = member_expr(ident(iterator, at), return_name, at);
    // `it.return?.()` — an OPTIONAL call, which is `GetMethod` exactly.
    //
    // It was `typeof it.return === "function"` followed by `it.return()`, and
    // the member expression was CLONED between the two — so the property was
    // read twice. For an ordinary method that is invisible; for a `return`
    // defined as a getter it is not, and an iterator whose `return` counts its
    // own reads saw two where the language performs one.
    //
    // The optional form also gets the refusal right without a second test: it
    // skips `null` and `undefined`, and throws a `TypeError` for anything else
    // that is not callable — which is what `GetMethod` says and what the
    // `typeof` guard silently swallowed. A plain iterator-like object with a
    // bare `next()` and no `return` still closes without calling anything,
    // which is the case the guard existed for.
    let called = Expr {
        kind: ExprKind::Call {
            callee: Box::new(return_member),
            arguments: Vec::new(),
            optional: true,
        },
        at,
    };
    // Wrapped in the chain BOUNDARY, which is what makes the `optional` flag do
    // anything at all. Without it the flag says "this link may skip" with
    // nowhere to skip TO, and the call happened regardless — six fixtures went
    // from passing to `TypeError: it.return is not a function` before this line
    // existed, because every iterator without a `return` was suddenly called.
    // `ExprKind::Chain`'s own documentation says the flag on a link is only half
    // of it; this is the other half.
    let called = Expr {
        kind: ExprKind::Chain(Box::new(called)),
        at,
    };
    let called = match awaited {
        false => called,
        true => Expr {
            kind: ExprKind::Await(Box::new(called)),
            at,
        },
    };
    let inner = Stmt {
        kind: StmtKind::Expr(called),
        at,
    };
    Stmt {
        kind: StmtKind::If {
            condition: guard,
            then_branch: Box::new(inner),
            else_branch: None,
        },
        at,
    }
}

/// `name`, as a synthetic identifier expression.
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

fn ident(name: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Ident(name),
        at,
    }
}

/// `object.property`, as a synthetic expression.
fn member_expr(object: Expr, property: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Member {
            object: Box::new(object),
            property,
            optional: false,
        },
        at,
    }
}

/// `place = value;`, as a synthetic statement.
fn assign_stmt(place: Expr, value: Expr, at: Position) -> Stmt {
    use crate::syntax::{AssignOp, AssignTarget, StmtKind};

    Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Assign {
                target: AssignTarget::Place(Box::new(place)),
                value: Box::new(value),
                op: AssignOp::Plain,
            },
            at,
        }),
        at,
    }
}

/// `undefined`, as a synthetic expression — the language's own singleton
/// literal rather than a name, which nothing here binds.
fn undefined_expr(at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
        at,
    }
}

/// A string literal, as a synthetic expression.
fn text_expr(text: &str, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::String(text.into())),
        at,
    }
}
