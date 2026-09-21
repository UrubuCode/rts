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
/// Built fresh by whoever needs them rather than threaded out of [`reach`], which is
/// sound for a stated reason: `lower` takes them IMMUTABLY, so nothing it does can add a
/// row. Two empty registries built the same way are the same registry.
fn registries() -> (TypeRegistry, FuncRegistry) {
    (TypeRegistry::new(), FuncRegistry::new())
}

/// Lowers one script's first function to the machine, and answers what happened.
fn reach(source: &str) -> Result<Function, Unlowerable> {
    reach_in(source, Tier::Generic)
}

/// The same, in a named tier. A guard exists only in the specialised one, because the
/// generic one is where a fall LANDS.
fn reach_in(source: &str, tier: Tier) -> Result<Function, Unlowerable> {
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
    let (types, _funcs) = registries();
    let signature = Signature {
        params,
        returns,
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let entry = func.entry;
    let start: Vec<_> = func.block(entry).expect("an entry block").params.clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, entry);
    let mut ops = JsMachine::new(&lowered.domain, inferred);
    lower(graph, &mut into, &mut ops, &start)?;
    Ok(func)
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
    let func = reach("function f() { return 7 - 3; }").expect("it reaches the machine");
    let (types, funcs) = registries();
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
        let func = reach(source).unwrap_or_else(|held| panic!("{source}: {held:?}"));
        let (types, funcs) = registries();
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

/// **A CLAIM BECOMES A GUARD, and after the guard the narrowing is a proof.** This
/// replaced two tests that pinned the opposite — that an annotation changes nothing and
/// that a parameter is always `Anything`. Both were describing the missing half of rule
/// 4 as though it were the rule: *"it becomes a guard where it cannot"* had no
/// implementation, and a test asserting the absence of one is a gap wearing a rule's
/// clothes.
///
/// So `function f(a: number, b: number)` reaches the machine in the specialised tier,
/// and its subtraction is one float instruction over two values a guard checked.
#[test]
fn an_annotated_parameter_is_guarded_and_the_refusal_moves_to_the_machine() {
    let source = "function f(a: number, b: number) { return a - b; }";
    // WITHOUT THE GUARD the LANGUAGE refuses: nothing was proved about either operand.
    let without = reach_in(source, Tier::Generic).expect_err("the generic tier guards nothing");
    assert!(said(without).contains("not proved numeric"));

    // WITH IT the language is satisfied and the MACHINE refuses instead, naming the
    // point the fall would land at. That move from one side of the boundary to the other
    // is the whole proof that the chain works: claim -> guard -> narrowed -> an
    // instruction the language can emit.
    let refused = reach_in(source, Tier::Specialised)
        .expect_err("the side exit of deopt-lateral.md D3 does not exist");
    assert_eq!(refused, Unlowerable::NeedsSideExit(rts_mir::PointId(0)));
}

/// Nothing here believes TypeScript, and the un-annotated form is what says so: the
/// same function with no claim has no guard, so nothing narrows and the boundary refuses
/// it. The annotation did not make the value a number — it made the CHECK worth
/// emitting.
#[test]
fn the_same_function_without_the_claim_is_still_refused() {
    let words = said(
        reach_in("function f(a, b) { return a - b; }", Tier::Specialised)
            .expect_err("no claim, no guard"),
    );
    assert!(words.contains("not proved numeric"), "{words}");
    assert!(words.contains("v0 is Anything"), "{words}");
}

/// A guard lives only in the SPECIALISED tier, because the generic one is where a fall
/// lands — a guard there would be a check whose failure had no destination. So the same
/// annotated function is refused in the generic tier and lowered in the specialised one,
/// which is the two-tier arrangement rather than a shortfall.
#[test]
fn the_generic_tier_emits_no_guard_because_it_is_where_a_fall_lands() {
    let source = "function f(a: number, b: number) { return a - b; }";
    // The two tiers are refused by DIFFERENT layers over the same source, which is the
    // sharpest statement of what the guard did: the generic body has nothing proved, the
    // specialised one has everything the language needs and waits on the machine.
    let generic = reach_in(source, Tier::Generic).expect_err("no guard in the generic body");
    assert!(matches!(generic, Unlowerable::Language(_)));
    let specialised = reach_in(source, Tier::Specialised).expect_err("no side exit yet");
    assert!(matches!(specialised, Unlowerable::NeedsSideExit(_)));
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
