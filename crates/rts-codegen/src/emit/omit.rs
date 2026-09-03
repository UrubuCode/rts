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

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

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
pub(super) fn omittable(
    ctx: &Ctx,
    body: &[Stmt],
    captured: &BTreeSet<Name>,
    // Handed over rather than read off `Ctx`, which does not hold it yet: this
    // runs before the emission it belongs to, because `captured` is recomputed
    // from what this answers and the ENVIRONMENT is decided from that.
    flattened: &super::escape::Flattened,
    length: Name,
    arguments: Name,
) -> Omission {
    let mut named = Vec::new();
    for statement in body {
        helper_bindings(statement, &mut named);
    }
    if named.is_empty() {
        return Omission::default();
    }
    // A `with` ANYWHERE in this body. The gate outside `emit_substituted` is
    // `ctx.with_objects.is_empty()` and it is false for a call written inside
    // one — a refusal in `call.rs` that nothing in `inline.rs` can see, which is
    // exactly why the naive version of this was refused when it was proposed.
    //
    // Asked of the whole body rather than of the call site, because this
    // decision is taken before either exists.
    if body.iter().any(has_with) {
        return Omission::default();
    }

    let mut read_as_value = BTreeSet::new();
    for statement in body {
        value_reads_in_statement(statement, &mut read_as_value);
    }

    let mut answer = Omission::default();
    for (name, function) in named {
        // DECLARED EXACTLY ONCE IN THIS BODY.
        //
        // The candidate clause below used to carry the sentence "the two
        // clauses above proved that every call to this name is inside this
        // body, so the declaration in hand is the one every call reaches".
        // The first half is true and the second does not follow from it:
        // not being read as a value and not being captured say WHERE the
        // calls are, never how many declarations they choose between.
        // `helper_bindings` descends into sibling blocks, so one body can
        // offer the same spelling twice with a DIFFERENT function under
        // each, and `answer.local` is keyed by name — the second insert
        // overwrote the first, and every call in the body, including the
        // ones written inside the earlier block, reached the last one.
        //
        // # Why this counts declarations and not candidates
        //
        // Counting the entries `helper_bindings` returned is the version of
        // this guard that was written first, and it is WRONG in the exact
        // case the issue reports. That function filters as it collects (a
        // rest parameter, a defaulted one), so a body holding two `nm`s of
        // which only one is collectable counts ONE — the guard stays quiet
        // and the survivor takes over every call site, which is the whole
        // defect. `declarations_of` counts the name in the tree, which is
        // the question actually being asked, and it is already the refusal
        // `candidates` uses one door over for a map of the same shape.
        //
        // Refused rather than resolved per block: substituting the right
        // one needs each call site attributed to the declaration whose
        // block encloses it, and this analysis runs ONCE for the whole
        // body, before any of it is emitted and before there is a block to
        // ask about. A per-block answer is a different pass, not a stricter
        // version of this one.
        //
        // The cost is the substitution of a helper whose spelling the same
        // body spends twice — which `bench/analytic.ts` does not do (its
        // four `c`s are four separate function bodies, which
        // `local_candidate` already covers) and which the whole-program map
        // refused anyway before this pass existed.
        //
        // It stood because it was a wrong ANSWER and not a crash: issue
        // #2617 is a ~950-line protobuf decoder in which a field NUMBER
        // came out where that field VALUE belonged, no error raised at all.
        // Where the two declarations close over different names it surfaces
        // as a `ReferenceError` instead — the other body emitted into this
        // block environment, which does not hold what that body reads.
        // `tests/claude-helper-declarado-duas-vezes.test.ts` pins both.
        if super::inline::declarations_of(body, name) != 1 {
            continue;
        }
        // NEVER READ AS A VALUE. `g(f)`, `f.name`, `const h = f`, `[f]`,
        // `typeof f` and `f?.()` are all reads and all refuse; only the callee
        // of a direct call is not one, because a call is the single use a
        // substitution removes entirely.
        if read_as_value.contains(&name) {
            continue;
        }
        // NOT CAPTURED, so it never reaches an environment. A call from inside a
        // nested function might be substitutable too, but the substitution would
        // happen while emitting THAT function, and this analysis is about this
        // one.
        if captured.contains(&name) {
            continue;
        }
        // A CANDIDATE, and it may be one this body builds for itself.
        //
        // `ctx.inlinable` is keyed by name over the whole program, so it refuses
        // a spelling two functions use — and that refusal is what made this pass
        // do nothing on ordinary code. `bench/analytic.ts` declares `c` four
        // times, so the row that exists to MEASURE closure cost could not be
        // helped by anything that asks the map.
        //
        // Nothing here needs the map. The two clauses above proved that every
        // call to this name is inside this body, so the declaration in hand is
        // the one every call reaches, however many other functions spend the
        // same spelling. `inline::local_candidate` builds the candidate from it.
        let candidate = match ctx.inlinable(name) {
            Some(shared) => shared,
            None => match super::inline::local_candidate(function, length, name, arguments, false) {
                Some((built, _)) => Rc::new(built),
                None => continue,
            },
        };
        // NO NAME OF THE CALLEE IS ONE THIS BODY FLATTENED. `emit_substituted`
        // asks exactly this, per call site, and the answer is the same at every
        // site in this body — `ctx.flattened` is installed for the body before
        // any of it is emitted.
        if candidate
            .parameters
            .iter()
            .chain(candidate.free.iter())
            .any(|held| flattened.properties(*held).is_some())
        {
            continue;
        }
        // NO SPREAD AT ANY CALL SITE, which `emit_substituted` refuses.
        let mut plain = true;
        for statement in body {
            spread_calls(statement, name, &mut plain);
        }
        if !plain {
            continue;
        }
        // AND WHETHER ITS ENVIRONMENT CAN GO WITH ITS CLOSURE.
        //
        // A name reaches an environment because a nested function mentions
        // it, so a helper that will not exist is a reason that can be
        // withdrawn — `capture::captured` is asked again without it. That is
        // true for what the helper READS and false for what it WRITES.
        //
        // A substituted write lands through `Scope::assign`, which rebinds in
        // the layer `emit_substituted` opened, and that layer is gone at the
        // next statement. While the name was captured the write was a STORE to
        // the environment object and outlived it; withdrawing the capture turns
        // it back into a rebinding and the write is lost. Measured, and it was
        // a wrong ANSWER rather than a crash: an accumulator over four
        // iterations answered 0 where node answers 6.
        //
        // So the closure still goes and the environment stays. The helper this
        // costs is the one written for its effects, which is exactly the shape
        // `emit_substituted` was extended to admit.
        if !assigns_anything_free(function) {
            answer.uncaptured.insert(name);
        }
        answer.local.insert(name, candidate);
        answer.omitted.insert(name);
    }
    answer
}

/// What one body decided about its helper closures.
#[derive(Default)]
pub(super) struct Omission {
    /// Helpers whose closure is not built.
    pub omitted: BTreeSet<Name>,
    /// The candidate each was built from, for a name the program-wide map had
    /// to refuse.
    pub local: BTreeMap<Name, Rc<super::inline::Inlinable>>,
    /// The subset whose ENVIRONMENT can go too: those that only read.
    pub uncaptured: BTreeSet<Name>,
}

/// Whether a helper assigns a name it does not itself bind.
///
/// Crude in the safe direction: a write to its own parameter counts, because
/// telling the two apart needs the bound set `closed_over` builds and getting
/// it wrong here loses a write silently.
fn assigns_anything_free(function: &Function) -> bool {
    let mut found = false;
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                assigns_in_statement(statement, &mut found);
            }
        }
        FunctionBody::Expression(expr) => assigns_in_expr(expr, &mut found),
    }
    found
}

fn assigns_in_statement(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => assigns_in_statement(inner, found),
        StmtChild::Expr(expr) => assigns_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                assigns_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                assigns_in_statement(inner, found);
            }
        }
        StmtChild::Function(_) | StmtChild::Class(_) => *found = true,
    });
}

fn assigns_in_expr(expr: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    if matches!(
        &expr.kind,
        ExprKind::Assign { .. } | ExprKind::Update { .. }
    ) {
        *found = true;
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => assigns_in_expr(inner, found),
        Child::Function(_) | Child::Class(_) => *found = true,
    });
}

/// Every `const`/`let` binding of this body whose initialiser is a function and
/// whose shape a call site cannot refuse.
///
/// `var` never: it is a write to a binding hoisting already made, which is not a
/// shape this can reason about from here.
///
/// A BLOCK IS DESCENDED INTO, and the reason it is safe is the reason the whole
/// analysis is: the name is proved dead as a VALUE over the entire function
/// body, not over the block. A binding whose value nothing reads has no
/// observable scope — leaving the block takes nothing with it — and every call
/// to it is substituted wherever it stands.
///
/// It matters because the shape is written inside LOOPS, where the cost is paid
/// once per iteration. Measured 2026-08-30, release, min of 9:
///
/// ```text
/// for (…) { const q = (x) => x + 1; a = q(a) | 0; }        148.00 -> 8.00
/// const q = (x) => x + 1; for (…) { a = q(a) | 0; }          8.00 CONTROL
/// ```
///
/// A NESTED FUNCTION is not descended into, and that is a different question
/// with a different answer: a helper declared inside one is that function's, and
/// its substitution happens while emitting it rather than this one.
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
fn helper_bindings<'a>(statement: &'a Stmt, found: &mut Vec<(Name, &'a Function)>) {
    let StmtKind::Declare { kind, bindings } = &statement.kind else {
        // Anything else is descended into for the declarations it holds — a
        // loop body, an `if` arm, a bare block, a `try`. Not a nested function
        // and not a class: those are their own bodies and their own analysis.
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => helper_bindings(inner, found),
            StmtChild::Catch(catch) => {
                for inner in &catch.body {
                    helper_bindings(inner, found);
                }
            }
            StmtChild::Expr(_)
            | StmtChild::Binding(_)
            | StmtChild::Function(_)
            | StmtChild::Class(_) => {}
        });
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
        found.push((*name, function));
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
