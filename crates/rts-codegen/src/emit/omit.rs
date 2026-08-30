//! Helper bindings whose CLOSURE is never built, because nothing reads it.
//!
//! # The shape, and what it costs
//!
//! ```text
//! function usesNested(x) { const step = (y) => y + 1; return step(x); }   162.00 ns
//! const stepOut = (y) => y + 1;
//! function usesOuter(x)  { return stepOut(x); }                            7.67 ns
//! function straight(x)   { return x + 1; }                                 7.67 ns
//! ```
//!
//! Measured 2026-08-30, release, min of 9. Twenty-one times, and `rts ir` says
//! why in three lines: the call to `step` is already SUBSTITUTED — the body
//! `y + 1` is emitted inline — and `__rts_closure_new` still runs on every call
//! of `usesNested`, allocating a cell and a `prototype`, registering two GC
//! roots, and handing back a value nothing reads.
//!
//! # Why this OMITS and does not DEFER
//!
//! The obvious design is to build the closure lazily, at the first call site
//! that refuses to substitute. It is a JIT's design and it does not transfer
//! here.
//!
//! **RTS is deterministic**, and deferring moves WHEN an allocation happens.
//! What the conservative collector sees depends on the order of allocations —
//! `docs/engine/lost-roots.md` records a case where adding one `eprintln!`
//! inside `collect` changed a program's ANSWER — so a design whose correctness
//! rests on "nobody can observe when the object was made" imports a freedom
//! this compiler does not have.
//!
//! Omitting is a different claim and a smaller one: the allocation has no
//! reachable use, so it is not moved, it is not there. That is dead-code
//! elimination, it is decided entirely at compile time, and the same source
//! produces the same program every run.
//!
//! # Why the proof has to be COMPLETE
//!
//! The binding is omitted only when every call to it is certain to be
//! substituted. It is not enough that most are: the name is bound to nothing,
//! so a call that fell back to a real one would read a binding that does not
//! exist.
//!
//! That failure would be loud — an unbound name at compile time — rather than a
//! wrong answer, which is the direction to be wrong in. The point is not to be
//! wrong: every clause below closes one thing `inline::emit_substituted`, or the
//! gate outside it, can refuse for. They are conservative on purpose. A refused
//! omission costs one closure; a wrong one costs a program.

use std::collections::BTreeSet;

use crate::Name;
use crate::syntax::{
    BindingKind, Expr, ExprKind, Function, FunctionBody, Pattern, Spreadable, Stmt, StmtKind,
};

use super::Ctx;
use super::capture::{
    Child, StmtChild, all_names_in_expr, all_names_in_statement, walk_expr, walk_stmt,
};

/// The helper bindings of one body whose closure need not be built.
///
/// Answered once per function body, before any of it is emitted, from facts
/// that are all already computed by then: the inliner's candidates, the
/// captured set, and this body's own escape analysis.
pub(super) fn omittable(ctx: &Ctx, body: &[Stmt], captured: &BTreeSet<Name>) -> BTreeSet<Name> {
    let mut named = Vec::new();
    for statement in body {
        helper_bindings(statement, &mut named);
    }
    if named.is_empty() {
        return BTreeSet::new();
    }
    // A `with` ANYWHERE in this body. The gate outside `emit_substituted` is
    // `ctx.with_objects.is_empty()` and it is false for a call written inside
    // one — a refusal in `call.rs` that nothing in `inline.rs` can see, which is
    // exactly why the naive version of this was refused when it was proposed.
    //
    // Asked of the whole body rather than of the call site, because this
    // decision is taken before either exists.
    if body.iter().any(has_with) {
        return BTreeSet::new();
    }

    let mut read_as_value = BTreeSet::new();
    for statement in body {
        value_reads_in_statement(statement, &mut read_as_value);
    }

    named
        .into_iter()
        .filter(|name| {
            // NEVER READ AS A VALUE. `g(f)`, `f.name`, `const h = f`, `[f]`,
            // `typeof f` and `f?.()` are all reads and all refuse; only the
            // callee of a direct call is not one, because a call is the single
            // use a substitution removes entirely.
            if read_as_value.contains(name) {
                return false;
            }
            // NOT CAPTURED, so it never reaches an environment. A call from
            // inside a nested function might be substitutable too, but the
            // substitution would happen while emitting THAT function, and this
            // analysis is about this one.
            if captured.contains(name) {
                return false;
            }
            // THE INLINER ACCEPTED IT. The one clause that is not a refusal of
            // its own: without a candidate there is nothing to substitute with
            // and the closure is simply needed.
            let Some(candidate) = ctx.inlinable(*name) else {
                return false;
            };
            // NO NAME OF THE CALLEE IS ONE THIS BODY FLATTENED.
            // `emit_substituted` asks exactly this, per call site, and the
            // answer is the same at every site in this body — `ctx.flattened`
            // is installed for the body before any of it is emitted.
            if candidate
                .parameters
                .iter()
                .chain(candidate.free.iter())
                .any(|held| ctx.flattens(*held))
            {
                return false;
            }
            // NO SPREAD AT ANY CALL SITE, which `emit_substituted` refuses.
            let mut plain = true;
            for statement in body {
                spread_calls(statement, *name, &mut plain);
            }
            plain
        })
        .collect()
}

/// Every `const`/`let` binding of this body whose initialiser is a function and
/// whose shape a call site cannot refuse.
///
/// TOP LEVEL only, and `var` never. A helper declared inside a block leaves that
/// block's scope, and a `var` is a write to a binding hoisting already made —
/// neither is a shape this can reason about from here.
///
/// Three properties of the function itself are required beyond what the inliner
/// asks, and each is a refusal this cannot otherwise see:
///
/// - a DEFAULTED parameter is refused at any site that writes the argument;
/// - a REST parameter takes a different path through `emit_substituted`
///   entirely;
/// - a CALL inside the helper's own body can hit `ctx.substituting` and fall
///   back. That is the cycle case — `f` calling `g` calling `f` — and tracing
///   the call graph would answer it exactly. Refusing a body with any call
///   answers it cheaply, and the helpers this exists for do not have one.
fn helper_bindings(statement: &Stmt, found: &mut Vec<Name>) {
    let StmtKind::Declare { kind, bindings } = &statement.kind else {
        return;
    };
    if matches!(kind, BindingKind::Var) {
        return;
    }
    for binding in bindings {
        let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value) else {
            continue;
        };
        let ExprKind::Function(function) = &value.kind else {
            continue;
        };
        if function.rest_parameter.is_some()
            || function.parameters.iter().any(|p| p.default.is_some())
            || calls_anything(function)
        {
            continue;
        }
        found.push(*name);
    }
}

/// Whether a function's body contains a call, a construction, or nested code.
fn calls_anything(function: &Function) -> bool {
    let mut found = false;
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                calls_in_statement(statement, &mut found);
            }
        }
        FunctionBody::Expression(expr) => calls_in_expr(expr, &mut found),
    }
    found
}

fn calls_in_statement(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => calls_in_statement(inner, found),
        StmtChild::Expr(expr) => calls_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                calls_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                calls_in_statement(inner, found);
            }
        }
        // Nested code is refused rather than walked: whatever it calls, it calls
        // from a scope this analysis is not reasoning about.
        StmtChild::Function(_) | StmtChild::Class(_) => *found = true,
    });
}

fn calls_in_expr(expr: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    if matches!(
        &expr.kind,
        ExprKind::Call { .. } | ExprKind::New { .. } | ExprKind::TaggedTemplate { .. }
    ) {
        *found = true;
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => calls_in_expr(inner, found),
        Child::Function(_) | Child::Class(_) => *found = true,
    });
}

/// Clears `plain` if any call to this name passes a spread argument.
fn spread_calls(statement: &Stmt, name: Name, plain: &mut bool) {
    if !*plain {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => spread_calls(inner, name, plain),
        StmtChild::Expr(expr) => spread_in_expr(expr, name, plain),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                spread_in_expr(value, name, plain);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                spread_calls(inner, name, plain);
            }
        }
        // A call from nested code was already refused by the captured clause.
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

fn spread_in_expr(expr: &Expr, name: Name, plain: &mut bool) {
    if !*plain {
        return;
    }
    if let ExprKind::Call {
        callee, arguments, ..
    } = &expr.kind
        && matches!(&callee.kind, ExprKind::Ident(called) if *called == name)
        && arguments
            .iter()
            .any(|argument| matches!(argument, Spreadable::Spread(_)))
    {
        *plain = false;
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => spread_in_expr(inner, name, plain),
        Child::Function(_) | Child::Class(_) => {}
    });
}

/// Whether a statement contains a `with`, at any depth inside this function.
fn has_with(statement: &Stmt) -> bool {
    if matches!(&statement.kind, StmtKind::With { .. }) {
        return true;
    }
    let mut found = false;
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => found = found || has_with(inner),
        StmtChild::Catch(catch) => found = found || catch.body.iter().any(has_with),
        StmtChild::Expr(_)
        | StmtChild::Binding(_)
        | StmtChild::Function(_)
        | StmtChild::Class(_) => {}
    });
    found
}

/// Every name this statement reads AS A VALUE, at any depth.
fn value_reads_in_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => value_reads_in_statement(inner, found),
        StmtChild::Expr(expr) => value_reads(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                value_reads(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                value_reads_in_statement(inner, found);
            }
        }
        // Nested code, where EVERY name counts. `capture::captured` refuses
        // these already; walking them here as well is the cheap belt to that
        // brace, and costs a set insert on a body this analysis has otherwise
        // finished with.
        StmtChild::Function(function) => nested_names(function, found),
        StmtChild::Class(class) => class_names(class, found),
    });
}

/// The same, over an expression. A direct call's callee is not a value read.
fn value_reads(expr: &Expr, found: &mut BTreeSet<Name>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            found.insert(*name);
            return;
        }
        // `f(…)` and nothing else. `f?.()` TESTS the callee before calling it,
        // so the value has to exist and the optional form is a read.
        ExprKind::Call {
            callee,
            arguments,
            optional: false,
        } if matches!(&callee.kind, ExprKind::Ident(_)) => {
            for argument in arguments {
                let (Spreadable::Single(value) | Spreadable::Spread(value)) = argument;
                value_reads(value, found);
            }
            return;
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => value_reads(inner, found),
        Child::Function(function) => nested_names(function, found),
        Child::Class(class) => class_names(class, found),
    });
}

fn nested_names(function: &Function, found: &mut BTreeSet<Name>) {
    for parameter in &function.parameters {
        if let Some(default) = &parameter.default {
            all_names_in_expr(default, found);
        }
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                all_names_in_statement(statement, found);
            }
        }
        FunctionBody::Expression(expr) => all_names_in_expr(expr, found),
    }
}

/// Every name a class body mentions, through the ONE walk that already knows
/// what a class body holds.
///
/// A second traversal of the same tree is how a node comes to be walked by one
/// analysis and silently skipped by the other — this module's sibling says so —
/// and a class is the shape with the most places to forget: a heritage
/// expression, a computed key, a field initialiser, a static block.
fn class_names(class: &crate::syntax::Class, found: &mut BTreeSet<Name>) {
    super::capture::names_in_class(class, found);
}
