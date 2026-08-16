//! What a module may not be.
//!
//! Two rules live here, and both are about the module as a whole rather than
//! about any item in it.
//!
//! The first is that a module's top level is not a script's. A function
//! declaration at the top of a script is var-declared — `function f() {} var f;`
//! is a program — and at the top of a module it is *lexical*, so the same two
//! lines are not one. Nothing in the tree marks the difference; the goal does,
//! and that is why this asks for it.
//!
//! The second is that no name is exported twice. It cannot be checked anywhere
//! an item can see, because the collision is between two items that are each
//! fine — and it is not a scope question either: `export { x as z }` beside
//! `export * as z from "m"` collides in the export table while declaring
//! nothing at all locally.

use crate::names::{Name, Names};
use crate::syntax::{Export, ExportDefault, ExportKind, ImportBinding, ModuleItem, Program, Stmt};

use super::scope::Declared;

/// The statements a module's items contribute to its top-level scope.
///
/// `export function f() {}` declares `f` exactly as the unexported form does —
/// the export is what happens to the name, not instead of it — so the statement
/// is handed on whole rather than reduced to a name.
pub(super) fn statements(program: &Program) -> Vec<Stmt> {
    program
        .body
        .iter()
        .filter_map(|item| match item {
            ModuleItem::Stmt(statement) => Some(statement.clone()),
            ModuleItem::Export(Export {
                kind: ExportKind::Declaration(statement),
                ..
            })
            | ModuleItem::Export(Export {
                kind: ExportKind::Default(ExportDefault::Declaration(statement)),
                ..
            }) => Some((**statement).clone()),
            _ => None,
        })
        .collect()
}

/// The names an import binds, which are lexical names of the module.
///
/// `import x from "m"; let x;` is not a program, and neither is a second import
/// of the same local name. Nothing else in the module declares them, so they
/// would be invisible to a scope rule that only read statements.
pub(super) fn imported_names(program: &Program) -> Vec<Declared> {
    let mut names = Vec::new();
    for item in &program.body {
        let ModuleItem::Import(import) = item else {
            continue;
        };
        for binding in &import.bindings {
            let name = match binding {
                ImportBinding::Default(name) | ImportBinding::Namespace(name) => *name,
                ImportBinding::Named { local, .. } => *local,
            };
            names.push(Declared {
                name,
                relaxable: false,
            });
        }
    }
    names
}

/// The first `with { … }` clause that gives one key twice.
///
/// The keys arrive decoded, which is the whole reason this is checkable here:
/// `type` and `type` are one key written two ways, and a rule reading the
/// source would have to decode them to find that out.
pub(super) fn duplicate_attribute(program: &Program) -> Option<String> {
    for item in &program.body {
        let attributes = match item {
            ModuleItem::Import(import) => &import.attributes,
            ModuleItem::Export(Export {
                kind:
                    ExportKind::Named { attributes, .. } | ExportKind::All { attributes, .. },
                ..
            }) => attributes,
            _ => continue,
        };
        for (index, attribute) in attributes.iter().enumerate() {
            if attributes[..index]
                .iter()
                .any(|earlier| earlier.key == attribute.key)
            {
                return Some(format!("`{}` is given twice in one `with`", attribute.key));
            }
        }
    }
    None
}

/// An `export { x }` that names nothing this module declares.
///
/// Only the form without a source: `export { x } from "m"` forwards a name the
/// other module answers for, and this one never sees it. Without a source the
/// name has to be something here, and if it is not there is nothing to export —
/// which the specification makes a syntax error rather than an export of
/// `undefined`.
pub(super) fn unresolvable_export(program: &Program, declared: &[String]) -> Option<String> {
    for item in &program.body {
        let ModuleItem::Export(Export {
            kind:
                ExportKind::Named {
                    specifiers,
                    source: None,
                    ..
                },
            ..
        }) = item
        else {
            continue;
        };
        if let Some(specifier) = specifiers
            .iter()
            .find(|specifier| !declared.contains(&specifier.local))
        {
            return Some(format!(
                "`{}` is exported and never declared",
                specifier.local
            ));
        }
    }
    None
}

/// The first name exported twice, if any.
///
/// The name compared is the *exported* one — what the outside sees — which is
/// why these are strings rather than `Name`s: `export { x as "a b" }` is legal
/// and `a b` is not an identifier. A module with two of one exported name has
/// no answer to give an importer of it, which is why this is refused before
/// anything runs rather than resolved by order.
pub(super) fn duplicate_export(program: &Program, names: &Names) -> Option<String> {
    let mut exported: Vec<String> = Vec::new();
    for item in &program.body {
        let ModuleItem::Export(export) = item else {
            continue;
        };
        match &export.kind {
            ExportKind::Declaration(statement) => {
                exported.extend(declared_names(statement, names));
            }
            ExportKind::Default(ExportDefault::Declaration(_) | ExportDefault::Expr(_)) => {
                exported.push("default".to_owned());
            }
            ExportKind::Named { specifiers, .. } => {
                exported.extend(specifiers.iter().map(|s| s.exported.clone()));
            }
            // `export * from "m"` exports names this module cannot know, so it
            // can never be found to collide here. `export * as ns` exports one
            // name and can.
            ExportKind::All { alias, .. } => exported.extend(alias.clone()),
        }
    }

    for (index, name) in exported.iter().enumerate() {
        if exported[..index].contains(name) {
            return Some(format!("`{name}` is exported twice"));
        }
    }
    None
}

/// The names an exported declaration introduces, as text.
fn declared_names(statement: &Stmt, names: &Names) -> Vec<String> {
    let mut bound: Vec<Name> = Vec::new();
    match &statement.kind {
        crate::syntax::StmtKind::Declare { bindings, .. }
        | crate::syntax::StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                binding.target.bound_names(&mut bound);
            }
        }
        crate::syntax::StmtKind::Function(function) => bound.extend(function.name),
        crate::syntax::StmtKind::Class(class) => bound.extend(class.name),
        _ => {}
    }
    bound
        .into_iter()
        .map(|name| names.text(name).to_owned())
        .collect()
}
