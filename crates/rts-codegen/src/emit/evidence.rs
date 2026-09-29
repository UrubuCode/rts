//! Which names are collections, as far as the program's own text says.
//!
//! # Why this exists: a regression, measured
//!
//! `emit/methods` compiles `x.get(k)`, `x.has(k)`, `x.set(k, v)`, `x.add(v)`
//! and `x.push(v)` to a direct entry, and it chose by the MEMBER'S NAME alone.
//! The entry checks the receiver's brand, so every answer was right — and every
//! receiver that was not a collection paid the entry's fallback, which reads the
//! method with no inline cache. A user class with a method called `get` went
//! from 25 ns a call to 108, `has` to 109, `add` to 117, `push` to 116
//! (release, 2026-09-29, against the binary before the doors), and those are
//! among the commonest method names a program writes. The fixtures and the
//! bench measured the door on real collections only, so nothing said so.
//!
//! So the door is emitted only where the text gives a reason to expect a
//! collection, and everything else is the ordinary call it was.
//!
//! # What counts as a reason
//!
//! For a name, over the WHOLE program:
//!
//! - a `const` initialised with `new Map(…)`, `new Set(…)`, `new Array(…)` or
//!   an array literal — which is a proof, since a `const` is never reassigned
//!   and `emit/methods` separately requires the constructor to be the
//!   language's;
//! - a declaration or a parameter ANNOTATED `Map`, `Set`, `T[]` and their
//!   read-only spellings — a claim, and rule 4 of this crate says a claim may
//!   choose a specialisation where something still checks it. The entry's brand
//!   check is that something: a claim that is wrong costs the fallback and
//!   changes no answer;
//! - a rest parameter, which is an array by construction.
//!
//! A name qualifies only when EVERY declaration of it in the program says the
//! same brand. `Names` are interned by spelling, so two unrelated bindings
//! called `m` are one name here; asking them to agree is what makes a table
//! keyed by name safe to read without resolving scopes, the same trade
//! `inline::declarations_of` makes.
//!
//! # What it deliberately does not see
//!
//! A member of an object — `this.cache.get(k)` — has no name of its own, so it
//! stays the ordinary call: no gain, and no tax. An import is declared in
//! another unit's text and is not visited. Both are losses of speed only, in
//! the direction that cannot be wrong.

use std::collections::BTreeMap;

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::names::{Name, Names};
use crate::syntax::{
    BindingKind, Claim, ClassElement, Expr, ExprKind, Function, FunctionBody, Pattern, Stmt, StmtKind,
};

/// What a receiver is expected to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Brand {
    /// A `Map`.
    Map,
    /// A `Set`.
    Set,
    /// An array.
    Array,
}

/// The names the program's text expects to be collections.
#[derive(Clone, Default, Debug)]
pub(crate) struct Evidence {
    /// `None` is a name declared without a brand somewhere, or with two.
    known: BTreeMap<Name, Option<Brand>>,
}

impl Evidence {
    /// What `value` is expected to be, when it is a name every declaration of
    /// which agrees.
    pub(crate) fn of(&self, value: &Expr) -> Option<Brand> {
        match &super::light_arguments::unasserted(value).kind {
            ExprKind::Ident(name) => self.known.get(name).copied().flatten(),
            _ => None,
        }
    }

    fn declare(&mut self, name: Name, brand: Option<Brand>) {
        self.known
            .entry(name)
            .and_modify(|held| {
                if *held != brand {
                    *held = None;
                }
            })
            .or_insert(brand);
    }

    fn refuse(&mut self, pattern: &Pattern) {
        let mut bound = Vec::new();
        pattern.bound_names(&mut bound);
        for name in bound {
            self.declare(name, None);
        }
    }
}

/// Gathers the evidence of every body of a program.
pub(crate) fn gather<'a>(bodies: impl IntoIterator<Item = &'a [Stmt]>, names: &Names) -> Evidence {
    let mut walk = Walk {
        found: Evidence::default(),
        names,
    };
    for body in bodies {
        for statement in body {
            walk.statement(statement);
        }
    }
    walk.found
}

struct Walk<'a> {
    found: Evidence,
    names: &'a Names,
}

impl Walk<'_> {
    fn statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Declare { kind, bindings } => {
                for binding in bindings {
                    match &binding.target {
                        Pattern::Name(name) => {
                            let brand = match (&binding.claim, kind, &binding.value) {
                                (Some(claim), _, _) => self.claimed(claim),
                                (None, BindingKind::Const, Some(value)) => self.made(value),
                                _ => None,
                            };
                            self.found.declare(*name, brand);
                        }
                        other => self.found.refuse(other),
                    }
                }
            }
            StmtKind::Using { bindings, .. } => {
                for binding in bindings {
                    self.found.refuse(&binding.target);
                }
            }
            _ => {}
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner),
            StmtChild::Expr(value) => self.expression(value),
            StmtChild::Binding(binding) => {
                // A `for` header's own declaration arrives here without its kind,
                // and a `Declare` statement's bindings arrive here a second time:
                // the claim decides for the first, and the second says again what
                // the arm above already said.
                if !matches!(statement.kind, StmtKind::Declare { .. } | StmtKind::Using { .. }) {
                    match (&binding.target, &binding.claim) {
                        (Pattern::Name(name), Some(claim)) => {
                            let brand = self.claimed(claim);
                            self.found.declare(*name, brand);
                        }
                        (other, _) => self.found.refuse(other),
                    }
                }
                if let Some(value) = &binding.value {
                    self.expression(value);
                }
            }
            StmtChild::Catch(clause) => {
                if let Some(bound) = &clause.binding {
                    self.found.refuse(bound);
                }
                for inner in &clause.body {
                    self.statement(inner);
                }
            }
            StmtChild::Function(function) => {
                if let Some(name) = function.name {
                    self.found.declare(name, None);
                }
                self.function(function);
            }
            StmtChild::Class(class) => self.class(class),
        });
    }

    fn expression(&mut self, value: &Expr) {
        walk_expr(value, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(function) => {
                if let Some(name) = function.name {
                    self.found.declare(name, None);
                }
                self.function(function);
            }
            Child::Class(class) => self.class(class),
        });
    }

    fn class(&mut self, class: &crate::syntax::Class) {
        if let Some(name) = class.name {
            self.found.declare(name, None);
        }
        for element in &class.body {
            match element {
                ClassElement::Method(method) => self.function(&method.function),
                ClassElement::Field(field) => {
                    if let Some(value) = &field.value {
                        self.expression(value);
                    }
                }
                ClassElement::StaticBlock(body) => {
                    for inner in body {
                        self.statement(inner);
                    }
                }
            }
        }
    }

    fn function(&mut self, function: &Function) {
        for parameter in &function.parameters {
            match (&parameter.target, &parameter.claim) {
                (Pattern::Name(name), Some(claim)) => {
                    let brand = self.claimed(claim);
                    self.found.declare(*name, brand);
                }
                (other, _) => self.found.refuse(other),
            }
            if let Some(value) = &parameter.default {
                self.expression(value);
            }
        }
        match &function.rest_parameter {
            // `...xs` is an array the language makes, whatever the call passed.
            Some(Pattern::Name(name)) => self.found.declare(*name, Some(Brand::Array)),
            Some(other) => self.found.refuse(other),
            None => {}
        }
        match &function.body {
            FunctionBody::Block(body) => {
                for inner in body {
                    self.statement(inner);
                }
            }
            FunctionBody::Expression(value) => self.expression(value),
        }
    }

    /// What an annotation says the value is.
    fn claimed(&self, claim: &Claim) -> Option<Brand> {
        match claim {
            Claim::Array(_) => Some(Brand::Array),
            Claim::Object(name) => match self.names.text(*name) {
                "Map" | "ReadonlyMap" => Some(Brand::Map),
                "Set" | "ReadonlySet" => Some(Brand::Set),
                "Array" | "ReadonlyArray" => Some(Brand::Array),
                _ => None,
            },
            _ => None,
        }
    }

    /// What an initialiser makes.
    fn made(&self, value: &Expr) -> Option<Brand> {
        match &super::light_arguments::unasserted(value).kind {
            ExprKind::Array { .. } => Some(Brand::Array),
            ExprKind::New { callee, .. } => match &callee.kind {
                ExprKind::Ident(name) => match self.names.text(*name) {
                    "Map" => Some(Brand::Map),
                    "Set" => Some(Brand::Set),
                    "Array" => Some(Brand::Array),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}
