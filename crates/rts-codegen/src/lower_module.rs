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

/// The tier a type claim's guard exists in.
///
/// Re-exported as a constant so a caller can ask for it without depending on
/// `rts-mir` itself. `rts-host` is the crate that may name every layer and even it
/// should not have to reach past this one to name a tier.
pub const SPECIALISED: Tier = Tier::Specialised;
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
    /// What the effect pass narrowed, and what it refused to.
    pub refined: rts_mir::passes::Refined,
    /// How many guards were removed for proving nothing.
    pub dropped: rts_mir::passes::Dropped,
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

/// Both tiers of a module, with the pairing between them CHECKED.
///
/// # Why this exists, and what it caught the first time it ran
///
/// `rts_mir::guard::pair` says of itself that it exists because *"a property nothing
/// checks is a property nobody finds out about"* -- and nothing called it. It had no
/// producer outside its own tests, which is rule 10's gap rather than a feature, and the
/// property it guards had just become load-bearing: a fall from point three of the
/// specialised body must land at point three of the generic one.
///
/// The first thing it caught was live. The generic tier emitted no guard, and declaring
/// a point was a side effect of emitting one -- so the generic body claimed NO points
/// and `pair` answered `Unresumable` for every function that speculates about anything.
/// `FuncBuilder::resumable` is the fix and this is what would have found it.
///
/// # Why the check and not just the two lowerings
///
/// Because README rule 7 of `rts-mir` says both tiers come from ONE traversal, so the
/// ids match by construction -- and that is precisely the kind of claim that stops being
/// true quietly. Two lowerings handed back unchecked would be the same arrangement with
/// the net removed.
pub fn lower_module_paired(items: &[ModuleItem], resolution: &Resolution, names: &Names) -> Paired {
    let specialised = lower_module(items, resolution, names, Tier::Specialised);
    let generic = lower_module(items, resolution, names, Tier::Generic);
    let mut verdicts = Vec::with_capacity(specialised.functions.len());
    for (fast, slow) in specialised.functions.iter().zip(&generic.functions) {
        // A FUNCTION THAT DID NOT LOWER IN BOTH TIERS HAS NO PAIRING TO CHECK, and that
        // is not a failure: the two tiers refuse different things by design, since only
        // one of them speculates.
        let verdict = match (&fast.result, &slow.result) {
            (Ok(fast), Ok(slow)) => Some(rts_mir::guard::pair(fast, slow)),
            _ => None,
        };
        verdicts.push(verdict);
    }
    Paired {
        specialised,
        generic,
        verdicts,
    }
}

/// Two tiers of one module and what the pairing check said about each function.
pub struct Paired {
    /// The tier that speculates.
    pub specialised: Module,
    /// The tier a fall lands in.
    pub generic: Module,
    /// Per function, in the same order: `None` where one tier did not lower it.
    pub verdicts: Vec<Option<Result<(), rts_mir::guard::Mismatch>>>,
}

impl Paired {
    /// Every function whose tiers disagree, by name.
    pub fn unpaired(&self) -> Vec<(&str, rts_mir::guard::Mismatch)> {
        self.specialised
            .functions
            .iter()
            .zip(&self.verdicts)
            .filter_map(|(held, verdict)| match verdict {
                Some(Err(held_mismatch)) => Some((held.named.as_str(), held_mismatch.clone())),
                _ => None,
            })
            .collect()
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
        let mut result = lower_with(function, resolution, &callees, &mut domain, names, tier);
        // THE PASSES RUN HERE, so that every consumer sees the same graph. They used to
        // run in `mir_dump` alone, which meant the dump reported a refined graph and the
        // machine boundary was handed an unrefined one -- two answers to one question,
        // and the measurement that mattered was reading the worse of them.
        let mut refined = rts_mir::passes::Refined::default();
        let mut dropped = rts_mir::passes::Dropped::default();
        if let Ok(func) = result.as_mut() {
            // GUARDS FIRST, because dropping one changes which types the effect pass
            // sees: a use pointed back at an unguarded operand is a wider type, and a
            // wider type is a wider effect. The other order would narrow an effect on the
            // strength of a proof about to be removed.
            dropped = rts_mir::passes::drop_proved_guards(func, &domain);
            refined = rts_mir::passes::refine_effects(func, &domain);
        }
        out.push(Entry {
            named,
            result,
            refined,
            dropped,
        });
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
