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
    AssignTarget, Expr, ExprKind, Function, FunctionBody, Parameter, Pattern, Property,
    PropertyKey, Stmt, StmtKind,
};

/// The names a function declares that some nested function could still see.
///
/// Ordered rather than hashed, and that is rule 13's reasoning applied one
/// crate up: the environment's properties are created in this order, so a set
/// that iterated differently between runs would produce a different shape for
/// the same program — and two shapes for one layout is the thing the whole
/// shape tree exists to avoid.
pub fn captured(body: &[Stmt], parameters: &[Name]) -> BTreeSet<Name> {
    let mut inner = BTreeSet::new();
    let mut protected = BTreeSet::new();
    for statement in body {
        referenced_inside_statement(statement, &mut inner);
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
fn referenced_inside_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    match &statement.kind {
        // The nested function itself. Everything in it counts, which is the
        // over-approximation the module doc argues for.
        StmtKind::Function(function) => names_in_function(function, found),

        // A class body is nested code too, and every name in it counts for the
        // same reason a function's does. Skipping it was not a missing feature
        // but a wrong answer: a local a method closes over would be decided
        // uncaptured, and the method would read a register the enclosing
        // activation had already left.
        StmtKind::Class(class) => names_in_class(class, found),

        StmtKind::Expr(expr) | StmtKind::Throw(expr) => referenced_inside_expr(expr, found),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                referenced_inside_expr(expr, found);
            }
        }
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Some(expr) = &binding.value {
                    referenced_inside_expr(expr, found);
                }
            }
        }
        StmtKind::Block(body) => {
            for inner in body {
                referenced_inside_statement(inner, found);
            }
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            referenced_inside_expr(condition, found);
            referenced_inside_statement(then_branch, found);
            if let Some(otherwise) = else_branch {
                referenced_inside_statement(otherwise, found);
            }
        }
        StmtKind::While { condition, body } => {
            referenced_inside_expr(condition, found);
            referenced_inside_statement(body, found);
        }
        StmtKind::DoWhile { body, condition } => {
            referenced_inside_statement(body, found);
            referenced_inside_expr(condition, found);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            if let Some(crate::syntax::ForInit::Expr(expr)) = init {
                referenced_inside_expr(expr, found);
            }
            if let Some(crate::syntax::ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    if let Some(expr) = &binding.value {
                        referenced_inside_expr(expr, found);
                    }
                }
            }
            if let Some(expr) = test {
                referenced_inside_expr(expr, found);
            }
            if let Some(expr) = update {
                referenced_inside_expr(expr, found);
            }
            referenced_inside_statement(body, found);
        }
        StmtKind::Labelled { body, .. } => referenced_inside_statement(body, found),
        StmtKind::ForEach { subject, body, .. } => {
            referenced_inside_expr(subject, found);
            referenced_inside_statement(body, found);
        }
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            referenced_inside_expr(discriminant, found);
            for clause in clauses {
                if let Some(test) = &clause.test {
                    referenced_inside_expr(test, found);
                }
                for statement in &clause.body {
                    referenced_inside_statement(statement, found);
                }
            }
        }

        // Everything else is refused by the emitter, so a name inside one can
        // never be reached. Listed as a wildcard rather than enumerated because
        // the emitter's refusal is what makes it unreachable, and duplicating
        // that list here is a second place to update.
        _ => {}
    }
}

/// Every name mentioned inside a function, including its own.
fn names_in_function(function: &Function, found: &mut BTreeSet<Name>) {
    if let Some(name) = function.name {
        found.insert(name);
    }
    for parameter in &function.parameters {
        names_in_pattern(&parameter.target, found);
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

/// Every identifier in a statement, without caring what introduced it.
fn all_names_in_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    // Reuses the traversal above and adds the one case that differs: inside a
    // nested function everything counts, where outside it only nested
    // functions did. Written as a flag rather than as a second traversal, so
    // there is one description of the tree's shape.
    referenced_inside_statement(statement, found);
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                names_in_pattern(&binding.target, found);
            }
        }
        StmtKind::Expr(expr) => all_names_in_expr(expr, found),
        StmtKind::Return(Some(expr)) => all_names_in_expr(expr, found),
        _ => {}
    }
}

/// Every identifier in an expression.
fn all_names_in_expr(expr: &Expr, found: &mut BTreeSet<Name>) {
    if let ExprKind::Ident(name) = &expr.kind {
        found.insert(*name);
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => all_names_in_expr(inner, found),
        Child::Function(function) => names_in_function(function, found),
        Child::Class(class) => names_in_class(class, found),
    });
}

/// Every name a nested function inside an expression mentions.
fn referenced_inside_expr(expr: &Expr, found: &mut BTreeSet<Name>) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => referenced_inside_expr(inner, found),
        Child::Function(function) => names_in_function(function, found),
        Child::Class(class) => names_in_class(class, found),
    });
}

/// What sits directly inside an expression.
///
/// One callback taking this rather than two callbacks, because two would each
/// need `&mut` on the same accumulator and the borrow checker is right to
/// refuse it — the traversal genuinely visits both kinds of child into one set.
enum Child<'a> {
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
fn walk_expr(expr: &Expr, on: &mut impl FnMut(Child)) {
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
            if let AssignTarget::Place(place) = target {
                on(Child::Expr(place));
            }
            on(Child::Expr(value));
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            on(Child::Expr(callee));
            for argument in arguments {
                if let crate::syntax::Spreadable::Single(value) = argument {
                    on(Child::Expr(value));
                }
            }
        }
        ExprKind::New {
            callee, arguments, ..
        } => {
            on(Child::Expr(callee));
            for argument in arguments {
                if let crate::syntax::Spreadable::Single(value) = argument {
                    on(Child::Expr(value));
                }
            }
        }
        ExprKind::Member { object, .. } => on(Child::Expr(object)),
        ExprKind::Index { object, index, .. } => {
            on(Child::Expr(object));
            on(Child::Expr(index));
        }
        ExprKind::Object { properties } => {
            for property in properties {
                if let Property::Value { key, value, .. } = property {
                    if let PropertyKey::Computed(computed) = key {
                        on(Child::Expr(computed));
                    }
                    on(Child::Expr(value));
                }
            }
        }
        ExprKind::Array { elements } => {
            for element in elements.iter().flatten() {
                if let crate::syntax::Spreadable::Single(value) = element {
                    on(Child::Expr(value));
                }
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

        // Refused by the emitter, so nothing inside one is ever reached.
        ExprKind::Template { .. }
        | ExprKind::TaggedTemplate { .. }
        | ExprKind::SuperMember { .. }
        | ExprKind::SuperCall { .. } => {}
    }
}

/// The names a statement introduces in the function that contains it.
///
/// Block scoping is deliberately ignored: `{ let x = 1; }` inside a function
/// declares `x` for this purpose even though it is not in scope at the end. The
/// error is again in the safe direction — a name that did not need to be in the
/// environment costs a load, and one missing from it is a wrong program.
fn declared_by_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                names_in_pattern(&binding.target, found);
            }
        }
        // A declaration binds its own name in the enclosing scope, which is how
        // recursion works: `f` inside `f` is the enclosing function's binding,
        // captured like any other.
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                found.insert(name);
            }
        }
        // The same for a class: the name is a binding in the enclosing scope,
        // and a method naming its own class reads it from there.
        StmtKind::Class(class) => {
            if let Some(name) = class.name {
                found.insert(name);
            }
        }
        StmtKind::Block(body) => {
            for inner in body {
                declared_by_statement(inner, found);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            declared_by_statement(then_branch, found);
            if let Some(otherwise) = else_branch {
                declared_by_statement(otherwise, found);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            declared_by_statement(body, found);
        }
        StmtKind::For { init, body, .. } => {
            if let Some(crate::syntax::ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    names_in_pattern(&binding.target, found);
                }
            }
            declared_by_statement(body, found);
        }
        StmtKind::Labelled { body, .. } => declared_by_statement(body, found),
        StmtKind::ForEach { target, body, .. } => {
            if let crate::syntax::ForEachTarget::Declare { target, .. } = target {
                names_in_pattern(target, found);
            }
            declared_by_statement(body, found);
        }
        StmtKind::Switch { clauses, .. } => {
            // A clause body is a scope in the language and is not one here, for
            // the same reason a block is not: erring toward MORE names in the
            // environment costs a load, and erring toward fewer is two closures
            // disagreeing about a variable.
            for clause in clauses {
                for statement in &clause.body {
                    declared_by_statement(statement, found);
                }
            }
        }
        _ => {}
    }
}

/// The names a binding pattern introduces.
fn names_in_pattern(pattern: &Pattern, found: &mut BTreeSet<Name>) {
    if let Pattern::Name(name) = pattern {
        found.insert(*name);
    }
    // Destructuring is refused by the emitter, so a pattern that is not a plain
    // name never reaches emission and nothing it would have bound can be read.
}

/// The plain names a parameter list introduces, or `None` if any is not one.
///
/// Returning `None` rather than skipping: a default or a destructured parameter
/// is a gap the emitter names, and quietly analysing the ones around it would
/// produce an environment that does not match the function that gets emitted.
pub fn plain_parameters(parameters: &[Parameter]) -> Option<Vec<Name>> {
    parameters
        .iter()
        .map(|parameter| match (&parameter.target, &parameter.default) {
            (Pattern::Name(name), None) => Some(*name),
            _ => None,
        })
        .collect()
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
fn names_in_class(class: &crate::syntax::Class, found: &mut BTreeSet<Name>) {
    use crate::syntax::ClassElement;
    if let Some(heritage) = &class.heritage {
        referenced_inside_expr(heritage, found);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => names_in_function(&method.function, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    referenced_inside_expr(value, found);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    referenced_inside_statement(statement, found);
                }
            }
        }
    }
}
