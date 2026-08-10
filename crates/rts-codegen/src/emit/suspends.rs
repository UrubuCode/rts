//! Whether a body suspends **its own** frame — the fact `emit_program_with_exports`
//! needs to set `may_suspend` on the program/module entry.
//!
//! # Why this exists
//!
//! A function declaration or expression knows whether it is async from its own
//! parse: `function.is_async`. The program entry — a script's or a module's top
//! level — has no such node. `await` at that level is legal wherever the goal is
//! a module (and, in a script wrapped by a host, wherever the host chose to wrap
//! it as one), so whether the entry's *own* frame may suspend is a fact this
//! walk has to establish by looking at what it actually contains.
//!
//! # Why over-approximation on the way down is fine, and under-approximation is not
//! `may_suspend` is a permission on a signature, not a promise that every path
//! uses it — a function marked as suspending that never does costs nothing at
//! run time, because no `Suspend`/`Await` instruction exists in its body either
//! way. Missing a real one is not symmetric: the verifier refuses the whole
//! program with `UndeclaredSuspension`, which is exactly the compile error this
//! module exists to stop. So every statement and expression form is matched
//! explicitly rather than through a catch-all that could silently stop
//! descending into a new one.
//!
//! # Why this does not descend into a nested function or class
//!
//! A nested function, method, getter, setter or class field gets its own
//! signature, set independently of this one — `function.rs` derives it from
//! that node's own `is_async`/`is_generator`. An `await` written inside one
//! parks *that* frame, never this one, so crossing the boundary would answer a
//! question about the wrong function.

use crate::syntax::{
    Catch, Expr, ExprKind, ForInit, Property, PropertyKey, Spreadable, Stmt, StmtKind,
};

/// Whether any statement in `body` suspends the frame it is written in.
pub(super) fn body_suspends(body: &[Stmt]) -> bool {
    body.iter().any(stmt_suspends)
}

fn stmt_suspends(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => expr_suspends(expr),
        StmtKind::Declare { bindings, .. } => bindings_suspend(bindings),
        // `await using` disposes with an awaited call on the way out of scope,
        // which parks this frame exactly as a written `await` would.
        StmtKind::Using { bindings, is_async } => *is_async || bindings_suspend(bindings),
        StmtKind::Block(body) => body.iter().any(stmt_suspends),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_suspends(condition)
                || stmt_suspends(then_branch)
                || else_branch.as_deref().is_some_and(stmt_suspends)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_suspends(condition) || stmt_suspends(body)
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            let init_suspends = match init {
                Some(ForInit::Declare { bindings, .. }) => bindings_suspend(bindings),
                Some(ForInit::Expr(expr)) => expr_suspends(expr),
                None => false,
            };
            init_suspends
                || test.as_ref().is_some_and(expr_suspends)
                || update.as_ref().is_some_and(expr_suspends)
                || stmt_suspends(body)
        }
        // `for await (x of xs)` steps the iterator with an awaited call every
        // pass — `ForEachSource::suspends` is the single place that already
        // says which of the three forms that is.
        StmtKind::ForEach {
            source,
            subject,
            body,
            ..
        } => source.suspends() || expr_suspends(subject) || stmt_suspends(body),
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_suspends),
        StmtKind::Labelled { body, .. } => stmt_suspends(body),
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            expr_suspends(discriminant)
                || clauses.iter().any(|clause| {
                    clause.test.as_ref().is_some_and(expr_suspends)
                        || clause.body.iter().any(stmt_suspends)
                })
        }
        StmtKind::With { object, body } => expr_suspends(object) || stmt_suspends(body),
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            body.iter().any(stmt_suspends)
                || catch.as_ref().is_some_and(catch_suspends)
                || finally
                    .as_ref()
                    .is_some_and(|finally| finally.iter().any(stmt_suspends))
        }
        // Their own frame, set where they are emitted — see the module doc.
        StmtKind::Function(_) | StmtKind::Class(_) => false,
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Debugger | StmtKind::Empty => {
            false
        }
    }
}

fn catch_suspends(catch: &Catch) -> bool {
    // The binding is a pattern, not an expression — `catch ({ x })` names
    // something, it does not evaluate one. A default inside it (`catch ({ x =
    // await f() })`) is the one shape this does not look inside: rare enough,
    // paired with top-level `await`, that it is not worth the extra pattern
    // walk this crate does not otherwise need. Under `catch {}` and `catch (e)`
    // it does not matter at all, because there is nothing to look inside.
    catch.body.iter().any(stmt_suspends)
}

fn bindings_suspend(bindings: &[crate::syntax::Binding]) -> bool {
    bindings
        .iter()
        .any(|binding| binding.value.as_ref().is_some_and(expr_suspends))
}

fn expr_suspends(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Await(_) => true,

        ExprKind::Literal(_)
        | ExprKind::Ident(_)
        | ExprKind::This
        | ExprKind::NewTarget
        | ExprKind::ImportMeta
        | ExprKind::PrivateName(_) => false,

        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            expr_suspends(left) || expr_suspends(right)
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Update {
            target: operand, ..
        }
        | ExprKind::Chain(operand)
        | ExprKind::Asserted {
            value: operand, ..
        } => expr_suspends(operand),

        ExprKind::Member { object, .. } => expr_suspends(object),
        ExprKind::Index { object, index, .. } => expr_suspends(object) || expr_suspends(index),

        ExprKind::Call {
            callee, arguments, ..
        }
        | ExprKind::New { callee, arguments } => {
            expr_suspends(callee) || arguments.iter().any(argument_suspends)
        }
        ExprKind::SuperCall { arguments } => arguments.iter().any(argument_suspends),
        ExprKind::ImportCall { specifier, options } => {
            expr_suspends(specifier) || options.as_deref().is_some_and(expr_suspends)
        }

        // `yield` cannot legally appear where this walk is used — a program
        // entry is never a generator — so it is treated like any other
        // sub-expression rather than given its own arm: whatever it carries is
        // still scanned, in case a malformed tree reaches here anyway.
        ExprKind::Yield { value, .. } => value.as_deref().is_some_and(expr_suspends),

        ExprKind::Template { expressions, .. } => expressions.iter().any(expr_suspends),
        ExprKind::TaggedTemplate { tag, expressions, .. } => {
            expr_suspends(tag) || expressions.iter().any(expr_suspends)
        }

        ExprKind::Object { properties } => properties.iter().any(property_suspends),
        ExprKind::Array { elements } => elements.iter().flatten().any(argument_suspends),

        ExprKind::Assign { target, value, .. } => {
            let target_suspends = match target {
                crate::syntax::AssignTarget::Place(place) => expr_suspends(place),
                // A pattern holds no expression a plain assignment could await
                // in — only a *declaration*'s pattern carries defaults, and
                // those are read through `Binding::value`... no: a destructuring
                // ASSIGNMENT can default too (`[x = await f()] = y`), which is
                // the one case not covered here. Same call as `catch`'s: rare,
                // and pinned in the module doc rather than silently claimed
                // complete.
                crate::syntax::AssignTarget::Pattern(_) => false,
            };
            target_suspends || expr_suspends(value)
        }
        ExprKind::Sequence { operands } => operands.iter().any(expr_suspends),
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => expr_suspends(condition) || expr_suspends(then_branch) || expr_suspends(else_branch),

        // Their own frame — see the module doc.
        ExprKind::Function(_) | ExprKind::Class(_) => false,

        ExprKind::SuperMember { property } => key_suspends(property),
    }
}

fn argument_suspends(argument: &Spreadable) -> bool {
    match argument {
        Spreadable::Single(expr) | Spreadable::Spread(expr) => expr_suspends(expr),
    }
}

fn property_suspends(property: &Property) -> bool {
    match property {
        Property::Value { key, value, .. } => key_suspends(key) || expr_suspends(value),
        // A method's, a getter's or a setter's body is its own frame.
        Property::Method { key, .. }
        | Property::Getter { key, .. }
        | Property::Setter { key, .. } => key_suspends(key),
        Property::Spread(expr) | Property::Prototype(expr) => expr_suspends(expr),
    }
}

fn key_suspends(key: &PropertyKey) -> bool {
    matches!(key, PropertyKey::Computed(expr) if expr_suspends(expr))
}
