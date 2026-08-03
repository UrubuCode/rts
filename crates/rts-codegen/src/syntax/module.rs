//! Imports, exports, and the fact that a script and a module are different
//! languages.
//!
//! Not a flag on a program. `Script` and `Module` are separate goal symbols in
//! the grammar, and three things differ before a single statement is read:
//! module code is always strict, its top-level `this` is `undefined` rather than
//! the global object, and `await` is legal at the top level. A program that
//! records only "these are the imports" has already lost the first two.
//!
//! # An import is a binding to somewhere else, not a copy
//!
//! `import { x } from "m"` does not read `x` once. The binding tracks the one in
//! `m`, so a later assignment there is visible here — and the binding is
//! immutable on this side, which is why `x = 1` after importing it is an error
//! rather than a local shadow.
//!
//! That makes an imported name unlike every other binding a scope holds, and it
//! is why imports are their own node rather than declarations with a source
//! attached.
//!
//! # Order of what happens is not the order of what is written
//!
//! Every import in a module is resolved and instantiated before any of its
//! statements run, wherever the `import` was written. Function declarations are
//! hoisted; `let`, `const`, `class` and imported names exist from the start but
//! are unreadable until initialised.

use rts_cranelift::fault::Position;

use super::expr::Expr;
use super::stmt::{Directive, Stmt};
use crate::names::Name;

/// What kind of program this is.
///
/// Kept as a goal rather than inferred from whether the body has an `import`,
/// because a module with no imports is still a module, and the difference is
/// visible in code that mentions neither: `this` at the top level, and whether
/// an undeclared assignment is an error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    /// A script. Sloppy unless a directive says otherwise; top-level `this` is
    /// the global object; top-level `await` does not parse.
    Script,
    /// A module. Always strict; top-level `this` is `undefined`; top-level
    /// `await` is legal.
    Module,
}

impl Goal {
    /// Whether the code is strict before any directive is read.
    pub fn is_strict(self) -> bool {
        matches!(self, Goal::Module)
    }

    /// Whether `await` may appear at the top level.
    pub fn allows_top_level_await(self) -> bool {
        matches!(self, Goal::Module)
    }
}

/// One top-level item of a module.
#[derive(Clone, PartialEq, Debug)]
pub enum ModuleItem {
    /// An ordinary statement.
    Stmt(Stmt),
    /// `import …`.
    Import(Import),
    /// `export …`.
    Export(Export),
}

/// `import … from "m"`, or `import "m"`.
#[derive(Clone, PartialEq, Debug)]
pub struct Import {
    /// What is bound. Empty for a side-effect-only import.
    pub bindings: Vec<ImportBinding>,
    /// Which module.
    pub source: String,
    /// `with { type: "json" }` — import attributes.
    ///
    /// Part of the module's identity, not decoration: the same specifier
    /// imported with different attributes is a different request.
    pub attributes: Vec<ImportAttribute>,
    /// Where it was written.
    pub at: Position,
}

impl Import {
    /// Whether this import binds nothing and exists only to run the module.
    pub fn is_side_effect_only(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// One name an import introduces.
#[derive(Clone, PartialEq, Debug)]
pub enum ImportBinding {
    /// `import d from "m"`.
    Default(Name),
    /// `import { a }`, `import { a as b }`, `import { "a b" as c }`.
    Named {
        /// The name in the other module.
        ///
        /// A string, not a `Name`, because the exported name need not be an
        /// identifier: `export { x as "a b" }` is legal, and so is importing it.
        exported: String,
        /// What it is called here.
        local: Name,
    },
    /// `import * as ns from "m"` — one object holding every export.
    ///
    /// Not equivalent to importing each name: the namespace object is sealed,
    /// its keys are sorted, and it has no default unless one was exported.
    Namespace(Name),
}

/// `type: "json"` in an import's `with` clause.
#[derive(Clone, PartialEq, Debug)]
pub struct ImportAttribute {
    /// The key.
    pub key: String,
    /// The value. Always a string literal — no expressions, so this is decided
    /// before anything runs.
    pub value: String,
}

/// `export …`, in every shape it has.
#[derive(Clone, PartialEq, Debug)]
pub struct Export {
    /// What is exported.
    pub kind: ExportKind,
    /// Where it was written.
    pub at: Position,
}

/// What an export does.
#[derive(Clone, PartialEq, Debug)]
pub enum ExportKind {
    /// `export let x = 1`, `export function f() {}`, `export class C {}`.
    ///
    /// Declares *and* exports. The statement still introduces the name locally,
    /// which is why it is carried whole rather than reduced to a name.
    Declaration(Box<Stmt>),

    /// `export { a, b as c }` — names that already exist here.
    Named {
        /// What is exported, and under which name.
        specifiers: Vec<ExportSpecifier>,
        /// `from "m"`, which makes this a re-export.
        ///
        /// The difference matters: with a source, nothing is bound locally at
        /// all — the names are forwarded, and this module never sees them.
        source: Option<String>,
        /// Import attributes, legal only when there is a source.
        attributes: Vec<ImportAttribute>,
    },

    /// `export default …`.
    ///
    /// The expression form binds the value once, so a later change to what the
    /// expression referred to is not visible — unlike every named export, which
    /// is live. `export default f` and `export { f as default }` differ for
    /// exactly that reason.
    Default(ExportDefault),

    /// `export * from "m"` — every named export of `m`, except `default`.
    All {
        /// Which module.
        source: String,
        /// `as ns`, which exports them as one namespace object instead.
        alias: Option<String>,
        /// Import attributes.
        attributes: Vec<ImportAttribute>,
    },
}

/// What `export default` was given.
#[derive(Clone, PartialEq, Debug)]
pub enum ExportDefault {
    /// A function or class declaration, which may be anonymous here and nowhere
    /// else. If it has a name, that name is also bound locally.
    Declaration(Box<Stmt>),
    /// An expression, evaluated once at this point.
    Expr(Expr),
}

/// One entry of `export { … }`.
#[derive(Clone, PartialEq, Debug)]
pub struct ExportSpecifier {
    /// The local name, or — in a re-export — the name in the other module.
    ///
    /// A string for the same reason as [`ImportBinding::Named`]: an exported
    /// name need not be an identifier.
    pub local: String,
    /// What it is called to the outside. Equal to `local` when no `as` was
    /// written.
    pub exported: String,
}

/// A whole program, of either goal.
#[derive(Clone, PartialEq, Debug)]
pub struct Program {
    /// Which language this is.
    pub goal: Goal,
    /// A directive prologue, if the program opened with one.
    ///
    /// A module ignores `"use strict"` because it is already strict; a script
    /// does not, and that is where the directive earns its existence.
    pub directives: Vec<Directive>,
    /// What it contains, in order.
    pub body: Vec<ModuleItem>,
}

impl Program {
    /// An empty program of a given goal.
    pub fn new(goal: Goal) -> Self {
        Self {
            goal,
            directives: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Every import, in source order.
    ///
    /// Order matters even though imports are all resolved before anything runs:
    /// it decides the order the modules are evaluated in, which is observable
    /// through their side effects.
    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.body.iter().filter_map(|item| match item {
            ModuleItem::Import(i) => Some(i),
            _ => None,
        })
    }

    /// Every export, in source order.
    pub fn exports(&self) -> impl Iterator<Item = &Export> {
        self.body.iter().filter_map(|item| match item {
            ModuleItem::Export(e) => Some(e),
            _ => None,
        })
    }

    /// Whether anything here requires the module goal.
    ///
    /// An import or export does. Note the converse does not hold: a module with
    /// neither is still a module, which is why [`Goal`] is recorded rather than
    /// derived.
    pub fn requires_module_goal(&self) -> bool {
        self.body
            .iter()
            .any(|item| matches!(item, ModuleItem::Import(_) | ModuleItem::Export(_)))
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new(Goal::Script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;

    #[test]
    fn a_module_differs_from_a_script_before_a_statement_is_read() {
        assert!(Goal::Module.is_strict());
        assert!(!Goal::Script.is_strict());
        assert!(Goal::Module.allows_top_level_await());
        assert!(!Goal::Script.allows_top_level_await());
    }

    #[test]
    fn a_module_with_no_imports_is_still_a_module() {
        let program = Program::new(Goal::Module);
        assert!(
            !program.requires_module_goal(),
            "nothing in it needs the goal"
        );
        assert_eq!(
            program.goal,
            Goal::Module,
            "and it is one anyway — which is why the goal is recorded, not derived"
        );
    }

    #[test]
    fn an_exported_name_need_not_be_an_identifier() {
        let specifier = ExportSpecifier {
            local: "x".into(),
            exported: "a b".into(),
        };
        assert_ne!(specifier.local, specifier.exported);
    }

    #[test]
    fn a_re_export_binds_nothing_locally() {
        let forwarded = ExportKind::Named {
            specifiers: vec![ExportSpecifier {
                local: "x".into(),
                exported: "x".into(),
            }],
            source: Some("m".into()),
            attributes: vec![],
        };
        let local = ExportKind::Named {
            specifiers: vec![ExportSpecifier {
                local: "x".into(),
                exported: "x".into(),
            }],
            source: None,
            attributes: vec![],
        };

        assert_ne!(
            forwarded, local,
            "with a source the name is forwarded and this module never sees it"
        );
    }

    #[test]
    fn a_side_effect_import_binds_nothing() {
        let mut names = Names::new();
        let bare = Import {
            bindings: vec![],
            source: "m".into(),
            attributes: vec![],
            at: Position::UNKNOWN,
        };
        let bound = Import {
            bindings: vec![ImportBinding::Default(names.intern("d"))],
            ..bare.clone()
        };

        assert!(bare.is_side_effect_only());
        assert!(!bound.is_side_effect_only());
    }

    #[test]
    fn attributes_are_part_of_what_is_being_asked_for() {
        let plain = Import {
            bindings: vec![],
            source: "data".into(),
            attributes: vec![],
            at: Position::UNKNOWN,
        };
        let as_json = Import {
            attributes: vec![ImportAttribute {
                key: "type".into(),
                value: "json".into(),
            }],
            ..plain.clone()
        };

        assert_ne!(
            plain, as_json,
            "the same specifier with different attributes is a different request"
        );
    }
}
