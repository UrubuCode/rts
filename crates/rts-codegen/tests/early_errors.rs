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
