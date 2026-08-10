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
//! # `globalThis.x = …` creates a global exactly as `x = …` does
//!
//! Both are how a program can create a property nothing declared, and only one
//! of them used to be recognised: a program that never writes the bare name
//! `__pubFn` — only `globalThis.__pubFn = …` — had a global with a real value
//! that this scan did not know existed, so a later bare read of `__pubFn` found
//! neither a declaration nor a created name. [`created`] now treats
//! `globalThis.x = …` as creating `x`, which is what lets that later read
//! resolve through [`super::globals::resolves`] instead of falling all the way
//! to [`super::globals::unbound_read`].
//!
//! # Why the shadow check is a stack, and not one flat set
//!
//! It used to be one flat `declared` set across the *whole program*, checked
//! once at the end against one flat `assigned` set — and that conflates two
//! functions that happen to use the same name for different things. A
//! parameter named `__pubFn` in one function is not a declaration of the
//! `__pubFn` a DIFFERENT function assigns to `globalThis.__pubFn` — but a flat
//! set cannot tell those apart, so the parameter silently cancelled the global.
//! Each scope now pushes its OWN declared set — its parameters plus everything
//! it declares directly, stopping at a nested function or class body, which
//! declares into its own — and an assignment only counts as creating a global
//! if the name is absent from every set on the stack at that point, innermost
//! to outermost. A global, once found, is recorded directly rather than into a
//! second set to diff at the end, because there is now nothing left to diff
//! against that is not already scope-correct.
//!
//! # Why an unassigned unknown name is no longer refused at compile time
//!
//! Reading an undeclared name is a `ReferenceError`, raised by the language only
//! when the read actually runs. Refusing at compile time was **stricter** than
//! that and wrong for a read on a path never taken — see [`super::globals::
//! unbound_read`] for where it is handled now. `typeof` is exempt, as it is in
//! the specification, and that exemption is implemented rather than
//! approximated.

use std::collections::BTreeSet;

use crate::names::Name;
use crate::syntax::{
    AssignTarget, Class, ClassElement, Expr, ExprKind, ForEachTarget, ForInit, Function,
    FunctionBody, Pattern, Stmt, StmtKind,
};

/// The scopes enclosing the point being walked, innermost last.
///
/// A name is shadowed if it is in ANY of these — not only the last one — which
/// is what makes a parameter two functions deep still block a global three
/// scopes further out.
type Shadow = Vec<BTreeSet<Name>>;

/// Every name the program assigns to, anywhere, that no scope on the stack at
/// that point declares.
///
/// `global_this` is the interned name of the identifier `globalThis` — passed
/// in rather than interned here, because a fresh intern of a name the caller
/// already holds would be a second copy of the same fact the compiler's own
/// name table exists to make singular.
pub(super) fn created(body: &[Stmt], global_this: Name) -> BTreeSet<Name> {
    let mut globals = BTreeSet::new();
    let mut own = BTreeSet::new();
    own_declared_stmts(body, &mut own);
    let mut shadow = vec![own];
    for statement in body {
        walk_stmt(statement, &mut globals, &mut shadow, global_this);
    }
    globals
}

/// Whether some enclosing scope already claims this name.
fn shadowed(shadow: &Shadow, name: Name) -> bool {
    shadow.iter().any(|scope| scope.contains(&name))
}

/// Records an assignment target, if it is a bare name or `globalThis.<name>`,
/// as a global exactly when nothing on the stack shadows it.
fn record_target(place: &Expr, globals: &mut BTreeSet<Name>, shadow: &Shadow, global_this: Name) {
    let name = match &place.kind {
        ExprKind::Ident(name) => Some(*name),
        // `globalThis.x = …` creates `x` exactly as a bare `x = …` does — both
        // are how sloppy mode makes a global with no declaration anywhere.
        // `(globalThis as any).x = …` reaches here the same way: `as any` IS a
        // syntax node this tree keeps ([`ExprKind::Asserted`], kept rather than
        // applied), so the object is peeled to what it asserts before asking
        // whether it names `globalThis`.
        ExprKind::Member {
            object,
            property,
            optional: false,
        } => match &unwrap_asserted(object).kind {
            ExprKind::Ident(name) if *name == global_this => Some(*property),
            _ => None,
        },
        _ => None,
    };
    if let Some(name) = name
        && !shadowed(shadow, name)
    {
        globals.insert(name);
    }
}

/// Every name a scope declares directly — its own `let`/`const`/`var`,
/// function and class declarations, catch bindings, `for` declarations — by
/// descending through control structures but stopping at a nested function or
/// class body, which declares into a scope of its own.
fn own_declared_stmts(body: &[Stmt], declared: &mut BTreeSet<Name>) {
    for statement in body {
        own_declared_stmt(statement, declared);
    }
}

fn own_declared_stmt(statement: &Stmt, declared: &mut BTreeSet<Name>) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                names_in_pattern(&binding.target, declared);
            }
        }
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                declared.insert(name);
            }
        }
        StmtKind::Class(class) => {
            if let Some(name) = class.name {
                declared.insert(name);
            }
        }
        StmtKind::Block(body) => own_declared_stmts(body, declared),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            own_declared_stmt(then_branch, declared);
            if let Some(otherwise) = else_branch {
                own_declared_stmt(otherwise, declared);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            own_declared_stmt(body, declared);
        }
        StmtKind::For { init, body, .. } => {
            if let Some(ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    names_in_pattern(&binding.target, declared);
                }
            }
            own_declared_stmt(body, declared);
        }
        StmtKind::ForEach { target, body, .. } => {
            match target {
                ForEachTarget::Declare { target, .. } => names_in_pattern(target, declared),
                ForEachTarget::Dispose { target, .. } => {
                    declared.insert(*target);
                }
                ForEachTarget::Assign(_) => {}
            }
            own_declared_stmt(body, declared);
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => {
            own_declared_stmt(body, declared);
        }
        StmtKind::Switch { clauses, .. } => {
            for clause in clauses {
                own_declared_stmts(&clause.body, declared);
            }
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            own_declared_stmts(body, declared);
            if let Some(catch) = catch {
                if let Some(binding) = &catch.binding {
                    names_in_pattern(binding, declared);
                }
                own_declared_stmts(&catch.body, declared);
            }
            if let Some(finally) = finally {
                own_declared_stmts(finally, declared);
            }
        }
        StmtKind::Expr(_)
        | StmtKind::Throw(_)
        | StmtKind::Return(_)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Debugger
        | StmtKind::Empty => {}
    }
}

/// The walk that finds assignments, descending into everything including
/// nested functions and classes — each of which pushes its own scope first.
fn walk_stmt(statement: &Stmt, globals: &mut BTreeSet<Name>, shadow: &mut Shadow, global_this: Name) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                if let Some(value) = &binding.value {
                    walk_expr(value, globals, shadow, global_this);
                }
            }
        }
        StmtKind::Function(function) => walk_function(function, globals, shadow, global_this),
        StmtKind::Class(class) => walk_class(class, globals, shadow, global_this),
        StmtKind::Expr(expr) | StmtKind::Throw(expr) => {
            walk_expr(expr, globals, shadow, global_this)
        }
        StmtKind::Return(value) => {
            if let Some(expr) = value {
                walk_expr(expr, globals, shadow, global_this);
            }
        }
        StmtKind::Block(body) => {
            for inner in body {
                walk_stmt(inner, globals, shadow, global_this);
            }
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, globals, shadow, global_this);
            walk_stmt(then_branch, globals, shadow, global_this);
            if let Some(otherwise) = else_branch {
                walk_stmt(otherwise, globals, shadow, global_this);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr(condition, globals, shadow, global_this);
            walk_stmt(body, globals, shadow, global_this);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            if let Some(ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    if let Some(value) = &binding.value {
                        walk_expr(value, globals, shadow, global_this);
                    }
                }
            }
            if let Some(ForInit::Expr(expr)) = init {
                walk_expr(expr, globals, shadow, global_this);
            }
            for expr in test.iter().chain(update.iter()) {
                walk_expr(expr, globals, shadow, global_this);
            }
            walk_stmt(body, globals, shadow, global_this);
        }
        StmtKind::ForEach {
            target,
            subject,
            body,
            ..
        } => {
            // `for (x of xs)` writes to something that already exists — and if
            // nothing declared it, that something is a global the loop creates
            // on its first pass.
            if let ForEachTarget::Assign(Pattern::Name(name)) = target
                && !shadowed(shadow, *name)
            {
                globals.insert(*name);
            }
            walk_expr(subject, globals, shadow, global_this);
            walk_stmt(body, globals, shadow, global_this);
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => {
            walk_stmt(body, globals, shadow, global_this);
        }
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            walk_expr(discriminant, globals, shadow, global_this);
            for clause in clauses {
                if let Some(test) = &clause.test {
                    walk_expr(test, globals, shadow, global_this);
                }
                for inner in &clause.body {
                    walk_stmt(inner, globals, shadow, global_this);
                }
            }
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            for inner in body {
                walk_stmt(inner, globals, shadow, global_this);
            }
            if let Some(catch) = catch {
                for inner in &catch.body {
                    walk_stmt(inner, globals, shadow, global_this);
                }
            }
            if let Some(finally) = finally {
                for inner in finally {
                    walk_stmt(inner, globals, shadow, global_this);
                }
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Debugger | StmtKind::Empty => {}
    }
}

/// A function is its own scope: its parameters and everything its body
/// declares directly are pushed before the body is walked for assignments, and
/// popped once it is done — so a name that scope claims cannot shadow anything
/// outside it, and nothing outside it can be mistaken for a declaration here.
fn walk_function(function: &Function, globals: &mut BTreeSet<Name>, shadow: &mut Shadow, global_this: Name) {
    let mut own = BTreeSet::new();
    for parameter in &function.parameters {
        names_in_pattern(&parameter.target, &mut own);
    }
    if let Some(rest) = &function.rest_parameter {
        names_in_pattern(rest, &mut own);
    }
    match &function.body {
        FunctionBody::Block(body) => own_declared_stmts(body, &mut own),
        FunctionBody::Expression(_) => {}
    }
    shadow.push(own);
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                walk_stmt(statement, globals, shadow, global_this);
            }
        }
        FunctionBody::Expression(value) => walk_expr(value, globals, shadow, global_this),
    }
    shadow.pop();
}

/// A class body is nested code, like a function's — heritage is evaluated in
/// the ENCLOSING scope (it is an expression before the class exists), and each
/// method is its own function scope by way of [`walk_function`].
fn walk_class(class: &Class, globals: &mut BTreeSet<Name>, shadow: &mut Shadow, global_this: Name) {
    if let Some(heritage) = &class.heritage {
        walk_expr(heritage, globals, shadow, global_this);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => {
                walk_function(&method.function, globals, shadow, global_this)
            }
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    walk_expr(value, globals, shadow, global_this);
                }
            }
            ClassElement::StaticBlock(body) => {
                // Its own scope, like a function's: a `let` inside one must
                // not leak into whatever else the class body declares.
                let mut own = BTreeSet::new();
                own_declared_stmts(body, &mut own);
                shadow.push(own);
                for statement in body {
                    walk_stmt(statement, globals, shadow, global_this);
                }
                shadow.pop();
            }
        }
    }
}

/// The same, over an expression.
fn walk_expr(expr: &Expr, globals: &mut BTreeSet<Name>, shadow: &mut Shadow, global_this: Name) {
    if let ExprKind::Assign { target, .. } = &expr.kind
        && let AssignTarget::Place(place) = target
    {
        record_target(place, globals, shadow, global_this);
    }
    // `x++` on an undeclared name creates one too, and it is the case an
    // implementation forgets because it does not look like an assignment.
    if let ExprKind::Update { target, .. } = &expr.kind
        && let ExprKind::Ident(name) = &target.kind
        && !shadowed(shadow, *name)
    {
        globals.insert(*name);
    }
    super::capture::children(expr, &mut |child| match child {
        super::capture::Child::Expr(inner) => walk_expr(inner, globals, shadow, global_this),
        super::capture::Child::Function(function) => {
            walk_function(function, globals, shadow, global_this)
        }
        super::capture::Child::Class(class) => walk_class(class, globals, shadow, global_this),
    });
}

/// Peels a `... as T` off, since the assertion is kept in the tree as its own
/// node ([`ExprKind::Asserted`]) rather than applied — `(globalThis as
/// any).x` and `globalThis.x` must recognise the same object underneath.
fn unwrap_asserted(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Asserted { value, .. } => unwrap_asserted(value),
        _ => expr,
    }
}

/// The names a binding pattern introduces.
fn names_in_pattern(pattern: &Pattern, declared: &mut BTreeSet<Name>) {
    let mut found = Vec::new();
    pattern.bound_names(&mut found);
    declared.extend(found);
}
