//! `import` and `export` — the module half of what a file's items can be.
//!
//! Split out of `item.rs` because that file was 1395 lines against this crate's
//! 1000-line ceiling (rule 8), and because import and export are one subject:
//! they are the two sides of the same table, and TypeScript's erasure rules
//! apply to both. `item.rs` keeps statements, functions and classes.

use swc_common::Spanned;
use swc_ecma_ast as swc;

use super::expr::expr;
use super::item::{class_expr, function_expr};
use super::stmt::{decl, stmt};
use super::{Cx, Result, position, unsupported};
use crate::syntax::{
    Export, ExportDefault, ExportKind, ExportSpecifier, Import, ImportAttribute, ImportBinding,
    ModuleItem, Stmt, StmtKind,
};

pub(super) fn module_item(cx: &mut Cx, item: &swc::ModuleItem) -> Result<Option<ModuleItem>> {
    Ok(match item {
        swc::ModuleItem::Stmt(statement) => Some(ModuleItem::Stmt(stmt(cx, statement)?)),
        swc::ModuleItem::ModuleDecl(declaration) => match declaration {
            swc::ModuleDecl::Import(import) => import_decl(cx, import)?.map(ModuleItem::Import),
            other => export_decl(cx, other)?.map(ModuleItem::Export),
        },
    })
}

/// One `import`, or `None` when TypeScript erases the whole statement.
///
/// # Import elision, and why it is the parser's job
///
/// `import type { Foo } from "./x"` names nothing that exists at run time, and
/// the language says so: the statement is ERASED, and no module is loaded. That
/// is what lets `tsc` compile a file without reading the files it imports types
/// from, and a program that relies on it is not doing anything unusual — it is
/// the ordinary way a `.d.ts`-shaped module is consumed.
///
/// Keeping the import made a type-only module a run-time request for a module
/// with no namespace, because `entry::module_publish` creates the namespace on
/// the first export and a module of only interfaces has none. That runtime
/// decision is right — *"a module that exports nothing has no namespace to speak
/// of"*, as it says — and it is not what changes here. What changes is that the
/// erased import stops reaching it.
///
/// This is done at the bridge rather than in a later pass because it is not a
/// lowering decision: the statement is not in the program at all, and every
/// pass after this one would otherwise have to know that some imports are not
/// imports. `relative_imports` in the host still walks the file, which costs one
/// parse of a module that is then compiled to nothing — the alternative is a
/// second place that decides what a type-only import is, and two answers to that
/// question is what this crate's rule 3 refuses.
///
/// Two spellings are erased, and they are the two TypeScript erases:
/// - `import type { … } from "x"` — the whole statement.
/// - `import { type A, type B } from "x"` — each marked specifier, and then the
///   statement too if nothing is left. NOT if the statement had no specifiers to
///   begin with: `import "x"` is a side-effect import and means the module RUNS,
///   which is the one case where an empty binding list is the point.
fn import_decl(cx: &mut Cx, import: &swc::ImportDecl) -> Result<Option<Import>> {
    if import.type_only {
        return Ok(None);
    }
    let had_specifiers = !import.specifiers.is_empty();
    let bindings = import
        .specifiers
        .iter()
        .filter(|specifier| match specifier {
            // `import { type A, B }` — only the marked one goes.
            swc::ImportSpecifier::Named(named) => !named.is_type_only,
            swc::ImportSpecifier::Default(_) | swc::ImportSpecifier::Namespace(_) => true,
        })
        .map(|specifier| {
            Ok(match specifier {
                swc::ImportSpecifier::Default(default) => {
                    ImportBinding::Default(cx.name(&default.local.sym))
                }
                swc::ImportSpecifier::Namespace(namespace) => {
                    ImportBinding::Namespace(cx.name(&namespace.local.sym))
                }
                swc::ImportSpecifier::Named(named) => ImportBinding::Named {
                    exported: match &named.imported {
                        Some(swc::ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(swc::ModuleExportName::Str(string)) => {
                            string.value.to_string_lossy().to_string()
                        }
                        None => named.local.sym.to_string(),
                    },
                    local: cx.name(&named.local.sym),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Every binding was type-only: the statement erases with them. The guard is
    // on what was WRITTEN, so `import "./x"` still runs the module.
    if had_specifiers && bindings.is_empty() {
        return Ok(None);
    }

    Ok(Some(Import {
        bindings,
        source: import.src.value.to_string_lossy().to_string(),
        attributes: attributes(import.with.as_deref()),
        at: position(import.span),
    }))
}
/// `with { type: "json" }`.
fn attributes(with: Option<&swc::ObjectLit>) -> Vec<ImportAttribute> {
    let Some(object) = with else {
        return Vec::new();
    };
    object
        .props
        .iter()
        .filter_map(|property| {
            let swc::PropOrSpread::Prop(prop) = property else {
                return None;
            };
            let swc::Prop::KeyValue(pair) = &**prop else {
                return None;
            };
            let key = match &pair.key {
                swc::PropName::Ident(ident) => ident.sym.to_string(),
                swc::PropName::Str(string) => string.value.to_string_lossy().to_string(),
                _ => return None,
            };
            let swc::Expr::Lit(swc::Lit::Str(value)) = &*pair.value else {
                return None;
            };
            Some(ImportAttribute {
                key,
                value: value.value.to_string_lossy().to_string(),
            })
        })
        .collect()
}

/// One `export`, or `None` when TypeScript erases the whole declaration.
///
/// The `None` is `export type`, and it is the other half of the elision
/// `import_decl` above performs. The two spellings mirror the import's exactly
/// — `export type { … }` and `export { type A, B }` — and until 2026-10-03 this
/// function read neither flag, so `export type { Cfg }` reached the checker as
/// an ordinary export of a name no runtime declaration produces and
/// `check::module::unresolvable_export` refused the file:
/// `` `Cfg` is exported and never declared ``. One rule applied on one side of
/// the module boundary and never on the other.
pub(super) fn export_decl(cx: &mut Cx, declaration: &swc::ModuleDecl) -> Result<Option<Export>> {
    let at = position(declaration.span());
    let kind = match declaration {
        swc::ModuleDecl::ExportDecl(export) => {
            Some(ExportKind::Declaration(Box::new(decl(cx, &export.decl)?)))
        }

        swc::ModuleDecl::ExportNamed(named) => export_named(named),

        swc::ModuleDecl::ExportDefaultDecl(default) => {
            Some(ExportKind::Default(ExportDefault::Declaration(Box::new(
                match &default.decl {
                    swc::DefaultDecl::Fn(function) => Stmt::new(
                        StmtKind::Function(Box::new(function_expr(cx, function)?)),
                        at,
                    ),
                    swc::DefaultDecl::Class(class) => {
                        Stmt::new(StmtKind::Class(Box::new(class_expr(cx, class)?)), at)
                    }
                    swc::DefaultDecl::TsInterfaceDecl(interface) => {
                        return unsupported("an exported interface", position(interface.span));
                    }
                },
            ))))
        }

        swc::ModuleDecl::ExportDefaultExpr(default) => Some(ExportKind::Default(
            ExportDefault::Expr(expr(cx, &default.expr)?),
        )),

        // `export * from "m"` carries no type-only spelling: there is no
        // `export type * from "m"` in the language, so nothing here is erased.
        swc::ModuleDecl::ExportAll(all) => Some(ExportKind::All {
            source: all.src.value.to_string_lossy().to_string(),
            alias: None,
            attributes: attributes(all.with.as_deref()),
        }),

        swc::ModuleDecl::Import(_) => return unsupported("an import reached as an export", at),
        swc::ModuleDecl::TsImportEquals(_)
        | swc::ModuleDecl::TsExportAssignment(_)
        | swc::ModuleDecl::TsNamespaceExport(_) => {
            return unsupported("a TypeScript-only module declaration", at);
        }
    };

    Ok(kind.map(|kind| Export { kind, at }))
}

/// `export { … }` and `export { … } from "m"`, or `None` when it all erases.
///
/// Two spellings go, and they are the two TypeScript erases:
/// - `export type { A }` and `export type { A } from "m"` — the whole
///   declaration, the `from` included. **The source form matters even though
///   `unresolvable_export` never looked at it**: it was emitted as a run-time
///   re-export, so `import * as ns from "./c"` saw a `Cfg` key that `bun`
///   does not. Measured 2026-10-03: `Object.keys(ns)` answered `Cfg,use` here
///   against `use` in bun 1.4.0, and `"Cfg" in ns` answered `true` against
///   `false`. Not harmless, so erased rather than documented as harmless.
/// - `export { type A, B }` — each marked specifier, and then the declaration
///   too if nothing is left.
///
/// A bare `export {}` survives, for the same reason `import "x"` does in
/// `import_decl`: the guard is on what was WRITTEN, and an empty list that was
/// written empty is a module marker rather than something erasure produced. The
/// rejected alternative was dropping an empty specifier list unconditionally,
/// which would also delete `export {} from "m"` — a declaration with no
/// bindings that still EVALUATES the other module.
fn export_named(named: &swc::NamedExport) -> Option<ExportKind> {
    if named.type_only {
        return None;
    }
    let had_specifiers = !named.specifiers.is_empty();
    let specifiers: Vec<ExportSpecifier> = named
        .specifiers
        .iter()
        .filter(|specifier| match specifier {
            // `export { type A, B }` — only the marked one goes. Neither
            // `export default` nor `export * as ns` has an inline modifier.
            swc::ExportSpecifier::Named(entry) => !entry.is_type_only,
            swc::ExportSpecifier::Default(_) | swc::ExportSpecifier::Namespace(_) => true,
        })
        .map(|specifier| match specifier {
            swc::ExportSpecifier::Named(entry) => ExportSpecifier {
                local: export_name(&entry.orig),
                exported: entry
                    .exported
                    .as_ref()
                    .map(export_name)
                    .unwrap_or_else(|| export_name(&entry.orig)),
            },
            swc::ExportSpecifier::Default(entry) => ExportSpecifier {
                local: entry.exported.sym.to_string(),
                exported: "default".to_owned(),
            },
            swc::ExportSpecifier::Namespace(entry) => ExportSpecifier {
                local: "*".to_owned(),
                exported: export_name(&entry.name),
            },
        })
        .collect();

    if had_specifiers && specifiers.is_empty() {
        return None;
    }

    Some(ExportKind::Named {
        specifiers,
        source: named
            .src
            .as_ref()
            .map(|s| s.value.to_string_lossy().to_string()),
        attributes: attributes(named.with.as_deref()),
    })
}

fn export_name(name: &swc::ModuleExportName) -> String {
    match name {
        swc::ModuleExportName::Ident(ident) => ident.sym.to_string(),
        swc::ModuleExportName::Str(string) => string.value.to_string_lossy().to_string(),
    }
}
