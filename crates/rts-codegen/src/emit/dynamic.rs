//! Which files a `import("…")` written with a literal names.
//!
//! # Why the language answers this and not the host
//!
//! The host has to know: a module reached only by a dynamic import must still be
//! compiled, and `rts-host`'s loader collects the graph before anything is
//! emitted. But finding the specifiers means walking every statement and every
//! expression of a parsed module, and that walk exists exactly once — in
//! [`super::capture`], whose own documentation says what a second copy costs: a
//! node walked by one analysis and silently skipped by another. So the walk
//! stays here and the host asks.
//!
//! # What it deliberately does not find
//!
//! `import(name)`, `import("./" + kind)`, and every other computed specifier.
//! There is nothing to load ahead of time for those, because what they name is
//! not decided until the program runs — and guessing would be loading a file the
//! program did not name. They resolve at run time or reject, which is
//! `rts-core`'s `dynamic_module` half of the same operation.

use crate::syntax::{Expr, ExprKind, Function, FunctionBody, ModuleItem, Stmt};

use super::capture::{Child, StmtChild, children, statement_children};

/// Every specifier a `import("…")` in these items names with a string literal,
/// in source order, duplicates included.
///
/// Duplicates are kept rather than filtered: the caller resolves each against
/// its own file and de-duplicates by the resolved path, which is the only
/// spelling two occurrences are guaranteed to share.
pub fn dynamic_specifiers(items: &[ModuleItem]) -> Vec<String> {
    let mut found = Vec::new();
    for item in items {
        match item {
            ModuleItem::Stmt(statement) => in_statement(statement, &mut found),
            // An `export` wraps a declaration, which is a statement that may
            // hold one anywhere inside it — `export const m = import("./x")`.
            ModuleItem::Export(export) => {
                if let crate::syntax::ExportKind::Declaration(statement) = &export.kind {
                    in_statement(statement, &mut found);
                }
                if let crate::syntax::ExportKind::Default(default) = &export.kind {
                    match default {
                        crate::syntax::ExportDefault::Declaration(statement) => {
                            in_statement(statement, &mut found);
                        }
                        crate::syntax::ExportDefault::Expr(expr) => in_expr(expr, &mut found),
                    }
                }
            }
            // An `import` names its file in the grammar, which the loader reads
            // directly. Nothing inside one is an expression.
            ModuleItem::Import(_) => {}
        }
    }
    found
}

fn in_statement(statement: &Stmt, found: &mut Vec<String>) {
    statement_children(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => in_statement(inner, found),
        StmtChild::Expr(expr) => in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                in_statement(inner, found);
            }
        }
        StmtChild::Function(function) => in_function(function, found),
        StmtChild::Class(class) => in_class(class, found),
    });
}

fn in_expr(expr: &Expr, found: &mut Vec<String>) {
    if let ExprKind::ImportCall { specifier, .. } = &expr.kind
        && let ExprKind::Literal(crate::syntax::Literal::String(text)) = &specifier.kind
        // `as_rust` and not the units: a specifier is a path, and a lone
        // surrogate in one names no file. Skipped rather than replaced, which
        // is the same refusal `Text::as_rust` documents.
        && let Some(text) = text.as_rust()
    {
        found.push(text);
    }
    children(expr, &mut |child| match child {
        Child::Expr(inner) => in_expr(inner, found),
        Child::Function(function) => in_function(function, found),
        Child::Class(class) => in_class(class, found),
    });
}

fn in_function(function: &Function, found: &mut Vec<String>) {
    for parameter in &function.parameters {
        if let Some(value) = &parameter.default {
            in_expr(value, found);
        }
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                in_statement(statement, found);
            }
        }
        FunctionBody::Expression(expr) => in_expr(expr, found),
    }
}

fn in_class(class: &crate::syntax::Class, found: &mut Vec<String>) {
    use crate::syntax::ClassElement;
    if let Some(heritage) = &class.heritage {
        in_expr(heritage, found);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => in_function(&method.function, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    in_expr(value, found);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    in_statement(statement, found);
                }
            }
        }
    }
}
