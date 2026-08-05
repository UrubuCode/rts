//! The two sets a statement list declares.
//!
//! The specification draws one line through every scope: a name is either
//! *lexically* declared — `let`, `const`, `class` — and belongs to the block it
//! is written in, or *var* declared and belongs to the enclosing function no
//! matter how deeply nested the statement is. Almost every redeclaration rule is
//! a question about those two sets, so they are computed here once rather than
//! re-derived by each rule that needs them.
//!
//! The asymmetry is the whole point of the pair. Lexical names stop at the
//! block, so gathering them looks only at the statements in the list. Var names
//! do not, so gathering them descends through every nested block, loop and
//! `try` — and stops at a function, because that is where a new var scope
//! begins.

use crate::names::Name;
use crate::syntax::{Binding, ForEachTarget, ForInit, Stmt, StmtKind};

/// The names a statement list declares lexically.
///
/// Function declarations are deliberately absent. In a block in sloppy code they
/// are lexical, hoisted, *and* additionally var-declared by Annex B's web
/// compatibility semantics, under which two of them in one block is legal — so
/// counting them here would refuse programs that run everywhere. That rule is
/// worth stating separately, with its conditions, rather than smuggled into this
/// one.
pub(super) fn lexical_names(statements: &[Stmt]) -> Vec<Name> {
    let mut names = Vec::new();
    for statement in statements {
        match &statement.kind {
            StmtKind::Declare { kind, bindings } if kind.is_block_scoped() => {
                bind_all(bindings, &mut names);
            }
            StmtKind::Using { bindings, .. } => bind_all(bindings, &mut names),
            StmtKind::Class(class) => names.extend(class.name),
            // A labelled declaration is not legal, but it parses, and reaching
            // through the label is what makes `a: let x;` a redeclaration of an
            // outer `x` rather than invisible.
            StmtKind::Labelled { body, .. } => {
                names.extend(lexical_names(std::slice::from_ref(body)));
            }
            _ => {}
        }
    }
    names
}

/// The names a statement list declares with `var`, however deeply nested.
///
/// Descends through everything that is not a function, because a `var` inside a
/// block, a loop, a `switch` clause or a `catch` still belongs to the enclosing
/// function. It stops at a function or a class body for the same reason: that is
/// a new var scope, and its `var`s are not these.
pub(super) fn var_names(statements: &[Stmt], names: &mut Vec<Name>) {
    for statement in statements {
        match &statement.kind {
            StmtKind::Declare { kind, bindings } if !kind.is_block_scoped() => {
                bind_all(bindings, names);
            }

            StmtKind::Block(body) => var_names(body, names),
            StmtKind::Labelled { body, .. } => var_names(std::slice::from_ref(body), names),
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
                var_names(std::slice::from_ref(body), names);
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                var_names(std::slice::from_ref(then_branch), names);
                if let Some(otherwise) = else_branch {
                    var_names(std::slice::from_ref(otherwise), names);
                }
            }

            StmtKind::For { init, body, .. } => {
                if let Some(ForInit::Declare { kind, bindings }) = init
                    && !kind.is_block_scoped()
                {
                    bind_all(bindings, names);
                }
                var_names(std::slice::from_ref(body), names);
            }

            StmtKind::ForEach { target, body, .. } => {
                if let ForEachTarget::Declare { kind, target } = target
                    && !kind.is_block_scoped()
                {
                    target.bound_names(names);
                }
                var_names(std::slice::from_ref(body), names);
            }

            StmtKind::Switch { clauses, .. } => {
                for clause in clauses {
                    var_names(&clause.body, names);
                }
            }

            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                var_names(body, names);
                if let Some(catch) = catch {
                    var_names(&catch.body, names);
                }
                if let Some(finally) = finally {
                    var_names(finally, names);
                }
            }

            StmtKind::With { body, .. } => var_names(std::slice::from_ref(body), names),

            // Everything else either declares nothing with `var` or opens a var
            // scope of its own, and in both cases there is nothing here to take.
            _ => {}
        }
    }
}

fn bind_all(bindings: &[Binding], names: &mut Vec<Name>) {
    for binding in bindings {
        binding.target.bound_names(names);
    }
}

/// The first name that appears twice, if any.
pub(super) fn first_repeat(names: &[Name]) -> Option<Name> {
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Some(*name);
        }
    }
    None
}

/// The first name in `left` that also appears in `right`.
pub(super) fn first_shared(left: &[Name], right: &[Name]) -> Option<Name> {
    left.iter().copied().find(|name| right.contains(name))
}
