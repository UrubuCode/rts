//! A module's functions, numbered and lowered together.
//!
//! # Why this exists, and why it is what the measurement asked for
//!
//! `rts mir` over every fifth file of `tests/` — 182 files, 1 725 functions —
//! refused **1 074 of them for a call**, thirteen times the next reason. The list
//! written before that survey put `do`-`while` and `for` next; they were ten.
//! `docs/engine/four-stages.md` carries the table.
//!
//! And the top item turned out not to be a construct to lower. A call has to name
//! a callee, `rts_mir::cfg::Callee::Func` names one by index, and an index exists
//! only if the program's functions have been numbered. A lowering that takes one
//! function at a time has nothing for a call to name — so the answer to "lower
//! calls" is "lower modules", and that is a different shape rather than a bigger
//! version of the same one.
//!
//! # What a module shares, and why each thing is shared
//!
//! - **The domain.** An assertion index minted for one function must mean the same
//!   thing in the next, because a guard hoisted out of a call is one assertion in
//!   two graphs.
//! - **The function numbering.** It is what a call names.
//! - **The callee map.** Which binding is which function, so that a call by name
//!   can be resolved without guessing from a spelling — E2's answer, used.

use std::collections::BTreeMap;

use rts_mir::cfg::{Func, FuncId};
use rts_mir::guard::Tier;

use crate::domain::Js;
use crate::emit::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::lower::{Callees, Unsupported, lower_with};
use crate::names::Names;
use crate::names::resolve::Resolution;
use crate::syntax::{
    Class, ClassElement, ExportDefault, ExportKind, Expr, Function, FunctionBody, ModuleItem,
    Pattern, Stmt, StmtKind,
};

/// One function of a module: what it is called, and its graph or its refusal.
pub struct Entry {
    /// What to call it in a report. A function with no name is identified by where
    /// it was written, which is the only thing that distinguishes two of them.
    pub named: String,
    /// The graph, or why there is none.
    pub result: Result<Func, Unsupported>,
}

/// A module, lowered.
pub struct Module {
    /// Its functions, in the order they were found — which is the order their
    /// [`FuncId`]s run in, so an index into this is a `FuncId`.
    pub functions: Vec<Entry>,
    /// The tables every graph's indices refer to.
    pub domain: Js,
}

impl Module {
    /// How many lowered.
    pub fn lowered(&self) -> usize {
        self.functions
            .iter()
            .filter(|held| held.result.is_ok())
            .count()
    }
}

/// Every function of a module, numbered and lowered against one set of tables.
pub fn lower_module(
    items: &[ModuleItem],
    resolution: &Resolution,
    names: &Names,
    tier: Tier,
) -> Module {
    let functions = every_function(items);
    // THE CALLEE MAP IS BUILT FIRST, over every function, because a call may name
    // one written later in the file — `function a() { return b(); } function b() {}`
    // is ordinary code, and a map built as the lowering walked would answer nothing
    // for `b`.
    let callees = Callees::of(items, &functions, resolution);

    let mut domain = Js::new();
    let mut out = Vec::with_capacity(functions.len());
    for function in &functions {
        let named = match function.name {
            Some(name) => names.text(name).to_owned(),
            None => format!("<anonymous at {}>", function.at.0),
        };
        let result = lower_with(function, resolution, &callees, &mut domain, tier);
        out.push(Entry { named, result });
    }
    Module {
        functions: out,
        domain,
    }
}

/// Every function a module contains, at any depth, in source order.
///
/// The order IS the numbering, so it must be stable: a `FuncId` that meant one
/// function in one compilation and another in the next would make a call name the
/// wrong body, and nothing would say so. Source order is stable by construction.
///
/// The traversal goes through `emit::capture` rather than describing the tree a
/// second time, for the reason that file's header gives — and matching
/// `StmtKind::Function` here as well was how every declared function came out
/// TWICE, because `walk_stmt` hands one over as a child and stops.
pub fn every_function(items: &[ModuleItem]) -> Vec<&Function> {
    let mut found = Vec::new();
    for item in items {
        match item {
            ModuleItem::Stmt(statement) => in_statement(statement, &mut found),
            ModuleItem::Export(export) => match &export.kind {
                ExportKind::Declaration(statement) => in_statement(statement, &mut found),
                ExportKind::Default(ExportDefault::Declaration(statement)) => {
                    in_statement(statement, &mut found)
                }
                ExportKind::Default(ExportDefault::Expr(expr)) => in_expr(expr, &mut found),
                ExportKind::Named { .. } | ExportKind::All { .. } => {}
            },
            ModuleItem::Import(_) => {}
        }
    }
    found
}

fn in_statement<'a>(statement: &'a Stmt, found: &mut Vec<&'a Function>) {
    walk_stmt(statement, &mut |child| match child {
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
        StmtChild::Function(function) => {
            found.push(function);
            body_of(function, found);
        }
        StmtChild::Class(class) => in_class(class, found),
    });
}

fn in_expr<'a>(expr: &'a Expr, found: &mut Vec<&'a Function>) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => in_expr(inner, found),
        Child::Function(function) => {
            found.push(function);
            body_of(function, found);
        }
        Child::Class(class) => in_class(class, found),
    });
}

fn body_of<'a>(function: &'a Function, found: &mut Vec<&'a Function>) {
    match &function.body {
        FunctionBody::Block(statements) => {
            for held in statements {
                in_statement(held, found);
            }
        }
        FunctionBody::Expression(expr) => in_expr(expr, found),
    }
}

fn in_class<'a>(class: &'a Class, found: &mut Vec<&'a Function>) {
    for element in &class.body {
        match element {
            ClassElement::Method(method) => {
                found.push(&method.function);
                body_of(&method.function, found);
            }
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    in_expr(value, found);
                }
            }
            ClassElement::StaticBlock(statements) => {
                for held in statements {
                    in_statement(held, found);
                }
            }
        }
    }
}

/// Which declaration is which function, for the whole module.
///
/// Built from the numbering above plus the scope tree: a function declaration and a
/// `const f = …` both bind a name in a scope, and E2 says which binding that is. So
/// a call written as a bare name resolves to a binding, and a binding resolves to a
/// function — without anything here comparing spellings.
///
/// A method is deliberately absent: it is reached through a receiver rather than
/// through a binding, and proving which one it is is `emit/receiver.rs`'s question.
pub(crate) fn callee_map(
    functions: &[&Function],
    resolution: &Resolution,
) -> BTreeMap<crate::names::resolve::BindingId, FuncId> {
    let mut found = BTreeMap::new();
    for (at, function) in functions.iter().enumerate() {
        let id = FuncId(at as u32);
        // A function knows its own name only when it was written with one, and the
        // binding it landed in is the one the scope ENCLOSING its body holds.
        let Some(body) = resolution.function_scope(function.at) else {
            continue;
        };
        let Some(outside) = resolution.scope(body).parent else {
            continue;
        };
        let Some(name) = function.name else {
            continue;
        };
        if let Some(binding) = resolution.binding_in(outside, name) {
            found.insert(binding, id);
        }
    }
    found
}

/// Names a `const f = function () {}` and a `const f = () => …` too.
///
/// A function EXPRESSION usually has no name of its own, so the binding it lands in
/// is the declaration's, not the function's — which the map above cannot see. This
/// walks the declarations instead and matches them to the numbering by POSITION,
/// which is the only thing the two views share.
pub(crate) fn bound_expressions(
    items: &[ModuleItem],
    functions: &[&Function],
    resolution: &Resolution,
) -> BTreeMap<crate::names::resolve::BindingId, FuncId> {
    let by_position: BTreeMap<rts_cranelift::fault::Position, FuncId> = functions
        .iter()
        .enumerate()
        .map(|(at, held)| (held.at, FuncId(at as u32)))
        .collect();
    let mut found = BTreeMap::new();
    let mut pending: Vec<(&Stmt, ())> = Vec::new();
    for item in items {
        if let ModuleItem::Stmt(statement) = item {
            pending.push((statement, ()));
        }
        if let ModuleItem::Export(export) = item
            && let ExportKind::Declaration(statement) = &export.kind
        {
            pending.push((statement, ()));
        }
    }
    while let Some((statement, ())) = pending.pop() {
        if let StmtKind::Declare { bindings, .. } = &statement.kind {
            for binding in bindings {
                let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value) else {
                    continue;
                };
                let crate::syntax::ExprKind::Function(function) = &value.kind else {
                    continue;
                };
                let Some(id) = by_position.get(&function.at) else {
                    continue;
                };
                // The scope the DECLARATION is in, which is where its name lives.
                // Resolving from the module scope would find a different binding
                // wherever the name is declared more than once.
                let Some(body) = resolution.function_scope(function.at) else {
                    continue;
                };
                let Some(outside) = resolution.scope(body).parent else {
                    continue;
                };
                if let Some(binding) = resolution.binding_in(outside, *name) {
                    found.insert(binding, *id);
                }
            }
        }
        walk_stmt(statement, &mut |child| {
            if let StmtChild::Stmt(inner) = child {
                pending.push((inner, ()));
            }
        });
    }
    found
}

#[cfg(test)]
#[path = "lower_module_tests.rs"]
mod tests;
