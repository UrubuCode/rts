//! `IteratorClose`, and the handful of synthetic nodes the loops that need it
//! are built out of.
//!
//! # Why this is not in `foreach.rs`
//!
//! Three loops owe an iterator its `return()` — `for`-`of`, `for await`, and an
//! array pattern over a source that steps — and the rule has three parts a
//! second copy would get differently: `return()` is called only when the source
//! has not already reported `done`, only when it exists and is callable (a plain
//! iterator-like object with a bare `next()` has none, and that is legal), and
//! `for await` must AWAIT what it answers, which is what makes an async
//! `return()` that suspends finish before the loop's caller carries on.
//!
//! The synthetic-expression helpers travel with it because the close statement
//! is built out of them and so is every other statement those expansions mint.
//! They are here rather than in `foreach.rs` for the reason that file's own
//! header gives about `switch.rs`: it had reached the thousand-line ceiling.

use rts_cranelift::fault::Position;

use super::Ctx;
use crate::names::Name;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal, Stmt};
use crate::values::Singleton;

/// `true`, as the guard for a close that has already been decided.
pub(super) fn always(at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Boolean(true)),
        at,
    }
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
pub(super) fn ident(name: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Ident(name),
        at,
    }
}

/// `object.property`, as a synthetic expression.
pub(super) fn member_expr(object: Expr, property: Name, at: Position) -> Expr {
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
pub(super) fn assign_stmt(place: Expr, value: Expr, at: Position) -> Stmt {
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
pub(super) fn undefined_expr(at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
        at,
    }
}

/// A string literal, as a synthetic expression.
pub(super) fn text_expr(text: &str, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::String(text.into())),
        at,
    }
}
