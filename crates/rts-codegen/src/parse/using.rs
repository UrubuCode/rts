//! `using` and `await using`, desugared to the `try`/`finally` the engine has.
//!
//! # Why here and not in emission
//!
//! A disposal is a `finally` whose body is a call, and everything a `finally`
//! needs -- the cleanup copy on every exit (fall-through, `return`, `break`,
//! `throw`), the handler shape when the body suspends -- `emit_try` already
//! does. Emitting a second region kind for `using` would be a second copy of
//! that machinery that has to agree with the first. So the statement list is
//! rewritten before it reaches emission, and `StmtKind::Using` survives only for
//! the shapes this does not cover (a pattern), which emission still refuses by
//! name.
//!
//! # The shape
//!
//! `using x = e; rest` becomes
//!
//! ```text
//! const x = e;
//! let %dispose;
//! if (x !== null && x !== undefined) {
//!     %dispose = x[Symbol.dispose];
//!     if (typeof %dispose !== "function") throw new TypeError(...);
//! }
//! try { rest } finally { if (%dispose !== undefined) %dispose.call(x); }
//! ```
//!
//! The lookup and the check sit OUTSIDE the `try`, as the specification has
//! them: a non-disposable throws at the declaration, and nothing is disposed
//! for a binding that never got one. Everything after the declaration is inside
//! the `try`, so disposal runs after it and -- because each later `using` nests
//! inside the earlier one's `try` -- in reverse declaration order.
//!
//! `%dispose` is not a spelling a program can write, so it cannot collide, and
//! the nested ones shadow rather than clash: each lives in its own `try` body.
//!
//! # What this does not do
//!
//! When the body threw AND a disposer throws, the specification wraps both in a
//! `SuppressedError`. Here the disposer's throw replaces the pending one, which
//! is what a plain `finally` does.

use rts_cranelift::fault::Position;

use super::Cx;
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Binding, BindingKind, Expr, ExprKind, Literal, LogicalOp,
    Pattern, Spreadable, Stmt, StmtKind, UnaryOp,
};
use crate::values::Singleton;

/// Rewrites every `using` in a statement list into a guarded `try`/`finally`
/// around the statements that follow it.
pub(super) fn lower(cx: &mut Cx, list: Vec<Stmt>) -> Vec<Stmt> {
    let Some(index) = list.iter().position(is_lowerable) else {
        return list;
    };
    let mut list = list;
    let rest = list.split_off(index + 1);
    let using = list.pop().expect("the position found it");
    let StmtKind::Using { bindings, is_async } = using.kind else {
        unreachable!("`is_lowerable` matched a `Using`")
    };
    let at = using.at;
    // The tail first, so the innermost binding is wrapped first and the
    // outermost last: that nesting is what makes disposal run in reverse.
    let mut inner = lower(cx, rest);
    for binding in bindings.into_iter().rev() {
        inner = wrap(cx, binding, is_async, at, inner);
    }
    list.extend(inner);
    list
}

/// Whether this is a `using` this module can express: every binding is a plain
/// name with an initialiser.
fn is_lowerable(statement: &Stmt) -> bool {
    matches!(&statement.kind, StmtKind::Using { bindings, .. }
        if bindings.iter().all(|b| matches!(b.target, Pattern::Name(_)) && b.value.is_some()))
}

fn expr(kind: ExprKind, at: Position) -> Expr {
    Expr::new(kind, at)
}

fn ident(name: Name, at: Position) -> Expr {
    expr(ExprKind::Ident(name), at)
}

fn text(cx: &mut Cx, value: &str, at: Position) -> Expr {
    let _ = cx;
    expr(ExprKind::Literal(Literal::String(value.into())), at)
}

fn singleton(value: Singleton, at: Position) -> Expr {
    expr(ExprKind::Literal(Literal::Singleton(value)), at)
}

fn member(object: Expr, cx: &mut Cx, property: &str, at: Position) -> Expr {
    let property = cx.names.intern(property);
    expr(
        ExprKind::Member {
            object: Box::new(object),
            property,
            optional: false,
        },
        at,
    )
}

fn strict(op: BinaryOp, left: Expr, right: Expr, at: Position) -> Expr {
    expr(
        ExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        at,
    )
}

/// `body` wrapped in the declaration, lookup, and `try`/`finally` for ONE
/// binding.
fn wrap(cx: &mut Cx, binding: Binding, is_async: bool, at: Position, body: Vec<Stmt>) -> Vec<Stmt> {
    let Pattern::Name(name) = binding.target else {
        unreachable!("`is_lowerable` checked the target")
    };
    // Scopes are keyed by the position of the statement that opens them, so two
    // bindings of one declaration -- which share `at` -- must not share a `try`
    // or a block position. An initialiser starts somewhere only it does.
    let site = binding.value.as_ref().map_or(at, |value| value.at);
    let slot = cx.names.intern("%dispose");
    let symbol = cx.names.intern("Symbol");
    let type_error = cx.names.intern("TypeError");

    // `Symbol.dispose`, or `Symbol.asyncDispose ?? Symbol.dispose`.
    let lookup = |cx: &mut Cx, key: &str| {
        let symbol = ident(symbol, at);
        let key = member(symbol, cx, key, at);
        expr(
            ExprKind::Index {
                object: Box::new(ident(name, at)),
                index: Box::new(key),
                optional: false,
            },
            at,
        )
    };
    let method = if is_async {
        let first = lookup(cx, "asyncDispose");
        let second = lookup(cx, "dispose");
        expr(
            ExprKind::Logical {
                op: LogicalOp::Coalesce,
                left: Box::new(first),
                right: Box::new(second),
            },
            at,
        )
    } else {
        lookup(cx, "dispose")
    };

    let declare = Stmt::new(
        StmtKind::Declare {
            kind: BindingKind::Const,
            bindings: vec![Binding {
                claim: binding.claim,
                target: Pattern::Name(name),
                value: binding.value,
            }],
        },
        at,
    );
    let slot_decl = Stmt::new(
        StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![Binding {
                claim: None,
                target: Pattern::Name(slot),
                value: None,
            }],
        },
        at,
    );

    let assign = Stmt::new(
        StmtKind::Expr(expr(
            ExprKind::Assign {
                op: AssignOp::Plain,
                target: AssignTarget::Place(Box::new(ident(slot, at))),
                value: Box::new(method),
            },
            at,
        )),
        at,
    );
    let message = text(
        cx,
        if is_async {
            "value is not async disposable"
        } else {
            "value is not disposable"
        },
        at,
    );
    let refuse = Stmt::new(
        StmtKind::If {
            condition: strict(
                BinaryOp::StrictNotEqual,
                expr(
                    ExprKind::Unary {
                        op: UnaryOp::TypeOf,
                        operand: Box::new(ident(slot, at)),
                    },
                    at,
                ),
                text(cx, "function", at),
                at,
            ),
            then_branch: Box::new(Stmt::new(
                StmtKind::Throw(expr(
                    ExprKind::New {
                        callee: Box::new(ident(type_error, at)),
                        arguments: vec![Spreadable::Single(message)],
                    },
                    at,
                )),
                at,
            )),
            else_branch: None,
        },
        at,
    );
    let present = expr(
        ExprKind::Logical {
            op: LogicalOp::And,
            left: Box::new(strict(
                BinaryOp::StrictNotEqual,
                ident(name, at),
                singleton(Singleton::Null, at),
                at,
            )),
            right: Box::new(strict(
                BinaryOp::StrictNotEqual,
                ident(name, at),
                singleton(Singleton::Undefined, at),
                at,
            )),
        },
        at,
    );
    let check = Stmt::new(
        StmtKind::If {
            condition: present,
            then_branch: Box::new(Stmt::new(StmtKind::Block(vec![assign, refuse]), site)),
            else_branch: None,
        },
        at,
    );

    // `%dispose.call(x)`, awaited for `await using`.
    let call_callee = member(ident(slot, at), cx, "call", at);
    let mut call = expr(
        ExprKind::Call {
            callee: Box::new(call_callee),
            arguments: vec![Spreadable::Single(ident(name, at))],
            optional: false,
        },
        at,
    );
    if is_async {
        call = expr(ExprKind::Await(Box::new(call)), at);
    }
    let dispose = Stmt::new(
        StmtKind::If {
            condition: strict(
                BinaryOp::StrictNotEqual,
                ident(slot, at),
                singleton(Singleton::Undefined, at),
                at,
            ),
            then_branch: Box::new(Stmt::new(StmtKind::Expr(call), at)),
            else_branch: None,
        },
        at,
    );

    let guarded = Stmt::new(
        StmtKind::Try {
            body,
            catch: None,
            finally: Some(vec![dispose]),
        },
        site,
    );
    vec![declare, slot_decl, check, guarded]
}
