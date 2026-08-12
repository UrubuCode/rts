//! Which names a class declares as FIELDS rather than as methods.
//!
//! # The one question this answers, and why it is worth answering
//!
//! `emit/property.rs` chooses between two read forms by SYNTACTIC POSITION: the
//! callee of a call gets the chain-capable one, because a method lives on the
//! prototype and the cheap cache can never arm for it. Its own documentation
//! says why that is a guess rather than a proof — *"nothing in this crate knows
//! what an expression's type is (there is no type pass)"* — and prices the wrong
//! guess: an own-property callee pays one load and one predicted branch it did
//! not need, measured at +1.4%, on 59.8% of member-callee sites.
//!
//! A class body is the one place the program says which of its members are
//! fields. `class C { cb: () => void; m() {} }` declares `cb` as a field and `m`
//! as a method, and `c.cb()` is therefore an OWN property read while `c.m()` is
//! a prototype one.
//!
//! # Why this may only ever choose between two legal emissions
//!
//! Because the resolution is unsound in a way nothing here can fix. A type name
//! and a value name are one `Name`: `parse/item.rs` interns `Claim::Object`'s
//! name into the same table identifiers come from, and TypeScript keeps types
//! and values in separate namespaces — so `interface Foo` and `const Foo`
//! collide here. And `interface` and `type` are erased to nothing at all, so a
//! claim naming one resolves to no class and reads as unknown.
//!
//! Neither matters, because both answers are already-legal emissions of the same
//! read. A collision picks the cheap form for something that is really a
//! prototype method: the site re-resolves on every pass, which is exactly what
//! it did before the chain form existed. Slower, never wrong.

use std::collections::{HashMap, HashSet};

use super::super::capture::{StmtChild, walk_stmt};
use crate::names::Name;
use crate::syntax::{Class, ClassElement, ClassKey, PropertyKey, Stmt, StmtKind};

/// Which classes a program declares, and what each declares as a field.
#[derive(Default)]
pub(in crate::emit) struct Classes {
    fields: HashMap<Name, HashSet<Name>>,
}

impl Classes {
    /// Whether a class of this name declares this member as a field.
    ///
    /// `false` for a class nothing declared, for a member it declares as a
    /// method, and for one it does not declare at all — all three of which are
    /// "do not take the cheap form", which is the answer that changes nothing.
    pub(in crate::emit) fn declares_field(&self, class: Name, member: Name) -> bool {
        self.fields
            .get(&class)
            .is_some_and(|fields| fields.contains(&member))
    }
}

/// Every class declared anywhere in a program body.
///
/// Whole-program rather than per-body, because a claim written in one function
/// names a class declared in another — which is the ordinary case, and the
/// reason this is not scoped the way `Facts` is.
///
/// Descends through the shared walker, so a class inside an `if`, a block or a
/// `try` is found without this file knowing those exist.
pub(in crate::emit) fn declared(body: &[Stmt]) -> Classes {
    let mut classes = Classes::default();
    for statement in body {
        collect(statement, &mut classes);
    }
    classes
}

fn collect(statement: &Stmt, into: &mut Classes) {
    if let StmtKind::Class(class) = &statement.kind {
        record(class, into);
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => collect(inner, into),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                collect(inner, into);
            }
        }
        StmtChild::Class(class) => record(class, into),
        StmtChild::Binding(_) | StmtChild::Expr(_) | StmtChild::Function(_) => {}
    });
}

/// What one class declares as instance fields.
///
/// A static field is skipped: it lives on the constructor, not on an instance,
/// so it says nothing about what `c.x` reads. A private one is skipped for a
/// blunter reason — `#x` is not spellable at a call site outside the body, so no
/// callee position can name it.
fn record(class: &Class, into: &mut Classes) {
    let Some(name) = class.name else {
        return;
    };
    let mut fields = HashSet::new();
    for element in &class.body {
        if let ClassElement::Field(field) = element
            && !field.is_static
            && let ClassKey::Public(PropertyKey::Named(key)) = &field.key
        {
            fields.insert(*key);
        }
    }
    into.fields.insert(name, fields);
}
