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
    use crate::syntax::{Binding as SyntaxBinding, BindingKind, ExprKind, ForEachTarget, StmtKind};

    let crate::syntax::ForEachTarget::Declare {
        target: pattern, ..
    } = target
    else {
        // `for (x in o)` writes to something that already exists, once per
        // pass, with no fresh binding at all. A different rule, and one that
        // matters as soon as a closure is made in the body.
        //
        // This said `unreachable!` for the third case until there was one. A
        // panic in the compiler is what a closed match is worth when the thing
        // it was closed over opens — so both remaining cases are named.
        return match target {
            ForEachTarget::Assign(_) => super::expr::gap("`for-in` writing to an existing binding"),
            ForEachTarget::Dispose { .. } => {
                super::expr::gap("`using` in a for-head, which needs `Symbol.dispose`")
            }
            ForEachTarget::Declare { .. } => unreachable!("matched above"),
        };
    };
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

    let test = Expr {
        kind: ExprKind::Binary {
            op: crate::syntax::BinaryOp::Less,
            left: Box::new(name(index)),
            right: Box::new(Expr {
                kind: ExprKind::Member {
                    object: Box::new(name(keys)),
                    property: ctx.names.intern("length"),
                    optional: false,
                },
                at,
            }),
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
    let bind = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: pattern.clone(),
                value: Some(Expr {
                    kind: ExprKind::Index {
                        object: Box::new(name(keys)),
                        index: Box::new(name(index)),
                        optional: false,
                    },
                    at,
                }),
                claim: None,
            }],
        },
        at,
    };
    let inner = Stmt {
        kind: StmtKind::Block(vec![bind, body.clone()]),
        at,
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
    scope.leave();
    result
}
