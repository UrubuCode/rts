//! What a name means, as tests over `Scope`.
//!
//! Its own file for the reason `proven_tests.rs` is one: rule 8 stops a file at a
//! thousand lines, and `scope.rs` spends most of itself on why a binding is a
//! name for a value rather than a cell, and on what a per-iteration environment
//! has to re-bind. Splitting the tests off leaves the room the next answer to
//! "what does this name mean" will need — the import alias was that answer, and
//! it is what pushed the file over.
//!
//! Every test here names a behaviour of the LANGUAGE's scoping, never that this
//! module does what this module does.

use super::*;
use crate::names::Names;
use rts_cranelift::ir::{Function, Signature};
use rts_cranelift::repr::Repr;

/// Two distinct values to bind, without needing a whole emission.
fn two_values() -> (ValueId, ValueId) {
    let mut func = Function::new(Signature::default());
    let block = func.push_block();
    (
        func.push_block_param(block, Repr::Tagged),
        func.push_block_param(block, Repr::Tagged),
    )
}

#[test]
fn an_inner_declaration_hides_an_outer_one_and_the_outer_survives() {
    let mut names = Names::default();
    let x = names.intern("x");
    let (outer, inner) = two_values();

    let mut scope = Scope::new();
    scope.declare(x, outer);
    scope.enter();
    scope.declare(x, inner);
    assert_eq!(scope.lookup(x), Some(Binding::Value(inner)));
    scope.leave();
    assert_eq!(
        scope.lookup(x),
        Some(Binding::Value(outer)),
        "`{{ let x = 1; {{ let x = 2; }} }}` leaves the outer binding \
         untouched — a rename-based implementation gets this right and \
         loses the fact that they are two bindings"
    );
}

#[test]
fn assigning_reaches_an_outer_layer_where_declaring_would_not() {
    let mut names = Names::default();
    let x = names.intern("x");
    let (first, second) = two_values();

    let mut scope = Scope::new();
    scope.declare(x, first);
    scope.enter();
    assert!(scope.assign(x, second));
    scope.leave();
    assert_eq!(
        scope.lookup(x),
        Some(Binding::Value(second)),
        "assignment writes the binding it found; it does not introduce a \
         new one in the block it was written in"
    );
}

#[test]
fn a_lexical_name_is_unreadable_until_its_own_declaration_and_readable_after() {
    let mut names = Names::default();
    let x = names.intern("x");
    let (value, _) = two_values();

    let mut scope = Scope::new();
    scope.expect_lexical(&[x]);
    assert!(
        scope.in_dead_zone(x),
        "`{{ x; let x = 1; }}` reads a name the block declares below, which \
         is the temporal dead zone and a ReferenceError"
    );
    scope.declare(x, value);
    scope.initialize(x);
    assert!(
        !scope.in_dead_zone(x),
        "the zone ends at the declaration, not at the end of the block"
    );
}

#[test]
fn an_inner_declaration_puts_an_outer_binding_of_the_same_name_in_the_zone() {
    let mut names = Names::default();
    let x = names.intern("x");
    let (outer, _) = two_values();

    let mut scope = Scope::new();
    scope.declare(x, outer);
    scope.enter();
    scope.expect_lexical(&[x]);
    assert!(
        scope.in_dead_zone(x),
        "`let x = 1; {{ x; let x = 2; }}` throws: inside the block the name \
         refers to the INNER declaration for the whole block, so the outer \
         binding is not what the read finds"
    );
    scope.leave();
    assert!(
        !scope.in_dead_zone(x),
        "leaving the block leaves the outer binding readable again"
    );
}

#[test]
fn a_name_a_nested_block_declares_leaves_the_enclosing_one_readable() {
    let mut names = Names::default();
    let x = names.intern("x");

    let mut scope = Scope::new();
    scope.enter();
    // Nothing pending here: `{ x; { let x = 1; } }` reads the OUTER `x`,
    // because a block's declarations are the block's alone.
    assert!(!scope.in_dead_zone(x));
}

#[test]
fn a_per_iteration_environment_does_not_resurrect_a_shadowed_enclosing_name() {
    let mut names = Names::default();
    let t = names.intern("t");
    let (environment, parameter) = two_values();

    // The shape a `for (let …)` under a `try` produces: an enclosing
    // environment holds `t`, the function's own parameter is also `t`, and
    // the loop opens a record of its own for `w`.
    let mut scope = Scope::for_function(
        Some(environment),
        BTreeSet::new(),
        &BTreeSet::new(),
        &[(t, 1)],
        &[],
    );
    scope.declare(t, parameter);
    let w = names.intern("w");
    scope.enter_environment(environment, &[w]);

    assert_eq!(
        scope.lookup(t),
        Some(Binding::Value(parameter)),
        "the parameter still shadows the enclosing `t` inside the pass's \
         record. Re-binding every environment name one hop further out put \
         the ENCLOSING binding in the innermost layer, and `lookup` scans \
         in reverse — so the loop body read the enclosing variable and \
         answered its value, silently, wherever one existed"
    );
}

#[test]
fn a_per_iteration_environment_still_pushes_a_name_it_does_not_shadow_one_hop_out() {
    let mut names = Names::default();
    let outer = names.intern("outer");
    let (environment, _) = two_values();

    let mut scope = Scope::for_function(
        Some(environment),
        BTreeSet::new(),
        &BTreeSet::new(),
        &[(outer, 1)],
        &[],
    );
    let w = names.intern("w");
    scope.enter_environment(environment, &[w]);

    assert_eq!(
        scope.lookup(outer),
        Some(Binding::InEnvironment {
            hops: 2,
            name: outer
        }),
        "a name nothing shadows travels: inserting a link means every \
         binding past it is one hop further out, and dropping the re-bind \
         would read the pass's own record for something written in the \
         function's"
    );
}

#[test]
fn assigning_a_name_nothing_declared_reports_the_miss() {
    let mut names = Names::default();
    let x = names.intern("x");
    let (value, _) = two_values();

    // Sloppy mode makes this a global store and strict mode makes it a
    // ReferenceError. Both need to know it was not found, so neither is
    // served by quietly declaring a local.
    assert!(!Scope::new().assign(x, value));
}

#[test]
fn a_declaration_of_the_same_spelling_is_not_the_imported_name() {
    let mut names = Names::default();
    let n = names.intern("n");
    let (namespace, parameter) = two_values();

    // `import { n } from "m"` binds the exporting module's namespace and says a
    // read of `n` is a property of it. A nested `function f(n) { … }`, or a
    // `{ let n = … }`, binds something else entirely — and reading THAT through
    // the alias answers a property of somebody else's namespace, which compiles
    // and lies.
    let mut scope = Scope::new();
    scope.declare(n, namespace);
    scope.set_alias(n, Alias::Import { property: n });
    assert_eq!(scope.alias_of(n), Some(Alias::Import { property: n }));

    scope.enter();
    scope.declare(n, parameter);
    assert_eq!(
        scope.alias_of(n),
        None,
        "a declaration introduces a binding of its own, so inside it the name \
         does not mean a property of the exporting module"
    );

    scope.leave();
    assert_eq!(
        scope.alias_of(n),
        Some(Alias::Import { property: n }),
        "and once the block is left the name means what it meant again"
    );
}

#[test]
fn a_loop_head_name_of_the_same_spelling_is_not_it_either() {
    let mut names = Names::default();
    let n = names.intern("n");
    let (namespace, environment) = two_values();

    // A classic `for (let n = …)` opens an environment of its own and binds the
    // head name directly rather than through `declare`, so the suspension has to
    // be taken there as well: those two are the only places a layer binds a name
    // without a declaration passing through `declare`.
    let mut scope = Scope::new();
    scope.declare(n, namespace);
    scope.set_alias(n, Alias::Import { property: n });

    let previous = scope.enter_environment(environment, &[n]);
    assert_eq!(scope.alias_of(n), None);
    scope.leave_environment(previous);
    assert_eq!(scope.alias_of(n), Some(Alias::Import { property: n }));
}
