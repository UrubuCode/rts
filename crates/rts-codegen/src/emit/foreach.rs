//! `for (k in o)` — walking an object's keys.
//!
//! Its own file for the same reason `switch.rs` is: `loops.rs` had reached the
//! thousand-line ceiling and this is part of what pushed it there.

use rts_cranelift::ir::FuncBuilder;

use super::loops::{Loops, emit_for};
use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::syntax::{Expr, Pattern, Stmt};

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
        AssignOp, AssignTarget, Binding as SyntaxBinding, BindingKind, ExprKind, ForEachTarget,
        StmtKind,
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
    let name = |of: Name| Expr {
        kind: ExprKind::Ident(of),
        at,
    };
    let number = |value: f64| Expr {
        kind: ExprKind::Literal(crate::syntax::Literal::Number(value)),
        at,
    };

    // The keys are emitted HERE rather than as a node in the expansion,
    // because there is no name a program could write that means "the runtime
    // operation". Bound in a scope of its own so the loop below sees an
    // ordinary binding and needs to know nothing about where it came from.
    scope.enter();
    let enumerated = super::expr::emit_expr(builder, scope, ctx, subject)?;
    let enumerated = super::expr::call(
        builder,
        ctx,
        over,
        &[enumerated],
    )?[0];
    super::binding::declare(builder, scope, ctx, keys, enumerated)?;

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
    let element = Expr {
        kind: ExprKind::Index {
            object: Box::new(name(keys)),
            index: Box::new(name(index)),
            optional: false,
        },
        at,
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
    let inner = Stmt {
        kind: StmtKind::Block(vec![bind, body.clone()]),
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
    let hoistable = !super::suspends::body_suspends(std::slice::from_ref(body))
        && builder.repr_of(bound) == rts_cranelift::repr::Repr::F64;
    let outer_run = match hoistable {
        false => ctx.set_element_run(None),
        true => {
            let base = super::expr::call(builder, ctx, crate::runtime::RuntimeOp::ElementsBase, &[enumerated])?[0];
            let count = builder.to_int32(bound)?;
            ctx.set_element_run(Some((base, count)))
        }
    };
    let result = emit_for(
        builder,
        scope,
        ctx,
        loops,
        Some(&init),
        Some(&test),
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
    scope.leave();
    result
}
