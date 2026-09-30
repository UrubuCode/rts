//! Whether a setter can have reached a prototype — asked of the whole program.

use super::*;

/// Whether the program spells any of the ways a setter reaches a prototype.
///
/// # What is being ruled out
///
/// `this.k = v` in a constructor calls a setter named `k` anywhere on the
/// chain, and a literal's `k: v` does not. A class with a layout declares no
/// accessor and is never handed over, so the only chain left is
/// `Object.prototype`.
///
/// # Why it is not "`Object` is only ever a base"
///
/// That was the first condition, and it refuses every program that SPELLS
/// `Object.prototype` — `Object.prototype.hasOwnProperty.call(o, k)` among
/// them, which is how a careful program asks whether a key is its own. It
/// switched the whole pass off in `bench/analytic.ts` and in this pass's own
/// fixture, which therefore tested nothing; the IR gate was what ran.
///
/// A setter is installed by a DEFINITION, and every definition is one of a few
/// spellings. So those are what is looked for: the four members that define or
/// relink, and a computed member of `Object` or `Reflect`, which could be any
/// of them. Writing through `Object` itself is `untouched`'s, asked beside
/// this.
pub(in crate::emit) fn setters_reachable(body: &[Stmt], names: &Names) -> bool {
    let mut found = false;
    for statement in body {
        defining_in(statement, names, &mut found);
    }
    found
}

/// The members that put an accessor on an object or change what it inherits.
pub(super) const DEFINING: [&str; 5] = [
    "defineProperty",
    "defineProperties",
    "__defineSetter__",
    "setPrototypeOf",
    "__proto__",
];

pub(super) fn defining_in(statement: &Stmt, names: &Names, found: &mut bool) {
    if *found {
        return;
    }
    if let StmtKind::Class(class) = &statement.kind {
        defining_in_class(class, names, found);
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => defining_in(inner, names, found),
        StmtChild::Expr(value) => defining_in_expr(value, names, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                defining_in_expr(value, names, found);
            }
        }
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                defining_in(inner, names, found);
            }
        }
        StmtChild::Function(function) => defining_in_function(function, names, found),
        StmtChild::Class(class) => defining_in_class(class, names, found),
    });
}

pub(super) fn defining_in_function(function: &crate::syntax::Function, names: &Names, found: &mut bool) {
    for parameter in &function.parameters {
        if let Some(value) = &parameter.default {
            defining_in_expr(value, names, found);
        }
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                defining_in(statement, names, found);
            }
        }
        FunctionBody::Expression(value) => defining_in_expr(value, names, found),
    }
}

pub(super) fn defining_in_class(class: &Class, names: &Names, found: &mut bool) {
    if let Some(heritage) = &class.heritage {
        defining_in_expr(heritage, names, found);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => defining_in_function(&method.function, names, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    defining_in_expr(value, names, found);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    defining_in(statement, names, found);
                }
            }
        }
    }
}

pub(super) fn defining_in_expr(expr: &Expr, names: &Names, found: &mut bool) {
    if *found {
        return;
    }
    match &expr.kind {
        ExprKind::Member { property, .. } if DEFINING.contains(&names.text(*property)) => {
            *found = true;
            return;
        }
        ExprKind::Index { object, .. }
            if matches!(&object.kind, ExprKind::Ident(name)
                if matches!(names.text(*name), "Object" | "Reflect")) =>
        {
            *found = true;
            return;
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => defining_in_expr(inner, names, found),
        Child::Function(function) => defining_in_function(function, names, found),
        Child::Class(class) => defining_in_class(class, names, found),
    });
}
