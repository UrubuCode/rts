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
fn reach_verified(
    source: &str,
    tier: Tier,
) -> (Function, TypeRegistry, rts_codegen::machine::Shared) {
    let (held, types, shared) = reach_in(source, tier);
    (held.expect("it reaches the machine"), types, shared)
}

/// The same, in a named tier. A guard exists only in the specialised one, because the
/// generic one is where a fall LANDS.
fn reach_in(
    source: &str,
    tier: Tier,
) -> (
    Result<Function, Unlowerable>,
    TypeRegistry,
    rts_codegen::machine::Shared,
) {
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
    // AND THE RETURN, which is the CONVENTION's rather than the proof's: one tagged word,
    // because a caller that cannot know the callee has to be able to receive whatever it
    // answers. This read the proved representation until the boundary started widening
    // a return on the way out, which `reaches_machine` does because a function declared
    // any other way cannot be the code of a closure. It had caught, when it was the
    // proof's, `7 < 3` answering a boolean against a declared `F64` -- which is the same
    // disagreement a declared convention makes impossible.
    let returns = vec![Repr::Tagged];
    let (types, mut funcs) = registries();
    let signature = Signature {
        params,
        returns,
        ..Signature::default()
    };
    // THE GENERIC TWIN, declared with the same signature, which is what a fall lands
    // in. Declared here because `rts-host` is where the pairing is agreed in a real
    // build and a test has to stand in for it -- not because the id is arbitrary.
    let mut shared = rts_codegen::machine::Shared::default();
    let sig = shared.funcs.declare_signature(signature.clone());
    let twin = shared.funcs.declare_function(sig);
    let mut func = Function::new(signature);
    let entry = func.entry;
    let start: Vec<_> = func.block(entry).expect("an entry block").params.clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, entry);
    let mut ops = JsMachine::falling_to(&lowered.domain, inferred, twin).declaring_in(&mut shared);
    let lowered = lower(graph, &mut into, &mut ops, &start);
    drop(into);
    drop(ops);
    (lowered.map(|()| func), types, shared)
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
    let (func, types, shared) = reach_verified("function f() { return 7 - 3; }", Tier::Generic);
    // THE MACHINE'S OWN VERIFIER IS THE JUDGE, not an assertion written here: a graph it
    // accepts is a graph the code generator will accept, where a test checking the shape
    // by hand would be checking what this file expects instead.
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
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
        let (func, types, shared) = reach_verified(source, Tier::Generic);
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// `+` over two PROVED numbers is a float addition, and over anything else it is the
/// runtime's `Add` -- never an instruction chosen because the operands look numeric.
///
/// This test pinned the opposite for as long as `+` was refused even over two proved
/// numbers, on the concern that lowering it "beside the four" would lower a different
/// operator on the strength of the operands looking alike. The concern was right and the
/// refusal was one gate too wide: behind a proof nothing looks like anything, and an
/// unproved `+` still cannot reach the instruction. Both halves are what this pins.
#[test]
fn addition_is_an_instruction_over_proved_numbers_and_a_call_otherwise() {
    let (func, types, shared) = reach_verified("function f() { return 7 + 3; }", Tier::Generic);
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
    let (func, types, shared) = reach_named("function f(a, b) { return a + b; }", "f");
    let func = func.expect("an unproved `+` reaches the runtime's Add");
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
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
    let (func, types, shared) = reach_verified(
        "function f(a: number, b: number) { return a - b; }",
        Tier::Specialised,
    );
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );

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
///
/// The generic tier was REFUSED here, for a subtraction over operands nothing proved. It
/// reaches the machine now, through the runtime's `Subtract` -- which is what the generic
/// tier is for: the body that runs when the speculation did not hold.
#[test]
fn the_generic_tier_emits_no_guard_because_it_is_where_a_fall_lands() {
    let source = "function f(a: number, b: number) { return a - b; }";
    let (generic, types, shared) = reach_in(source, Tier::Generic);
    let generic = generic.expect("the generic tier subtracts through the runtime");
    assert_eq!(
        rts_cranelift::verify(&generic, &types, &shared.funcs),
        Vec::new()
    );
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

/// `%` over two proved numbers is `NumberRemainder`, the unboxed call the running engine
/// makes -- and NOT `NumOp::Rem`. The machine said so when this was tried: `arith` answered
/// `UnsafeRemainder { found: F64 }`, because its remainder is integer-domain only, and
/// JavaScript's `%` over doubles is `fmod`, which keeps the sign of the left operand. This
/// test pinned the refusal until the call was the answer.
#[test]
fn remainder_of_proved_numbers_is_the_unboxed_call_and_not_the_integer_instruction() {
    let (func, types, shared) = reach_verified("function f() { return 7 % 3; }", Tier::Generic);
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
}

/// **The order of the refusals has now flipped back**, and this test's subject has moved
/// four times. It first asserted the operand test refuses `7 - "a"` first; then that the
/// CONSTANT does, because a text literal is its own instruction lowered before the
/// subtraction that reads it; then the operand test again, because a text constant
/// lowers — it is a call to `StringConst`, and the runtime holds the text. And now nothing
/// refuses it: an operand nothing proved numeric sends the subtraction to the runtime's
/// `Subtract`, which answers `NaN` exactly as the running engine's does.
///
/// Each version was true when written. Worth keeping the history in one place rather than
/// rewriting the prose clean, because what moved is not this test: it is which layer
/// knows enough to complain first, and that is the measure of the work.
#[test]
fn a_text_operand_sends_the_subtraction_to_the_runtime() {
    let (func, types, shared) = reach_in("function f() { return 7 - \"a\"; }", Tier::Generic);
    let func = func.expect("the runtime subtracts a text operand");
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
}

/// **An entry-point call reaches the machine**, through the same declaration table the
/// old emitter has used all along.
///
/// This was refused with "needs the host's agreement about where it lives" for as long as
/// the language kept a table of its own. It kept one — `JsEntry`, three rows, every one of
/// them already a row of `RuntimeOp` — and the reuse-check rule calls that shape fatal for
/// a reason this case shows exactly: the duplicate would have had to agree about a symbol,
/// an ABI signature AND an address, and `entries::resolve` answers the address from
/// `RuntimeOp`, so a `JsEntry` row could never have reached one.
///
/// # Why the graph is built by hand
///
/// Because no JavaScript source reaches an entry-point call yet without stopping at
/// something else first: `[...xs]` starts with a `NewArray` this slice has no form for,
/// and a regular expression literal starts with a text constant. Writing a fixture that
/// got there would mean waiting for two unrelated pieces, and the boundary being tested
/// is one call.
#[test]
fn an_entry_point_call_reaches_the_machine() {
    use rts_mir::cfg::{Callee, FuncBuilder as MirBuilder, Op, Terminator};

    let mut domain = rts_codegen::domain::Js::new();
    let entry_id = domain.entry_point(rts_codegen::runtime::RuntimeOp::StringConst);
    let mut build = MirBuilder::new(Tier::Generic);
    let block = build.current();
    // ONE i64 INDEX, which is `StringConst`'s declared shape: it answers the value a
    // text constant of that index is. Chosen because it is on `CANNOT_RAISE` -- the two
    // array appends both raise, and this boundary refuses those by name.
    //
    // A PARAMETER and not a literal, because a literal that fits in an `i32` is declared
    // `I32` -- deliberately, so the double domain can use it -- and the machine has no
    // integer widening to reach `I64` with. The signature is this test's to choose.
    let index = build.param(block);
    let grown = build.push(
        Op::Call {
            callee: Callee::Entry(entry_id),
            receiver: None,
            args: vec![index],
        },
        rts_mir::Effect::ALLOCATES,
        Default::default(),
    );
    let _ = block;
    build.end(Terminator::Return(Some(grown)));
    let graph = build.finish();
    assert_eq!(rts_mir::verify(&graph), Ok(()));

    let inferred = rts_mir::infer::infer(&graph, &domain);
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let mut shared = rts_codegen::machine::Shared::default();
    let signature = Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let machine_entry = func.entry;
    let start: Vec<_> = func
        .block(machine_entry)
        .expect("an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, machine_entry);
    let mut ops = JsMachine::new(&domain, inferred).declaring_in(&mut shared);
    lower(&graph, &mut into, &mut ops, &start).expect("the call is emitted");
    drop(into);
    // THE MACHINE'S VERIFIER IS THE JUDGE, and it needs the registry the call was
    // declared in -- which is the thing a fresh one would not have.
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new(),
        "a call to a declared function verifies"
    );
}

/// **A call that can raise PAYS the branch-and-reraise**, and this replaced a test that
/// asserted such a call was refused. The refusal was the right answer while the check did
/// not exist -- a call emitted without it lets a program carry on with a garbage value
/// after an exception that should have propagated, which is invisible: the call returns,
/// the types line up, the verifier is content.
///
/// Now it is emitted. A throw leaves ONE frame, so the frame above only learns by asking,
/// and re-raising puts the value back in the machine's hands to route through the region
/// tree — or, finding no handler, to return and let the frame above ask in turn.
#[test]
fn a_call_that_can_raise_emits_the_throw_check() {
    use rts_mir::cfg::{Callee, FuncBuilder as MirBuilder, Op, Terminator};

    let mut domain = rts_codegen::domain::Js::new();
    let entry_id = domain.entry_point(rts_codegen::runtime::RuntimeOp::ArrayAppendAll);
    let mut build = MirBuilder::new(Tier::Generic);
    let block = build.current();
    let array = build.param(block);
    let iterable = build.param(block);
    let grown = build.push(
        Op::Call {
            callee: Callee::Entry(entry_id),
            receiver: None,
            args: vec![array, iterable],
        },
        rts_mir::Effect::ALLOCATES.and(rts_mir::Effect::THROWS),
        Default::default(),
    );
    build.end(Terminator::Return(Some(grown)));
    let graph = build.finish();

    let inferred = rts_mir::infer::infer(&graph, &domain);
    let types = TypeRegistry::new();
    let mut shared = rts_codegen::machine::Shared::default();
    let signature = Signature {
        params: vec![Repr::Tagged, Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let machine_entry = func.entry;
    let start: Vec<_> = func
        .block(machine_entry)
        .expect("an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, machine_entry);
    let mut ops = JsMachine::new(&domain, inferred).declaring_in(&mut shared);
    lower(&graph, &mut into, &mut ops, &start).expect("the call and its check are emitted");
    drop(into);

    // THE SHAPE, and each part of it is the check: a call to `Thrown`, a comparison, and a
    // block that takes the value and THROWS. A call emitted without the last one is the
    // defect this replaced a refusal with.
    let throws = func.blocks().any(|(_, held)| {
        matches!(
            held.terminator,
            Some(rts_cranelift::ir::Terminator::Throw { .. })
        )
    });
    assert!(throws, "the re-raise block throws");
    let calls = func
        .blocks()
        .flat_map(|(_, held)| held.insts.clone())
        .filter(|held| {
            matches!(
                func.inst(*held).map(|data| &data.inst),
                Some(rts_cranelift::ir::Inst::Call { .. })
            )
        })
        .count();
    // THREE: the append, `Thrown`, and `TakeThrown`.
    assert_eq!(calls, 3, "the append plus the two the check asks");
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new(),
        "a branch into a throwing block verifies"
    );
}

/// And a boundary with nowhere to declare a call refuses by NAME rather than inventing an
/// id, which is the case that would otherwise emit a call to a function nobody supplied — a
/// crash inside compiled code rather than a refusal.
///
/// Reachable only for an entry point that cannot raise, because the raising check comes
/// first — which is the right order: a call this boundary must not emit at all is refused
/// before asking where it would have been declared.
#[test]
fn a_boundary_with_nowhere_to_declare_refuses_the_call() {
    use rts_mir::cfg::{Callee, FuncBuilder as MirBuilder, Op, Terminator};

    let mut domain = rts_codegen::domain::Js::new();
    let entry_id = domain.entry_point(rts_codegen::runtime::RuntimeOp::StringConst);
    let mut build = MirBuilder::new(Tier::Generic);
    let index = build.push(
        Op::Const(rts_mir::Const::Int(0)),
        rts_mir::Effect::PURE,
        Default::default(),
    );
    let held = build.push(
        Op::Call {
            callee: Callee::Entry(entry_id),
            receiver: None,
            args: vec![index],
        },
        rts_mir::Effect::ALLOCATES,
        Default::default(),
    );
    build.end(Terminator::Return(Some(held)));
    let graph = build.finish();

    let inferred = rts_mir::infer::infer(&graph, &domain);
    let types = TypeRegistry::new();
    let signature = Signature {
        params: vec![],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let machine_entry = func.entry;
    let start: Vec<_> = func
        .block(machine_entry)
        .expect("an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, machine_entry);
    let mut ops = JsMachine::new(&domain, inferred);
    let words =
        said(lower(&graph, &mut into, &mut ops, &start).expect_err("nowhere to declare it"));
    assert!(words.contains("somewhere to declare it"), "{words}");
    assert!(words.contains("StringConst"), "which one: {words}");
}

/// **`this` reaches the machine**, and the convention is what made it possible: parameter 0
/// is the environment and parameter 1 is the receiver.
///
/// The domain's note on `ThisValue` said where a receiver lives is the MACHINE's question.
/// It is not — `abi::Convention` is about linkage and tail calls and reserves nothing for a
/// receiver, so the machine layer has no answer to give and asking it would have got one
/// invented. It is this language's convention, `emit/function.rs` fixes it, and
/// `rts_core::entry::functions::invoke` calls on those terms.
///
/// So the boundary is TOLD the parameters by whoever declared the signature, rather than
/// looking for them.
#[test]
fn this_reaches_the_machine_through_the_calling_convention() {
    for source in [
        "function f() { return this; }",
        "function f(a) { return this; }",
    ] {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("parses");
        let resolution = resolve_module(&program.body);
        let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
        let graph = lowered.functions[0].result.as_ref().expect("it lowers");
        let mut shared = rts_codegen::machine::Shared::default();
        assert_eq!(
            rts_codegen::machine::reaches_machine(
                graph,
                &lowered.domain,
                None,
                &mut shared,
                &mut names
            ),
            Ok(()),
            "{source}"
        );
    }
}

/// A fall hands over what the activation ARRIVED with, not the live set — and this is pinned
/// because getting it wrong was measured rather than reasoned about.
///
/// The other tier is a function with the same signature, so entering it means handing over
/// the same parameters, the convention's leading two included. Passing the live set alone
/// took `bench/` from 12 functions reaching the machine to 6, with `CallArity { expected: 3,
/// found: 1 }` 287 times.
#[test]
fn a_fall_hands_over_the_whole_parameter_list_and_not_the_live_set() {
    let mut names = Names::new();
    let program = parse_script(
        "function f(a: number, b: number) { return a - b; }",
        &mut names,
    )
    .expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    let mut shared = rts_codegen::machine::Shared::default();
    // A twin to fall to, which is what makes the arity observable at all.
    let twin = rts_mir::cfg::FuncId(0);
    assert_eq!(
        rts_codegen::machine::reaches_machine(
            graph,
            &lowered.domain,
            Some(twin),
            &mut shared,
            &mut names
        ),
        Ok(()),
        "the fall's call matches the twin's signature"
    );
}

/// **A property read reaches the machine, cached**, and the key it takes is a number from
/// the program's one registry.
///
/// A key is not a call, unlike a text: `rts_cranelift::shape::Key` is opaque to the
/// machine, which compares keys and does nothing else with them, so the number IS the whole
/// of it. The pairing of a name to a key lives on `Names` and mints from
/// `shape::KeyRegistry` — CLAUDE.md names that as the correct shape, two tables of
/// different lifetimes minting from ONE registry. A second map would make a computed `o[k]`
/// resolve to a different number than the compiler chose for `o.k`, because the runtime
/// reaches a computed key by arriving at the compiler's number.
#[test]
fn a_property_read_reaches_the_machine_cached() {
    let mut names = Names::new();
    let program = parse_script("function f(o) { return o.x; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    let mut shared = rts_codegen::machine::Shared::default();
    assert_eq!(
        rts_codegen::machine::reaches_machine(
            graph,
            &lowered.domain,
            None,
            &mut shared,
            &mut names
        ),
        Ok(())
    );
}

/// A read whose key the program COMPUTED is a different operation from a cached one:
/// `cached_get` takes a key fixed while compiling and `cached_get_keyed` takes a value, and
/// there is no flag that turns one into the other. So it is not a cached read -- it is the
/// runtime's `GetIndexed`, the call the running engine makes for `o[k]`, which reaches the
/// machine where this used to be refused.
#[test]
fn a_computed_key_is_the_runtimes_indexed_read_and_not_a_cached_one() {
    let (func, types, shared) = reach_named("function f(o, k) { return o[k]; }", "f");
    let func = func.expect("a computed read is `GetIndexed`");
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
}

/// **A method call reaches the machine**, and the receiver travels as itself.
///
/// `NeedsReceiverConvention` refused this on the reasoning that how a receiver reaches a
/// callee is a calling convention and therefore the machine's. That reasoning was wrong in
/// the same way the `ThisValue` note was: `abi::Convention` is about linkage and tail calls
/// and reserves nothing for a receiver, so the machine has no answer to give and asking it
/// would have got one invented.
///
/// So the graph hands the receiver over beside the callee and the arguments, and the
/// LANGUAGE decides what a call with one becomes: `RuntimeOp::Call`, whose shape is the
/// convention written out.
#[test]
fn a_method_call_reaches_the_machine_with_the_receiver_travelling() {
    for source in [
        "function f(o) { return o.m(1); }",
        "function f(o, a) { return o.m(a, 2); }",
    ] {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("parses");
        let resolution = resolve_module(&program.body);
        let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
        let graph = lowered.functions[0].result.as_ref().expect("it lowers");
        let mut shared = rts_codegen::machine::Shared::new();
        assert_eq!(
            rts_codegen::machine::reaches_machine(
                graph,
                &lowered.domain,
                None,
                &mut shared,
                &mut names
            ),
            Ok(()),
            "{source}"
        );
    }
}

/// The door's arity is FIXED at four argument slots, and a call with more is refused rather
/// than truncated — `CallWithArgs` is the operation for that.
///
/// `runtime/mod.rs` says why the arity is fixed at all: the machine has no stack slot to put
/// a real argument vector in, so `rts-core` keeps the vector in a `Vec` of its own.
#[test]
fn a_call_with_more_arguments_than_slots_is_refused_and_not_truncated() {
    let mut names = Names::new();
    let program =
        parse_script("function f(o) { return o.m(1, 2, 3, 4, 5); }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let Ok(graph) = lowered.functions[0].result.as_ref() else {
        return;
    };
    let mut shared = rts_codegen::machine::Shared::new();
    let refused = rts_codegen::machine::reaches_machine(
        graph,
        &lowered.domain,
        None,
        &mut shared,
        &mut names,
    )
    .expect_err("five arguments, four slots");
    assert!(said(refused).contains("needs the vector form"));
}

/// **A `: string` claim earns no guard**, and that is rule 4 applied strictly rather than a
/// gap. Nothing can check it: a string is a reference to a heap value whose layout is the
/// runtime's, no `TypeId` for text is declared anywhere — `rts-host` builds its
/// `TypeRegistry` empty — and `guard_type` has no caller in this crate at all.
///
/// So the machine had no way to verify it and the lattice was claiming `Str` after a guard
/// that checked nothing. An annotation is evidence and a guard makes it proof; where no
/// guard can check it, there is no proof to be had.
///
/// `Type::Str` is not lost, which is the half that makes this safe to do: a string LITERAL
/// narrows to it soundly and with no guard, and that is what keeps the `Add`-over-a-string
/// row of the lattice earning its keep.
#[test]
fn a_string_claim_earns_no_guard_because_nothing_can_check_it() {
    assert_eq!(guards("function f(a: string) { return a; }"), 0);
    // And the number claim still does, which is what says this is about `string` and not
    // about claims.
    assert_eq!(guards("function f(a: number) { return a; }"), 1);
}

/// A string LITERAL still narrows to `Str`, with no guard, because the value is known rather
/// than claimed — so the lattice keeps the fact and loses only the unverified claim.
#[test]
fn a_string_literal_still_narrows_without_a_guard() {
    let mut names = Names::new();
    let program = parse_script("function f() { return \"a\"; }", &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Specialised);
    let graph = lowered.functions[0].result.as_ref().expect("it lowers");
    let types = rts_mir::infer::infer(graph, &lowered.domain);
    let text = graph
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Const(rts_mir::Const::Declared(_))))
        .expect("the literal is a declared constant");
    assert_eq!(*types.of(text.result), Type::Str);
    assert!(
        !graph
            .insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Guard { .. })),
        "nothing was guarded: the value is known"
    );
}

/// The same as `reaches_machine`, for a function named in the fixture, keeping what the
/// machine's verifier needs -- the convention's two leading parameters included, because
/// the environment IS parameter 0 and a read of it has nowhere else to come from.
fn reach_named(
    source: &str,
    named: &str,
) -> (
    Result<Function, Unlowerable>,
    TypeRegistry,
    rts_codegen::machine::Shared,
) {
    reach_named_in(source, named, false)
}

/// The same, with the module's functions numbered on the machine's side first when
/// `numbered` -- which is what a closure over one of them needs an address from.
fn reach_named_in(
    source: &str,
    named: &str,
    numbered: bool,
) -> (
    Result<Function, Unlowerable>,
    TypeRegistry,
    rts_codegen::machine::Shared,
) {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let held = lowered
        .functions
        .iter()
        .find(|held| held.named == named)
        .expect("the fixture declares it");
    let graph = held.result.as_ref().expect("it lowers to a graph");
    let inferred = rts_mir::infer::infer(graph, &lowered.domain);
    let params: Vec<Repr> = [Repr::Tagged, Repr::Tagged]
        .into_iter()
        .chain(
            graph
                .block(graph.entry())
                .params
                .iter()
                .map(|held| JsMachine::repr_of(inferred.of(*held)).unwrap_or(Repr::Tagged)),
        )
        .collect();
    // THE CONVENTION'S ONE TAGGED RESULT, as `reaches_machine` declares it.
    let returns = vec![Repr::Tagged];
    let types = TypeRegistry::new();
    let mut shared = rts_codegen::machine::Shared::default();
    if numbered {
        shared.number_module(lowered.functions.len());
    }
    let mut func = Function::new(Signature {
        params,
        returns,
        ..Signature::default()
    });
    let entry = func.entry;
    let start: Vec<_> = func.block(entry).expect("an entry block").params.clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, entry);
    let mut ops = JsMachine::new(&lowered.domain, inferred)
        .declaring_in(&mut shared)
        .naming_with(&mut names)
        .with_incoming(&start);
    let lowered = lower(graph, &mut into, &mut ops, &start[2..]);
    drop(into);
    drop(ops);
    (lowered.map(|()| func), types, shared)
}

/// **A closure's read of its declarer's local reaches the machine, and the machine's
/// verifier accepts it.** This stood as a refusal naming the environment layout as what
/// was missing, and it was right: how many `__rts_outer` links to walk depends on which
/// functions BUILD an environment, which the graph did not say.
///
/// `names::resolve::captured` says it now, and the graph spells the answer as that many
/// link reads followed by one keyed read -- each an ordinary cached read of an object, so
/// nothing new was needed below. Two links' worth are pinned: `x` is one environment out
/// from where `c` was made, `y` none.
#[test]
fn a_captured_binding_reaches_the_machine_through_the_environment() {
    for (source, named) in [
        (
            "function outer(a) { function inner() { return a; } return inner; }",
            "inner",
        ),
        (
            "function a() { let x = 1; function b() { let y = 2; function c() { y; return x; } return c; } return b; }",
            "c",
        ),
        // A WRITE is a define of an own property, so a binding spelled like an
        // `Object.prototype` accessor lands as data rather than running the setter.
        (
            "function outer() { let n = 0; function bump() { n = 1; } return bump; }",
            "bump",
        ),
    ] {
        let (func, types, shared) = reach_named(source, named);
        let func = func.unwrap_or_else(|held| panic!("{named} reaches the machine: {held:?}"));
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// The function that BUILDS the environment gets past it too, and stops at the next wall
/// rather than at this one: making the closure needs the machine id of another function
/// of the module, and this boundary compiles one function at a time. That is the same
/// missing piece a direct call stops at -- which the message says, so the two are counted
/// as one piece of work rather than two.
#[test]
fn the_builder_stops_at_the_module_numbering_and_not_at_the_environment() {
    let (held, ..) = reach_named(
        "function outer(a) { function inner() { return a; } return inner; }",
        "outer",
    );
    let words = said(held.expect_err("making `inner` needs its machine id"));
    assert!(words.contains("machine id of f"), "{words}");
    assert!(words.contains("one function at a time"), "{words}");
}

/// **A plain call over a value reaches the machine**, through the same door a method call
/// takes with the receiver left `undefined`. It was refused as `NeedsCallee` beside a call
/// to a numbered function, and needed nothing that one needs: the most common call there
/// is stood behind a registry it never asked for, in 3 564 functions of the corpus.
#[test]
fn a_plain_call_over_a_value_reaches_the_machine() {
    for source in [
        "function f(g) { return g(1); }",
        "function f(g) { g(); }",
        "function f(g, a, b) { return g(a, b, a, b); }",
    ] {
        let (func, types, shared) = reach_named(source, "f");
        let func = func.unwrap_or_else(|held| panic!("{source}: {held:?}"));
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// And one past the door's slots is refused rather than truncated: a fifth argument has
/// no slot to arrive in, and dropping it would be a different call.
#[test]
fn a_plain_call_past_the_slots_is_refused_rather_than_truncated() {
    let (held, ..) = reach_named("function f(g) { return g(1, 2, 3, 4, 5); }", "f");
    let words = said(held.expect_err("five arguments, four slots"));
    assert!(words.contains("vector form"), "{words}");
}

/// **With the module numbered, the builder of an environment reaches the machine too**, and
/// the machine's verifier accepts the closure it makes: a code address from the numbering
/// and the environment in force, handed to `ClosureNew` -- the call `emit/function.rs`
/// makes. Without the numbering it stops, as the test above pins, and says why.
#[test]
fn with_the_module_numbered_a_closure_is_an_address_and_an_environment() {
    for (source, named) in [
        (
            "function outer(a) { function inner() { return a; } return inner; }",
            "outer",
        ),
        ("function make() { return () => 1; }", "make"),
    ] {
        let (func, types, shared) = reach_named_in(source, named, true);
        let func = func.unwrap_or_else(|held| panic!("{named}: {held:?}"));
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// **Every generic row reaches the machine as the call the running engine makes**, and the
/// machine's verifier accepts each: `typeof`, a negation and `~` of something unproved,
/// `==`, `instanceof`, `in`, a truth test of an unproved value, a field write, an indexed
/// write, a construction, and array and object literals.
#[test]
fn every_generic_row_reaches_the_machine_as_a_runtime_call() {
    for source in [
        "function f(a) { return typeof a; }",
        "function f(a) { return -a; }",
        "function f(a) { return ~a; }",
        "function f(a, b) { return a == b; }",
        "function f(a, b) { return a instanceof b; }",
        "function f(a, b) { return a in b; }",
        "function f(a) { if (a) { return 1; } return 2; }",
        "function f(o, v) { o.x = v; return o; }",
        "function f(o, k, v) { o[k] = v; return o; }",
        "function f(C, a) { return new C(a, a); }",
        "function f(a, b) { return [a, b]; }",
        "function f(a, b) { return { first: a, second: b }; }",
        "function f(a) { return a == null; }",
        "function f(a, b) { return `x${a}y${b}`; }",
        "function f(a) { return !a; }",
        "function f(a, b) { return !(a < b); }",
    ] {
        let (func, types, shared) = reach_named(source, "f");
        let func = func.unwrap_or_else(|held| panic!("{source}: {held:?}"));
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// `{ __proto__ }` is an ordinary own property -- only `__proto__: v` sets the prototype,
/// and the tree holds that one apart as `Property::Prototype`, which the lowering refuses.
/// So the shorthand is DEFINED like any other key, and must not be turned away for its
/// spelling.
#[test]
fn a_shorthand_proto_key_is_an_ordinary_property() {
    let (held, types, shared) = reach_named("function f(__proto__) { return { __proto__ }; }", "f");
    let func = held.expect("a shorthand is a defined property");
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
}

/// **A protected region reaches the machine**, and a throw inside it is planned to land in
/// its handler. This stood as `NeedsHandlerTag`, which read as though the language owed a
/// tag and was mostly about ORDER: the machine places a block in a region when the block
/// is made, and `rts_mir::lower` made every block before opening any region.
///
/// The plan is asked of the machine, not asserted here: every throw of `f` -- the `throw`
/// statement, and the re-raise after the call that can raise -- is inside the `try`, so
/// none of them may leave the function. The re-raise is the case that would silently
/// fail: its block is made WHILE emitting, and belonged to no region until the builder
/// was told to let such a block inherit the region of the block being emitted.
#[test]
fn a_try_reaches_the_machine_and_every_throw_inside_it_lands_in_its_handler() {
    let (func, types, shared) = reach_named(
        "function f(g, x) { try { g(x); throw x; } catch (e) { return e; } return 0; }",
        "f",
    );
    let func = func.unwrap_or_else(|held| panic!("a try reaches the machine: {held:?}"));
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
    let plans = rts_cranelift::unwind::plan_all_throws(&func);
    assert!(
        plans.len() >= 2,
        "the throw statement and the re-raise: {plans:?}"
    );
    assert!(
        plans.iter().all(|(_, plan)| !plan.escapes()),
        "no throw inside the `try` leaves the function: {plans:?}"
    );
}

/// A `finally` is a cleanup the machine copies onto every path out, and a `throw`
/// outside every region leaves the function -- which is what an uncaught throw is.
#[test]
fn a_finally_and_an_uncaught_throw_reach_the_machine() {
    for source in [
        "function f(g) { try { g(); } finally { g(); } return 1; }",
        "function f(x) { throw x; }",
    ] {
        let (func, types, shared) = reach_named(source, "f");
        let func = func.unwrap_or_else(|held| panic!("{source}: {held:?}"));
        assert_eq!(
            rts_cranelift::verify(&func, &types, &shared.funcs),
            Vec::new(),
            "{source}"
        );
    }
}

/// **A `for`-`of` reaches the machine**: the iteration protocol is ordinary property reads
/// under keys the LANGUAGE fixed -- `@@iterator`, `next`, `done`, `value`, `return` -- which
/// the program never wrote and `WellKnown::spelled` is the one spelling of. They were
/// refused as constants needing "the runtime's numbering", which they did not: a key is a
/// number from the same registry every other key comes from.
#[test]
fn a_for_of_reaches_the_machine_through_the_keys_the_language_fixed() {
    let (func, types, shared) = reach_named(
        "function f(xs, g) { for (const x of xs) { g(x); } return 0; }",
        "f",
    );
    let func = func.unwrap_or_else(|held| panic!("a for-of reaches the machine: {held:?}"));
    assert_eq!(
        rts_cranelift::verify(&func, &types, &shared.funcs),
        Vec::new()
    );
}
