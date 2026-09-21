//! Do the two tiers of a function agree about where a fall lands?
//!
//! `rts_mir::guard::pair` says of itself that it exists because *"a property nothing
//! checks is a property nobody finds out about"* — and for as long as it existed,
//! **nothing called it**. It had no producer outside its own unit tests, which is rule
//! 10's gap rather than a feature, and the property had just become load-bearing: a fall
//! from point three of the specialised body must land at point three of the generic one.
//!
//! The first thing wiring it up caught was live and silent. The generic tier emits no
//! guard, and declaring a point was a side effect of emitting one — so the generic body
//! claimed **no points at all**, and the check answers `Unresumable` for every function
//! that speculates about anything. `FuncBuilder::resumable` is the fix; this file is what
//! would have found it, and is now what keeps it found.

use rts_codegen::lower_module::lower_module_paired;
use rts_codegen::names::Names;
use rts_codegen::names::resolve::resolve_module;
use rts_codegen::parse::parse_script;

/// Lowers both tiers of a script and answers which functions' tiers disagree.
fn unpaired(source: &str) -> Vec<String> {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("the fixture parses");
    let resolution = resolve_module(&program.body);
    let paired = lower_module_paired(&program.body, &resolution, &names);
    paired
        .unpaired()
        .into_iter()
        .map(|(named, held)| format!("{named}: {held:?}"))
        .collect()
}

/// A function that speculates pairs, which is the case the check exists for and the one
/// that was broken: the specialised body falls to a point, and the generic body has to
/// claim it.
#[test]
fn a_function_with_a_guarded_parameter_pairs() {
    assert_eq!(
        unpaired("function f(a: number, b: number) { return a - b; }"),
        Vec::<String>::new()
    );
}

/// And one that speculates about nothing pairs trivially — stated so that the test above
/// is known to be testing something rather than passing for want of a point.
#[test]
fn a_function_with_no_claim_pairs_because_neither_tier_has_a_point() {
    assert_eq!(
        unpaired("function f(a, b) { return a - b; }"),
        Vec::<String>::new()
    );
}

/// EVERY POINT, not just the first. Two annotated parameters are two guards and two
/// falls, and a generic body that claimed only one of them would pair on the fixture
/// above and fail here.
#[test]
fn several_claims_pair_point_for_point() {
    assert_eq!(
        unpaired("function f(a: number, b: string, c: number) { return a; }"),
        Vec::<String>::new()
    );
}

/// A claim in the middle that produces no guard takes NO number either, and the tiers
/// stay in step because both run the same three refusals before minting one — not
/// because the numbering is positional.
///
/// This doc said the opposite first, and the assertion passed either way. Worth keeping
/// the correction: a comment that describes the mechanism wrongly is worse than none,
/// because a reader trusting it would conclude a `boolean` parameter in the middle is
/// safe for a reason that is not the reason.
///
/// Positional numbering would be the more robust arrangement — it cannot drift if the
/// two tiers ever stop agreeing about which claims are guardable — and this is the test
/// that would keep failing until they did agree again, which is why it is here rather
/// than the change being made on the strength of the argument.
#[test]
fn a_claim_that_produces_no_guard_takes_no_number_in_either_tier() {
    assert_eq!(
        unpaired("function f(a: number, b: boolean, c: number) { return a - c; }"),
        Vec::<String>::new()
    );
}
