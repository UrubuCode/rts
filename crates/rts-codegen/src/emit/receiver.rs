//! Method calls whose callee the program decides, so no call is made.
//!
//! # What it costs today
//!
//! Measured 2026-08-30, release, min of 9 over 3 M iterations:
//!
//! ```text
//! callee.m(a)                             19.00 ns
//! the same function through a binding      18.00 ns
//! the property read alone                   6.00 ns
//! a substituted call                        1.00 ns
//! ```
//!
//! Being a method costs about ONE nanosecond. The row is a real call, and a
//! real call is one runtime crossing — 15.7 ns here — plus about two of
//! bookkeeping in `entry::called`, which keeps three stacks JavaScript observes.
//! So the way to remove it is not to make the call cheaper. It is not to call.
//!
//! # Why this is a proof and not a guard
//!
//! The published design is speculate, guard, deoptimise. RTS is ahead-of-time
//! and has nothing to bail out to, and the variant that fits — a guard whose
//! miss is an ordinary call — needs an identity to compare against. The cheapest
//! one a compiled site could compare is the closure CELL, which does not exist
//! at compile time, so it would need a cache word and a resolver.
//!
//! None of that is needed when the program already says which function it is.
//! `const o = new C(); … o.m(x)` names one method as surely as `f(x)` names one
//! function, provided nothing can change what `o.m` reaches — and every way of
//! changing it has to SPELL either `o` or `C`, which is a search.
//!
//! Deciding it at compile time also keeps the engine's own rule: the emitted
//! program is the same on every build, there is no cache whose contents steer
//! anything, and nothing is decided while emitting.
//!
//! # The clauses, and what each one closes
//!
//! For `const o = new C()` to make `o.m` a known function:
//!
//! - `o` is a `const`, initialised by `new C()` with `C` a plain identifier.
//! - `o` IS NEVER READ AS A VALUE. Every occurrence of it is the object of a
//!   `o.<name>(…)` call. This is the clause that closes `o.m = f`, `o.__proto__
//!   = …`, `Object.setPrototypeOf(o, …)`, `delete o.m`, and handing `o` to
//!   anything that could do those — all of them spell `o` somewhere that is not
//!   a call's receiver.
//! - `C` IS NEVER READ AS A VALUE EITHER, except as the callee of `new C()`.
//!   That closes `C.prototype.m = f` and `C.prototype = {}` by the same
//!   argument, and it is why the search is over the program rather than the
//!   body: a class can be written to from anywhere it is named.
//! - `C` is declared exactly once, as a `class`, and so is every class in its
//!   `extends` chain — each of which is reached by the same rule.
//! - No `with` and no `eval`, which put bindings in scope that no declaration
//!   spells and would make both counts prove nothing.
//!
//! What is deliberately NOT required: that `m` is on `C` itself. The chain is
//! walked, because `derived.bp()` where `bp` is on the base is the shape half of
//! the benchmark's method rows are written in.

use std::collections::{BTreeMap, BTreeSet};

use crate::Name;
use crate::syntax::{
    BindingKind, Class, ClassElement, Expr, ExprKind, Function, Pattern, Stmt, StmtKind,
};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};

/// A receiver whose class the program decides, and the class it holds.
pub(super) struct Resolved {
    /// Every `(receiver, method)` pair this body may substitute, and the
    /// function each names.
    pub methods: BTreeMap<(Name, Name), Function>,
}

/// Which `const o = new C()` receivers of this body have a decidable class.
///
/// Answered ONCE for the whole module rather than per body, because every
/// clause it rests on is about the whole module already: what `C` is, whether
/// anything writes through it, whether `o` is ever read as a value. The only
/// per-body part would be which receivers this activation holds, and a receiver
/// declared exactly once — required below for the same reason
/// `inline::candidates` requires it — is in scope wherever its spelling is.
pub(super) fn resolve(program: &[Stmt]) -> Resolved {
    let mut methods = BTreeMap::new();
    let mut constructed = Vec::new();
    for statement in program {
        constructions(statement, &mut constructed);
    }
    if constructed.is_empty() {
        return Resolved { methods };
    }

    // Every class the program declares, by name, and how many times the name is
    // declared at all. A second declaration means the spelling does not decide
    // the class, which is the same reason `inline::candidates` counts.
    let mut classes: BTreeMap<Name, &Class> = BTreeMap::new();
    let mut declared: BTreeMap<Name, usize> = BTreeMap::new();
    for statement in program {
        class_declarations(statement, &mut classes, &mut declared);
    }

    // Every name read as a VALUE anywhere in the program, where the receiver of
    // a call and the callee of `new` do not count. One walk for both questions,
    // because they are the same question asked about two names.
    let mut read_as_value = BTreeSet::new();
    for statement in program {
        value_reads_in_statement(statement, &mut read_as_value);
    }

    for (receiver, class_name) in constructed {
        if read_as_value.contains(&receiver) || read_as_value.contains(&class_name) {
            continue;
        }
        // BOTH names declared exactly once. For the class it is what makes the
        // spelling decide which class; for the RECEIVER it is what makes one
        // answer serve the whole module, since two `const o = new …` in two
        // functions would otherwise share a key and the first would win.
        if declared.get(&class_name).copied() != Some(1)
            || declared.get(&receiver).copied() != Some(1)
        {
            continue;
        }
        let mut walking = classes.get(&class_name).copied();
        let mut seen = BTreeSet::new();
        while let Some(class) = walking {
            for element in &class.body {
                let ClassElement::Method(method) = element else {
                    continue;
                };
                if method.is_static || method.kind != crate::syntax::MethodKind::Normal {
                    continue;
                }
                let Some(name) = plain_key(&method.key) else {
                    continue;
                };
                // The FIRST one wins, which is what a prototype chain does: a
                // derived class's own method shadows the base's, and the walk
                // starts at the derived one.
                methods
                    .entry((receiver, name))
                    .or_insert_with(|| (*method.function).clone());
            }
            // Up one link, by the same rule that let us in: a base named by an
            // expression that is not a once-declared class ends the walk rather
            // than being guessed at.
            let Some(heritage) = &class.heritage else {
                break;
            };
            let ExprKind::Ident(base) = &heritage.kind else {
                break;
            };
            if read_as_value.contains(base) || declared.get(base).copied() != Some(1) {
                break;
            }
            // A cycle cannot happen in a legal program, and a walk that trusts
            // that is a walk that hangs on an illegal one.
            if !seen.insert(*base) {
                break;
            }
            walking = classes.get(base).copied();
        }
    }
    Resolved { methods }
}

/// Every `const o = new C()` of this body, at any depth.
fn constructions(statement: &Stmt, found: &mut Vec<(Name, Name)>) {
    if let StmtKind::Declare {
        kind: BindingKind::Const,
        bindings,
    } = &statement.kind
    {
        for binding in bindings {
            let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value) else {
                continue;
            };
            let ExprKind::New { callee, .. } = &value.kind else {
                continue;
            };
            if let ExprKind::Ident(class) = &callee.kind {
                found.push((*name, *class));
            }
        }
        return;
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => constructions(inner, found),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                constructions(inner, found);
            }
        }
        StmtChild::Expr(_) | StmtChild::Binding(_) => {}
        // Into nested code as well, because the answer is for the module: a
        // receiver declared inside a function is still declared once, and every
        // clause that lets it in is about everywhere its name appears.
        StmtChild::Function(function) => constructions_in_function(function, found),
        StmtChild::Class(_) => {}
    });
}

/// Every class the program declares, and how often each name is declared.
fn class_declarations<'a>(
    statement: &'a Stmt,
    classes: &mut BTreeMap<Name, &'a Class>,
    declared: &mut BTreeMap<Name, usize>,
) {
    match &statement.kind {
        StmtKind::Class(class) => {
            if let Some(name) = class.name {
                *declared.entry(name).or_insert(0) += 1;
                classes.insert(name, class);
            }
        }
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Pattern::Name(name) = &binding.target {
                    *declared.entry(*name).or_insert(0) += 1;
                    if let Some(value) = &binding.value
                        && let ExprKind::Class(class) = &value.kind
                    {
                        classes.insert(*name, class);
                    }
                }
            }
        }
        StmtKind::Function(function) => {
            if let Some(name) = function.name {
                *declared.entry(name).or_insert(0) += 1;
            }
        }
        _ => {}
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => class_declarations(inner, classes, declared),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                class_declarations(inner, classes, declared);
            }
        }
        StmtChild::Expr(_) | StmtChild::Binding(_) => {}
        // INTO nested code, because this is a COUNT and a class declared inside
        // a function still spends the spelling. Skipping it made a top-level `C`
        // and a nested `C` look like one declaration, which is the count saying
        // a name decides a class when it does not.
        StmtChild::Function(function) => {
            if let crate::syntax::FunctionBody::Block(body) = &function.body {
                for inner in body {
                    class_declarations(inner, classes, declared);
                }
            }
        }
        StmtChild::Class(_) => {}
    });
}

/// A method key that is a plain name. A computed one names nothing decidable.
fn plain_key(key: &crate::syntax::ClassKey) -> Option<Name> {
    match key {
        crate::syntax::ClassKey::Public(crate::syntax::PropertyKey::Named(name)) => Some(*name),
        // A COMPUTED key names nothing decidable, and a PRIVATE one is
        // reachable only from inside the class body — which a substituted call
        // site is not.
        _ => None,
    }
}

/// Every name read AS A VALUE, where two positions are not reads.
///
/// The receiver of a method call — `o` in `o.m(…)` — and the callee of `new` are
/// the two the analysis is asking about, so they are excluded and everything
/// else counts. `o.m` without a call IS a read, because it produces the function
/// rather than calling it, and so is `o.m = f`.
fn value_reads_in_statement(statement: &Stmt, found: &mut BTreeSet<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => value_reads_in_statement(inner, found),
        StmtChild::Expr(expr) => value_reads(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                value_reads(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                value_reads_in_statement(inner, found);
            }
        }
        StmtChild::Function(function) => nested_names(function, found),
        StmtChild::Class(class) => class_names(class, found),
    });
}

fn value_reads(expr: &Expr, found: &mut BTreeSet<Name>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            found.insert(*name);
            return;
        }
        // `o.m(args)` — the receiver is not a value read, the arguments are.
        ExprKind::Call {
            callee,
            arguments,
            optional: false,
        } if receiver_of(callee).is_some() => {
            for argument in arguments {
                let (crate::syntax::Spreadable::Single(value)
                | crate::syntax::Spreadable::Spread(value)) = argument;
                value_reads(value, found);
            }
            return;
        }
        // `new C(args)` — the callee is not a value read.
        ExprKind::New { callee, arguments } if matches!(&callee.kind, ExprKind::Ident(_)) => {
            for argument in arguments {
                let (crate::syntax::Spreadable::Single(value)
                | crate::syntax::Spreadable::Spread(value)) = argument;
                value_reads(value, found);
            }
            return;
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => value_reads(inner, found),
        Child::Function(function) => nested_names(function, found),
        Child::Class(class) => class_names(class, found),
    });
}

/// The receiver of `o.m(…)`, when the callee is exactly that shape.
pub(super) fn receiver_of(callee: &Expr) -> Option<(Name, Name)> {
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return None;
    };
    let ExprKind::Ident(receiver) = &object.kind else {
        return None;
    };
    Some((*receiver, *property))
}

fn nested_names(function: &Function, found: &mut BTreeSet<Name>) {
    for parameter in &function.parameters {
        if let Some(default) = &parameter.default {
            value_reads(default, found);
        }
    }
    match &function.body {
        crate::syntax::FunctionBody::Block(body) => {
            for statement in body {
                value_reads_in_statement(statement, found);
            }
        }
        crate::syntax::FunctionBody::Expression(expr) => value_reads(expr, found),
    }
}

fn class_names(class: &Class, found: &mut BTreeSet<Name>) {
    // `class D extends B` is NOT a value read of `B`, and counting it as one
    // broke the chain walk: every base was refused by the clause meant to catch
    // someone WRITING through the name. Extending only reads a prototype — it
    // cannot reassign one — which is the same argument that excludes the callee
    // of `new C()`.
    if let Some(heritage) = &class.heritage
        && !matches!(&heritage.kind, ExprKind::Ident(_))
    {
        value_reads(heritage, found);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => nested_names(&method.function, found),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    value_reads(value, found);
                }
            }
            ClassElement::StaticBlock(body) => {
                for statement in body {
                    value_reads_in_statement(statement, found);
                }
            }
        }
    }
}

/// The same, through a nested function's body.
fn constructions_in_function(function: &Function, found: &mut Vec<(Name, Name)>) {
    if let crate::syntax::FunctionBody::Block(body) = &function.body {
        for statement in body {
            constructions(statement, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;
    use crate::syntax::ModuleItem;

    /// The `(receiver, method)` pairs a source decides, as strings.
    fn decided(source: &str) -> Vec<String> {
        let mut names = Names::default();
        let program = parse_script(source, &mut names).expect("parses");
        let body: Vec<Stmt> = program
            .body
            .into_iter()
            .filter_map(|item| match item {
                ModuleItem::Stmt(statement) => Some(statement),
                _ => None,
            })
            .collect();
        let mut answered: Vec<String> = resolve(&body)
            .methods
            .keys()
            .map(|(held, method)| format!("{}.{}", names.text(*held), names.text(*method)))
            .collect();
        answered.sort();
        answered
    }

    #[test]
    fn a_const_instance_of_a_once_declared_class_decides_its_methods() {
        assert_eq!(
            decided("class C { m(x) { return x; } } const o = new C(); o.m(1);"),
            ["o.m"]
        );
    }

    #[test]
    fn the_chain_is_walked_so_an_inherited_method_counts() {
        assert_eq!(
            decided("class B { bp() { return 1; } } class D extends B {} const d = new D(); d.bp();"),
            ["d.bp"]
        );
    }

    #[test]
    fn a_receiver_read_as_a_value_decides_nothing() {
        // Anything could write `o.m` through that read.
        assert!(decided("class C { m() {} } const o = new C(); f(o); o.m();").is_empty());
    }

    #[test]
    fn a_class_read_as_a_value_decides_nothing() {
        // `C.prototype.m = f` spells `C`, and so does every other way of
        // changing what `o.m` reaches through the class.
        assert!(
            decided("class C { m() {} } const o = new C(); C.prototype.m = g; o.m();").is_empty()
        );
    }

    #[test]
    fn a_let_receiver_decides_nothing() {
        assert!(decided("class C { m() {} } let o = new C(); o.m();").is_empty());
    }

    #[test]
    fn a_name_declared_twice_decides_nothing() {
        // Two classes of one spelling: the receiver could be either, so the
        // method it names is not decided.
        assert!(
            decided(
                "class C { m() {} } const o = new C(); function g() { class C {} } o.m();"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_static_method_is_not_on_the_instance() {
        assert!(decided("class C { static m() {} } const o = new C(); o.m();").is_empty());
    }

    #[test]
    fn a_getter_is_not_a_method_call() {
        assert!(decided("class C { get m() { return 1; } } const o = new C(); o.m;").is_empty());
    }

    #[test]
    fn a_derived_method_shadows_the_base() {
        // Both are found; the DERIVED one must be the answer, which the walk
        // gets by starting there and refusing to overwrite.
        let mut names = Names::default();
        let program = parse_script(
            "class B { m() { return 1; } } class D extends B { m() { return 2; } } \
             const d = new D(); d.m();",
            &mut names,
        )
        .expect("parses");
        let body: Vec<Stmt> = program
            .body
            .into_iter()
            .filter_map(|item| match item {
                ModuleItem::Stmt(statement) => Some(statement),
                _ => None,
            })
            .collect();
        let resolved = resolve(&body);
        let held = names.intern("d");
        let method = names.intern("m");
        let function = resolved.methods.get(&(held, method)).expect("decided");
        let crate::syntax::FunctionBody::Block(statements) = &function.body else {
            panic!("a block");
        };
        let [statement] = statements.as_slice() else {
            panic!("one statement");
        };
        let StmtKind::Return(Some(answer)) = &statement.kind else {
            panic!("a return");
        };
        assert!(
            matches!(&answer.kind, ExprKind::Literal(crate::syntax::Literal::Number(two)) if *two == 2.0),
            "the derived method, not the base's"
        );
    }
}
