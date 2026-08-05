//! Programs that parse and are not programs.
//!
//! Every case here is well-formed by the grammar — the tree could hold it, and
//! did until the checker ran. That is what makes these worth pinning separately
//! from the bridge tests: a bridge test proves a construct arrives, and these
//! prove a construct that arrives is still refused when the language says the
//! program does not exist.
//!
//! Each rule is tested from both sides. A checker is only useful if it refuses
//! the invalid program *and* accepts the valid one next to it, and the failure
//! mode of a redeclaration rule is refusing something that runs everywhere —
//! which no test that only feeds it invalid programs would ever catch.

use rts_codegen::names::Names;
use rts_codegen::parse::{ParseError, parse_script};

#[track_caller]
fn refused(source: &str) -> String {
    let mut names = Names::new();
    match parse_script(source, &mut names) {
        Err(ParseError::Syntax(message)) => message,
        other => panic!("{source:?} was not refused: {other:?}"),
    }
}

#[track_caller]
fn accepted(source: &str) {
    let mut names = Names::new();
    if let Err(error) = parse_script(source, &mut names) {
        panic!("{source:?} is a valid program and was refused: {error}");
    }
}

#[test]
fn a_lexical_name_cannot_be_declared_twice_in_one_scope() {
    assert!(refused("let x; let x;").contains("declared twice"));
    assert!(refused("{ const y = 1; class y {} }").contains("declared twice"));

    // Two `var`s are not a redeclaration — they are one binding declared twice,
    // which the language has always allowed.
    accepted("var x; var x;");
    // And a different scope is a different question.
    accepted("let x; { let x; }");
}

#[test]
fn a_lexical_name_and_a_var_cannot_be_the_same_name() {
    assert!(refused("let x; var x;").contains("lexically and with `var`"));
    // The `var` reaches the whole function however deep it is written, which is
    // the asymmetry that makes this rule different from the one above.
    assert!(refused("let x; { { var x; } }").contains("lexically and with `var`"));

    // The reverse nesting is fine: the `let` belongs to the block, and the
    // `var` to the function, so they never share a scope.
    accepted("var x; { let x; }");
}

#[test]
fn a_switch_has_at_most_one_default() {
    // Refused by SWC, not by the checker: the checker's own rule for this was
    // written, measured as unreachable, and removed. The test stays, because
    // what is pinned is that the program is refused — not which layer says so.
    assert!(
        refused("switch (a) { default: break; default: break; }").contains("multiple defaults")
    );
    accepted("switch (a) { case 1: break; default: break; }");
}

#[test]
fn the_clauses_of_a_switch_are_one_scope() {
    // Not one scope per clause: a `let` in one case is visible from the others,
    // and in its temporal dead zone there. So two of them collide even though
    // no single clause declares anything twice.
    assert!(
        refused("switch (a) { case 1: let x; break; case 2: let x; break; }").contains("twice")
    );
}

#[test]
fn a_catch_binding_shares_the_scope_of_its_block() {
    assert!(refused("try {} catch (e) { let e; }").contains("already binds it"));
    // But not with a `var`: Annex B makes that legal, and it is what a great
    // deal of existing code does to widen the caught value's scope.
    accepted("try {} catch (e) { var e; }");
}

#[test]
fn a_parameter_and_a_lexical_declaration_in_the_body_collide() {
    assert!(refused("function f(a) { let a; }").contains("is a parameter"));
    // A `var` of the same name as a parameter is the parameter, redeclared —
    // legal, and the reason this rule names lexical declarations specifically.
    accepted("function f(a) { var a; }");
}

#[test]
fn a_for_head_cannot_declare_the_same_name_twice() {
    assert!(refused("for (let x, x;;) {}").contains("`for` head"));
    accepted("for (var x, x;;) {}");
}

#[test]
fn the_checker_does_not_reach_past_a_function_boundary() {
    // A `var` inside a nested function belongs to that function, not this one.
    // Getting this wrong is the failure mode that matters: it refuses programs
    // that run, and it would do so silently on any file with a helper in it.
    accepted("let x; function f() { var x; }");
    accepted("function f() { let x; } var x;");
}

#[test]
fn await_cannot_name_anything_inside_an_async_function() {
    assert!(refused("async function f() { var await; }").contains("`await`"));
    // These two SWC reaches first, with a message of its own. What is pinned
    // is that the program is refused, not which layer says so.
    refused("async function f() { void await; }");
    refused("async function f() { await: ; }");

    // Neither word is a keyword. Outside the context that reserves it, the
    // same text is an ordinary identifier and the program is fine.
    accepted("var await = 1;");
    accepted("function f() { var await; }");
}

#[test]
fn yield_cannot_name_anything_inside_a_generator() {
    refused("function* g() { var yield; }");
    accepted("function f() { var yield; }");
}

#[test]
fn an_arrow_inherits_the_context_and_a_function_replaces_it() {
    // An arrow has no `await` of its own to shadow the outer one, exactly as it
    // has no `this`. This is the whole reason the rule is a walk with a context
    // rather than one look at the nearest function.
    assert!(refused("async function f() { const g = () => { var await; }; }").contains("`await`"));
    // An ordinary function does have one, so it resets.
    accepted("async function f() { function g() { var await; } }");
}

#[test]
fn a_property_named_await_is_a_property() {
    // The rule is about identifiers that name something. `o.await` names a
    // property, and a checker that scanned text rather than the tree would
    // refuse every one of these.
    accepted("async function f() { o.await; ({ await: 1 }); o.yield; }");
    accepted("function* g() { ({ yield: 1 }).yield; }");
}

#[test]
fn a_class_static_block_reserves_await_with_no_async_anywhere() {
    // Not because the block is async — it is not — but so that it cannot
    // introduce a name a future `await` in it would collide with.
    refused("class C { static { var await; } }");
    // `yield` is refused there too, but for a different reason and by SWC: a
    // class body is strict code, where `yield` is reserved outright. Asserting
    // it was accepted here was this test being wrong about the language.
    refused("class C { static { var yield; } }");
}

#[test]
fn a_function_expression_binds_its_own_name_inside_itself() {
    // The name of a function expression is visible only within it, in a scope
    // nothing outside can see — so it is checked in the inner context, where
    // `yield` is no longer reserved. A declaration is the opposite: it
    // introduces its name where it is written.
    accepted("function* g() { (function yield() {}); }");
    // SWC reaches the declaration first, with a message of its own; what is
    // pinned is that the two spellings get opposite answers.
    refused("function* g() { function yield() {} }");
}

#[test]
fn a_static_block_reserves_await_less_far_than_an_async_function_does() {
    // `ContainsAwait` does not descend into an arrow body, so this is valid —
    // even though the same shape inside an async function is not. Two reasons
    // to forbid one word, reaching different distances.
    accepted("class C { static { (() => ({ await })); } }");
    assert!(refused("async function f() { (() => ({ await })); }").contains("`await`"));
}

#[test]
fn an_import_call_has_its_own_argument_list() {
    // Not a call of a function value: the production is one expression, an
    // optional second, and no spread. Refused at the bridge rather than by the
    // checker, because the tree holds a specifier and an options expression —
    // so a third argument and a `...` would both vanish into it, turning
    // `import(...urls)` into `import(urls)`, a different program that runs.
    refused("import(...['a']);");
    refused("import('a', {}, '');");

    accepted("import('a');");
    accepted("import('a', { with: {} });");
}

#[test]
fn the_constructor_is_the_one_member_that_cannot_be_modified() {
    // A getter, a generator or an async function named `constructor` is not the
    // constructor with a modifier — it is a member that cannot exist, because
    // `new` needs an ordinary function to run.
    refused("class C { get constructor() {} }");
    refused("class C { async constructor() {} }");
    refused("class C { *constructor() {} }");
    refused("class C { constructor() {} constructor() {} }");
    refused("class C { constructor; }");

    accepted("class C { constructor() {} }");
    // A *static* member named `constructor` is an ordinary static member with
    // an unfortunate name — it is not what `new` runs, and the checker lets it
    // through for exactly that reason.
    //
    // It is not asserted accepted here because SWC refuses it on its own, which
    // is SWC being wrong: `expressions/class/elements/syntax/valid/
    // grammar-static-ctor-meth-valid.js` is a valid program in the corpus and
    // is one of the files this front end wrongly rejects.
}

#[test]
fn a_computed_key_is_exempt_from_every_name_rule() {
    // These are early errors, and a computed key is not known until the class
    // is defined. So the same text in brackets is a different question, and
    // answering it here would refuse a legal program.
    accepted("class C { [\"constructor\"]() {} }");
    accepted("class C { static [\"prototype\"]() {} }");
}

#[test]
fn a_static_member_cannot_be_named_prototype() {
    // `C.prototype` already exists and is not writable, so the member has
    // nowhere to go.
    assert!(refused("class C { static prototype() {} }").contains("prototype"));
    // On an instance it is an ordinary name.
    accepted("class C { prototype() {} }");
}
