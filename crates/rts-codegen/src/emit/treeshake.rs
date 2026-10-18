//! What a declaration-level tree-shaker WOULD remove, counted and not removed.
//!
//! `RTS_TREESHAKE_STATS=1` prints, once per compilation, how many top-level
//! functions, classes and function-valued bindings the program declares and
//! how many nothing reaches. It changes no emitted code: the precedent is
//! `RTS_ESCAPE_STATS` (299 candidates, 10 substitutions) — measure the prize
//! before building the pruner.
//!
//! # How honest the count is
//!
//! Reachability is by NAME, over the concatenation of every unit (the same
//! slice `whole_program_facts` receives). `graph::front_end` parses every file
//! into one `Names`, so two modules declaring `helper` share a `Name`; the
//! count then treats them as one declaration and one reference keeps both. That
//! only ever OVER-keeps, so `unreferenced` is a floor on what a real pruner
//! could drop, never a promise it could drop more.
//!
//! Roots are every top-level statement that is not a function/class/function
//! binding declaration (they run), every export of the ENTRY unit, and every
//! export another unit imports by name. A namespace import, a re-export, or any
//! of `eval`, `Function`, `globalThis`, `with`, `import(...)` anywhere in the
//! program ends the analysis: "dynamic — all kept", the same refusal
//! `primordial` makes.
//!
//! Not counted: class members (a method of a reachable class is kept whole) and
//! non-function top-level bindings, whose initialisers may have effects.

use std::collections::{BTreeSet, HashMap};

use crate::names::{Name, Names};
use crate::syntax::{
    ClassElement, Expr, ExprKind, Function, FunctionBody, Import, ImportBinding,
    Pattern, Stmt, StmtKind,
};

use super::capture::{Child, StmtChild, all_names_in_statement, walk_expr, walk_stmt};
use super::module::{Publication, PublicationSource};

/// Whether anything asked for the report. Read once per compilation.
pub(super) fn wanted() -> bool {
    std::env::var_os("RTS_TREESHAKE_STATS").is_some()
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Function,
    Class,
    FunctionBinding,
}

/// The units of a compilation, entry LAST, as `emit_unit` holds them.
pub(super) struct Unit<'a> {
    pub imports: &'a [Import],
    pub body: &'a [Stmt],
    pub publications: &'a [Publication],
}

/// The one function-valued thing a `const f = ...` binding names, if it is one.
fn function_valued(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Function(_) | ExprKind::Class(_) => true,
        ExprKind::Asserted { value, .. } => function_valued(value),
        _ => false,
    }
}

/// The name a top-level statement declares, when it is a prunable declaration.
fn declared(statement: &Stmt) -> Option<(Name, Shape)> {
    match &statement.kind {
        StmtKind::Function(function) => function.name.map(|n| (n, Shape::Function)),
        StmtKind::Class(class) => class.name.map(|n| (n, Shape::Class)),
        StmtKind::Declare { bindings, .. } => {
            let [binding] = bindings.as_slice() else {
                return None;
            };
            let Pattern::Name(name) = &binding.target else {
                return None;
            };
            let value = binding.value.as_ref()?;
            function_valued(value).then_some((*name, Shape::FunctionBinding))
        }
        _ => None,
    }
}

/// Whether one name is among the ones that make static reasoning unsound.
struct Dynamic {
    found: bool,
    eval: Name,
    function: Name,
    global_this: Name,
}

impl Dynamic {
    fn function(&mut self, function: &Function) {
        match &function.body {
            FunctionBody::Block(body) => body.iter().for_each(|s| self.stmt(s)),
            FunctionBody::Expression(value) => self.expr(value),
        }
    }

    fn stmt(&mut self, statement: &Stmt) {
        if self.found {
            return;
        }
        if matches!(statement.kind, StmtKind::With { .. }) {
            self.found = true;
            return;
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.stmt(inner),
            StmtChild::Expr(expr) => self.expr(expr),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    self.expr(value);
                }
            }
            StmtChild::Catch(catch) => catch.body.iter().for_each(|s| self.stmt(s)),
            StmtChild::Function(function) => self.function(function),
            StmtChild::Class(class) => self.class(&class.body),
        });
    }

    fn class(&mut self, body: &[ClassElement]) {
        for element in body {
            match element {
                ClassElement::Method(method) => self.function(&method.function),
                ClassElement::Field(field) => {
                    if let Some(value) = &field.value {
                        self.expr(value);
                    }
                }
                ClassElement::StaticBlock(statements) => {
                    statements.iter().for_each(|s| self.stmt(s))
                }
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        if self.found {
            return;
        }
        match &expr.kind {
            ExprKind::Ident(name)
                if *name == self.eval || *name == self.function || *name == self.global_this =>
            {
                self.found = true;
                return;
            }
            ExprKind::ImportCall { .. } => {
                self.found = true;
                return;
            }
            _ => {}
        }
        walk_expr(expr, &mut |child| match child {
            Child::Expr(inner) => self.expr(inner),
            Child::Function(function) => self.function(function),
            Child::Class(class) => self.class(&class.body),
        });
    }
}

/// Counts and prints. `units` is dependencies first, the entry last.
pub(super) fn report(units: &[Unit], names: &mut Names) {
    let mut dynamic = Dynamic {
        found: false,
        eval: names.intern("eval"),
        function: names.intern("Function"),
        global_this: names.intern("globalThis"),
    };
    units.iter().for_each(|u| u.body.iter().for_each(|s| dynamic.stmt(s)));

    // Every declaration, by name, with its statements.
    let mut declarations: HashMap<Name, Vec<(&Stmt, Shape)>> = HashMap::new();
    let mut order: Vec<Name> = Vec::new();
    let mut roots: BTreeSet<Name> = BTreeSet::new();
    for unit in units {
        for statement in unit.body {
            match declared(statement) {
                Some((name, shape)) => {
                    let slot = declarations.entry(name).or_default();
                    if slot.is_empty() {
                        order.push(name);
                    }
                    slot.push((statement, shape));
                }
                None => all_names_in_statement(statement, &mut roots),
            }
        }
    }

    // Exports: the entry's are the program's surface; another unit's matter
    // only when somebody imports them (or imports a whole namespace).
    let mut imported: BTreeSet<String> = BTreeSet::new();
    let mut everything_imported = false;
    for unit in units {
        for import in unit.imports {
            for binding in &import.bindings {
                match binding {
                    ImportBinding::Default(_) => {
                        imported.insert("default".to_string());
                    }
                    ImportBinding::Named { exported, .. } => {
                        imported.insert(exported.clone());
                    }
                    ImportBinding::Namespace(_) => everything_imported = true,
                }
            }
        }
    }
    let last = units.len().saturating_sub(1);
    for (position, unit) in units.iter().enumerate() {
        for publication in unit.publications {
            match &publication.source {
                PublicationSource::Local(name) => {
                    if position == last || everything_imported || imported.contains(&publication.exported)
                    {
                        roots.insert(*name);
                    }
                }
                _ => everything_imported = true,
            }
        }
    }

    // Fixed point: a reached declaration reaches whatever its text names.
    let mut reached: BTreeSet<Name> = BTreeSet::new();
    let mut work: Vec<Name> = roots.iter().copied().collect();
    while let Some(name) = work.pop() {
        if !reached.insert(name) {
            continue;
        }
        if let Some(slot) = declarations.get(&name) {
            let mut inside = BTreeSet::new();
            for (statement, _) in slot {
                all_names_in_statement(statement, &mut inside);
            }
            work.extend(inside.into_iter().filter(|n| !reached.contains(n)));
        }
    }

    let count = |shape: Shape, only_dead: bool| {
        order
            .iter()
            .flat_map(|n| declarations[n].iter().map(move |(_, s)| (*n, *s)))
            .filter(|(n, s)| *s == shape && (!only_dead || !reached.contains(n)))
            .count()
    };
    let total = |s| count(s, false);
    let dead = |s| count(s, true);
    eprintln!("treeshake-census units {}", units.len());
    if dynamic.found {
        eprintln!("treeshake-census dynamic-all-kept 1");
    }
    for (label, shape) in [
        ("function", Shape::Function),
        ("class", Shape::Class),
        ("function-binding", Shape::FunctionBinding),
    ] {
        eprintln!("treeshake-census {label}-total {}", total(shape));
        eprintln!("treeshake-census {label}-unreferenced {}", dead(shape));
    }
    let all = total(Shape::Function) + total(Shape::Class) + total(Shape::FunctionBinding);
    let gone = dead(Shape::Function) + dead(Shape::Class) + dead(Shape::FunctionBinding);
    eprintln!("treeshake-census declarations-total {all}");
    eprintln!(
        "treeshake-census declarations-prunable {}",
        if dynamic.found { 0 } else { gone }
    );
    eprintln!("treeshake-census declarations-unreferenced-ignoring-dynamic {gone}");
}
