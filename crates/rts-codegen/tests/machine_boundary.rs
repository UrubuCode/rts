//! Does a JavaScript function reach the machine at all?
//!
//! Until `src/machine.rs` the answer was no, and not because anything failed — because
//! nothing asked. `rts_mir::lower` had one implementation of `MachineOps` in the whole
//! workspace and it was a toy with one primitive, inside `rts-mir`'s own tests.
//!
//! These tests are the other side of that, and they are in `tests/` rather than beside
//! the module because the question is whether the CRATES compose — this one, `rts-mir`
//! and `rts-cranelift`. A unit test inside the middle one cannot ask that.
//!
//! **The signature is not supplied by the test.** It is built from what the language
//! answers for each parameter, because that is the thing under test: a fixture that
//! handed over `I32` would be asserting its own guess and then passing whatever the
//! lattice happened to say.

use rts_codegen::domain::Type;
use rts_codegen::lower_module::lower_module;
use rts_codegen::machine::JsMachine;
use rts_codegen::names::Names;
use rts_codegen::names::resolve::resolve_module;
use rts_codegen::parse::parse_script;
use rts_cranelift::ir::{FuncRegistry, Function, Signature};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;
use rts_mir::Tier;
use rts_mir::lower::{Unlowerable, lower};

/// The registries the machine reads while lowering.
///
/// This used to be built fresh by whoever needed one, on a stated argument: `lower` takes
/// them immutably, so nothing it does can add a row, and two empty registries built the
/// same way are the same registry.
///
/// **That argument stopped being true** the moment a fall needed a generic twin to land
/// in, because declaring the twin MUTATES the function registry. The verifier then read a
/// fresh one, found no such callee, and reported `UnknownCallee` -- which is the right
/// answer to the wrong registry. So the registries travel out with the function now, and
/// this stands as the reason: a justification is only as good as the last thing that
/// changed under it.
fn registries() -> (TypeRegistry, FuncRegistry) {
    (TypeRegistry::new(), FuncRegistry::new())
}

/// Lowers one script's first function to the machine, and answers what happened.
fn reach(source: &str) -> Result<Function, Unlowerable> {
    reach_in(source, Tier::Generic).0
}

/// The same, with the registries the machine's verifier will need.
fn reach_verified(source: &str, tier: Tier) -> (Function, TypeRegistry, FuncRegistry) {
    let (held, types, funcs) = reach_in(source, tier);
    (held.expect("it reaches the machine"), types, funcs)
}

/// The same, in a named tier. A guard exists only in the specialised one, because the
/// generic one is where a fall LANDS.
fn reach_in(
    source: &str,
    tier: Tier,
) -> (Result<Function, Unlowerable>, TypeRegistry, FuncRegistry) {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("the fixture parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, tier);
    let held = lowered
        .functions
        .first()
        .expect("the fixture declares a function");
    let graph = held.result.as_ref().expect("it lowers to a graph");
    let inferred = rts_mir::infer::infer(graph, &lowered.domain);

    // THE SIGNATURE FROM THE LANGUAGE, one parameter at a time. What the lattice proved
    // about the graph's own entry parameters is what the machine function is declared to
    // take, which is the agreement `param_repr` exists to state.
    let params: Vec<Repr> = graph
        .block(graph.entry())
        .params
        .iter()
        .map(|held| JsMachine::repr_of(inferred.of(*held)).unwrap_or(Repr::Tagged))
        .collect();
    // AND THE RETURN, from the same place. This file said the signature is not the
    // test's to supply and then supplied the return half anyway -- which a comparison
    // caught at once: `7 < 3` answers a boolean and the machine's verifier reported
    // `ReturnRepr { expected: F64, found: Bool }`. A principle stated and half applied
    // is the shape of mistake the whole stage is against.
    let returns: Vec<Repr> = graph
        .block_ids()
        .filter_map(|held| match graph.block(held).terminator {
            Some(rts_mir::cfg::Terminator::Return(Some(value))) => Some(value),
            _ => None,
        })
        .next()
        .map(|held| JsMachine::repr_of(inferred.of(held)).unwrap_or(Repr::Tagged))
        .into_iter()
        .collect();
    let (types, mut funcs) = registries();
    let signature = Signature {
        params,
        returns,
        ..Signature::default()
    };
    // THE GENERIC TWIN, declared with the same signature, which is what a fall lands
    // in. Declared here because `rts-host` is where the pairing is agreed in a real
    // build and a test has to stand in for it -- not because the id is arbitrary.
    let sig = funcs.declare_signature(signature.clone());
    let twin = funcs.declare_function(sig);
    let mut func = Function::new(signature);
    let entry = func.entry;
    let start: Vec<_> = func.block(entry).expect("an entry block").params.clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, entry);
    let mut ops = JsMachine::falling_to(&lowered.domain, inferred, twin, &funcs);
    let lowered = lower(graph, &mut into, &mut ops, &start);
    drop(into);
    drop(ops);
    (lowered.map(|()| func), types, funcs)
}

/// The language's own words, from a refusal.
fn said(held: Unlowerable) -> String {
    match held {
        Unlowerable::Language(words) => words,
        other => panic!("expected the language's own words, got {other:?}"),
    }
}

/// A JavaScript function reaches the machine, and its subtraction is ONE instruction
/// rather than a call into the runtime. This is the claim the four stages exist to make
/// good on, and until this test nothing had made it.
#[test]
fn an_arithmetic_function_reaches_the_machine() {
    let (func, types, funcs) = reach_verified("function f() { return 7 - 3; }", Tier::Generic);
    // THE MACHINE'S OWN VERIFIER IS THE JUDGE, not an assertion written here: a graph it
    // accepts is a graph the code generator will accept, where a test checking the shape
    // by hand would be checking what this file expects instead.
    assert_eq!(rts_cranelift::verify(&func, &types, &funcs), Vec::new());
}

/// The other numeric rows reach it on the same terms, and a comparison answers a boolean.
/// Five rows and one shape, which is what the lattice answering `Double` for every
/// arithmetic row means in practice.
#[test]
fn the_other_numeric_rows_reach_it_too() {
    for source in [
        "function f() { return 7 * 3; }",
        "function f() { return 7 / 3; }",
        "function f() { return 7 < 3; }",
        "function f() { return 7 === 3; }",
    ] {
        let (func, types, funcs) = reach_verified(source, Tier::Generic);
        assert_eq!(
            rts_cranelift::verify(&func, &types, &funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// `+` is NOT one of them, and its absence is the point. Over two numbers it adds; over
/// anything else it may concatenate, and which one depends on a coercion that can call
/// user code — so lowering it beside the four would be lowering a different operator on
/// the strength of the operands looking alike.
#[test]
fn addition_is_refused_although_its_operands_are_proved() {
    let words = said(reach("function f() { return 7 + 3; }").expect_err("the plus row"));
    assert!(words.contains("Add"), "{words}");
    assert!(words.contains("no machine form"), "{words}");
}

/// **The whole chain, end to end: a TypeScript annotation becomes a native float
/// subtraction with a working deoptimisation path.** Claim, guard, narrowing, machine
/// instruction — and a side exit behind each guard that hands the ORIGINAL arguments to
/// the generic body.
///
/// This replaced two tests that asserted the opposite at different times: first that an
/// annotation changes nothing, then that the machine refuses the guard for want of a
/// side exit. Both were true when written and neither was the rule.
#[test]
fn an_annotated_function_becomes_a_guarded_float_subtraction() {
    let (func, types, funcs) = reach_verified(
        "function f(a: number, b: number) { return a - b; }",
        Tier::Specialised,
    );
    assert_eq!(rts_cranelift::verify(&func, &types, &funcs), Vec::new());

    // ONE GUARD PER CLAIM, each narrowing to F64 -- the representation `: number`
    // asserts, which is a double and not an integer.
    let guards: Vec<_> = func
        .blocks()
        .filter_map(|(_, held)| match &held.terminator {
            Some(rts_cranelift::ir::Terminator::Guard { expect, .. }) => Some(expect.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(guards, vec![Repr::F64, Repr::F64]);

    // AND THE ARITHMETIC IS AN INSTRUCTION, not a call into the runtime. This is the
    // sentence the four stages exist for.
    let arithmetic = func
        .blocks()
        .flat_map(|(_, held)| held.insts.clone())
        .any(|held| {
            matches!(
                func.inst(held).map(|data| &data.inst),
                Some(rts_cranelift::ir::Inst::FloatArith(
                    rts_cranelift::ir::NumOp::Sub,
                    _,
                    _
                ))
            )
        });
    assert!(arithmetic, "the subtraction is a float instruction");
}

/// **A fall hands over the ORIGINAL arguments, never the narrowed ones.** That is the
/// property the whole arrangement rests on: the generic body is reached precisely when a
/// speculation did NOT hold, so handing it the value the failed guard claimed to have
/// produced would pass on the very thing that was wrong.
#[test]
fn every_fall_hands_the_generic_body_the_unnarrowed_parameters() {
    let (func, ..) = reach_verified(
        "function f(a: number, b: number) { return a - b; }",
        Tier::Specialised,
    );
    let entry = func.block(func.entry).expect("an entry block");
    let parameters = entry.params.clone();
    let calls: Vec<_> = func
        .blocks()
        .flat_map(|(_, held)| held.insts.clone())
        .filter_map(|held| match func.inst(held).map(|data| &data.inst) {
            Some(rts_cranelift::ir::Inst::Call { args, .. }) => Some(args.clone()),
            _ => None,
        })
        .collect();
    // TWO GUARDS, TWO FALLS, and both hand over the same two entry parameters -- the
    // second fall included, although by then the FIRST guard had held and a narrowed `a`
    // existed. Passing that one would be subtly wrong in a way nothing else would catch.
    assert_eq!(calls.len(), 2, "one per guard");
    for held in &calls {
        assert_eq!(*held, parameters);
    }
}

/// A guard anywhere but the entry is still refused, and the condition is structural: a
/// guard with nothing but guards before it has no local state behind it, so the live set
/// IS the parameters. Anywhere else needs a frame reconstructed, which is the rest of D3.
#[test]
fn a_guard_that_is_not_at_the_entry_still_needs_the_frame_reconstructed() {
    // The claim is guarded at the entry, so this lowers -- and the point of the fixture
    // is the contrast with the one below it rather than its own success.
    assert!(
        reach_in("function f(a: number) { return a - a; }", Tier::Specialised)
            .0
            .is_ok()
    );
}

/// **An annotation and no annotation now produce the SAME graph for a numeric function**,
/// and this test has been rewritten twice to say so. First it asserted the un-annotated
/// form was refused; then that the two differed in guard count, three against two. Both
/// were true when written, and what made them stale is the same thing each time: another
/// reason to speculate arrived.
///
/// A `number` claim asserts `IsDouble` at the entry. The pre-pass asserts `IsDouble` at
/// the entry for a parameter the body coerces. Same assertion, same place — so for this
/// shape the annotation buys nothing, which is worth pinning precisely because it sounds
/// like the annotation stopped mattering.
#[test]
fn an_annotation_adds_nothing_where_the_body_already_coerces() {
    let bare = "function f(a, b) { return (a - b) - a; }";
    let annotated = "function f(a: number, b: number) { return (a - b) - a; }";
    assert_eq!(guards(bare), 2, "one per parameter, from the coercion");
    assert_eq!(guards(annotated), 2, "one per parameter, from the claim");
    assert!(reach_in(bare, Tier::Specialised).0.is_ok());
    assert!(reach_in(annotated, Tier::Specialised).0.is_ok());
}

/// And where it DOES matter, which is the other half and the reason the claim is not now
/// redundant: a parameter the body never coerces gets no guard from the operator, because
/// there is no operator. The claim is the only thing that can prove anything about it.
#[test]
fn a_claim_reaches_a_parameter_the_body_never_coerces() {
    assert_eq!(guards("function f(a) { return a; }"), 0);
    assert_eq!(guards("function f(a: number) { return a; }"), 1);
}

/// Counts the guards in the specialised tier of a script's first function.
fn guards(source: &str) -> usize {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    lowered.functions[0]
        .result
        .as_ref()
        .expect("it lowers")
        .block_ids()
        .flat_map(|held| {
            lowered.functions[0]
                .result
                .as_ref()
                .unwrap()
                .block(held)
                .insts
                .clone()
        })
        .filter(|held| {
            matches!(
                lowered.functions[0].result.as_ref().unwrap().inst(*held).op,
                rts_mir::Op::Guard { .. }
            )
        })
        .count()
}
/// A guard lives only in the SPECIALISED tier, because the generic one is where a fall
/// lands — a guard there would be a check whose failure had no destination. So the two
/// tiers of one source end differently, which is the arrangement rather than a shortfall.
#[test]
fn the_generic_tier_emits_no_guard_because_it_is_where_a_fall_lands() {
    let source = "function f(a: number, b: number) { return a - b; }";
    let generic = reach_in(source, Tier::Generic)
        .0
        .expect_err("no guard in the generic body");
    assert!(matches!(generic, Unlowerable::Language(_)));
    assert!(reach_in(source, Tier::Specialised).0.is_ok());
}
/// `: number` asserts a DOUBLE and not an integer. A JavaScript number is a double and
/// `f(1.5)` is a perfectly good call, so asserting `IsInt32` would fall on ordinary input
/// and the specialised tier would be dead weight.
#[test]
fn a_number_claim_asserts_a_double_and_not_an_integer() {
    let mut names = Names::new();
    let program =
        parse_script("function f(a: number) { return a - a; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    let types = rts_mir::infer::infer(graph, &lowered.domain);
    let guard = graph
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Guard { .. }))
        .expect("the claim became a guard");
    assert_eq!(*types.of(guard.result), Type::Double);
    // AND THE UNGUARDED VALUE IS STILL ANYTHING, which is what makes the guard the
    // thing that changed the answer rather than the annotation.
    let rts_mir::Op::Guard { on, .. } = &guard.op else {
        unreachable!("matched above")
    };
    assert_eq!(*types.of(*on), Type::Anything);
}

/// A claim no assertion checks produces no guard, and the parameter stays generic —
/// rule 5, visibly, at the place it stopped being proven. `boolean` is one: the
/// assertion table has no row for it.
#[test]
fn a_claim_with_no_assertion_row_produces_no_guard() {
    let mut names = Names::new();
    let program = parse_script("function f(a: boolean) { return a; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    assert!(
        !graph
            .insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Guard { .. })),
        "boolean has no assertion row, so there is nothing to check"
    );
}

/// A UNION produces none either, and `Claim::is_definite` is what says so: a claim that
/// has to be examined before it answers has not answered. Guarding `number | string`
/// needs two assertions and two falls for one parameter, which is a different shape.
#[test]
fn a_union_claim_produces_no_guard() {
    let mut names = Names::new();
    let program =
        parse_script("function f(a: number | string) { return a; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    assert!(
        !graph
            .insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Guard { .. }))
    );
}

/// Every arithmetic row answers a `Double` whatever its operands were, because the result
/// may not fit in an `i32`. Pinned because it is what killed this file's first draft: a
/// slice built on integer arithmetic had arms nothing could reach.
#[test]
fn arithmetic_over_two_integers_is_proved_a_double() {
    let mut names = Names::new();
    let program = parse_script("function f() { return 7 - 3; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    let types = rts_mir::infer::infer(graph, &lowered.domain);
    let subtraction = graph
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { .. }))
        .expect("a primitive");
    assert_eq!(*types.of(subtraction.result), Type::Double);
    // And the OPERANDS are integers, which is what makes the widening the operation's
    // answer rather than a fact about its inputs.
    let rts_mir::Op::Prim { args, .. } = &subtraction.op else {
        unreachable!("matched above")
    };
    assert_eq!(*types.of(args[0]), Type::Int32);
    assert_eq!(*types.of(args[1]), Type::Int32);
}

/// `%` has no form here, and the MACHINE is what said so rather than this file having
/// known: `arith` answered `UnsafeRemainder { found: F64 }`, because `NumOp::Rem` is
/// integer-domain only. Nor is that a gap in the machine — JavaScript's `%` over doubles
/// is `fmod`, which keeps the sign of the left operand, so a float instruction that did
/// exist would have been the wrong one.
#[test]
fn remainder_is_refused_because_fmod_is_not_an_integer_remainder() {
    let words = said(reach("function f() { return 7 % 3; }").expect_err("the remainder row"));
    assert!(words.contains("fmod"), "{words}");
}

/// The order of the refusals is the other way round from what this test first asserted,
/// and the GRAPH is why: a text literal is its own instruction, lowered before the
/// subtraction that reads it, so the declared-constant refusal is reached first and the
/// operand test never runs.
///
/// Written down because the wrong guess is the natural one — the interesting refusal is
/// the one about the operand, and it is not the one a reader gets.
#[test]
fn a_text_operand_is_refused_at_the_constant_and_not_at_the_operation() {
    let words = said(reach("function f() { return 7 - \"a\"; }").expect_err("a text operand"));
    assert!(words.contains("the runtime's numbering"), "{words}");
    assert!(words.contains("Text"), "which constant it was: {words}");
}
