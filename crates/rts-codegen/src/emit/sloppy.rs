//! Which names a program creates by assigning to them.
//!
//! # Why an engine has to answer this
//!
//! `x = 1` with no declaration anywhere creates a global. That is sloppy mode,
//! and it is the only way a script without a module system introduces one — so
//! an engine that refused it would refuse a large amount of real JavaScript
//! while being right about the specification's *strict* mode, which is a
//! different language.
//!
//! # Why it is answered before emission rather than at each site
//!
//! Because the read comes first. `function f() { return count; } count = 0;`
//! reads `count` in a body emitted before the assignment is reached, and a
//! decision taken at the read would have nothing to go on. So the whole program
//! is scanned once, and every name it assigns without declaring is known to be a
//! global before anything is emitted.
//!
//! # Why an unassigned unknown name is still refused
//!
//! Reading an undeclared name is a `ReferenceError`, and this engine cannot
//! raise one where a handler could catch it. Refusing at compile time is
//! **stricter** than the language and wrong only for a program that meant to
//! catch that error — where the alternative, answering `undefined`, is wrong for
//! every program with a typo in it. `typeof` is exempt, as it is in the
//! specification, and that exemption is implemented rather than approximated.

use std::collections::BTreeSet;

use crate::names::Name;
use crate::syntax::{
    AssignTarget, Class, ClassElement, Expr, ExprKind, Function, FunctionBody, Pattern, Stmt,
    StmtKind,
};

/// Every name the program assigns to and never declares.
pub(super) fn created(body: &[Stmt]) -> BTreeSet<Name> {
    let mut assigned = BTreeSet::new();
    let mut declared = BTreeSet::new();
    for statement in body {
        walk_stmt(statement, &mut assigned, &mut declared);
    }
    assigned.difference(&declared).copied().collect()
}

/// Both sets, from one statement and everything under it.
///
/// One traversal rather than two, because the two questions are asked about the
/// same nodes and a second walk is where one of them comes to miss a case the
/// other handles.
fn walk_stmt(statement: &Stmt, assigned: &mut BTreeSet<Name>, declared: &mut BTreeSet<Name>) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                names_in_pattern(&binding.target, declared);
                if let Some(value) = &binding.value {
                    walk_expr(value, assigned, declared);
                }
            }
        }
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                declared.insert(name);
            }
            walk_function(function, assigned, declared);
        }
        StmtKind::Class(class) => {
            if let Some(name) = class.name {
                declared.insert(name);
            }
            walk_class(class, assigned, declared);
        }
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => walk_expr(expr, assigned, declared),
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                walk_expr(expr, assigned, declared);
            }
        }
        StmtKind::Block(body) => {
            for inner in body {
                walk_stmt(inner, assigned, declared);
            }
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, assigned, declared);
            walk_stmt(then_branch, assigned, declared);
            if let Some(otherwise) = else_branch {
                walk_stmt(otherwise, assigned, declared);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr(condition, assigned, declared);
            walk_stmt(body, assigned, declared);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            match init {
                Some(crate::syntax::ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
                        names_in_pattern(&binding.target, declared);
                        if let Some(value) = &binding.value {
                            walk_expr(value, assigned, declared);
                        }
                    }
                }
                Some(crate::syntax::ForInit::Expr(expr)) => {
                    walk_expr(expr, assigned, declared);
                }
                None => {}
            }
            for expr in test.iter().chain(update.iter()) {
                walk_expr(expr, assigned, declared);
            }
            walk_stmt(body, assigned, declared);
        }
        StmtKind::ForEach {
            target,
            subject,
            body,
            ..
        } => {
            match target {
                crate::syntax::ForEachTarget::Declare { target, .. } => {
                    names_in_pattern(target, declared);
                }
                // `for (x of xs)` writes to something that already exists — and
                // if nothing declared it, that something is a global the loop
                // creates on its first pass.
                crate::syntax::ForEachTarget::Assign(Pattern::Name(name)) => {
                    assigned.insert(*name);
                }
                crate::syntax::ForEachTarget::Assign(_) => {}
                crate::syntax::ForEachTarget::Dispose { target, .. } => {
                    declared.insert(*target);
                }
            }
            walk_expr(subject, assigned, declared);
            walk_stmt(body, assigned, declared);
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => {
            walk_stmt(body, assigned, declared);
        }
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            walk_expr(discriminant, assigned, declared);
            for clause in clauses {
                if let Some(test) = &clause.test {
                    walk_expr(test, assigned, declared);
                }
                for inner in &clause.body {
                    walk_stmt(inner, assigned, declared);
                }
            }
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            for inner in body {
                walk_stmt(inner, assigned, declared);
            }
            if let Some(catch) = catch {
                if let Some(binding) = &catch.binding {
                    names_in_pattern(binding, declared);
                }
                for inner in &catch.body {
                    walk_stmt(inner, assigned, declared);
                }
            }
            if let Some(finally) = finally {
                for inner in finally {
                    walk_stmt(inner, assigned, declared);
                }
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Debugger | StmtKind::Empty => {}
    }
}

/// A function's parameters are declarations; its body is more of the same
/// question.
fn walk_function(function: &Function, assigned: &mut BTreeSet<Name>, declared: &mut BTreeSet<Name>) {
    for parameter in &function.parameters {
        names_in_pattern(&parameter.target, declared);
    }
    if let Some(rest) = &function.rest_parameter {
        names_in_pattern(rest, declared);
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                walk_stmt(statement, assigned, declared);
            }
        }
        FunctionBody::Expression(value) => walk_expr(value, assigned, declared),
    }
}

/// A class body is nested code, like a function's.
fn walk_class(class: &Class, assigned: &mut BTreeSet<Name>, declared: &mut BTreeSet<Name>) {
    if let Some(heritage) = &class.heritage {
        walk_expr(heritage, assigned, declared);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => walk_function(&method.function, assigned, declared),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    walk_expr(value, assigned, declared);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    walk_stmt(statement, assigned, declared);
                }
            }
        }
    }
}

/// The same, over an expression.
fn walk_expr(expr: &Expr, assigned: &mut BTreeSet<Name>, declared: &mut BTreeSet<Name>) {
    if let ExprKind::Assign { target, .. } = &expr.kind
        && let AssignTarget::Place(place) = target
        && let ExprKind::Ident(name) = &place.kind
    {
        assigned.insert(*name);
    }
    // `x++` on an undeclared name creates one too, and it is the case an
    // implementation forgets because it does not look like an assignment.
    if let ExprKind::Update { target, .. } = &expr.kind
        && let ExprKind::Ident(name) = &target.kind
    {
        assigned.insert(*name);
    }
    super::capture::children(expr, &mut |child| match child {
        super::capture::Child::Expr(inner) => walk_expr(inner, assigned, declared),
        super::capture::Child::Function(function) => walk_function(function, assigned, declared),
        super::capture::Child::Class(class) => walk_class(class, assigned, declared),
    });
}

/// The names a binding pattern introduces.
fn names_in_pattern(pattern: &Pattern, declared: &mut BTreeSet<Name>) {
    let mut found = Vec::new();
    pattern.bound_names(&mut found);
    declared.extend(found);
}
