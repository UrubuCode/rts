//! What this pass proves, and the writings of a program that must take a proof
//! away.
//!
//! Its own file for the reason `rts-core`'s `context_tests.rs` is one: rule 8
//! stops a file at a thousand lines, and `proven.rs` spends most of itself on
//! why each arm of `is_numeric` claims what it claims. Splitting the tests off
//! leaves the room the next arm will need.
//!
//! Every test here names a behaviour of the LANGUAGE or a representation the
//! emission can actually deliver — never that this pass does what this pass
//! does, which rule 6 says is worth nothing.

use super::*;
use crate::names::Names;
use crate::parse::parse_script;
use crate::syntax::{FunctionBody, ModuleItem, StmtKind};

/// The names proved numeric in a function body, as strings.
fn proved(source: &str) -> Vec<String> {
    let mut names = Names::default();
    let program =
        parse_script(&format!("function t() {{ {source} }}"), &mut names).expect("parses");
    let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
        panic!("one statement");
    };
    let StmtKind::Function(function) = &statement.kind else {
        panic!("a function");
    };
    let FunctionBody::Block(body) = &function.body else {
        panic!("a block");
    };
    // Composed the way `function.rs` composes it, rather than against an
    // empty `Flattened`: what is being tested includes what this pass says
    // about a replaced property, and an empty one would answer "nothing was
    // replaced" for every source and pass by not looking.
    let captured = super::super::capture::captured(body, &[]);
    let flattened = super::super::escape::analyse(body, &[], &captured);
    let mut numeric = analyse(body, &flattened);
    numeric.name_fields(|object, property| {
        let text = format!(
            "{}{}.{}",
            super::super::escape::MARKER,
            names.text(object),
            names.text(property)
        );
        names.intern(&text)
    });
    // Every interned name, asked whether it survived. `Names` has no
    // iterator, and adding one for a test would be adding surface to the
    // crate for the benefit of this file.
    // Re-interning is how a test turns a name back into text without
    // `Names` growing an iterator for its benefit: interning is idempotent,
    // so asking for a spelling hands back the name it already had.
    let mut found: Vec<String> = ["a", "b", "i", "s", "x", "y"]
        .into_iter()
        .filter(|text| {
            let name = names.intern(text);
            numeric.holds_number(name)
        })
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

#[test]
fn a_literal_initialiser_proves_a_local() {
    assert_eq!(proved("let x = 1;"), ["x"]);
}

#[test]
fn a_local_that_is_later_given_something_else_is_not_proved() {
    // The reason this is an analysis and not a declaration: nothing about
    // the first line is wrong, and the second is what decides.
    assert!(proved("let x = 1; x = f();").is_empty());
}

#[test]
fn the_fixpoint_reaches_a_loop_counter() {
    // `i` is numeric only if `i + 1` is, and `i + 1` is only if `i` is. The
    // optimistic start is what makes this reachable; starting from nothing
    // and adding never proves either.
    assert_eq!(proved("let i = 0; while (i) { i = i + 1; }"), ["i"]);
}

#[test]
fn one_local_losing_its_proof_takes_the_ones_that_depended_on_it() {
    // The reason a single pass is not enough: `b` looks numeric until `a`
    // stops being, and only a second round sees it.
    assert!(proved("let a = 1; let b = a; a = f();").is_empty());
}

#[test]
fn plus_needs_both_sides_because_it_might_concatenate() {
    // The one arithmetic-looking operator with two answers. `x` here is a
    // string, and proving it numeric would emit an add on one.
    assert!(!proved("let s = g(); let x = 1 + s;").contains(&"x".to_owned()));
}

#[test]
fn subtraction_still_needs_both_sides_proved() {
    // `-` always produces a number, so it is tempting to prove `x` without
    // looking at `s`. It is wrong for a different reason: an unproved
    // operand might be an object whose `valueOf` runs user code, and this
    // pass may not decide that a call happens.
    assert!(!proved("let s = g(); let x = 1 - s;").contains(&"x".to_owned()));
}

#[test]
fn a_comparison_is_a_boolean_and_not_a_number() {
    assert_eq!(proved("let a = 1; let b = a < 2;"), ["a"]);
}

#[test]
fn a_declaration_with_no_initialiser_is_undefined_and_not_a_number() {
    assert!(proved("let x;").is_empty());
}

#[test]
fn a_parameter_is_not_proved_because_nothing_here_knows_what_a_caller_passes() {
    // Not a limitation to fix later by guessing. A caller can pass anything,
    // and the evidence for what it passes is not in this function.
    assert!(proved("x = 1;").is_empty());
}

// The bug class: a way of writing a name that the walk did not recognise
// as a write. Each test below pins one writing-form the earlier,
// hand-maintained `match` in `keep_only_numeric`/`check_expr` had no arm
// for, and each would have stayed "proved" before this file started
// delegating recursion to `capture::walk_stmt`/`walk_expr`.

#[test]
fn an_assignment_inside_a_try_body_is_not_invisible_to_the_pass() {
    // `StmtKind::Try` had no arm at all in `keep_only_numeric`, so this
    // assignment was never visited and `x` stayed "proved" straight
    // through it.
    assert!(proved("let x = 1; try { x = f(); } catch (e) {}").is_empty());
}

#[test]
fn an_assignment_inside_a_catch_body_is_not_invisible_to_the_pass() {
    assert!(proved("let x = 1; try {} catch (e) { x = f(); }").is_empty());
}

#[test]
fn an_assignment_inside_a_finally_body_is_not_invisible_to_the_pass() {
    assert!(proved("let x = 1; try {} finally { x = f(); }").is_empty());
}

#[test]
fn a_catch_binding_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
    // `Names` interns by TEXT, not by scope, so a `catch (x)` and an
    // outer numeric `x` are one `Name` to this whole pass. Written this
    // way because the emitter does not accept a caught value being used
    // as a number, so the hazard is the SHADOW, not the catch value's
    // own (non-)numeric-ness.
    assert!(proved("let x = 1; try { f(); } catch (x) {}").is_empty());
}

#[test]
fn a_name_assigned_only_inside_a_call_argument_is_not_invisible_to_the_pass() {
    // Before this pass delegated to `capture::walk_expr`, `check_expr`
    // only recursed into `Binary`/`Logical`/`Unary`/`Sequence`/
    // `Conditional` — an assignment nested inside a call's arguments,
    // `new`'s arguments, an array element, an object property, or a
    // template substitution was invisible to it.
    assert!(proved("let x = 1; f(x = g());").is_empty());
}

#[test]
fn a_name_assigned_only_inside_an_array_literal_is_not_invisible_to_the_pass() {
    assert!(proved("let x = 1; let a = [x = g()];").is_empty());
}

#[test]
fn a_name_assigned_only_inside_a_template_substitution_is_not_invisible_to_the_pass() {
    assert!(proved("let x = 1; let s = `${x = g()}`;").is_empty());
}

#[test]
fn a_name_assigned_only_inside_a_computed_member_index_is_not_invisible_to_the_pass() {
    assert!(proved("let x = 1; a[x = g()] = 1;").is_empty());
}

#[test]
fn a_for_each_dispose_target_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
    // `for (using x of xs)` — the fifth-and-a-half gap: `Declare` and
    // `Assign` were handled, `Dispose` was not, and all three share the
    // same hazard the catch-binding test above pins.
    assert!(proved("let x = 1; for (using x of y) {}").is_empty());
}

#[test]
fn a_using_binding_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
    assert!(proved("let x = 1; { using x = f(); }").is_empty());
}

#[test]
fn a_conditional_is_a_number_this_pass_may_still_not_claim() {
    // `1 > 0 ? 1 : 2` IS a number in the language, and that is exactly why
    // the claim is refused: what this pass hands the emitter is a
    // REPRESENTATION, and `choice::merge` declares the join parameter of a
    // `?:` generic whatever its arms produced. Claiming it made `stored`
    // skip the widening, and the tagged value then failed
    // `ImplicitNarrowing` against a `switch` body's `Repr::F64` parameter —
    // a program that does not compile, which is what
    // `tests/cross-runtime/syntax/352_obf_control_flow_flat.ts` was.
    assert!(proved("let x = 1; x = 1 > 0 ? 1 : 2;").is_empty());
}

#[test]
fn an_assignment_answers_what_the_binding_holds_and_not_what_was_written() {
    // `s = 1` writes a number, and the value the expression produces is
    // still whatever `s` holds it as — `binding::write` answers
    // `expr::stored`, which widens for a name nothing proved. So `x` may
    // not inherit a proof from the literal on the right of an inner
    // assignment whose target has none.
    assert!(proved("let s = g(); let x = 1; x = (s = 1);").is_empty());
}

#[test]
fn an_assignment_into_a_proved_name_still_carries_its_proof() {
    // The other direction, so the conjunct above is not read as "an
    // assignment is never numeric": both names survive here, because `s`
    // holds a number on every path and the value `x` receives is the one
    // `s` was stored as.
    assert_eq!(proved("let s = 0; let x = 1; x = (s = 2);"), ["s", "x"]);
}

#[test]
fn an_ordinary_numeric_loop_stays_proved_after_the_rewrite() {
    // The regression this whole change must not cause: a plain counter
    // loop, with none of the constructs above anywhere near it, still
    // gets its instruction rather than a runtime call.
    assert_eq!(proved("let i = 0; while (i < 10) { i = i + 1; }"), ["i"]);
}
