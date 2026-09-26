//! The scope tree's own tests, apart from `resolve.rs` for its line ceiling.

use super::*;
use crate::names::Names;
use crate::parse::parse_script;

/// The scope tree of a script, with the interner that read it.
fn tree(source: &str) -> (Resolution, Names) {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("the fixture parses");
    (resolve_module(&program.body), names)
}

/// Every binding of one spelling, anywhere in the program.
fn all(out: &Resolution, names: &mut Names, spelled: &str) -> Vec<BindingId> {
    let name = names.intern(spelled);
    (0..out.len())
        .map(|at| BindingId(at as u32))
        .filter(|held| out.binding(*held).name == name)
        .collect()
}

#[test]
fn two_blocks_that_spell_one_name_are_two_bindings() {
    let (out, mut names) = tree("let i = 1; { let i = 2; } { let i = 3; }");
    assert_eq!(all(&out, &mut names, "i").len(), 3);
}

/// The defect this pass exists for, asked as the question `omit` has to ask.
#[test]
fn a_block_of_the_declarer_shadowing_a_free_name_is_visible() {
    let (out, mut names) = tree(
        "function main() {
           let i = 1;
           const q = (x) => x + i;
           { let i = 100; q(10); }
         }",
    );
    let main = out
        .function_scope(
            // The one function whose scope is not an arrow's: it holds a
            // `const` and the arrow does not.
            *out.functions
                .iter()
                .map(|(at, _)| at)
                .next()
                .expect("the script declares a function"),
        )
        .expect("its body opened a scope");
    let i = names.intern("i");
    assert!(out.shadowed_within(main, i));
    let untouched = names.intern("q");
    assert!(!out.shadowed_within(main, untouched));
}

/// The case that must stay provable, and the reason the answer is not simply
/// "the program spells it twice".
#[test]
fn a_name_spelled_again_in_a_different_function_is_not_shadowing() {
    let (out, mut names) = tree(
        "function held() { let zwq = 5; const q = (x) => x + zwq; q(1); }
         function other() { let zwq = 9; return zwq; }",
    );
    let first = *out.functions.keys().next().expect("two functions");
    let held = out.function_scope(first).expect("a body scope");
    let zwq = names.intern("zwq");
    assert!(!out.shadowed_within(held, zwq));
    assert_eq!(all(&out, &mut names, "zwq").len(), 2);
}

#[test]
fn a_var_lands_in_the_function_and_a_let_stays_in_its_block() {
    let (out, mut names) = tree("function f() { { var hoisted = 1; let kept = 2; } }");
    let body = out
        .function_scope(*out.functions.keys().next().expect("one function"))
        .expect("a body scope");
    let hoisted = all(&out, &mut names, "hoisted");
    let kept = all(&out, &mut names, "kept");
    assert_eq!(out.binding(hoisted[0]).scope, body);
    assert_eq!(out.binding(hoisted[0]).origin, Origin::Var);
    assert_ne!(out.binding(kept[0]).scope, body);
    assert_eq!(out.binding(kept[0]).origin, Origin::Lexical);
}

#[test]
fn a_function_expression_binds_its_own_name_only_inside_itself() {
    let (out, mut names) = tree("const f = function fact(n) { return n; };");
    let fact = all(&out, &mut names, "fact");
    assert_eq!(fact.len(), 1);
    assert_eq!(out.binding(fact[0]).origin, Origin::OwnName);
    let inside = out.binding(fact[0]).scope;
    assert_eq!(out.scope(inside).kind, ScopeKind::Function);
    // And it is NOT reachable from the module, which is the whole point: a
    // substituted body carrying the name would land where nothing declares
    // it. `inline.rs` lost four assertions to that.
    let name = names.intern("fact");
    assert!(out.binding_in(out.module(), name).is_none());
}

#[test]
fn a_loop_target_is_a_binding_of_the_head_and_not_of_the_body() {
    let (out, mut names) = tree("let i = 7; for (let i = 0; i < 3; i++) { i; }");
    let both = all(&out, &mut names, "i");
    assert_eq!(both.len(), 2);
    let head = out.binding(both[1]).scope;
    assert_eq!(out.scope(head).kind, ScopeKind::ForHead);
    assert_eq!(out.scope(head).parent, Some(out.module()));
}

#[test]
fn a_catch_binding_belongs_to_its_clause() {
    let (out, mut names) = tree("try { } catch (e) { e; }");
    let caught = all(&out, &mut names, "e");
    assert_eq!(caught.len(), 1);
    assert_eq!(out.binding(caught[0]).origin, Origin::Caught);
    assert_eq!(
        out.scope(out.binding(caught[0]).scope).kind,
        ScopeKind::CatchClause
    );
}

/// A `with` ends the question for every name under it, however it is spelled.
#[test]
fn a_with_makes_every_name_under_it_unprovable() {
    let (out, mut names) = tree("function f() { let v = 1; with (o) { v; } }");
    let body = out
        .function_scope(*out.functions.keys().next().expect("one function"))
        .expect("a body scope");
    let v = names.intern("v");
    assert!(out.shadowed_within(body, v));
    let never_written = names.intern("absent");
    assert!(out.shadowed_within(body, never_written));
}

#[test]
fn a_name_no_scope_declares_resolves_to_nothing() {
    let (out, mut names) = tree("Math.abs(-1);");
    let math = names.intern("Math");
    assert!(out.binding_in(out.module(), math).is_none());
}

/// A `const` arrow only ever called, after it is written and in its own function, is
/// folded into that function: its read of the loop counter is no capture, so the
/// counter needs no environment per pass. Each use of another kind keeps it a function.
#[test]
fn an_arrow_only_called_is_folded_and_its_reads_are_not_captures() {
    let fold = |source: &str| {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("parses");
        let out = resolve_module(&program.body);
        let i = names.intern("i");
        let counter = (0..out.len())
            .map(BindingId::from_index)
            .find(|held| out.binding(*held).name == i);
        (!out.omitted.is_empty(), counter.is_some_and(|held| out.captured(held)))
    };
    assert_eq!(
        fold("function f(n) { let a = 0; for (let i = 0; i < n; i++) { const c = (x) => x + i; a = c(a); } return a; }"),
        (true, false)
    );
    for kept in [
        // handed on as a value
        "function f(n, g) { for (let i = 0; i < n; i++) { const c = (x) => x + i; g(c); } }",
        // called from another function
        "function f(n) { for (let i = 0; i < n; i++) { const c = (x) => x + i; [1].map(() => c(1)); } }",
        // a block between the arrow and the call shadows a name it reads
        "function f(n) { for (let i = 0; i < n; i++) { const c = (x) => x + i; { let i = 9; c(1); } } }",
        // a spread argument
        "function f(n, xs) { for (let i = 0; i < n; i++) { const c = (x) => x + i; c(...xs); } }",
    ] {
        assert_eq!(fold(kept).0, false, "{kept}");
    }
}
