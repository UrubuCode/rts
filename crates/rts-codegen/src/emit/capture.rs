//! Which of a function's own names an inner function can still see.
//!
//! # The question this answers, and why it has to be answered first
//!
//! A local is a `ValueId` — a name for a value in a register, which is what
//! makes reading one free. That representation cannot survive being captured:
//! two activations of the inner function share the variable, and a register
//! belongs to one frame.
//!
//! So a captured local lives on the heap instead, and *which* locals those are
//! has to be known before the first one is emitted. A binding cannot be a
//! register in the statement that declares it and a heap slot four statements
//! later, when the closure that captures it is written.
//!
//! # Why this over-approximates on purpose
//!
//! `referenced_inside` collects **every** identifier appearing anywhere in a
//! nested function, including that function's own locals and its parameters. So
//! `function outer() { let x = 1; function inner() { let x = 2; } }` puts
//! `outer`'s `x` in the environment although nothing captures it.
//!
//! That is a cost, not a bug, and the direction of the error is what makes it
//! safe to be crude: a name wrongly in the environment is read through one
//! extra load. A name wrongly *out* of it is two closures disagreeing about a
//! variable, which is a wrong program. An analysis that resolved scopes
//! properly would be the same shape with a scope stack in it, and it belongs
//! with the checker — which is where scope resolution is going to live anyway
//! (`PLAN.md` L10) rather than being written a second time here.

use std::collections::BTreeSet;

use super::loops::assigned_in_stmt as writes_of;

use crate::names::Name;
use crate::syntax::{
    AssignTarget, Binding, Catch, Class, Expr, ExprKind, ForInit, Function, FunctionBody,
    Pattern, Property, PropertyKey, Stmt, StmtKind,
};

/// The names a function declares that some nested function could still see.
///
/// Ordered rather than hashed, and that is rule 13's reasoning applied one
/// crate up: the environment's properties are created in this order, so a set
/// that iterated differently between runs would produce a different shape for
/// the same program — and two shapes for one layout is the thing the whole
/// shape tree exists to avoid.
/// # The `omitted` set, and why an environment can be unmade
///
/// A name reaches an environment because some NESTED FUNCTION mentions it. When
/// that nested function is a helper `omit::omittable` approved, there will be no
/// closure: its body is spliced into this one at every call site, and it reads
/// the name as an ordinary binding of this activation. So the reason the
/// environment existed goes away with the closure, and counting the name would
/// build an object for a function that is not there.
///
/// Measured 2026-08-30, release, min of 9 — the analytic benchmark's own row:
///
/// ```text
/// for (…) { const c = (x) => x + i; a = c(a) | 0; }    46.33 ns with the object
/// ```
///
/// It is asked here rather than filtered afterwards because the environment is
/// decided by this set: `escape::analyse` reads it, `Scope::for_function` binds
/// from it, and a name removed after the fact would be bound in a layer nothing
/// wrote.
///
/// A name a SECOND nested function also mentions stays captured, which is why
/// the skip is per declaration rather than per name.
pub fn captured(body: &[Stmt], parameters: &[Name], omitted: &BTreeSet<Name>) -> BTreeSet<Name> {
    let mut inner = BTreeSet::new();
    let mut protected = BTreeSet::new();
    for statement in body {
        referenced_inside_statement(statement, &mut inner, false, omitted);

        assigned_under_protection(statement, &mut protected);
    }
    if inner.is_empty() && protected.is_empty() {
        // Nothing nested and nothing protected, so no environment is built at
        // all. The common case, and worth short-circuiting because it is what
        // keeps a function with no closures paying nothing for them.
        return BTreeSet::new();
    }

    let mut declared: BTreeSet<Name> = parameters.iter().copied().collect();
    for statement in body {
        declared_by_statement(statement, &mut declared);
    }

    // Both intersected with what this function declares. A name assigned under
    // protection that this function did not declare is somebody else's — a
    // global, or an outer function's — and putting it in *this* environment
    // would give it a second home rather than a stable one.
    let mut resident: BTreeSet<Name> = declared.intersection(&inner).copied().collect();
    resident.extend(declared.intersection(&protected).copied());
    resident
}

/// Of the names a function's environment holds, the ones bound at the
/// function's OWN level.
///
/// # Why this is a second set and not a filter on the first
///
/// [`captured`] answers what the environment must have room for, and it is
/// deliberately crude — this module's header says so, and says why: a name
/// wrongly IN the environment costs one extra load, while a name wrongly out
/// of it is two closures disagreeing about a variable.
///
/// That reasoning is sound for the environment's CONTENTS and does not carry
/// to its BINDINGS, which is what this exists to separate. `Scope::for_function`
/// binds every captured name at zero hops in the function's own layer, and
/// `lookup` scans innermost-first — so a name declared inside a nested block,
/// over-included by [`captured`], shadows the correct outer binding for the
/// whole function.
///
/// Measured rather than reasoned about, 2026-08-21:
///
/// ```text
/// let v = 1;
/// function outer() { return v; }
/// function run() { { let v = 10; function inner() { return v; } } return v; }
/// ```
///
/// answered `undefined` where Node and Bun answer `1`. The final read resolved
/// at zero hops into `run`'s own environment, which has a slot for `v` and was
/// never written one — the block wrote its own object. Remove `inner` and the
/// name stops being captured and the program is correct, which is what made it
/// look like a closure defect rather than a scoping one.
///
/// # What counts as the own level
///
/// The candidates a caller already lists — parameters, `...rest`, imports, a
/// named function expression's own name, `arguments` — plus what the body
/// declares at its top level, plus `var` at ANY depth. The last is not an
/// exception: a `var` inside a block IS a binding of the enclosing function,
/// which is the whole difference between it and `let`.
///
/// A block-scoped declaration inside a nested block is what this leaves out,
/// and `Scope::enter_environment` is what binds it — at zero hops in the
/// block's own layer, for exactly as long as the block lasts.
/// Of `captured`, the names that may be bound at the function's own level.
///
/// # Why this subtracts rather than intersects
///
/// The obvious form — keep the names this function declares at its own level —
/// was written first and was WRONG, and the TS suite said so within the hour:
/// 746 files passing became 700, with `Unbound("__rts_this")` at the front of
/// the list.
///
/// Three names are forced into `captured` by [`super::function`] AFTER the
/// walk, precisely because no walk can see them: a module publication reads a
/// name from code emitted after the body, an arrow reads its enclosing `this`
/// through `Scope::late_this` rather than through an `Ident`, and a derived
/// constructor holds `this` in its own environment. None is DECLARED by any
/// statement, so an intersection with what the body declares drops all three —
/// and dropping `__rts_this` leaves every arrow with no `this` to reach.
///
/// So the question is asked the other way round. A captured name is own-level
/// unless it is declared ONLY inside a nested block: a name nothing declares
/// was forced in and belongs here, and a name declared at the top level or as
/// a `var` belongs here whatever else is true of it.
pub fn own_level(body: &[Stmt], candidates: &[Name], captured: &BTreeSet<Name>) -> BTreeSet<Name> {
    let own = declared_at_own_level(body, candidates);
    let mut elsewhere = BTreeSet::new();
    for statement in body {
        bound_by_a_block_environment(statement, &mut elsewhere);
    }
    captured
        .iter()
        .filter(|name| own.contains(name) || !elsewhere.contains(name))
        .copied()
        .collect()
}

/// The names some nested block will bind in an environment of its OWN.
///
/// # Why this is narrower than "declared inside a block", which is what it was
///
/// Only a [`StmtKind::Block`] gets an environment of its own, and only through
/// `emit::stmt`'s arm for one, which asks `binding::block_layer` for exactly
/// the block's top-level lexical names and hoisted functions. Every other body
/// that LOOKS like a block does not: a `try` body, a `catch` body and a
/// `finally` body are emitted by `emit::protect` straight into the enclosing
/// scope, so what they declare lives in the FUNCTION's environment and needs
/// the function's own zero-hop binding.
///
/// The first version of this asked "declared anywhere below the top level",
/// and the TS suite answered: 746 files passing became 735, because
/// `try { const pa = delayed(5); … }` had its `const` counted as bound
/// elsewhere and then bound nowhere. `tests/async_fn_advanced.test.ts`
/// reported `parallel_sum=0` where it expects 150 — three awaits that resolved
/// to the initial values because the names they were assigned to had no
/// binding the reads could find.
///
/// A name declared BOTH here and at the function's own level stays own-level:
/// [`own_level`] keeps anything `declared_at_own_level` claims, and this only
/// removes what nothing else claims.
fn bound_by_a_block_environment(statement: &Stmt, found: &mut BTreeSet<Name>) {
    if let StmtKind::Block(body) = &statement.kind {
        for inner in body {
            match &inner.kind {
                StmtKind::Declare { kind, bindings } if kind.is_block_scoped() => {
                    for binding in bindings {
                        names_in_pattern(&binding.target, found);
                    }
                }
                // `block_layer` adds a hoisted function's name for the same
                // reason: the environment must hold a slot for it.
                StmtKind::Function(function) => found.extend(function.name),
                StmtKind::Class(class) => found.extend(class.name),
                _ => {}
            }
        }
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => bound_by_a_block_environment(inner, found),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                bound_by_a_block_environment(inner, found);
            }
        }
        StmtChild::Binding(_)
        | StmtChild::Expr(_)
        | StmtChild::Function(_)
        | StmtChild::Class(_) => {}
    });
}

fn declared_at_own_level(body: &[Stmt], candidates: &[Name]) -> BTreeSet<Name> {
    let mut found: BTreeSet<Name> = candidates.iter().copied().collect();
    for statement in body {
        // The body's own top level: every declaration here belongs to the
        // function whatever its kind.
        match &statement.kind {
            StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
                for binding in bindings {
                    names_in_pattern(&binding.target, &mut found);
                }
            }
            StmtKind::Function(function) => found.extend(function.name),
            StmtKind::Class(class) => found.extend(class.name),
            _ => {}
        }
        vars_at_any_depth(statement, &mut found);
    }
    found
}

/// The `var` names a statement introduces, however deeply nested.
///
/// Stops at a nested function, like everything else in this module: a `var`
/// there belongs to that function and not to this one.
pub(super) fn vars_at_any_depth(statement: &Stmt, found: &mut BTreeSet<Name>) {
    if let StmtKind::Declare { kind, bindings } = &statement.kind
        && !kind.is_block_scoped()
    {
        for binding in bindings {
            names_in_pattern(&binding.target, found);
        }
    }
    // A `for (var i = …)` head, which is a declaration the walk below reports
    // as a binding rather than as a statement.
    if let StmtKind::For {
        init: Some(ForInit::Declare { kind, bindings }),
        ..
    } = &statement.kind
        && !kind.is_block_scoped()
    {
        for binding in bindings {
            names_in_pattern(&binding.target, found);
        }
    }
    if let StmtKind::ForEach {
        target: crate::syntax::ForEachTarget::Declare { kind, target },
        ..
    } = &statement.kind
        && !kind.is_block_scoped()
    {
        names_in_pattern(target, found);
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => vars_at_any_depth(inner, found),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                vars_at_any_depth(inner, found);
            }
        }
        // A binding is reported for the declaration handled above; taking it
        // here as well would collect the block-scoped ones this must leave out.
        StmtChild::Binding(_)
        | StmtChild::Expr(_)
        | StmtChild::Function(_)
        | StmtChild::Class(_) => {}
    });
}

/// Names assigned inside a `try` body or a `using` scope.
///
/// These belong in the environment for exactly the reason a captured name does,
/// arrived at from the other direction. A handler is entered from *every* point
/// in the region that can throw, so at its start a name assigned in the body has
/// as many reaching definitions as there are throwing points — and there is no
/// SSA value to name. Cleanup has the same problem and worse, being reached from
/// the normal path too.
///
/// A block parameter is the usual answer to several definitions meeting, and it
/// does not work here: it requires every predecessor to pass an argument, and a
/// throwing point does not branch to the handler, it unwinds to it. Memory does
/// work, and the mechanism already exists — so this is not a second answer to
/// the question, it is the same one asked by a different construct.
///
/// Collected without regard to whether anything ever throws, which is the same
/// crudeness the module doc argues for above: a name wrongly in the environment
/// costs a load, and a name wrongly out of it is a handler reading a value from
/// before the assignment it is supposed to see.
fn assigned_under_protection(statement: &Stmt, found: &mut BTreeSet<Name>) {
    match &statement.kind {
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            // The handler's own assignments count too, and not for the same
            // reason as the body's. The body's are reached by the *handler*,
            // entered from every throwing point in it. The handler's are
            // reached by the *cleanup*, which unwinds through a handler that
            // threw — so `try {} catch (e) { x = 3 } finally { use(x) }` reads
            // a value the handler wrote, from a copy the handler does not
            // branch to. Missing this read `x` from before the `try`, silently.
            for inner in body.iter().chain(catch.iter().flat_map(|c| &c.body)) {
                found.extend(writes(inner));
                assigned_under_protection(inner, found);
            }
            if let Some(finally) = finally {
                for inner in finally {
                    assigned_under_protection(inner, found);
                }
            }
        }

        // A `using` declaration protects the rest of its block: the disposal it
        // owes runs on the way out, however control leaves.
        StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                let mut bound = Vec::new();
                binding.target.bound_names(&mut bound);
                found.extend(bound);
            }
        }

        StmtKind::Block(body) => {
            // A `using` anywhere in a block makes the whole block protected, so
            // everything assigned after it is reached by cleanup.
            if body
                .iter()
                .any(|inner| matches!(inner.kind, StmtKind::Using { .. }))
            {
                for inner in body {
                    found.extend(writes(inner));
                }
            }
            for inner in body {
                assigned_under_protection(inner, found);
            }
        }

        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            assigned_under_protection(then_branch, found);
            if let Some(otherwise) = else_branch {
                assigned_under_protection(otherwise, found);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::ForEach { body, .. }
        | StmtKind::Labelled { body, .. }
        | StmtKind::With { body, .. } => assigned_under_protection(body, found),
        StmtKind::Switch { clauses, .. } => {
            for clause in clauses {
                for inner in &clause.body {
                    assigned_under_protection(inner, found);
                }
            }
        }

        // A nested function has its own environment, and its own answer to this
        // question when it is emitted.
        StmtKind::Function(_) | StmtKind::Class(_) => {}

        StmtKind::Expr(_)
        | StmtKind::Throw(_)
        | StmtKind::Return(_)
        | StmtKind::Declare { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Debugger
        | StmtKind::Empty => {}
    }
}

/// Every name a nested function mentions, anywhere inside it.
///
/// Routed through [`walk_stmt`] for the structural part. `Using` used to reach
/// the trailing wildcard the module used to end on — so a nested function
/// written inside a `using` binding's value, `using x = (function () {
/// return y; })()`, had its captures decided as if the binding did not exist:
/// not merely under-scanned but skipped whole, in both the `everything` walk
/// and the top-level one `captured()` runs to find nested functions at all.
/// Found by making this match exhaustive rather than by a report; fixed here
/// because the correct behaviour — scan it like `Declare`, which the language
/// treats `using` as being for binding purposes — is not in question.
///
/// `for (const x of xs)`'s own target is deliberately still not added under
/// `everything`, matching the original: a for-each target belongs to the loop
/// body's own scope and is never read from the enclosing function, so whether
/// it is counted cannot change which outer name gets captured.
fn referenced_inside_statement(
    statement: &Stmt,
    found: &mut BTreeSet<Name>,
    everything: bool,
    omitted: &BTreeSet<Name>,
) {
    match &statement.kind {
        // The nested function itself. Everything in it counts, which is the
        // over-approximation the module doc argues for.
        StmtKind::Function(function) => return names_in_function(function, found),

        // A class body is nested code too, and every name in it counts for the
        // same reason a function's does. Skipping it was not a missing feature
        // but a wrong answer: a local a method closes over would be decided
        // uncaptured, and the method would read a register the enclosing
        // activation had already left.
        StmtKind::Class(class) => return names_in_class(class, found),

        // Not a sub-statement, a sub-expression, a binding, or a `catch` — the
        // shared walker's own doc says it is silent about a `for`-each
        // target, so this is the one place left to reach the expressions a
        // destructuring target can hold: a default, and a computed key. Both
        // run whether or not `everything` is set, on the same footing as
        // `binding.value` below — an outer name a default reads is captured
        // whether or not the target's own binding is nested code.
        StmtKind::ForEach {
            target: crate::syntax::ForEachTarget::Declare { target, .. },
            ..
        } => pattern_exprs(target, found, everything, omitted),

        _ => {}
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => referenced_inside_statement(inner, found, everything, omitted),
        StmtChild::Expr(inner) => referenced_inside_expr(inner, found, everything, omitted),
        StmtChild::Binding(binding) => {
            // The declared name counts too when this whole subtree is nested
            // code: a nested function's own local shadows an outer one, and
            // the over-approximation the module argues for is to include it
            // rather than to work out which. Applies uniformly to `Declare`,
            // `for (...)`'s own declaration, and `using` — one binding is one
            // binding regardless of which statement introduces it, so the
            // previous split (only `Declare` got the target under
            // `everything`, `for`'s own declaration never did) was a second
            // place for this rule to disagree with itself, not a distinction
            // that meant anything.
            if everything {
                names_in_pattern(&binding.target, found);
            }
            // A default or a computed key is an ordinary expression that runs,
            // regardless of `everything` — the same footing `binding.value`
            // is on below.
            pattern_exprs(&binding.target, found, everything, omitted);
            if let Some(expr) = &binding.value {
                // NOT INTO A HELPER THAT WILL NOT EXIST. `omit::omittable`
                // approved this declaration, so no closure is built for it and
                // its body is spliced into this one — where it reads what it
                // reads as ordinary bindings of this activation. Walking it
                // would put those names in an environment for a function that
                // is not there.
                if let Pattern::Name(name) = &binding.target
                    && omitted.contains(name)
                    && matches!(&expr.kind, ExprKind::Function(_))
                {
                    continue_past(expr, found, everything, omitted);
                } else {
                    referenced_inside_expr(expr, found, everything, omitted);
                }
            }
        }

        StmtChild::Catch(catch) => {
            if let Some(binding) = &catch.binding {
                if everything {
                    names_in_pattern(binding, found);
                }
                pattern_exprs(binding, found, everything, omitted);
            }
            for statement in &catch.body {
                referenced_inside_statement(statement, found, everything, omitted);
            }
        }
        StmtChild::Function(function) => names_in_function(function, found),
        StmtChild::Class(class) => names_in_class(class, found),
    });
}

/// The expressions a pattern embeds — a [`Pattern::Target`] leaf, a slot's
/// default, or an object property's computed key — walked as ordinary
/// references.
///
/// Kept apart from [`names_in_pattern`] on purpose: a bound name and a name
/// read *while computing* one are different questions, the same distinction
/// [`referenced_inside_statement`]'s `Binding` arm already draws between a
/// declaration's target and its initialiser.
fn pattern_exprs(
    pattern: &Pattern,
    found: &mut BTreeSet<Name>,
    everything: bool,
    omitted: &BTreeSet<Name>,
) {
    match pattern {
        Pattern::Name(_) => {}
        Pattern::Target(expr) => referenced_inside_expr(expr, found, everything, omitted),
        Pattern::Object(object) => {
            for property in &object.properties {
                if let PropertyKey::Computed(key) = &property.key {
                    referenced_inside_expr(key, found, everything, omitted);
                }
                pattern_exprs(&property.value.pattern, found, everything, omitted);
                if let Some(default) = &property.value.default {
                    referenced_inside_expr(default, found, everything, omitted);
                }
            }
            if let Some(rest) = &object.rest {
                pattern_exprs(rest, found, everything, omitted);
            }
        }
        Pattern::Array(array) => {
            for element in array.elements.iter().flatten() {
                pattern_exprs(&element.pattern, found, everything, omitted);
                if let Some(default) = &element.default {
                    referenced_inside_expr(default, found, everything, omitted);
                }
            }
            if let Some(rest) = &array.rest {
                pattern_exprs(rest, found, everything, omitted);
            }
        }
    }
}

/// What sits directly inside a statement, one level down.
///
/// The statement-side sibling of [`Child`]. A statement has more shapes of
/// child than an expression does — a nested statement, a sub-expression, a
/// binding (target pattern plus optional initialiser), a `catch` clause — so a
/// caller that wants only the nested statements can match one variant and
/// ignore the rest, the same way [`walk_expr`]'s callers do.
pub(super) enum StmtChild<'a> {
    /// A nested statement, walked by whichever traversal is running.
    Stmt(&'a Stmt),
    /// A sub-expression.
    Expr(&'a Expr),
    /// One binding of a `let`/`const`/`var`/`using` declaration, or of a `for`
    /// header's own declaration. Carries both the target pattern and the
    /// optional initialiser, because a caller regularly wants to treat the two
    /// differently — a target's own name is a declaration, not a reference.
    Binding(&'a Binding),
    /// A `catch` clause: its optional binding and its body.
    Catch(&'a Catch),
    /// A nested function, whose every name counts.
    Function(&'a Function),
    /// A class, whose methods and initialisers are nested code.
    Class(&'a Class),
}

/// The children of a statement, one level down.
///
/// One description of the tree's shape, on the statement side of the same
/// argument [`walk_expr`] makes on the expression side: a second copy of this
/// match is how a node comes to be walked by one analysis and silently
/// skipped by another.
///
/// Deliberately silent about a `for`-each loop's own target and about a
/// `for`-await-`using` loop's disposed name — neither is a sub-statement, a
/// sub-expression, a binding with an initialiser, or a `catch`, and the two
/// callers that care about either (`declared_by_statement`,
/// `referenced_inside_statement`) want different things from them, so folding
/// them into this enum would not remove a distinction, only hide one two
/// callers still have to make.
pub(super) fn walk_stmt<'a>(statement: &'a Stmt, on: &mut impl FnMut(StmtChild<'a>)) {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => on(StmtChild::Expr(expr)),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                on(StmtChild::Expr(expr));
            }
        }
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                on(StmtChild::Binding(binding));
            }
        }
        StmtKind::Block(body) => body.iter().for_each(|inner| on(StmtChild::Stmt(inner))),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            on(StmtChild::Expr(condition));
            on(StmtChild::Stmt(then_branch));
            if let Some(otherwise) = else_branch {
                on(StmtChild::Stmt(otherwise));
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            on(StmtChild::Expr(condition));
            on(StmtChild::Stmt(body));
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            match init {
                Some(ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
                        on(StmtChild::Binding(binding));
                    }
                }
                Some(ForInit::Expr(expr)) => on(StmtChild::Expr(expr)),
                None => {}
            }
            if let Some(expr) = test {
                on(StmtChild::Expr(expr));
            }
            if let Some(expr) = update {
                on(StmtChild::Expr(expr));
            }
            on(StmtChild::Stmt(body));
        }
        // The target is deliberately not surfaced here — see the function doc.
        StmtKind::ForEach { subject, body, .. } => {
            on(StmtChild::Expr(subject));
            on(StmtChild::Stmt(body));
        }
        StmtKind::Break(_) | StmtKind::Continue(_) => {}
        StmtKind::Labelled { body, .. } => on(StmtChild::Stmt(body)),
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            on(StmtChild::Expr(discriminant));
            for clause in clauses {
                if let Some(test) = &clause.test {
                    on(StmtChild::Expr(test));
                }
                for statement in &clause.body {
                    on(StmtChild::Stmt(statement));
                }
            }
        }
        StmtKind::Debugger | StmtKind::Empty => {}
        StmtKind::With { object, body } => {
            on(StmtChild::Expr(object));
            on(StmtChild::Stmt(body));
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            body.iter().for_each(|inner| on(StmtChild::Stmt(inner)));
            if let Some(catch) = catch {
                on(StmtChild::Catch(catch));
            }
            if let Some(finally) = finally {
                finally.iter().for_each(|inner| on(StmtChild::Stmt(inner)));
            }
        }
        StmtKind::Function(function) => on(StmtChild::Function(function)),
        StmtKind::Class(class) => on(StmtChild::Class(class)),
    }
}

/// Every name a function mentions that it did NOT declare itself — which is
/// what "free" means, and what a name has to be to belong in somebody else's
/// environment.
///
/// # Why the subtraction, and what over-including actually cost
///
/// This used to be "every name mentioned inside a function, INCLUDING its own",
/// and the reasoning written beside it was that over-including is the safe
/// direction: a name that need not be in the environment costs a load, and one
/// missing from it is a wrong program.
///
/// That is true of a name this function does not otherwise have, and false of
/// one it does. [`declared_by_statement`] over-includes at the other end — it
/// counts a `catch (e)` binding as declared by the enclosing function, on the
/// same "safe direction" reasoning — and [`captured`] intersects the two sets.
/// So a `var e` inside a NESTED function and a `catch (e)` in the outer one,
/// each harmless alone, together made `e` resident in the outer function: an
/// empty local binding SHADOWING the real `e` outside it. Measured on a real
/// bundle, the outer `e` was a function, and the call died with
/// `TypeError: e is not a function` — on a line ABOVE both of them.
///
/// Two safe over-approximations composing into a wrong answer is the shape of
/// this defect, and it is why "the error is in the safe direction" cannot be
/// argued per analysis. The subtraction here is not a tightening for its own
/// sake: a `var e` inside a nested function IS that function's `e`, so counting
/// it as a mention of the outer one was never right.
///
/// # What is subtracted, and what deliberately is not
///
/// Only what is unambiguously this function's at EVERY point of its body: its
/// parameters, and the `var` and `function` declarations of its own body. A
/// `let` in a block is not — it binds in that block, and a mention outside the
/// block is the outer name — so it stays counted, which keeps the error in the
/// safe direction exactly where block scope is what decides.
fn names_in_function(function: &Function, found: &mut BTreeSet<Name>) {
    // ITS OWN NAME IS NOT MENTIONED BY BEING SPELLED IN THE HEADER, and the
    // unconditional insert that used to stand here said it was.
    //
    // The reason given was that a declaration's name binds in the enclosing
    // scope and recursion reaches it through the environment. Both halves are
    // true and the insert is not what makes them work: a nested function that
    // recurses WRITES ITS OWN NAME IN ITS BODY, so `mentioned` has it and the
    // subtraction below keeps it — `own` holds what the nested function binds
    // internally, which its own declaration name is not. A sibling that calls it
    // writes the name in HER body, and it arrives the same way.
    //
    // What the insert did add was the case where nothing mentions it at all.
    // This set is intersected with what the ENCLOSING function declares, so a
    // nested `function unused() {}` put `unused` in the enclosing function's
    // environment — which means an environment object allocated and filled on
    // every call, for a binding nothing reads.
    //
    // Measured 2026-08-30, release, min of 9, the helper never called:
    //
    // ```text
    // function bare(x)     { return x + 1; }                      8.00 ns
    // function withDecl(x) { function unused(y) { return y; }
    //                        return x + 1; }                    323.67 ns
    // ```
    //
    // — one `__rts_object_new`, three `__rts_set_property` and one
    // `__rts_closure_new` per call of the enclosing function.
    //
    // The module header's rule is untouched: over-including is safe and
    // under-including is two closures disagreeing about a variable. This is
    // neither. It is a name counted as mentioned by a body that never mentions
    // it, and the body is walked either way.

    let mut own = BTreeSet::new();
    let mut mentioned = BTreeSet::new();
    for parameter in &function.parameters {
        names_in_pattern(&parameter.target, &mut own);
        // Um DEFAULT é código, e os nomes que LÊ não são os que o padrão liga —
        // `names_in_pattern` responde `bound_names`, que é a outra pergunta.
        // Faltarem não era uma sobre-aproximação a falhar por pouco: era um
        // nome que a função de fora nunca punha no ambiente, então a leitura do
        // default não encontrava nada:
        //
        //     function outer() {
        //       let shared = 100;
        //       const g = (x, y = shared) => x + y;
        //       g(1);        // ReferenceError: shared is not defined
        //     }
        //
        // Só quando o default é mesmo AVALIADO, por isso `g(1, 5)` funcionava e
        // `g(1)` não — que é porque nada no corpus o apanhou.
        //
        // Vão para `mentioned` e não para `found`, e essa é a metade que a
        // composição das duas correções exige: um default pode ler um nome que
        // esta função LIGA — `function f(a, b = a)` — e entregá-lo direto como
        // livre devolveria o defeito que a subtração abaixo existe para
        // fechar, por outra porta.
        if let Some(default) = &parameter.default {
            all_names_in_expr(default, &mut mentioned);
        }
        // E um padrão carrega defaults próprios: `function f({ a = x })` lê `x`
        // sem nenhum `parameter.default` à vista. `walk_pattern_exprs` é a
        // única travessia que sabe onde eles estão.
        walk_pattern_exprs(&parameter.target, &mut |child| {
            if let Child::Expr(expr) = child {
                all_names_in_expr(expr, &mut mentioned);
            }
        });
    }
    // O rest é um padrão também, e não era percorrido de todo.
    if let Some(rest) = &function.rest_parameter {
        names_in_pattern(rest, &mut own);
        walk_pattern_exprs(rest, &mut |child| {
            if let Child::Expr(expr) = child {
                all_names_in_expr(expr, &mut mentioned);
            }
        });
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                all_names_in_statement(statement, &mut mentioned);
                own_function_scoped(statement, &mut own);
            }
        }
        FunctionBody::Expression(expr) => all_names_in_expr(expr, &mut mentioned),
    }
    found.extend(mentioned.difference(&own));
}

/// The names a function body binds for the WHOLE of itself: its `var`s and its
/// function declarations, wherever in the body they are written.
///
/// Nested functions and class bodies are not entered — what they declare is
/// theirs — and a `let`/`const` is not counted, because it binds in its block
/// and a mention outside that block reaches the enclosing name. A `for (var …)`
/// head is not counted either: [`walk_stmt`] hands a loop's initialiser over as
/// a binding without saying which keyword wrote it, and counting one that might
/// be `let` would subtract a name this function does not own everywhere.
/// Both omissions leave the analysis where it already was, which is the safe
/// direction for them.
fn own_function_scoped(statement: &Stmt, found: &mut BTreeSet<Name>) {
    match &statement.kind {
        StmtKind::Declare {
            kind: crate::syntax::BindingKind::Var,
            bindings,
        } => {
            for binding in bindings {
                names_in_pattern(&binding.target, found);
            }
            return;
        }
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                found.insert(name);
            }
            return;
        }
        StmtKind::Class(_) => return,
        _ => {}
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => own_function_scoped(inner, found),
        // A `catch` BODY is still this function's, so a `var` in it is too. The
        // catch's own binding is not: it belongs to the catch block.
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                own_function_scoped(inner, found);
            }
        }
        StmtChild::Binding(_) | StmtChild::Expr(_) | StmtChild::Function(_)
        | StmtChild::Class(_) => {}
    });
}

/// Every identifier in a statement, without caring what introduced it.
///
/// The same traversal with the flag set. It used to be a second function that
/// called the first and then added the identifiers of the statement's OWN
/// expressions — which meant a name mentioned inside a nested block was never
/// seen, because the first traversal descends into a block looking only for
/// further nested functions.
///
/// The symptom was a compiler refusal rather than a wrong answer, and only for
/// a specific shape: `function f() { if (c) { n = 1; } }` decided that `n` was
/// not captured, so `f` had no binding for it and the assignment was an unbound
/// name. The same statement one level up compiled. Two traversals over one tree
/// is exactly the failure the `walk_expr` comment below warns about, arrived at
/// on the statement side.
pub(super) fn all_names_in_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    referenced_inside_statement(statement, found, true, nothing_omitted());
}

/// Every identifier in an expression.
pub(super) fn all_names_in_expr(expr: &Expr, found: &mut BTreeSet<Name>) {
    referenced_inside_expr(expr, found, true, nothing_omitted());
}

/// Every name a nested function inside an expression mentions.
///
/// With `everything`, every identifier counts rather than only the ones inside a
/// further nested function — which is what "this whole subtree is nested code"
/// means, and the one thing the two callers differ in.
fn referenced_inside_expr(
    expr: &Expr,
    found: &mut BTreeSet<Name>,
    everything: bool,
    omitted: &BTreeSet<Name>,
) {
    if everything && let ExprKind::Ident(name) = &expr.kind {
        found.insert(*name);
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => referenced_inside_expr(inner, found, everything, omitted),
        Child::Function(function) => names_in_function(function, found),
        Child::Class(class) => names_in_class(class, found),
    });
}

/// What sits directly inside an expression.
///
/// One callback taking this rather than two callbacks, because two would each
/// need `&mut` on the same accumulator and the borrow checker is right to
/// refuse it — the traversal genuinely visits both kinds of child into one set.
pub(super) enum Child<'a> {
    /// A sub-expression, walked by whichever traversal is running.
    Expr(&'a Expr),
    /// A nested function, whose every name counts.
    Function(&'a Function),
    /// A class, whose methods and initialisers are nested code.
    Class(&'a crate::syntax::Class),
}

/// The children of an expression.
///
/// One description of the tree's shape, used by both traversals above. Two
/// copies of this match is how a node comes to be walked by one analysis and
/// silently skipped by the other — and the one that skips it decides a local is
/// not captured when it is.
pub(super) fn walk_expr<'a>(expr: &'a Expr, on: &mut impl FnMut(Child<'a>)) {
    match &expr.kind {
        ExprKind::Function(function) => on(Child::Function(function)),
        ExprKind::Class(class) => on(Child::Class(class)),

        ExprKind::Literal(_)
        | ExprKind::Ident(_)
        | ExprKind::This
        | ExprKind::NewTarget
        | ExprKind::ImportMeta
        | ExprKind::PrivateName(_) => {}

        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            on(Child::Expr(left));
            on(Child::Expr(right));
        }
        ExprKind::Sequence { operands } => operands.iter().for_each(|e| on(Child::Expr(e))),
        ExprKind::Assign { target, value, .. } => {
            match target {
                AssignTarget::Place(place) => on(Child::Expr(place)),
                // `([a.b] = xs)` writes through `a`, and `({[k()]: x} = o)`
                // computes `k()` — both are ordinary reads this walk must not
                // skip, or a local either one closes over is decided
                // uncaptured.
                AssignTarget::Pattern(pattern) => walk_pattern_exprs(pattern, on),
            }
            on(Child::Expr(value));
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            on(Child::Expr(callee));
            for argument in arguments {
                on(Child::Expr(argument.expression()));
            }
        }
        ExprKind::New {
            callee, arguments, ..
        } => {
            on(Child::Expr(callee));
            for argument in arguments {
                on(Child::Expr(argument.expression()));
            }
        }
        ExprKind::Member { object, .. } => on(Child::Expr(object)),
        ExprKind::Index { object, index, .. } => {
            on(Child::Expr(object));
            on(Child::Expr(index));
        }
        ExprKind::Object { properties } => {
            for property in properties {
                // Every form, not just `key: value`. A method, a getter and a
                // setter are nested code exactly as a function expression is,
                // and skipping them decided that a local one of them closes
                // over was not captured — so the accessor read a register the
                // enclosing activation had already left, and the program was
                // refused as an unbound name. The same omission the class body
                // arm was fixed for, one node over.
                match property {
                    Property::Value { key, value, .. } => {
                        if let PropertyKey::Computed(computed) = key {
                            on(Child::Expr(computed));
                        }
                        on(Child::Expr(value));
                    }
                    Property::Method { key, function }
                    | Property::Getter { key, function }
                    | Property::Setter { key, function } => {
                        if let PropertyKey::Computed(computed) = key {
                            on(Child::Expr(computed));
                        }
                        on(Child::Function(function));
                    }
                    Property::Spread(value) | Property::Prototype(value) => {
                        on(Child::Expr(value));
                    }
                }
            }
        }
        ExprKind::Array { elements } => {
            for element in elements.iter().flatten() {
                on(Child::Expr(element.expression()));
            }
        }
        ExprKind::Unary { operand, .. } => on(Child::Expr(operand)),
        ExprKind::Update { target, .. } => on(Child::Expr(target)),
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            on(Child::Expr(condition));
            on(Child::Expr(then_branch));
            on(Child::Expr(else_branch));
        }
        ExprKind::Await(inner) => on(Child::Expr(inner)),
        ExprKind::Yield { value, .. } => {
            if let Some(value) = value {
                on(Child::Expr(value));
            }
        }
        ExprKind::Chain(inner) => on(Child::Expr(inner)),
        ExprKind::Asserted { value, .. } => on(Child::Expr(value)),
        ExprKind::ImportCall { specifier, .. } => on(Child::Expr(specifier)),

        // These four carried a comment saying the emitter refused them, so that
        // nothing inside one was ever reached. It stopped being true: `emit_expr`
        // routes a template to `template.rs` and both `super` forms to
        // `class.rs`. What the stale comment cost was a name mentioned ONLY
        // inside a substitution — `function f() { return `${x}`; }` did not
        // count `x` as captured, so the environment had no slot for it and
        // emission failed with `UnboundName`. Legal JavaScript that would not
        // compile, and the error at least came from the safe direction.
        ExprKind::Template { expressions, .. } => {
            for expression in expressions {
                on(Child::Expr(expression));
            }
        }
        ExprKind::TaggedTemplate {
            tag, expressions, ..
        } => {
            on(Child::Expr(tag));
            for expression in expressions {
                on(Child::Expr(expression));
            }
        }
        // A computed key is an expression like any other. The receiver is
        // `this`, which is not a name this pass tracks.
        ExprKind::SuperMember { property } => {
            if let PropertyKey::Computed(computed) = property.as_ref() {
                on(Child::Expr(computed));
            }
        }
        ExprKind::SuperCall { arguments } => {
            for argument in arguments {
                on(Child::Expr(argument.expression()));
            }
        }
    }
}

/// The expressions an assignment **pattern** embeds — every [`Pattern::Target`]
/// leaf, every slot's default, every computed key — fed to the same callback
/// [`walk_expr`]'s other children are, so a caller matching on [`Child::Expr`]
/// sees these the same way it sees an ordinary sub-expression.
///
/// [`Pattern::Name`] is silently skipped rather than reported as a reference.
/// That is a known gap, harmless today because nothing yet builds an
/// `AssignTarget::Pattern` whose leaf is a name reachable from here —
/// `emit/expr.rs` still refuses the assignment role outright, so a program
/// containing one fails to compile before this walk's answer is ever used.
/// Wiring that role back in must also make this leaf report its name, the same
/// way an `Ident` would.
fn walk_pattern_exprs<'a>(pattern: &'a Pattern, on: &mut impl FnMut(Child<'a>)) {
    match pattern {
        Pattern::Name(_) => {}
        Pattern::Target(expr) => on(Child::Expr(expr)),
        Pattern::Object(object) => {
            for property in &object.properties {
                if let PropertyKey::Computed(key) = &property.key {
                    on(Child::Expr(key));
                }
                walk_pattern_exprs(&property.value.pattern, on);
                if let Some(default) = &property.value.default {
                    on(Child::Expr(default));
                }
            }
            if let Some(rest) = &object.rest {
                walk_pattern_exprs(rest, on);
            }
        }
        Pattern::Array(array) => {
            for element in array.elements.iter().flatten() {
                walk_pattern_exprs(&element.pattern, on);
                if let Some(default) = &element.default {
                    on(Child::Expr(default));
                }
            }
            if let Some(rest) = &array.rest {
                walk_pattern_exprs(rest, on);
            }
        }
    }
}

/// The names a statement introduces in the function that contains it.
///
/// Block scoping is deliberately ignored: `{ let x = 1; }` inside a function
/// declares `x` for this purpose even though it is not in scope at the end. The
/// error is again in the safe direction — a name that did not need to be in the
/// environment costs a load, and one missing from it is a wrong program.
/// Routed through [`walk_stmt`] for the structural part.
///
/// This is where the module's own warning was found to have been unheeded:
/// there was no `Try` arm here at all, so a `var` declared inside a `try`
/// body, a `catch` body, or a `finally` body was never counted as declared by
/// the enclosing function. A name assigned there under protection —
/// `assigned_under_protection` finds exactly this shape — was intersected
/// against a `declared` set that did not contain it, so `resident` dropped it
/// silently: the handler read a value from before the assignment it was
/// supposed to see, for a `var` specifically, in a function whose capture
/// analysis otherwise looked complete because the wildcard this used to end on
/// gave no warning that `Try` was missing.
///
/// `catch (e)`'s binding is now included too, on the same reasoning the module
/// doc gives for ignoring block scope generally: a name captured by a nested
/// function is put in the environment whether or not it is technically in
/// scope at the point captured, because the direction of the error is what
/// makes it safe to over-include. A `for`-await-`using` loop's disposed name
/// is included for the same reason — it is a declaration this function owns,
/// like the `for`-declare target next to it, which was already handled.
pub(super) fn declared_by_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    match &statement.kind {
        // A declaration binds its own name in the enclosing scope, which is how
        // recursion works: `f` inside `f` is the enclosing function's binding,
        // captured like any other.
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                found.insert(name);
            }
            return;
        }
        // The same for a class: the name is a binding in the enclosing scope,
        // and a method naming its own class reads it from there.
        StmtKind::Class(class) => {
            if let Some(name) = class.name {
                found.insert(name);
            }
            return;
        }
        StmtKind::ForEach { target, .. } => match target {
            crate::syntax::ForEachTarget::Declare { target, .. } => {
                names_in_pattern(target, found);
            }
            crate::syntax::ForEachTarget::Dispose { target, .. } => {
                found.insert(*target);
            }
            crate::syntax::ForEachTarget::Assign(_) => {}
        },
        _ => {}
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => declared_by_statement(inner, found),
        StmtChild::Binding(binding) => names_in_pattern(&binding.target, found),
        StmtChild::Catch(catch) => {
            if let Some(binding) = &catch.binding {
                names_in_pattern(binding, found);
            }
            for inner in &catch.body {
                declared_by_statement(inner, found);
            }
        }
        StmtChild::Expr(_) | StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// The names a binding pattern introduces.
///
/// `Pattern::bound_names` walks the whole tree — nested objects, nested
/// arrays, rest — which stopped being optional the day destructuring stopped
/// being refused by the emitter: `let {a: {b}} = o` in a closure needs `b`
/// found here or the environment has no slot for it, and the inner function
/// reads a register the outer activation already left.
fn names_in_pattern(pattern: &Pattern, found: &mut BTreeSet<Name>) {
    let mut bound = Vec::new();
    pattern.bound_names(&mut bound);
    found.extend(bound);
}
/// The names one statement writes, as a set.
///
/// A thin adapter over the loop emitter's collector rather than a second one:
/// "what does this statement write" has exactly one answer, and a copy here
/// would be a second place for it to be wrong. It is a `Vec` there because a
/// merge needs the order.
fn writes(statement: &Stmt) -> Vec<Name> {
    let mut names = Vec::new();
    writes_of(statement, &mut names);
    names
}

/// Every name a class body mentions.
///
/// The same over-approximation [`names_in_function`] makes, and for the same
/// reason: a method is nested code whose scope this analysis does not model, so
/// counting every name it mentions is safe where missing one is not.
pub(super) fn names_in_class(class: &crate::syntax::Class, found: &mut BTreeSet<Name>) {
    use crate::syntax::ClassElement;
    if let Some(heritage) = &class.heritage {
        referenced_inside_expr(heritage, found, true, nothing_omitted());
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => names_in_function(&method.function, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    referenced_inside_expr(value, found, true, nothing_omitted());
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    referenced_inside_statement(statement, found, true, nothing_omitted());
                }
            }
        }
    }
}

/// The children of an expression, for a traversal that is not one of this
/// module's own.
///
/// Exposed rather than duplicated: the match in [`walk_expr`] is the one
/// description of the tree's shape, and its own comment says what a second copy
/// costs — a node walked by one analysis and silently skipped by another.
pub(super) fn children(expr: &Expr, on: &mut impl FnMut(Child)) {
    walk_expr(expr, on);
}

/// The children of a statement, for a traversal that is not one of this
/// module's own.
///
/// The statement-side pair of [`children`], exposed for the reason that one is:
/// [`walk_stmt`] is the single description of the tree's shape on this side, and
/// a second copy is a node one analysis walks and another silently skips.
pub(super) fn statement_children<'a>(
    statement: &'a Stmt,
    on: &mut impl FnMut(StmtChild<'a>),
) {
    walk_stmt(statement, on);
}

/// Whether a body mentions a name anywhere inside it, nested code included.
///
/// # Why nested code counts, which over-approximates on purpose
///
/// One caller: deciding whether a function has to bind `arguments`. An ARROW
/// that reads it means the enclosing function must, because an arrow has no
/// arguments of its own — so the walk cannot stop at a function boundary. A
/// nested plain function that reads it does NOT need the outer one to bind,
/// because it binds its own and shadows; walking into it anyway costs an array
/// nothing reads and keeps this from being a rule about which kind of function
/// was nested where.
///
/// Reported rather than fixed because the cost is one allocation on a call to a
/// function that mentions the word, and the alternative is a second traversal
/// that has to agree with `emit_body` about what an arrow is.
/// Whether a `with` statement appears anywhere in this body.
///
/// Asked for the same reason [`mentions`] is asked about `eval`, and answered
/// separately because a `with` is a NODE rather than a name: nothing a program
/// can spell makes one, so there is no identifier to look for.
///
/// Nested functions are deliberately NOT walked. A `with` inside one is that
/// function's own problem and forces that function's bindings when it is
/// emitted; forcing this body's bindings for it would cost an environment for
/// nothing, since the inner `with` cannot reach a name the capture analysis has
/// not already put in one.
pub(super) fn has_with(body: &[Stmt]) -> bool {
    fn in_stmt(statement: &Stmt, found: &mut bool) {
        if *found {
            return;
        }
        if matches!(statement.kind, crate::syntax::StmtKind::With { .. }) {
            *found = true;
            return;
        }
        walk_stmt(statement, &mut |child| {
            if let StmtChild::Stmt(inner) = child {
                in_stmt(inner, found);
            }
            if let StmtChild::Catch(catch) = child {
                for inner in &catch.body {
                    in_stmt(inner, found);
                }
            }
        });
    }
    let mut found = false;
    for statement in body {
        in_stmt(statement, &mut found);
    }
    found
}

pub(super) fn mentions(body: &[Stmt], wanted: Name) -> bool {
    let mut found = false;
    for statement in body {
        mentions_in_stmt(statement, wanted, &mut found);
    }
    found
}

fn mentions_in_stmt(statement: &Stmt, wanted: Name, found: &mut bool) {
    if *found {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => mentions_in_stmt(inner, wanted, found),
        StmtChild::Expr(expr) => mentions_in_expr(expr, wanted, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                mentions_in_expr(value, wanted, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                mentions_in_stmt(inner, wanted, found);
            }
        }
        StmtChild::Function(function) => mentions_in_function(function, wanted, found),
        StmtChild::Class(class) => mentions_in_class(class, wanted, found),
    });
}

fn mentions_in_expr(expr: &Expr, wanted: Name, found: &mut bool) {
    if *found {
        return;
    }
    if let ExprKind::Ident(name) = &expr.kind
        && *name == wanted
    {
        *found = true;
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => mentions_in_expr(inner, wanted, found),
        Child::Function(function) => mentions_in_function(function, wanted, found),
        Child::Class(class) => mentions_in_class(class, wanted, found),
    });
}

fn mentions_in_function(function: &Function, wanted: Name, found: &mut bool) {
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                mentions_in_stmt(statement, wanted, found);
            }
        }
        FunctionBody::Expression(value) => mentions_in_expr(value, wanted, found),
    }
}

/// Whether a class body mentions one name.
///
/// Asked about the class's OWN name, to decide whether it has to live in the
/// class environment: a method is a separately compiled function, and a name
/// bound as a register in the defining activation is unreachable from inside
/// one. The heritage is deliberately not walked — it is evaluated OUTSIDE the
/// class's own scope, so a name mentioned there is the enclosing binding.
pub(super) fn class_mentions(body: &[crate::syntax::ClassElement], name: Name) -> bool {
    let mut found = false;
    for element in body {
        match element {
            crate::syntax::ClassElement::Method(method) => {
                mentions_in_function(&method.function, name, &mut found)
            }
            crate::syntax::ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    mentions_in_expr(value, name, &mut found);
                }
            }
            crate::syntax::ClassElement::StaticBlock(statements) => {
                for statement in statements {
                    mentions_in_stmt(statement, name, &mut found);
                }
            }
        }
    }
    found
}

fn mentions_in_class(class: &crate::syntax::Class, wanted: Name, found: &mut bool) {
    for element in &class.body {
        match element {
            crate::syntax::ClassElement::Method(method) => {
                mentions_in_function(&method.function, wanted, found)
            }
            crate::syntax::ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    mentions_in_expr(value, wanted, found);
                }
            }
            crate::syntax::ClassElement::StaticBlock(body) => {
                for statement in body {
                    mentions_in_stmt(statement, wanted, found);
                }
            }
        }
    }
}

/// Whether an arrow inside this body reads `this`.
///
/// # Why the question is asked of the ENCLOSING function
///
/// An arrow takes `this` from where it was written rather than from how it is
/// called, so the answer has to come from the function that wrote it — which is
/// the only one that has a `this` to hand over. It hands it over as an ordinary
/// name, so the capture machinery carries it in like any other.
///
/// The walk descends into arrows, because an arrow inside an arrow still means
/// the outermost non-arrow function, and stops at a plain function, because that
/// one has its own `this` and answers for the arrows inside it.
pub(super) fn arrow_reads_this(body: &[Stmt]) -> bool {
    let mut found = false;
    for statement in body {
        arrow_this_in_stmt(statement, &mut found);
    }
    found
}

fn arrow_this_in_stmt(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => arrow_this_in_stmt(inner, found),
        StmtChild::Expr(expr) => arrow_this_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                arrow_this_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                arrow_this_in_stmt(inner, found);
            }
        }
        // A plain function has its own `this`, so it answers for what is inside
        // it. A class body is the same: a method is not an arrow.
        StmtChild::Function(function) => {
            if function.captures_this {
                reads_this(function, found);
            }
        }
        StmtChild::Class(_) => {}
    });
}

fn arrow_this_in_expr(expr: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => arrow_this_in_expr(inner, found),
        Child::Function(function) => {
            if function.captures_this {
                reads_this(function, found);
            }
        }
        Child::Class(_) => {}
    });
}

/// Whether an arrow's own body reads `this`, descending through further arrows.
fn reads_this(function: &Function, found: &mut bool) {
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                this_in_stmt(statement, found);
            }
        }
        FunctionBody::Expression(value) => this_in_expr(value, found),
    }
}

fn this_in_stmt(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => this_in_stmt(inner, found),
        StmtChild::Expr(expr) => this_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                this_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                this_in_stmt(inner, found);
            }
        }
        StmtChild::Function(function) => {
            if function.captures_this {
                reads_this(function, found);
            }
        }
        StmtChild::Class(_) => {}
    });
}

fn this_in_expr(expr: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    // `super.x` counts as a read of `this`, and not as a technicality: the
    // access is a lookup that starts above the home object and a call that runs
    // against `this`, so an arrow containing one needs the enclosing function's
    // receiver exactly as `this` does. Without this arm
    // `m() { const f = () => super.label; }` compiled an arrow nothing handed a
    // receiver to.
    if matches!(expr.kind, ExprKind::This | ExprKind::SuperMember { .. }) {
        *found = true;
        return;
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => this_in_expr(inner, found),
        Child::Function(function) => {
            if function.captures_this {
                reads_this(function, found);
            }
        }
        Child::Class(_) => {}
    });
}

/// What is still nested code inside an omitted helper's initialiser.
///
/// Nothing, today: the initialiser IS the function, and `omit::helper_bindings`
/// refuses one whose body holds a call, a construction, or nested code of its
/// own. Written as a named no-op rather than an empty arm so that admitting a
/// richer helper shape has one place to reach, and so the reader is told that
/// the omission is doing the refusing rather than this walk being incomplete.
fn continue_past(
    _expr: &Expr,
    _found: &mut BTreeSet<Name>,
    _everything: bool,
    _omitted: &BTreeSet<Name>,
) {
}

/// No declaration is omitted, for the walks that count EVERY name.
///
/// Those run inside nested code, where the question this set answers does not
/// arise: a helper `omit` approved is called only from the body that declares
/// it, so a walk that has descended into some other function cannot be looking
/// at one. Named rather than written inline so the reason is stated once.
pub(super) fn nothing_omitted() -> &'static BTreeSet<Name> {
    static NONE: std::sync::OnceLock<BTreeSet<Name>> = std::sync::OnceLock::new();
    NONE.get_or_init(BTreeSet::new)
}
