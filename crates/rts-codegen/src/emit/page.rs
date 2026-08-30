//! A `<script>` of a page: a script whose TOP-LEVEL declarations land in a
//! global object the next script will read them from.
//!
//! # Why this is a third door and not a flag on the other two
//!
//! [`super::emit_program_with_exports`] says *"nothing encloses a script"*, and
//! [`super::emit_eval_program`] is the exception that reads an enclosing scope
//! without writing to it. A page script is neither: several of them share one
//! global object, so it both READS what earlier ones left and DECLARES into
//! what later ones will read. That is what the language says script code does —
//! ECMA-262 §16.1.7 puts a top-level `var` and a top-level `function` on the
//! global object, and leaves `let` and `const` in the Script Record — and it is
//! the entire mechanism a page bundle depends on: script 1 defines `__d` and
//! script 30 calls it.
//!
//! Putting the exception beside the rule is what keeps the rule readable, which
//! is the same reason `eval` has its own door rather than a parameter on this
//! one.
//!
//! # What this replaces, and why the replacement is smaller
//!
//! The DOM facade got this effect by REWRITING THE SOURCE: a lexical scan
//! found the free names, qualified each one to `__G.<name>`, and compiled the
//! rewritten text. That is a scope resolver written outside the compiler,
//! against text rather than a tree, and it cannot converge — shadowing,
//! destructuring, a regular expression that looks like a division, and `typeof`
//! of a free name are each a case the scan has to be taught. It also compiled a
//! program the page did not serve, which is a divergence per rewrite.
//!
//! Here the compiler answers it, because the compiler is what knows what a
//! binding is. `docs/ui/page-script-bridge.md` records what the rewrite cost.
//!
//! # Why a tree transformation and not a change to hoisting
//!
//! Because the language's own words are a transformation: a top-level `var x =
//! 1` in script code *is* a property creation followed by an assignment. Saying
//! it that way reuses every path this crate already has — [`super::sloppy`]
//! decides which names a program creates, [`super::globals`] reads and writes
//! one — instead of teaching [`super::function::hoist_vars`] a second kind of
//! scope. Nothing about binding, capture or hops changes, which is the part of
//! this crate where a mistake reads another activation's variable.

use std::collections::BTreeSet;

use super::{Ctx, EmitResult, Program, Scope, emit_program_into};
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, BindingKind, Expr, ExprKind, Function, Pattern, Stmt, StmtKind,
};

/// Emits a page script: free names resolve against `enclosing`, and top-level
/// `var` and `function` declarations become writes to the global object.
///
/// `enclosing` is what [`super::emit_eval_program`] takes and means the same
/// thing — every name reachable through the environment chain the caller will
/// pass, with how far out it lives, read off the running program by
/// `rts_core::entry::environment_names`.
pub fn emit_page_program(
    body: &[Stmt],
    enclosing: &[(Name, u32)],
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    let mut published = BTreeSet::new();
    let mut hoisted = Vec::new();
    let mut statements = Vec::new();
    for statement in body {
        lower_statement(statement, &mut published, &mut hoisted, &mut statements);
    }
    // The completion value, the same way [`super::emit_eval_program`] answers
    // it: a trailing expression statement becomes a `return`. Script code has
    // one by specification — it is what `vm.runInContext` hands back — and it
    // is taken over the statements as WRITTEN, before the hoisted functions are
    // put in front of them, because the last thing a script does is the last
    // thing its author wrote and not the last one this moved.
    if let Some(last) = statements.pop() {
        let last = match last.kind {
            StmtKind::Expr(value) => Stmt::new(crate::syntax::StmtKind::Return(Some(value)), last.at),
            other => Stmt::new(other, last.at),
        };
        statements.push(last);
    }

    // Function declarations run FIRST, and that is not a detail. A bundle calls
    // a function above the line that declares it — hoisting is what makes that
    // legal — so emitting them in place would make the call read `undefined`.
    // A `var` needs no such move: the name is in `ctx.globals` below, so a read
    // before its line answers `undefined`, which is what the language says it
    // holds until the declaration runs.
    hoisted.append(&mut statements);
    let body = hoisted;

    // What an assignment creates, from the one place this crate already asks:
    // `x = 1` with no declaration anywhere is a global, and after the rewrite
    // above every top-level `var` looks like exactly that. So the scan finds
    // both halves and needs to be taught nothing.
    let global_this = ctx.names.intern("globalThis");
    published.extend(super::sloppy::created(&body, global_this));

    // The published names join the ENCLOSING CHAIN at zero hops, and this is
    // the whole of how a page script's globals reach the right object.
    //
    // The alternative was `ctx.globals`, which is what [`super::globals`] reads
    // and writes through `GlobalGet`/`GlobalSet` — and those name the global
    // object of the PROCESS. Measured: two documents then shared every name a
    // script assigned, so the second page saw the first one's variables. A
    // browser gives each document its own global object, and here that object
    // is the environment the caller passes.
    //
    // Saying it as a binding rather than as a special case is what makes it
    // correct inside nested functions for free: `Scope` already counts hops
    // outward, so a name read three closures deep resolves through three
    // `__rts_outer` links to the same object, which a second global path would
    // have had to re-derive.
    let mut chain: Vec<(Name, u32)> = enclosing.to_vec();
    for name in &published {
        if !chain.iter().any(|(held, _)| held == name) {
            chain.push((*name, 0));
        }
    }

    let scope = Scope::for_function(None, BTreeSet::new(), &BTreeSet::new(), &chain);
    emit_program_into(&body, &[], None, &[], &scope, ctx)
}

/// Turns one top-level statement into what it means for script code.
///
/// `published` collects the names that become global properties;
/// `hoisted` collects the function assignments that must run before anything
/// else; `out` collects the statements in their written order.
fn lower_statement(
    statement: &Stmt,
    published: &mut BTreeSet<Name>,
    hoisted: &mut Vec<Stmt>,
    out: &mut Vec<Stmt>,
) {
    match &statement.kind {
        // `var x = 1` → `x = 1`, and `var x` → nothing at all. The bare form
        // needs no statement because the name is published either way and an
        // unwritten global reads `undefined`, which is exactly what a `var`
        // holds before its initialiser runs.
        StmtKind::Declare { kind: BindingKind::Var, bindings } => {
            for binding in bindings {
                names_of(&binding.target, published);
                let Some(value) = binding.value.clone() else {
                    continue;
                };
                let target = match &binding.target {
                    Pattern::Name(name) => AssignTarget::Place(Box::new(Expr::new(
                        ExprKind::Ident(*name),
                        statement.at,
                    ))),
                    // `var [a, b] = xs` — the same destructuring, as an
                    // assignment. Legal in the tree because `AssignTarget`
                    // carries a pattern for exactly this shape.
                    other => AssignTarget::Pattern(other.clone()),
                };
                out.push(Stmt::new(
                    StmtKind::Expr(Expr::new(
                        ExprKind::Assign { target, value: Box::new(value), op: AssignOp::Plain },
                        statement.at,
                    )),
                    statement.at,
                ));
            }
        }
        // `function f() {}` → `f = function f() {}`, moved to the top. The
        // function KEEPS its name: `f.name` is observable, and a body that
        // calls itself resolves its own name from the expression rather than
        // from the global — which stays true if the page later reassigns `f`.
        StmtKind::Function(function) => {
            let Some(name) = function.name else {
                // An anonymous function declaration is not something the
                // grammar produces at statement position; leaving it alone is
                // what makes that the parser's answer rather than a second one
                // taken here.
                out.push(statement.clone());
                return;
            };
            published.insert(name);
            let value = Expr::new(
                ExprKind::Function(Box::new(Function::clone(function))),
                statement.at,
            );
            hoisted.push(Stmt::new(
                StmtKind::Expr(Expr::new(
                    ExprKind::Assign {
                        target: AssignTarget::Place(Box::new(Expr::new(
                            ExprKind::Ident(name),
                            statement.at,
                        ))),
                        value: Box::new(value),
                        op: AssignOp::Plain,
                    },
                    statement.at,
                )),
                statement.at,
            ));
        }
        // Everything else is script code that means what it means. `let` and
        // `const` in particular are deliberately untouched: the specification
        // keeps them in the Script Record, so a second script must NOT see
        // them, and the fixture asserts that as a control.
        _ => out.push(statement.clone()),
    }
}

/// Every name a binding pattern introduces.
///
/// [`Pattern::Target`] introduces none — it is an assignment leaf and only a
/// destructuring assignment can reach one — so it contributes nothing here
/// rather than being refused, which is the parser's call to make.
fn names_of(pattern: &Pattern, into: &mut BTreeSet<Name>) {
    match pattern {
        Pattern::Name(name) => {
            into.insert(*name);
        }
        Pattern::Target(_) => {}
        Pattern::Object(object) => {
            for property in &object.properties {
                names_of(&property.value.pattern, into);
            }
            if let Some(rest) = &object.rest {
                names_of(rest, into);
            }
        }
        Pattern::Array(array) => {
            for element in array.elements.iter().flatten() {
                names_of(&element.pattern, into);
            }
            if let Some(rest) = &array.rest {
                names_of(rest, into);
            }
        }
    }
}
