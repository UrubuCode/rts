//! What a declaration costs as the program grows, pinned by a COUNT and not
//! by a clock.
//!
//! The registry used to be a plain object with one property per name, and
//! each new property was a new layout whose index `put` built from the whole
//! chain — so the k-th declaration cost k, and a program of 4 000 top-level
//! functions started in 9.3 s where 318 ms was the figure without the pickle
//! (release, 2026-09-18). A timing test would catch that and also fail on a
//! busy machine; the number of layouts the shape tree grew by cannot, and it
//! is the mechanism itself: one layout per declaration IS the quadratic.

use super::super::{Context, Singletons};
use crate::text::Str;
use crate::value::Value;

fn fresh() -> Context {
    Context::new(Singletons { undefined: 0, null: 1, hole: 2 }, crate::value::Kinds::in_declaration_order())
}

fn target(context: &mut Context) -> u64 {
    Value::from_slot(super::super::native::plain(context).expect("room")).bits()
}

/// `n` distinct top-level names declared, and how many layouts that made.
fn declared(context: &mut Context, n: usize) -> usize {
    let targets: Vec<u64> = (0..n).map(|_| target(context)).collect();
    let before = context.shapes.len();
    for (i, held) in targets.iter().enumerate() {
        super::names::declare(context, *held, "", &format!("f{i}"), -1);
    }
    context.shapes.len() - before
}

#[test]
fn a_declaration_makes_no_layout_of_its_own() {
    let mut small = fresh();
    let mut large = fresh();
    let at_1000 = declared(&mut small, 1000);
    let at_4000 = declared(&mut large, 4000);
    assert_eq!(
        at_1000, at_4000,
        "the layouts a program's declarations add must not depend on how many there are: \
         one per declaration is the registry object growing a property per name, and that \
         is the quadratic startup"
    );
    assert!(at_4000 <= 3, "the marker on the target, the registry on the global, nothing per name");
}

#[test]
fn every_declaration_is_found_by_its_qualified_name() {
    let mut context = fresh();
    declared(&mut context, 4000);
    let found = super::names::resolve(&mut context, Some(&Str::from_str("")), &Str::from_str("f3999"));
    let cell = Value(found.expect("declared")).as_slot().expect("a cell");
    let declared = super::names::declared_as(&mut context, cell).expect("marked");
    assert_eq!(declared.name.to_rust_lossy(), "f3999");
}

#[test]
fn redeclaring_a_name_replaces_rather_than_grows() {
    let mut context = fresh();
    let first = target(&mut context);
    let second = target(&mut context);
    super::names::declare(&mut context, first, "", "Looped", -1);
    super::names::declare(&mut context, second, "", "Looped", -1);
    let found = super::names::resolve(&mut context, None, &Str::from_str("Looped"));
    assert_eq!(found.expect("one declaration, not an ambiguity"), second);
}

#[test]
fn a_plain_name_declared_in_two_modules_is_refused_by_name() {
    let mut context = fresh();
    let first = target(&mut context);
    let second = target(&mut context);
    super::names::declare(&mut context, first, "a.ts", "Twice", -1);
    super::names::declare(&mut context, second, "b.ts", "Twice", -1);
    let by_module = super::names::resolve(&mut context, Some(&Str::from_str("b.ts")), &Str::from_str("Twice"));
    assert_eq!(by_module.expect("the qualified name is exact"), second);
    let plain = super::names::resolve(&mut context, None, &Str::from_str("Twice"));
    assert!(plain.expect_err("two candidates is a guess").contains("ambiguous"));
}
