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
//! # Why `require("…")` is found by the same walk
//!
//! It is the same question — which files is this program made of — asked of the
//! other module system. Written as a flag on this walk rather than as a second
//! one for the reason above: two walks over one tree are two chances for a node
//! to be visited by one and skipped by the other, and the one that skips it
//! loads a file short of a dependency.
//!
//! # What it deliberately does not find
//!
//! `import(name)`, `import("./" + kind)`, and every other computed specifier.
//! There is nothing to load ahead of time for those, because what they name is
//! not decided until the program runs — and guessing would be loading a file the
//! program did not name. They resolve at run time or reject, which is
//! `rts-core`'s `dynamic_module` half of the same operation.
//!
//! What it DOES report about them is that one exists ([`Survey::computed`]),
//! because a second question rides the same walk: whether the program can reach
//! a module the compiler cannot see it name. `serde_names` asks it to decide
//! whether the pickle's registry is worth filling — a computed `require(x)`
//! resolves at run time against the table every declared module is in, so a
//! program holding one can reach `rts:serde` without ever spelling it. The
//! same flag is raised by a read of `eval` or a call of `Function`
//! ([`Survey::dynamic_code`]): code compiled while the program runs can write
//! either form. One walk, two more clauses, for the reason above — a second
//! walk for the flags would be a second chance to skip a node.

use crate::names::Name;
use crate::syntax::{Expr, ExprKind, Function, FunctionBody, ModuleItem, Spreadable, Stmt};

use super::capture::{Child, StmtChild, children, statement_children};

/// Which forms a walk is looking for.
///
/// A parameter rather than a second function, for the reason this module's
/// header gives about there being one walk: two walks over one tree are two
/// chances for a node to be visited by one and skipped by the other, and the one
/// that skips it reports a program short of a file.
#[derive(Clone, Copy)]
pub struct Wanted {
    /// Whether `import("…")` counts.
    pub dynamic_import: bool,
    /// The name `require` is spelled by, when `require("…")` counts too.
    ///
    /// A `Name` rather than text because that is what the tree holds: interning
    /// once in the caller is what keeps the comparison an integer one at every
    /// node the walk reaches.
    pub require: Option<Name>,
    /// The names `eval` and `Function` are spelled by, when a read of the first
    /// or a call of the second should raise [`Survey::dynamic_code`]. `None`
    /// when the caller only wants specifiers.
    pub dynamic_code: Option<(Name, Name)>,
}

/// What one walk found.
#[derive(Default)]
pub struct Survey {
    /// Every specifier written as a string literal, in source order,
    /// duplicates included.
    pub named: Vec<String>,
    /// Whether some `import(x)` or `require(x)` of a wanted form had a
    /// specifier that is NOT a string literal.
    pub computed: bool,
    /// Whether `eval` is read or `Function` is called or constructed anywhere,
    /// when [`Wanted::dynamic_code`] asked.
    pub dynamic_code: bool,
}

/// Every specifier a `import("…")` in these items names with a string literal,
/// in source order, duplicates included.
///
/// Duplicates are kept rather than filtered: the caller resolves each against
/// its own file and de-duplicates by the resolved path, which is the only
/// spelling two occurrences are guaranteed to share.
pub fn dynamic_specifiers(items: &[ModuleItem]) -> Vec<String> {
    specifiers(
        items,
        Wanted {
            dynamic_import: true,
            require: None,
            dynamic_code: None,
        },
    )
}

/// The same, for whichever forms the caller asked about.
pub fn specifiers(items: &[ModuleItem], wanted: Wanted) -> Vec<String> {
    survey(items, wanted).named
}

/// The whole answer of one walk over these items: the literal specifiers and
/// the two flags.
pub fn survey(items: &[ModuleItem], wanted: Wanted) -> Survey {
    let mut found = Survey::default();
    for item in items {
        match item {
            ModuleItem::Stmt(statement) => in_statement(statement, wanted, &mut found),
            // An `export` wraps a declaration, which is a statement that may
            // hold one anywhere inside it — `export const m = import("./x")`.
            ModuleItem::Export(export) => {
                if let crate::syntax::ExportKind::Declaration(statement) = &export.kind {
                    in_statement(statement, wanted, &mut found);
                }
                if let crate::syntax::ExportKind::Default(default) = &export.kind {
                    match default {
                        crate::syntax::ExportDefault::Declaration(statement) => {
                            in_statement(statement, wanted, &mut found);
                        }
                        crate::syntax::ExportDefault::Expr(expr) => {
                            in_expr(expr, wanted, &mut found)
                        }
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

/// The same walk from a list of statements — a script's body, which has no
/// module items to hold it.
pub fn survey_statements(body: &[Stmt], wanted: Wanted) -> Survey {
    let mut found = Survey::default();
    for statement in body {
        in_statement(statement, wanted, &mut found);
    }
    found
}

/// The text a string literal spells, when the expression is one.
fn literal(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Literal(crate::syntax::Literal::String(text)) => text.as_rust(),
        _ => None,
    }
}

fn in_statement(statement: &Stmt, wanted: Wanted, found: &mut Survey) {
    statement_children(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => in_statement(inner, wanted, found),
        StmtChild::Expr(expr) => in_expr(expr, wanted, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                in_expr(value, wanted, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                in_statement(inner, wanted, found);
            }
        }
        StmtChild::Function(function) => in_function(function, wanted, found),
        StmtChild::Class(class) => in_class(class, wanted, found),
    });
}

fn in_expr(expr: &Expr, wanted: Wanted, found: &mut Survey) {
    // `require("./x")` — a call of that one name with a single string literal
    // argument. Not `require(name)` and not a `require` reached through
    // anything else: what those name is not decided until the program runs,
    // which is the line this module already draws for a computed `import()`.
    // A `require` called with anything else raises `computed` instead.
    if let Some(require) = wanted.require
        && let ExprKind::Call {
            callee, arguments, ..
        } = &expr.kind
        && let ExprKind::Ident(called) = &callee.kind
        && *called == require
    {
        match arguments.as_slice() {
            [Spreadable::Single(only)] => match literal(only) {
                Some(text) => found.named.push(text),
                None => found.computed = true,
            },
            _ => found.computed = true,
        }
    }
    if wanted.dynamic_import
        && let ExprKind::ImportCall { specifier, .. } = &expr.kind
    {
        // `as_rust` and not the units: a specifier is a path, and a lone
        // surrogate in one names no file. Skipped rather than replaced, which
        // is the same refusal `Text::as_rust` documents.
        match literal(specifier) {
            Some(text) => found.named.push(text),
            None => found.computed = true,
        }
    }
    if let Some((eval, function)) = wanted.dynamic_code {
        // A READ of `eval`, wherever it sits: `(0, eval)(s)` is the indirect
        // form and it reads the name like any other. `Function` only as a
        // callee — `Function.prototype.bind` is what a bundle writes on every
        // page and constructs nothing.
        let called =
            |callee: &Expr| matches!(&callee.kind, ExprKind::Ident(seen) if *seen == function);
        found.dynamic_code |= match &expr.kind {
            ExprKind::Ident(seen) => *seen == eval,
            ExprKind::Call { callee, .. } | ExprKind::New { callee, .. } => called(callee),
            _ => false,
        };
    }
    children(expr, &mut |child| match child {
        Child::Expr(inner) => in_expr(inner, wanted, found),
        Child::Function(function) => in_function(function, wanted, found),
        Child::Class(class) => in_class(class, wanted, found),
    });
}

fn in_function(function: &Function, wanted: Wanted, found: &mut Survey) {
    for parameter in &function.parameters {
        if let Some(value) = &parameter.default {
            in_expr(value, wanted, found);
        }
    }
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                in_statement(statement, wanted, found);
            }
        }
        FunctionBody::Expression(expr) => in_expr(expr, wanted, found),
    }
}

fn in_class(class: &crate::syntax::Class, wanted: Wanted, found: &mut Survey) {
    use crate::syntax::ClassElement;
    if let Some(heritage) = &class.heritage {
        in_expr(heritage, wanted, found);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => in_function(&method.function, wanted, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    in_expr(value, wanted, found);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    in_statement(statement, wanted, found);
                }
            }
        }
    }
}
