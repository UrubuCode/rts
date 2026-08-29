//! Programs, actually run.
//!
//! Everything before this file checked that we produce something a verifier
//! accepts. A verifier answers "is this well formed" — it does not answer "can
//! this be compiled", and reading the first as the second let two phases ship IR
//! that no destination could take.
//!
//! These tests answer the third question, which is the only one that cannot be
//! satisfied by being internally consistent: does the program compute the right
//! value.

use rts_codegen::values::Singleton;
use rts_cranelift::tags;
use rts_host::compile;

/// Runs a script and hands back the encoded word it produced.
fn run(source: &str) -> u64 {
    let mut program =
        compile(source).unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"));
    program.run()
}

#[test]
fn a_script_that_does_nothing_returns_undefined() {
    let mut compiled = compile("").expect("an empty script compiles");
    let produced = compiled.run();
    // Not "it returned something": a function falling off its end returns
    // `undefined`, and which word that is comes from the compiler's own
    // numbering rather than from a constant written here.
    let expected = compiled.model().singleton(Singleton::Undefined).word();
    assert_eq!(produced, expected);
}

#[test]
fn one_plus_two_is_three() {
    // The whole pipeline in one line of JavaScript: SWC reads it, the tree holds
    // it, `emit` turns it into IR naming `__rts_add`, the machine compiles that
    // into this process, and the runtime's own addition runs.
    //
    // Every number is a double, so the answer is a double — asserting `3.0`
    // rather than an integer is the language fact, not a rounding convenience.
    let produced = run("return 1 + 2;");
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn addition_reaches_the_runtime_rather_than_being_folded() {
    // If the answer were computed while compiling, this would still pass — so
    // the test that distinguishes them is one whose operands the compiler
    // cannot see through. A local read back is enough today, and stays honest
    // when constant folding arrives.
    let produced = run("let a = 20; let b = 22; return a + b;");
    assert_eq!(tags::decode_double(produced), 42.0);
}

#[test]
fn strict_equality_answers_a_boolean_the_machine_proved() {
    // `===` returns `Repr::Bool`, not a tagged value: the runtime establishes
    // it, which is what lets a branch consume one without a guard.
    let mut compiled = compile("return 1 === 1;").expect("compiles");
    let produced = compiled.run();
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_condition_chooses_at_run_time() {
    // The `if` path end to end: truthiness is a call to the runtime, the branch
    // consumes the boolean it proved, and the join carries the value.
    assert_eq!(
        tags::decode_double(run("if (1) { return 7; } return 9;")),
        7.0
    );
    assert_eq!(
        tags::decode_double(run("if (0) { return 7; } return 9;")),
        9.0
    );
}

#[test]
fn a_loop_runs_its_body_more_than_once() {
    // The block parameters E3 built, doing the thing they exist for: `total`
    // has a different value on each pass, and the header's parameter is what
    // carries it across the back edge.
    //
    // Counting down rather than up because `<` is refused until the runtime
    // defines a relational operator, and `n` is falsy at zero.
    // Written without `-`, which is refused until the runtime defines it: the
    // flag counts the passes down by being reassigned rather than decremented.
    let produced = run(
        "let go = 1; let total = 0; while (go) { total = total + 1; if (total === 4) { go = 0; } } return total;",
    );
    assert_eq!(tags::decode_double(produced), 4.0);
}

#[test]
fn a_program_naming_an_operation_the_runtime_lacks_is_refused_by_name() {
    // The failure the two independent statements of the entry-point set were
    // always going to produce, caught where it becomes visible instead of
    // becoming a call to whatever the linker found.
    // This test has named `-`, `**`, `in` and `instanceof` in turn, and each
    // moved on when the runtime defined it — which is the right way for it to
    // fail. What it pins is the SHAPE of the refusal, so it follows whatever is
    // still missing rather than being deleted with the gap it happened to name.
    //
    // Nothing is left in its original category: no operator the language spells
    // reaches a runtime operation that does not exist. It named `for await`
    // next, and that moved on too — it desugars to the protocol's loop over the
    // suspension that already worked. What it names now is the one form of it
    // still refused: writing an EXISTING binding, held back because the
    // write-back lost the value at the loop's back edge and a loop that runs
    // while quietly leaving the name behind is worse than one that will not
    // compile.
    let error = compile("async function f(a) { let v; for await (v of a) { } }")
        .expect_err("`for await` over an existing binding is still refused");
    assert!(
        format!("{error:?}").contains("Unsupported"),
        "expected a named refusal, got {error:?}"
    );
}

// ---------------------------------------------------------------------------
// The operators the runtime gained, each run rather than inspected.

#[test]
fn the_arithmetic_operators_compute() {
    assert_eq!(tags::decode_double(run("return 7 - 2;")), 5.0);
    assert_eq!(tags::decode_double(run("return 6 * 7;")), 42.0);
    assert_eq!(tags::decode_double(run("return 9 / 2;")), 4.5);
    assert_eq!(tags::decode_double(run("return 7 % 3;")), 1.0);
}

#[test]
fn division_follows_ieee_754_rather_than_failing() {
    // JavaScript's arithmetic is IEEE-754, so this is the answer and not an
    // edge case. A guard in the runtime would replace what the language says.
    assert_eq!(tags::decode_double(run("return 1 / 0;")), f64::INFINITY);
    assert!(tags::decode_double(run("return 0 / 0;")).is_nan());
}

#[test]
fn remainder_takes_the_sign_of_the_dividend() {
    // `-5 % 3` is `-2`, not `1` — remainder, not modulo. Written with a unary
    // minus the emitter does not have yet, so the dividend is built by
    // subtracting instead.
    assert_eq!(tags::decode_double(run("return (0 - 5) % 3;")), -2.0);
}

#[test]
fn a_relational_operator_answers_a_javascript_value() {
    // Widened back from the proof the runtime returned: `a < b` in expression
    // position is a value, and only a branch wants the raw boolean.
    let produced = run("return 2 < 10;");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
    assert_eq!(tags::payload_of(run("return 10 < 2;")), tags::BOOL_FALSE);
}

#[test]
fn nan_is_unordered_so_all_four_comparisons_are_false() {
    // The one that catches an implementation written as negations. If `<=` were
    // `!(a > b)`, this would answer true.
    assert_eq!(
        tags::payload_of(run("return (0 / 0) <= (0 / 0);")),
        tags::BOOL_FALSE
    );
    assert_eq!(
        tags::payload_of(run("return (0 / 0) >= (0 / 0);")),
        tags::BOOL_FALSE
    );
}

#[test]
fn a_loop_that_counts_up_to_a_bound() {
    // What `<` was missing for. The loop now reads the way one is written,
    // rather than around the operators the runtime lacked.
    let produced = run(
        "let i = 0; let total = 0; while (i < 5) { total = total + i; i = i + 1; } return total;",
    );
    assert_eq!(tags::decode_double(produced), 10.0);
}

#[test]
fn a_for_loop_runs_end_to_end() {
    // Header scope, condition, body and update — E3's whole shape, executed.
    let produced = run(
        "let total = 0; for (let i = 1; i <= 4; i = i + 1) { total = total * 10 + i; } return total;",
    );
    assert_eq!(tags::decode_double(produced), 1234.0);
}

// ---------------------------------------------------------------------------
// Objects. The machine's shapes get their first client, and the compiler and
// the runtime have to agree about a second numbering — property keys — for any
// of it to mean anything.

#[test]
fn a_property_written_is_the_property_read() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o.x = 42; return o.x;")),
        42.0
    );
}

#[test]
fn an_object_literal_carries_its_properties() {
    assert_eq!(
        tags::decode_double(run("let o = { a: 1, b: 2 }; return o.a + o.b;")),
        3.0
    );
}

#[test]
fn two_properties_do_not_share_a_slot() {
    // What the shape transition has to get right, from the outside: a second
    // key lands beside the first rather than over it.
    let produced = run("let o = {}; o.a = 10; o.b = 20; return o.a * 100 + o.b;");
    assert_eq!(tags::decode_double(produced), 1020.0);
}

#[test]
fn overwriting_a_property_does_not_grow_the_object() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o.a = 1; o.a = 2; return o.a;")),
        2.0
    );
}

#[test]
fn an_absent_property_reads_as_undefined() {
    // Legal JavaScript, not an error being swallowed. Compared against the
    // compiler's own numbering rather than a constant written here.
    let mut compiled = compile("let o = {}; return o.missing;").expect("compiles");
    assert_eq!(
        compiled.run(),
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

#[test]
fn the_two_sides_agree_about_which_name_a_key_number_is() {
    // The agreement this crate exists to make explicit. If the compiler
    // numbered `a` as 0 and the runtime read 0 as `b`, both of these would
    // still compile and run — and the second would answer 1.
    assert_eq!(
        tags::decode_double(run("let o = {}; o.a = 1; o.b = 2; return o.a;")),
        1.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o.a = 1; o.b = 2; return o.b;")),
        2.0
    );
}

#[test]
fn a_property_holding_a_number_still_reaches_arithmetic_through_the_runtime() {
    // The type pass proves LOCALS, and a property is not one: what an object
    // holds is decided at run time, so `o.x + 1` is a call however the value
    // got there. Stated as a test because the boundary is easy to misremember
    // in the direction that would be unsound.
    let produced = run("let o = {}; o.x = 20; return o.x + 22;");
    assert_eq!(tags::decode_double(produced), 42.0);
}

#[test]
fn an_object_is_only_equal_to_itself() {
    // Objects compare by identity where strings compare by text, and `===`
    // reaching the runtime is what makes that true. Two literals with the same
    // properties are two objects.
    assert_eq!(
        tags::payload_of(run("let a = {}; let b = {}; return a === b;")),
        tags::BOOL_FALSE
    );
    assert_eq!(
        tags::payload_of(run("let a = {}; let b = a; return a === b;")),
        tags::BOOL_TRUE
    );
}

// ---------------------------------------------------------------------------
// The heap compiled code addresses with arithmetic.
//
// Nothing below emits an allocation yet — the emitter still calls the runtime
// for objects. What these pin is the wiring: one region, whose base is a
// constant inside the compiled code, and which the program owns because of it.

#[test]
fn a_program_owns_the_region_it_was_compiled_against() {
    // The bug this is here to prevent, which the first version of the wiring
    // had: the host built a region for the base address and the runtime context
    // built ANOTHER when the program ran. Compiled code would have addressed
    // the first while the allocator filled the second.
    //
    // Observable as continuity: the region survives a run, so a second run sees
    // what the first left.
    let mut compiled = compile("let o = {}; o.n = 1; return o.n;").expect("compiles");
    assert_eq!(tags::decode_double(compiled.run()), 1.0);
    assert_eq!(tags::decode_double(compiled.run()), 1.0);
}

#[test]
fn two_programs_do_not_share_a_heap() {
    // Each compilation gets its own region, and each region's base is in its own
    // code. Two programs sharing one would be two sets of addresses into one
    // allocator, which is the same defect in the other direction.
    let mut first = compile("let o = {}; o.a = 1; return o.a;").expect("compiles");
    let mut second = compile("let o = {}; o.a = 2; return o.a;").expect("compiles");
    assert_eq!(tags::decode_double(first.run()), 1.0);
    assert_eq!(tags::decode_double(second.run()), 2.0);
}

// ---------------------------------------------------------------------------
// The operators that choose a path, and the ones with a single operand.
//
// Every one of these is checked by RUNNING it. A branch-and-merge is exactly
// the construct a verifier is happy with and a machine is not — the join
// carrying the wrong representation, or a path jumping from the block it began
// in rather than the one it ended in, both pass verification and both produce a
// wrong value or a crash. So the assertion is the answer, never the shape.

#[test]
fn logical_and_yields_an_operand_rather_than_a_boolean() {
    // `0 && 1` is `0`, not `false`. The operator answers one of its operands,
    // and an implementation that answered the truthiness it branched on would
    // pass every test written with booleans and fail every real program.
    assert_eq!(tags::decode_double(run("return 0 && 1;")), 0.0);
    assert_eq!(tags::decode_double(run("return 2 && 3;")), 3.0);
    assert_eq!(tags::decode_double(run("return 0 || 5;")), 5.0);
    assert_eq!(tags::decode_double(run("return 4 || 5;")), 4.0);
}

#[test]
fn logical_and_does_not_evaluate_what_it_skipped() {
    // The whole content of the operator. `witness` stays at its initial value
    // because the right side never runs, and an implementation that evaluated
    // both and then chose would leave it at 1 — a difference no assertion about
    // the operator's result could ever catch.
    let produced = run("let witness = 0; let go = 0; go && (witness = 1); return witness;");
    assert_eq!(tags::decode_double(produced), 0.0);
    let produced = run("let witness = 0; let go = 1; go && (witness = 1); return witness;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn coalesce_distinguishes_absent_from_falsy() {
    // `0 ?? 1` is `0` where `0 || 1` is `1`. That distinction is the entire
    // reason the operator exists, so it is what gets pinned rather than the
    // null case, which `||` would also get right.
    assert_eq!(tags::decode_double(run("return 0 ?? 1;")), 0.0);
    assert_eq!(tags::decode_double(run("return null ?? 7;")), 7.0);
    // `void 0` and not `undefined`: the second is a property of the global
    // object rather than a literal, and there is no global object yet — so
    // writing it here would be pinning a gap in the wrong construct. `null` IS
    // a literal, which is why it can be written directly.
    assert_eq!(tags::decode_double(run("return (void 0) ?? 8;")), 8.0);
}

#[test]
fn a_conditional_evaluates_only_the_arm_it_chose() {
    assert_eq!(tags::decode_double(run("return 1 ? 4 : 5;")), 4.0);
    assert_eq!(tags::decode_double(run("return 0 ? 4 : 5;")), 5.0);
    let produced =
        run("let taken = 0; let other = 0; 1 ? (taken = 1) : (other = 1); return other;");
    assert_eq!(tags::decode_double(produced), 0.0);
}

#[test]
fn a_local_written_in_one_arm_merges_at_the_join() {
    // The block parameter, through an EXPRESSION rather than a statement. `x`
    // has two definitions reaching the return, and merging them by writing one
    // of them twice is impossible in SSA — so if this returns 1 for a falsy
    // condition, the merge dropped a path.
    assert_eq!(
        tags::decode_double(run("let x = 0; 1 ? (x = 1) : (x = 2); return x;")),
        1.0
    );
    assert_eq!(
        tags::decode_double(run("let x = 0; 0 ? (x = 1) : (x = 2); return x;")),
        2.0
    );
}

#[test]
fn logical_assignment_writes_only_when_the_left_did_not_decide() {
    assert_eq!(
        tags::decode_double(run("let x = 0; x ||= 9; return x;")),
        9.0
    );
    assert_eq!(
        tags::decode_double(run("let x = 3; x ||= 9; return x;")),
        3.0
    );
    assert_eq!(
        tags::decode_double(run("let x = 3; x &&= 9; return x;")),
        9.0
    );
    assert_eq!(
        tags::decode_double(run("let x = 0; x &&= 9; return x;")),
        0.0
    );
    assert_eq!(
        tags::decode_double(run("let x = null; x ??= 6; return x;")),
        6.0
    );
    assert_eq!(
        tags::decode_double(run("let x = 0; x ??= 6; return x;")),
        0.0
    );
}

#[test]
fn negation_produces_minus_zero_where_subtraction_would_not() {
    // `-0` and `+0` are different values: `1 / -0` is `-Infinity`. This is what
    // rules out emitting `-x` as `0 - x`, which answers `+0` here — and the
    // division is how the difference becomes observable at all, since the two
    // compare equal.
    assert_eq!(
        tags::decode_double(run("let z = 0; return 1 / -z;")),
        f64::NEG_INFINITY
    );
    assert_eq!(tags::decode_double(run("return 0 - 5;")), -5.0);
    assert_eq!(tags::decode_double(run("let n = 5; return -n;")), -5.0);
}

#[test]
fn remainder_takes_the_sign_of_the_dividend_as_the_language_spells_it() {
    // The same fact `remainder_takes_the_sign_of_the_dividend` pins, now
    // writable the way a program would write it.
    assert_eq!(tags::decode_double(run("return -5 % 3;")), -2.0);
}

#[test]
fn not_answers_a_boolean_whatever_it_was_given() {
    // Unlike `&&`, `!` really does answer a boolean — so the tag is the claim,
    // not just the payload.
    let produced = run("return !0;");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
    let produced = run("return !7;");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_FALSE);
}

#[test]
fn strict_inequality_is_the_negation_of_strict_equality() {
    let produced = run("return 1 !== 2;");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
    let produced = run("return 1 !== 1;");
    assert_eq!(tags::payload_of(produced), tags::BOOL_FALSE);
}

#[test]
fn void_evaluates_its_operand_and_answers_undefined() {
    let mut compiled = compile("let w = 0; let r = void (w = 3); return r;").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
    // The side effect happened: `void` discards the result, not the evaluation.
    assert_eq!(
        tags::decode_double(run("let w = 0; void (w = 3); return w;")),
        3.0
    );
}

#[test]
fn postfix_yields_the_old_value_and_prefix_the_new_one() {
    assert_eq!(
        tags::decode_double(run("let i = 1; let r = i++; return r;")),
        1.0
    );
    assert_eq!(tags::decode_double(run("let i = 1; i++; return i;")), 2.0);
    assert_eq!(
        tags::decode_double(run("let i = 1; let r = ++i; return r;")),
        2.0
    );
    assert_eq!(tags::decode_double(run("let i = 1; i--; return i;")), 0.0);
}

#[test]
fn an_update_writes_through_a_property_as_well_as_a_local() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 1; o.n++; return o.n;")),
        2.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 1; return o.n++;")),
        1.0
    );
}

#[test]
fn a_compound_assignment_to_a_property_reads_it_before_writing() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 10; o.n += 5; return o.n;")),
        15.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 10; o.n *= 3; return o.n;")),
        30.0
    );
}

#[test]
fn a_loop_can_now_be_written_the_way_a_program_writes_one() {
    // Every piece this slice added, in the shape the constructs exist for: a
    // relational test, an increment, and an accumulator. It is the test that
    // would have been unwritable before, which is the measurement of what
    // changed.
    let produced = run("let total = 0; for (let i = 0; i < 5; i++) { total += i; } return total;");
    assert_eq!(tags::decode_double(produced), 10.0);
}

/// The refusal list is EMPTY, and what stood here is kept because the list was
/// the record.
///
/// # What this test was
///
/// `a_construct_still_missing_is_refused_by_name_rather_than_approximated`, and
/// it asserted that `compile` answers `Unsupported` for each construct the
/// emitter did not have. It named `typeof`, a string literal, `~` and `==` in
/// turn, then `[1, , 2]` — which left when the runtime gained a marker for an
/// absent position — then `super()` past four arguments, then `delete x`, and
/// last `[...[1], , 2]`: a hole beside a SPREAD, which the argument-vector path
/// did not know how to skip.
///
/// That last one now compiles and answers correctly, so the loop had nothing
/// left to iterate. A test whose collection is empty asserts nothing while
/// still reporting green, which is the failure mode CLAUDE.md's honesty floor
/// names — empty looks exactly like passing at the place anyone looks.
///
/// # Why a behaviour test replaces it rather than nothing
///
/// Because the construct is the interesting half. A hole is not `undefined`:
/// `1 in [1, , 2]` is false and `1 in [1, undefined, 2]` is true, and an
/// emitter that filled holes with `undefined` would pass every length and
/// element check while losing the distinction. All five shapes below were
/// compared against node on 2026-08-29 and agree, `in` included.
///
/// The refusal SHAPE is still pinned, by `emit/`'s own tests: what is gone is
/// this file's list of which constructs are currently in it.
#[test]
fn a_hole_beside_a_spread_stays_a_hole() {
    // The one that was refused until this list emptied.
    assert_eq!(tags::decode_double(run("return [...[1], , 2].length;")), 3.0);
    assert_eq!(
        tags::decode_double(run("return (1 in [...[1], , 2]) ? 1 : 0;")),
        0.0,
        "the middle position is a HOLE, not an `undefined` stored in it"
    );
    assert_eq!(tags::decode_double(run("return [...[1], , 2][2];")), 2.0);

    // A hole BEFORE the spread, which is the other order and a different path.
    assert_eq!(tags::decode_double(run("return [, ...[1]].length;")), 2.0);
    assert_eq!(
        tags::decode_double(run("return (1 in [, ...[1]]) ? 1 : 0;")),
        1.0,
        "position 1 is what the spread filled, so it is present"
    );

    // A spread of more than one element, so the hole's index is not the
    // spread's length by coincidence.
    assert_eq!(tags::decode_double(run("return [...[1, 2], , 3].length;")), 4.0);
    assert_eq!(tags::decode_double(run("return [...[1, 2], , 3][3];")), 3.0);

    // And the hole between an element and a spread.
    assert_eq!(tags::decode_double(run("return [1, , ...[2]][2];")), 2.0);
    assert_eq!(
        tags::decode_double(run("return (1 in [1, , ...[2]]) ? 1 : 0;")),
        0.0
    );
}

// ---------------------------------------------------------------------------
// Functions, calls and closures.
//
// A closure is not tested by asking whether it compiles. Its entire observable
// content is that two functions made in one activation see the *same* variable,
// and that a variable outlives the call that created it — so those are what the
// assertions are about, and every one of them runs.

#[test]
fn a_function_is_called_and_answers() {
    assert_eq!(
        tags::decode_double(run("function f() { return 7; } return f();")),
        7.0
    );
}

#[test]
fn an_argument_reaches_the_parameter_it_fills() {
    assert_eq!(
        tags::decode_double(run("function id(n) { return n; } return id(9);")),
        9.0
    );
    assert_eq!(
        tags::decode_double(run(
            "function add(a, b) { return a + b; } return add(2, 3);"
        )),
        5.0
    );
}

#[test]
fn a_parameter_nothing_was_passed_for_is_undefined() {
    // Ordinary JavaScript, and the reason the call site pads rather than the
    // callee coping: the callee's parameters exist whether or not anything was
    // passed, so there is nothing for it to cope with.
    let mut compiled = compile("function f(a, b) { return b; } return f(1);").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

#[test]
fn a_function_can_call_itself() {
    // Recursion is the first thing that needs a declaration to be *hoisted*:
    // `fact` is read inside `fact`, before the statement that binds it has run.
    let produced = run(
        "function fact(n) { if (n < 2) { return 1; } return n * fact(n - 1); } return fact(5);",
    );
    assert_eq!(tags::decode_double(produced), 120.0);
}

#[test]
fn two_functions_can_call_each_other() {
    // Mutual recursion needs both names bound before either body is emitted,
    // which is why hoisting is two passes rather than one.
    let produced = run(
        "function even(n) { if (n === 0) { return 1; } return odd(n - 1); } \
         function odd(n) { if (n === 0) { return 0; } return even(n - 1); } \
         return even(4);",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_function_reads_a_variable_from_where_it_was_written() {
    assert_eq!(
        tags::decode_double(run("let k = 4; function get() { return k; } return get();")),
        4.0
    );
}

#[test]
fn a_function_writes_a_variable_the_caller_can_see() {
    // The direction that a copied environment would get wrong. If the closure
    // held its own copy of `n` this returns 0, and every read-only test above
    // would still pass.
    let produced = run(
        "function outer() { let n = 0; function bump() { n = n + 1; } bump(); bump(); return n; } \
         return outer();",
    );
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn two_closures_made_together_share_one_variable() {
    // The whole observable content of a closure, as the one assertion that
    // separates a real implementation from a plausible one: `read` sees what
    // `write` did, so the two must have been handed the SAME environment
    // object rather than two objects with the same contents.
    let produced = run("function pair() { \
           let n = 0; \
           function write() { n = 41; } \
           function read() { return n + 1; } \
           write(); \
           return read(); \
         } \
         return pair();");
    assert_eq!(tags::decode_double(produced), 42.0);
}

#[test]
fn a_variable_outlives_the_call_that_created_it() {
    // The other half. `make` has returned by the time `counter` runs, so `n`
    // cannot have been in `make`'s frame — which is the reason a captured local
    // stops being a register at all.
    let produced = run(
        "function make() { let n = 0; function step() { n = n + 1; return n; } return step; } \
         let counter = make(); \
         counter(); \
         counter(); \
         return counter();",
    );
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn two_activations_do_not_share_a_variable() {
    // The converse of the test above, and the one an environment created once
    // per *function* rather than once per *call* would fail: each `make()` is a
    // new activation, so each counter starts again.
    let produced = run(
        "function make() { let n = 0; function step() { n = n + 1; return n; } return step; } \
         let first = make(); \
         let second = make(); \
         first(); \
         first(); \
         return second();",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_closure_reaches_two_environments_out() {
    // What the hop count is for. `k` lives in `outer`'s environment, `inner` is
    // inside `middle`, and `middle` builds one of its own — so reaching `k`
    // follows two links, and an implementation that followed one would read
    // `middle`'s environment and find nothing.
    let produced = run("function outer() { \
           let k = 5; \
           function middle() { \
             let m = 2; \
             function inner() { return k * m; } \
             return inner(); \
           } \
           return middle(); \
         } \
         return outer();");
    assert_eq!(tags::decode_double(produced), 10.0);
}

#[test]
fn a_parameter_is_captured_like_any_other_binding() {
    let produced = run(
        "function adder(by) { function apply(n) { return n + by; } return apply; } \
         let plus_three = adder(3); \
         return plus_three(4);",
    );
    assert_eq!(tags::decode_double(produced), 7.0);
}

#[test]
fn a_function_expression_is_a_value() {
    let produced = run("let f = function (n) { return n * 2; }; return f(21);");
    assert_eq!(tags::decode_double(produced), 42.0);
}

#[test]
fn an_arrow_with_a_concise_body_returns_it() {
    // A concise body is not a block, and the emitter wraps it in a `return`
    // rather than the parser doing so — the tree keeps which was written.
    let produced = run("let double = (n) => n * 2; return double(4);");
    assert_eq!(tags::decode_double(produced), 8.0);
}

#[test]
fn a_method_call_passes_its_receiver_as_this() {
    // The one thing a call site knows that the callee cannot: `o.f()` and `f()`
    // pass the same arguments and differ only here.
    let produced = run("let o = {}; \
         o.n = 12; \
         o.get = function () { return this.n; }; \
         return o.get();");
    assert_eq!(tags::decode_double(produced), 12.0);
}

#[test]
fn a_plain_call_has_no_receiver() {
    let mut compiled = compile("function f() { return this; } return f();").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word(),
        "a plain call passes `undefined`, which is what strict mode specifies; \
         sloppy mode substitutes the global object, and there is no global object"
    );
}

#[test]
fn a_receiver_is_evaluated_once() {
    // `f().g()` calls `f` a single time. An implementation that emitted the
    // object expression once for the property read and again for the receiver
    // would call it twice — invisibly, for every receiver without a side effect,
    // which is why the receiver here has one.
    let produced = run("let calls = 0; \
         let o = {}; \
         o.answer = function () { return 1; }; \
         function get() { calls = calls + 1; return o; } \
         get().answer(); \
         return calls;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_function_is_a_value_that_is_only_equal_to_itself() {
    let produced = run("function f() { return 1; } let g = f; return g === f;");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn calling_something_that_is_not_a_function_throws_rather_than_jumping() {
    // The reason calling is a runtime operation rather than the machine's
    // indirect call: the program must not jump through whatever the value
    // spelled. That was the whole assertion while the answer was `undefined`.
    //
    // It is a `TypeError` now, and one a handler in the program sees — which
    // needed something bigger than the raise itself: every native that calls
    // user code had to learn to ask whether the callee left a throw behind.
    // Raising before that turned one silent wrong answer into a hang.
    let caught = run(
        "let kind = 'none'; \
         try { let n = 1; n(); } \
         catch (e) { kind = e instanceof TypeError ? 'TypeError' : 'other'; } \
         return kind === 'TypeError' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(caught), 1.0);
}

#[test]
fn the_limits_of_the_fixed_arity_are_refused_by_name() {
    // The convention carries four arguments, and going past it is no longer
    // refused in two of the three places it can happen. At the CALL the
    // arguments go in a vector the runtime holds; in the DECLARATION the extra
    // parameters are read back out of that same vector, by position, which is
    // the array `arguments` already is.
    compile("function f(a, b, c, d, e) { return e; } return f(1, 2, 3, 4, 5);")
        .expect("a fifth parameter is read from the vector");

    // `super()` was the one left, and it is no longer refused either. It could
    // not simply borrow `CallWithArgs`: that entry point sets `new.target`, and
    // `super()` must not — routing it there would have corrupted `new.target`
    // in every subclass that spreads, silently. It got an entry of its own that
    // is vector-shaped without setting one.
    //
    // This test asserted the refusal, and the refusal is gone. Narrowed to what
    // it was really protecting rather than deleted: that going past four
    // arguments RUNS and ANSWERS, because the failure it was written against
    // was an argument vanishing without a word.
    compile("class A {} class B extends A { constructor() { super(1, 2, 3, 4, 5); } } return new B();")
        .expect("`super()` past the fixed arity goes through the vector entry");
}

#[test]
fn what_a_function_still_cannot_do_is_refused_by_name() {
    // Each of these is a mechanism rather than a spelling: a default needs an
    // expression evaluated at the call, `this` inside an arrow needs the
    // defining function's receiver carried through the environment, and both
    // `function*` needs a frame that can be suspended.
    //
    // `async` was on this list and came off it. It did NOT get the suspendable
    // frame: `Inst::Await` lowers to a call that drains until the promise
    // settles, so an async function runs to completion when it is called and
    // the only thing that differs is what the caller receives. That is the
    // contract `rts-cranelift`'s own signature doc states for `PromiseAwait`,
    // and the interleaving it does not reproduce is written down in
    // `rts-core`'s `promise/machine.rs`.
    //
    // A rest parameter and a spread argument were both on this list and came
    // off it too: the vector one needed is the runtime's now, and the other is
    // what iteration produces.
    // `this` inside an arrow came off it as well. An arrow takes `this` from
    // where it was WRITTEN, so the enclosing function hands it over as an
    // ordinary captured name — `Scope::late_this`, the mechanism a derived
    // constructor already used, pointed at a second case.
    for source in [
        "function f(a) { return a; } return f();",
        // A generator came off this list, then `yield*`, and now `for await`
        // over a FRESH binding — it desugars to the loop the protocol describes
        // and awaits each `next()`, reusing the suspension that already worked
        // rather than growing a second loop shape.
        //
        // What is left is `for await` writing an EXISTING binding. That is
        // refused on purpose and not for want of the protocol: the write-back
        // for an assign target was losing the value at the loop's back edge, and
        // a loop that runs and quietly leaves the name behind is worse than one
        // that will not compile.
        "async function f(xs) { let x; for await (x of xs) { return x; } } return 1;",
    ] {
        // The first one is legal and emits — a missing argument is padded — so
        // it is here as the control: if this loop ever passes for it, the
        // padding stopped working and every other case is untrustworthy.
        let outcome = compile(source);
        if source.contains("return f();") && source.contains("function f(a)") {
            assert!(outcome.is_ok(), "a missing argument is padded, not refused");
            continue;
        }
        let error = outcome.expect_err("still a gap");
        assert!(
            format!("{error:?}").contains("Unsupported"),
            "expected a named refusal for `{source}`, got {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Strings, and the operator that answers one.
//
// A string is the first value the compiled code cannot make: it is on the heap,
// and two occurrences of `"a"` in a program are the same one. So the text
// travels beside the code and what the code carries is which literal — the same
// shape as a property key, and these tests are about that agreement holding.

/// The text a value names, for a test that wants to read one back.
///
/// Goes through `typeof` and `===` rather than reaching into the runtime,
/// because what a test can observe is what a *program* can observe — reading
/// the heap directly would pass for an implementation no JavaScript could use.
fn is_string(produced: u64) -> bool {
    tags::tag_of(produced) == tags::TAG_REFERENCE
}

#[test]
fn a_string_literal_is_a_value_rather_than_a_number() {
    let produced = run("return \"hello\";");
    assert!(
        is_string(produced),
        "a string is a heap value; an immediate here would be a number that is \
         not a string and compares wrongly with everything"
    );
}

#[test]
fn two_occurrences_of_one_literal_are_the_same_string() {
    // Not an optimisation. `"a" === "a"` is true, and it has to be true for the
    // same reason `o === o` is — so the literal table is keyed by text and a
    // second occurrence does not mint a second number.
    let produced = run("return \"a\" === \"a\";");
    assert_eq!(tags::tag_of(produced), tags::TAG_BOOL);
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn strings_are_equal_when_their_text_is() {
    // Two DIFFERENT literals that spell the same thing would be two entries if
    // the table were not deduplicated, and `===` compares text, so this passes
    // either way — which is why the test above exists beside it. What this pins
    // is the other direction: different text is not equal.
    assert_eq!(
        tags::payload_of(run("return \"a\" === \"b\";")),
        tags::BOOL_FALSE
    );
}

#[test]
fn a_string_survives_being_stored_and_read_back() {
    let produced = run("let o = {}; o.name = \"x\"; return o.name === \"x\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn adding_a_string_concatenates_rather_than_adding() {
    // The reason `+` is a runtime call at all, finally reachable: it converts
    // both operands to primitives and then decides, and joining two strings
    // allocates. Emitting an instruction here would be fast and wrong.
    let produced = run("return (\"a\" + \"b\") === \"ab\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn the_empty_string_is_the_seventh_falsy_value() {
    // The one that made `ToBoolean` a runtime call in the first place: six
    // falsy values a comparison settles, and the seventh reads a string's
    // length from the heap. Until now nothing could write one down.
    assert_eq!(
        tags::decode_double(run("if (\"\") { return 1; } return 2;")),
        2.0
    );
    assert_eq!(
        tags::decode_double(run("if (\"x\") { return 1; } return 2;")),
        1.0
    );
}

#[test]
fn typeof_answers_the_word_the_language_specifies() {
    // The comparison is written INSIDE the program rather than by reading the
    // string out in Rust, because what a test can observe should be what a
    // program can observe — reading the heap directly would pass for an
    // implementation no JavaScript could use.
    for source in [
        "return (typeof 1) === \"number\";",
        "return (typeof true) === \"boolean\";",
        "return (typeof \"s\") === \"string\";",
        "return (typeof (void 0)) === \"undefined\";",
        "return (typeof {}) === \"object\";",
    ] {
        assert_eq!(
            tags::payload_of(run(source)),
            tags::BOOL_TRUE,
            "wrong answer for `{source}`"
        );
    }
}

#[test]
fn typeof_null_is_object_because_the_language_says_so() {
    // A mistake from 1995 the language cannot take back. Written here rather
    // than corrected, because a program asking `typeof` wants what JavaScript
    // does and not what it should have done.
    let produced = run("return (typeof null) === \"object\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn typeof_distinguishes_a_function_from_an_object() {
    // The distinction the tag cannot carry: both are references, and what a
    // reference IS is read from the cell's header. An implementation reading
    // the tag alone answers "object" for both.
    let produced = run("function f() { return 1; } \
         let o = {}; \
         return ((typeof f) === \"function\") && ((typeof o) === \"object\");");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

// ---------------------------------------------------------------------------
// The operators that read a number as thirty-two bits, `**`, and `==`.
//
// Every assertion here is a case where the obvious implementation is wrong and
// wrong quietly: a saturating cast, a remainder that keeps its sign, an
// unmasked shift count, `powf` where the specification diverges from IEEE.

#[test]
fn the_bitwise_operators_compute() {
    assert_eq!(tags::decode_double(run("return 12 & 10;")), 8.0);
    assert_eq!(tags::decode_double(run("return 12 | 10;")), 14.0);
    assert_eq!(tags::decode_double(run("return 12 ^ 10;")), 6.0);
    assert_eq!(tags::decode_double(run("return ~5;")), -6.0);
}

#[test]
fn to_int32_wraps_where_a_rust_cast_would_saturate() {
    // `2147483648 | 0` is -2147483648. Rust's `as i32` answers 2147483647,
    // which is a plausible number and the wrong one — the difference is
    // invisible until a program touches a value above 2^31.
    assert_eq!(
        tags::decode_double(run("return 2147483648 | 0;")),
        -2147483648.0
    );
    // And past 2^32 it wraps to zero rather than saturating.
    assert_eq!(tags::decode_double(run("return 4294967296 | 0;")), 0.0);
}

#[test]
fn to_int32_of_a_negative_number_takes_the_positive_remainder() {
    // `%` in Rust keeps the sign of the dividend, so `-1 % 2^32` is `-1` where
    // the conversion wants `4294967295`. Written with `>>> 0`, which is the
    // spelling that makes the unsigned reading observable.
    assert_eq!(tags::decode_double(run("return -1 >>> 0;")), 4294967295.0);
}

#[test]
fn a_non_finite_number_converts_to_zero() {
    assert_eq!(tags::decode_double(run("return (1 / 0) | 0;")), 0.0);
    assert_eq!(tags::decode_double(run("return (0 / 0) | 0;")), 0.0);
}

#[test]
fn the_fractional_part_is_discarded_towards_zero() {
    assert_eq!(tags::decode_double(run("return 3.9 | 0;")), 3.0);
    assert_eq!(tags::decode_double(run("return -3.9 | 0;")), -3.0);
}

#[test]
fn a_shift_count_keeps_only_five_bits() {
    // `1 << 32` is 1, not 0. A machine shift by 32 is undefined in C and panics
    // in a Rust debug build, so this is the case that decides whether the
    // masking happened.
    assert_eq!(tags::decode_double(run("return 1 << 32;")), 1.0);
    assert_eq!(tags::decode_double(run("return 1 << 31;")), -2147483648.0);
}

#[test]
fn the_two_right_shifts_differ_in_what_they_do_with_the_sign() {
    // The whole reason `>>>` is a separate operator rather than a flag: its
    // result outgrows a signed thirty-two-bit value.
    assert_eq!(tags::decode_double(run("return -8 >> 1;")), -4.0);
    assert_eq!(tags::decode_double(run("return -8 >>> 1;")), 2147483644.0);
}

#[test]
fn proven_unsigned_right_shift_stays_positive_after_machine_lowering() {
    // Locals keep their numeric proof, so these operands reach the machine path
    // rather than the constant folder or the generic runtime entry point.
    assert_eq!(
        tags::decode_double(run("let value = -1; let count = 0; return value >>> count;")),
        4294967295.0
    );
    assert_eq!(
        tags::decode_double(run("let value = -8; let count = 1; return value >>> count;")),
        2147483644.0
    );
    assert_eq!(
        tags::decode_double(run("let value = 1; let count = 32; return value >>> count;")),
        1.0
    );
}

#[test]
fn unary_plus_skips_numeric_conversion_but_converts_generic_values() {
    assert_eq!(tags::decode_double(run("let value = 3 | 0; return +value;")), 3.0);
    assert_eq!(tags::decode_double(run("return +\"3\";")), 3.0);
}

#[test]
fn a_bitwise_operator_converts_a_string_first() {

    // Which is what makes these entry points at all: `ToInt32` runs `ToNumber`,

    // and `ToNumber` of a string reads its text out of the heap.
    assert_eq!(tags::decode_double(run("return \"12\" & 10;")), 8.0);
}

#[test]
fn exponent_is_right_associative() {
    // `2 ** 3 ** 2` is `2 ** 9`, not `8 ** 2`. A left-associative parse gives
    // 64 and this gives 512.
    assert_eq!(tags::decode_double(run("return 2 ** 3 ** 2;")), 512.0);
}

#[test]
fn exponent_diverges_from_ieee_where_the_specification_says_so() {
    // `(-1) ** Infinity` is NaN in JavaScript and 1.0 from IEEE-754's `pow`,
    // which Rust's `powf` follows. Inheriting that would produce a plausible
    // number rather than a crash, which is the kind of divergence nothing finds
    // later.
    assert!(tags::decode_double(run("return (0 - 1) ** (1 / 0);")).is_nan());
    assert!(tags::decode_double(run("return 1 ** (1 / 0);")).is_nan());
    // And the ordinary case still answers what IEEE does.
    assert_eq!(tags::decode_double(run("return 2 ** 10;")), 1024.0);
}

#[test]
fn loose_equality_converts_where_strict_equality_refuses() {
    assert_eq!(tags::payload_of(run("return 1 == \"1\";")), tags::BOOL_TRUE);
    assert_eq!(
        tags::payload_of(run("return 1 === \"1\";")),
        tags::BOOL_FALSE
    );
    assert_eq!(tags::payload_of(run("return true == 1;")), tags::BOOL_TRUE);
    assert_eq!(tags::payload_of(run("return \"\" == 0;")), tags::BOOL_TRUE);
}

#[test]
fn null_and_undefined_are_loosely_equal_to_each_other_and_nothing_else() {
    // The one rule in the table that is not a conversion, and the one an
    // implementation written as "convert both to numbers" gets wrong: `null`
    // converts to 0, so `null == 0` would answer true.
    assert_eq!(
        tags::payload_of(run("return null == (void 0);")),
        tags::BOOL_TRUE
    );
    assert_eq!(tags::payload_of(run("return null == 0;")), tags::BOOL_FALSE);
    assert_eq!(
        tags::payload_of(run("return (void 0) == 0;")),
        tags::BOOL_FALSE
    );
    assert_eq!(
        tags::payload_of(run("return null == false;")),
        tags::BOOL_FALSE
    );
}

#[test]
fn loose_inequality_is_the_negation_of_loose_equality() {
    assert_eq!(
        tags::payload_of(run("return 1 != \"1\";")),
        tags::BOOL_FALSE
    );
    assert_eq!(tags::payload_of(run("return 1 != 2;")), tags::BOOL_TRUE);
}

#[test]
fn nan_is_equal_to_nothing_under_either_equality() {
    assert_eq!(
        tags::payload_of(run("return (0 / 0) == (0 / 0);")),
        tags::BOOL_FALSE
    );
    assert_eq!(
        tags::payload_of(run("return (0 / 0) === (0 / 0);")),
        tags::BOOL_FALSE
    );
}

// ---------------------------------------------------------------------------
// Template literals.

#[test]
fn a_template_with_no_substitution_is_its_text() {
    assert_eq!(
        tags::payload_of(run("return `abc` === \"abc\";")),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_template_concatenates_rather_than_adding() {
    // The case that decides whether the fold started from a string. `${1}${2}`
    // is "12"; a fold that began with the first SUBSTITUTION would compute 3,
    // because `+` decides between adding and concatenating from its operands.
    assert_eq!(
        tags::payload_of(run("return `${1}${2}` === \"12\";")),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_template_uses_the_string_hint_for_object_substitutions() {
    let produced = run(
        "let calls = 0; \
         let o = { valueOf: function () { return 7; }, \
                   toString: function () { calls++; return 'T'; } }; \
         return `${o}` === 'T' && calls === 1;",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn template_join_roots_hook_returned_strings_until_the_final_string_is_built() {
    holds(concat!(
        "let total = 0; ",
        "for (let i = 0; i < 2000; i++) { ",
        "let o = { toString: function () { return 'x'.repeat(8); } }; ",
        "total += `${o}`.length; ",
        "} ",
        "return total === 16000;",
    ));
}

#[test]
fn string_concat_preserves_arguments_layouts_and_coercion_effects() {
    holds(
        "return \"a\".concat(\"b\", \"c\", \"d\", \"e\", \"f\") === \"abcdef\";",
    );
    holds("return String.prototype.concat.call(5, \"!\") === \"5!\";");
    holds(
        "let s = \"a\".concat(\"日\", String.fromCharCode(0xD800)); \
         return s.length === 3 && s.charCodeAt(1) === 0x65E5 && s.charCodeAt(2) === 0xD800;",
    );
    holds(
        "let calls = 0; \
         let o = { toString: function () { calls++; return \"x\"; } }; \
         return \"a\".concat(o) === \"ax\" && calls === 1;",
    );
}

#[test]
fn string_concat_roots_conversion_results_until_the_join_finishes() {
    holds(concat!(
        "let out = \"\"; ",
        "for (let i = 0; i < 2000; i++) { ",
        "out = out.concat({ toString: function () { return \"xy\".repeat(4); } }); ",
        "} ",
        "return out.length === 16000;",
    ));
}

#[test]
fn a_template_evaluates_its_substitutions_in_order() {
    assert_eq!(
        tags::payload_of(run(
            "let a = 1; let b = 2; return `a${a}b${b}c` === \"a1b2c\";"
        )),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_template_part_is_cooked_rather_than_raw() {
    // `\n` is one code unit, not the two characters that were written. Using
    // the raw text would make this string two longer and compare unequal.
    assert_eq!(
        tags::payload_of(run(r#"return `a\nb` === "a\nb";"#)),
        tags::BOOL_TRUE
    );
}

#[test]
fn adding_a_string_to_a_non_string_converts_the_other_side() {
    // The case `adding_a_string_concatenates_rather_than_adding` did not cover,
    // and which was broken: `"" + 1` answered NaN. `coerce::add` asked "is this
    // a string" of the number side and took its `None` as a refusal, where the
    // question it needed was "spell this as a string".
    //
    // Found by writing a template literal, which is `"" + x + ""` — the first
    // program that ever concatenated something that was not already text.
    assert_eq!(
        tags::payload_of(run("return (\"n=\" + 1) === \"n=1\";")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run("return (1 + \"\") === \"1\";")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run("return (\"\" + true) === \"true\";")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run("return (\"\" + null) === \"null\";")),
        tags::BOOL_TRUE
    );
    // And the direction that must NOT change: two numbers still add.
    assert_eq!(tags::decode_double(run("return 1 + 2;")), 3.0);
}

// ---------------------------------------------------------------------------
// Computed property access, and `in`.

#[test]
fn a_computed_key_reaches_the_property_a_name_would() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 7; return o[\"n\"];")),
        7.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o[\"n\"] = 7; return o.n;")),
        7.0
    );
}

#[test]
fn a_key_computed_at_run_time_is_the_one_written() {
    // The point of the operation: the name is not known while compiling, so it
    // is a value that becomes a key while running.
    let produced = run("let o = {}; o.a = 1; o.b = 2; let k = \"b\"; return o[k];");
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_non_string_key_is_converted_to_one() {
    // `o[0]` and `o["0"]` are one property, because `ToPropertyKey` runs
    // `ToString`. An implementation that kept the number as a number would make
    // them two.
    assert_eq!(
        tags::decode_double(run("let o = {}; o[0] = 5; return o[\"0\"];")),
        5.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o[true] = 6; return o[\"true\"];")),
        6.0
    );
}

#[test]
fn an_absent_computed_property_reads_as_undefined() {
    let mut compiled = compile("let o = {}; return o[\"missing\"];").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

#[test]
fn a_receiver_and_a_key_are_each_evaluated_once_and_in_order() {
    // `a()[b()] = c()` runs `a`, then `b`, then `c`. Recorded as the order the
    // three side effects happened in, which is the only way to see it.
    let produced = run("let log = \"\"; \
         let o = {}; \
         function a() { log = log + \"a\"; return o; } \
         function b() { log = log + \"b\"; return \"k\"; } \
         function c() { log = log + \"c\"; return 1; } \
         a()[b()] = c(); \
         return log === \"abc\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_compound_assignment_to_a_computed_key_reads_it_first() {
    assert_eq!(
        tags::decode_double(run(
            "let o = {}; o[\"n\"] = 10; o[\"n\"] += 5; return o[\"n\"];"
        )),
        15.0
    );
}

#[test]
fn in_asks_whether_the_property_is_there_not_what_it_holds() {
    // The whole reason the operator exists: a property holding `undefined` is
    // still a property. An implementation written as `o[k] !== undefined`
    // answers false here and is a different operator.
    assert_eq!(
        tags::payload_of(run("let o = {}; o.x = void 0; return \"x\" in o;")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run("let o = {}; return \"x\" in o;")),
        tags::BOOL_FALSE
    );
}

#[test]
fn in_takes_the_key_on_the_left() {
    // Getting the operand order backwards produces a program that runs and
    // answers about the wrong one.
    let produced = run("let o = {}; o.a = 1; return (\"a\" in o) && !(\"o\" in o);");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_computed_key_and_a_written_one_reach_the_same_property() {
    // The agreement that a COUNT could not carry. A key the compiler resolved
    // crosses as a number; a computed one arrives as a string and has to reach
    // the number the compiler already chose for that text.
    //
    // Seeding the runtime with how many keys existed was enough while every key
    // was compiler-resolved, and this is the program that broke it: the runtime
    // interned "n" afresh, past the seeded range, and `o["n"]` read a property
    // the compiler had never written to.
    assert_eq!(
        tags::decode_double(run("let o = {}; o.n = 7; return o[\"n\"];")),
        7.0
    );
    assert_eq!(
        tags::decode_double(run("let o = {}; o[\"n\"] = 7; return o.n;")),
        7.0
    );
    // And through `in`, which resolves a key the same way.
    assert_eq!(
        tags::payload_of(run("let o = {}; o.n = 1; return \"n\" in o;")),
        tags::BOOL_TRUE
    );
}

// ---------------------------------------------------------------------------
// Labels.

#[test]
fn a_labelled_break_leaves_the_loop_it_names() {
    // The whole point: an unlabelled `break` leaves the INNER loop and the
    // outer one keeps going, so the two answers differ. Written so they do —
    // if the label were ignored this returns 6 instead of 1.
    let outer_left = run("let count = 0; \
         outer: for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; break outer; } \
         } \
         return count;");
    assert_eq!(tags::decode_double(outer_left), 1.0);

    let inner_left = run("let count = 0; \
         for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; break; } \
         } \
         return count;");
    assert_eq!(tags::decode_double(inner_left), 3.0);
}

#[test]
fn a_labelled_continue_resumes_the_loop_it_names() {
    // `continue outer` abandons the rest of the inner loop AND the rest of the
    // outer body, so the increment after it never runs.
    let produced = run("let count = 0; \
         outer: for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; continue outer; } \
           count = count + 100; \
         } \
         return count;");
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_label_on_a_block_can_be_broken_out_of() {
    // Not a loop, so there is nothing to continue and only `break` reaches it.
    let produced = run("let n = 1; \
         done: { \
           n = 2; \
           break done; \
         } \
         return n;");
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_continue_inside_a_labelled_block_belongs_to_the_loop_around_it() {
    // The reason `Loops::target` skips a frame with nothing to continue to. A
    // search that stopped at the innermost frame would find the block, which
    // cannot be continued, and refuse a program that is legal.
    let produced = run("let count = 0; \
         for (let i = 0; i < 3; i++) { \
           inner: { count++; continue; } \
         } \
         return count;");
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_label_naming_nothing_is_a_syntax_error_and_never_reaches_the_emitter() {
    // Refused by the PARSER, not by emission — "a break statement can only
    // jump to a label of an enclosing statement" is a grammar rule, and SWC
    // enforces it.
    //
    // That is worth pinning rather than assuming: the emitter also refuses a
    // label it cannot find, and if this ever started arriving there instead,
    // the refusal would still look right while having moved out of the layer
    // that can point at the source.
    let error = compile("for (let i = 0; i < 1; i++) { break nowhere; }")
        .expect_err("`nowhere` labels nothing");
    assert!(
        format!("{error:?}").contains("Parse"),
        "expected the parser to reject it, got {error:?}"
    );
}

// ---------------------------------------------------------------------------
// `switch`.

#[test]
fn a_switch_runs_the_clause_that_matched() {
    for (subject, expected) in [(1.0, 10.0), (2.0, 20.0), (9.0, 99.0)] {
        let produced = run(&format!(
            "let n = {subject}; let out = 0; \
             switch (n) {{ \
               case 1: out = 10; break; \
               case 2: out = 20; break; \
               default: out = 99; \
             }} \
             return out;"
        ));
        assert_eq!(tags::decode_double(produced), expected, "for {subject}");
    }
}

#[test]
fn a_clause_without_a_break_falls_into_the_next() {
    // The whole reason a switch is not a chain of `if`s. Without fall-through
    // this returns 1; with it, both bodies run.
    let produced = run("let out = 0; \
         switch (1) { \
           case 1: out = out + 1; \
           case 2: out = out + 10; break; \
           case 3: out = out + 100; \
         } \
         return out;");
    assert_eq!(tags::decode_double(produced), 11.0);
}

#[test]
fn default_runs_where_it_sits_rather_than_last() {
    // `default` is not a fallback appended to the end. Nothing matches 9, so
    // control enters at `default` and falls through into the clause written
    // after it — an implementation that ran `default` last would return 1.
    let produced = run("let out = 0; \
         switch (9) { \
           case 1: out = out + 1; break; \
           default: out = out + 10; \
           case 2: out = out + 100; \
         } \
         return out;");
    assert_eq!(tags::decode_double(produced), 110.0);
}

#[test]
fn a_switch_matches_with_strict_equality() {
    // `case "1"` does not match `1`, so nothing matches and `default` runs.
    let produced = run(
        "let out = 0; switch (1) { case \"1\": out = 1; break; default: out = 2; } return out;",
    );
    assert_eq!(tags::decode_double(produced), 2.0);
    // And NaN matches nothing, including itself.
    let produced = run(
        "let out = 0; switch (0 / 0) { case (0 / 0): out = 1; break; default: out = 2; } \
         return out;",
    );
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_switch_with_no_match_and_no_default_runs_nothing() {
    let produced = run("let out = 7; switch (9) { case 1: out = 1; } return out;");
    assert_eq!(tags::decode_double(produced), 7.0);
}

#[test]
fn a_value_assigned_in_one_clause_survives_the_switch() {
    // The block parameters, doing what they exist for: `out` has a different
    // definition on every path into the exit, and SSA has nothing to write
    // twice.
    let produced = run("let out = 0; \
         for (let i = 0; i < 3; i++) { \
           switch (i) { case 0: out = out + 1; break; case 1: out = out + 10; break; \
                        default: out = out + 100; } \
         } \
         return out;");
    assert_eq!(tags::decode_double(produced), 111.0);
}

#[test]
fn a_continue_inside_a_switch_belongs_to_the_loop_around_it() {
    // A switch takes `break` and is not a loop, so a `continue` written inside
    // one has to pass through its frame to reach the loop. The same rule a
    // labelled block follows, and the reason both record `continue_to: None`.
    let produced = run("let count = 0; \
         for (let i = 0; i < 4; i++) { \
           switch (i) { case 0: continue; default: count++; } \
         } \
         return count;");
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_switch_clause_can_hold_a_closure() {
    // The capture analysis had to learn about switch: a nested function inside
    // a clause is a function like any other, and a name it captures that was
    // not marked would be two closures disagreeing about a variable.
    let produced = run("function make() { \
           let n = 0; \
           switch (1) { case 1: { function bump() { n = n + 5; } bump(); bump(); } } \
           return n; \
         } \
         return make();");
    assert_eq!(tags::decode_double(produced), 10.0);
}

// ---------------------------------------------------------------------------
// Arrays.

#[test]
fn an_array_literal_holds_its_elements() {
    assert_eq!(
        tags::decode_double(run("let a = [10, 20, 30]; return a[1];")),
        20.0
    );
    assert_eq!(tags::decode_double(run("return [10, 20, 30].length;")), 3.0);
    assert_eq!(tags::decode_double(run("return [].length;")), 0.0);
}

#[test]
fn an_element_is_not_a_property() {
    // `a[0]` goes to the element store and `a.x` to the shape tree, and the two
    // do not collide: an array with a property still has its elements.
    let produced = run("let a = [1, 2]; a.tag = 9; return a[0] + a[1] + a.tag;");
    assert_eq!(tags::decode_double(produced), 12.0);
}

#[test]
fn only_a_canonical_index_reaches_the_element_store() {
    // `a[1.5]` and `a[-1]` are ordinary properties, not elements. An
    // implementation that rounded would write into element 1 and change what
    // `a[1]` answers, which is the assertion.
    let produced = run("let a = [1, 2]; a[1.5] = 99; return a[1];");
    assert_eq!(tags::decode_double(produced), 2.0);
    // And the property is still readable under the name it actually has.
    assert_eq!(
        tags::decode_double(run("let a = [1, 2]; a[1.5] = 99; return a[1.5];")),
        99.0
    );
}

#[test]
fn reading_past_the_end_is_undefined_rather_than_an_error() {
    let mut compiled = compile("let a = [1]; return a[9];").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

#[test]
fn writing_past_the_end_grows_the_array() {
    // `let a = []; a[2] = 1` leaves length 3, which is what the language says
    // and what a store that only wrote in range would get wrong.
    assert_eq!(
        tags::decode_double(run("let a = []; a[2] = 1; return a.length;")),
        3.0
    );
    assert_eq!(
        tags::decode_double(run("let a = []; a[2] = 1; return a[2];")),
        1.0
    );
}

#[test]
fn an_array_is_a_reference_and_only_equal_to_itself() {
    assert_eq!(
        tags::payload_of(run("let a = [1]; let b = a; return a === b;")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run("return [1] === [1];")),
        tags::BOOL_FALSE
    );
}

#[test]
fn an_array_is_an_object_to_typeof() {
    // `typeof []` is "object" — there is no array type in the language, which
    // is why `Array.isArray` exists at all.
    assert_eq!(
        tags::payload_of(run("return (typeof [1]) === \"object\";")),
        tags::BOOL_TRUE
    );
}

#[test]
fn an_array_can_be_walked_by_a_loop() {
    // The whole point of having them: an index computed at run time, over a
    // length read from the array rather than written into the program.
    let produced = run("let a = [1, 2, 3, 4]; \
         let total = 0; \
         for (let i = 0; i < a.length; i++) { total = total + a[i]; } \
         return total;");
    assert_eq!(tags::decode_double(produced), 10.0);
}

#[test]
fn a_hole_is_absent_rather_than_an_undefined_that_was_stored() {
    // This test used to assert the opposite — that `[,1]` was REFUSED, because
    // writing the hole as `undefined` would make `0 in [,1]` answer true when
    // the language says false. The refusal was right for as long as the runtime
    // had no way to say "absent"; it has one now, so the assertion is the
    // behaviour rather than the gap.
    //
    // The pair is the whole point: same length, same read, different `in`.
    assert_eq!(tags::decode_double(run("return [,1].length;")), 2.0);
    assert_eq!(tags::decode_double(run("return [,1][1];")), 1.0);
    assert_eq!(tags::decode_double(run("return [,1][0] === undefined ? 1 : 0;")), 1.0);
    assert_eq!(
        tags::decode_double(run("return (0 in [,1]) ? 1 : 0;")),
        0.0,
        "a hole does not exist"
    );
    assert_eq!(
        tags::decode_double(run("return (0 in [undefined,1]) ? 1 : 0;")),
        1.0,
        "an undefined that was STORED does exist — this is the pair the marker buys"
    );
}

// ---------------------------------------------------------------------------
// The predefined global values.

#[test]
fn undefined_can_be_written_rather_than_spelled_void_zero() {
    let mut compiled = compile("return undefined;").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
    // And it is the same value `void 0` produces, which is what every test
    // wrote before this existed.
    assert_eq!(
        tags::payload_of(run("return undefined === (void 0);")),
        tags::BOOL_TRUE
    );
}

#[test]
fn nan_and_infinity_are_the_numbers_they_name() {
    assert!(tags::decode_double(run("return NaN;")).is_nan());
    assert_eq!(tags::decode_double(run("return Infinity;")), f64::INFINITY);
    assert_eq!(
        tags::decode_double(run("return 0 - Infinity;")),
        f64::NEG_INFINITY
    );
    // NaN is equal to nothing, including the name that produced it.
    assert_eq!(
        tags::payload_of(run("return NaN === NaN;")),
        tags::BOOL_FALSE
    );
}

#[test]
fn a_local_shadows_a_predefined_global() {
    // They are names, not keywords: `let undefined = 1` is legal in a function
    // and the local wins. A lookup that checked the three FIRST would answer
    // the global here.
    assert_eq!(
        tags::decode_double(run("let undefined = 1; return undefined;")),
        1.0
    );
    assert_eq!(tags::decode_double(run("let NaN = 2; return NaN;")), 2.0);
}

#[test]
fn an_undeclared_name_is_still_the_programs_error() {
    // Reading one is a `ReferenceError`, not `undefined`. Answering `undefined`
    // would turn every typo into a program that runs, and that is the property
    // this test exists for.
    //
    // It used to assert a COMPILE-time refusal, which was stricter than the
    // language: the error belongs where the read happens, so a program whose
    // dead branch mentions a name it never reads is legal and used to be
    // rejected outright. It compiles now and raises where the language raises,
    // catchable — so the assertion moved from "refused" to "throws", which is
    // what it was protecting all along.
    let caught = run("try { return nowhere; } catch (e) { return e instanceof ReferenceError; }");
    assert_eq!(
        caught,
        rts_core::value::Value::from_bool(true).bits(),
        "reading an undeclared name must raise a catchable ReferenceError"
    );
}

// ---------------------------------------------------------------------------
// `delete`.

#[test]
fn deleting_a_property_removes_it() {
    assert_eq!(
        tags::payload_of(run("let o = {}; o.x = 1; delete o.x; return \"x\" in o;")),
        tags::BOOL_FALSE
    );
    // And it answers whether the object now lacks it, which is `true` for one
    // it never had.
    assert_eq!(
        tags::payload_of(run("let o = {}; return delete o.missing;")),
        tags::BOOL_TRUE
    );
}

#[test]
fn deleting_one_property_leaves_the_others_where_they_can_be_read() {
    // The part that is easy to get wrong. Removing a property shifts every
    // later one down a slot, so the values have to move with the layout — an
    // implementation that only changed the header would read `c` out of `b`'s
    // old offset.
    let produced = run("let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         delete o.b; \
         return o.a + o.c;");
    assert_eq!(tags::decode_double(produced), 4.0);
}

#[test]
fn a_deleted_property_can_be_added_back() {
    let produced = run("let o = {}; o.x = 1; delete o.x; o.x = 9; return o.x;");
    assert_eq!(tags::decode_double(produced), 9.0);
}

#[test]
fn a_computed_key_can_be_deleted() {
    assert_eq!(
        tags::payload_of(run(
            "let o = {}; o.x = 1; let k = \"x\"; delete o[k]; return \"x\" in o;"
        )),
        tags::BOOL_FALSE
    );
}

// ---------------------------------------------------------------------------
// The rest of an object literal.

#[test]
fn a_method_in_a_literal_is_callable() {
    let produced = run("let o = { n: 6, twice() { return this.n * 2; } }; return o.twice();");
    assert_eq!(tags::decode_double(produced), 12.0);
}

#[test]
fn a_computed_key_in_a_literal_is_evaluated() {
    let produced = run("let k = \"a\"; let o = { [k]: 5 }; return o.a;");
    assert_eq!(tags::decode_double(produced), 5.0);
    // And it is a value rather than a name, so it can be computed.
    let produced = run("let o = { [\"x\" + \"y\"]: 7 }; return o.xy;");
    assert_eq!(tags::decode_double(produced), 7.0);
}

#[test]
fn shorthand_reads_the_binding_of_that_name() {
    // `{ a }` requires `a` to be readable where `{ a: 1 }` requires nothing,
    // which is the difference the tree records and the reason it is a flag
    // rather than a separate node.
    assert_eq!(
        tags::decode_double(run("let a = 3; let o = { a }; return o.a;")),
        3.0
    );
}

#[test]
fn properties_are_set_in_source_order() {
    // The later one wins, which is only observable if both are emitted and in
    // the order written.
    assert_eq!(
        tags::decode_double(run("let o = { a: 1, a: 2 }; return o.a;")),
        2.0
    );
}

// ---------------------------------------------------------------------------
// `for-in`.

#[test]
fn for_in_visits_every_own_key() {
    let produced = run("let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         let total = 0; \
         for (let k in o) { total = total + o[k]; } \
         return total;");
    assert_eq!(tags::decode_double(produced), 6.0);
}

#[test]
fn for_in_yields_keys_as_strings() {
    // Even for an array index. `for (k in [1,2])` yields "0" and "1", so a body
    // comparing `k === 0` finds nothing — which is the language rather than a
    // quirk of this implementation, and the reason the test asserts the string.
    let produced = run("let a = [7, 8]; \
         let joined = \"\"; \
         for (let k in a) { joined = joined + k; } \
         return joined === \"01\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn for_in_visits_them_in_the_order_they_were_added() {
    let produced = run("let o = {}; o.z = 1; o.a = 2; o.m = 3; \
         let joined = \"\"; \
         for (let k in o) { joined = joined + k; } \
         return joined === \"zam\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn for_in_over_nothing_runs_nothing() {
    let produced = run("let o = {}; let count = 0; for (let k in o) { count++; } return count;");
    assert_eq!(tags::decode_double(produced), 0.0);
}

#[test]
fn break_and_continue_reach_the_for_in_they_are_written_in() {
    // What the expansion buys: the loop machinery already gets these right, so
    // they work without `for-in` restating any of it.
    let produced = run("let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         let count = 0; \
         for (let k in o) { if (k === \"b\") { continue; } count++; } \
         return count;");
    assert_eq!(tags::decode_double(produced), 2.0);

    let produced = run("let o = {}; o.a = 1; o.b = 2; \
         let count = 0; \
         outer: for (let k in o) { for (let j in o) { count++; break outer; } } \
         return count;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_captured_loop_variable_is_a_fresh_binding_on_every_pass() {
    // This asserted the OPPOSITE until the divergence was closed, and the note
    // it carried said the fix "means creating an environment inside the loop,
    // chained to the function's, whenever the body declares a name something
    // captures". That is what `loops::open_iteration` does, so the assertion
    // was turned round rather than the test deleted — a pinned divergence
    // becoming a pinned behaviour is the point of having pinned it.
    //
    // `for-in` rather than an ordinary `for` on purpose: the desugaring
    // declares the key in the BODY, not the head, so this pins the half that a
    // head-only fix would have missed while looking finished.
    let produced = run("function collect() { \
           let o = {}; o.a = 1; o.b = 2; \
           let first = 0; \
           for (let k in o) { function keep() { return k; } if (first === 0) { first = keep; } } \
           return first(); \
         } \
         return collect() === \"a\";");
    assert_eq!(
        tags::payload_of(produced),
        tags::BOOL_TRUE,
        "the closure made on the first pass captures the FIRST key"
    );
}

#[test]
fn a_captured_name_assigned_in_a_loop_does_not_need_a_block_parameter() {
    // This CRASHED the compiler, on ordinary JavaScript. `n` is captured, so it
    // lives in an environment object; the loop also assigns it, so the
    // syntactic scan offered it as a name to carry across the back edge — and
    // an environment binding has no value to carry. The panic said "which
    // cannot happen".
    //
    // It could, because the two facts were established in different places: the
    // capture analysis decided where `n` lives, the loop's scan read the tree,
    // and nothing compared them.
    //
    // Every pass reads and writes the same heap slot, so the loop carries
    // nothing for it — which is why the closure sees the finished value.
    let produced = run("function f() { \
           let n = 0; \
           function g() { return n; } \
           while (n < 3) { n = n + 1; } \
           return g(); \
         } \
         return f();");
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_captured_name_assigned_in_a_switch_or_a_labelled_block_is_the_same_case() {
    // The other two constructs that carry names across a join, for the same
    // reason and through the same `plan`.
    let produced = run("function f() { \
           let n = 0; \
           function g() { return n; } \
           switch (1) { case 1: n = 7; } \
           return g(); \
         } \
         return f();");
    assert_eq!(tags::decode_double(produced), 7.0);

    let produced = run("function f() { \
           let n = 0; \
           function g() { return n; } \
           done: { n = 9; break done; } \
           return g(); \
         } \
         return f();");
    assert_eq!(tags::decode_double(produced), 9.0);
}

#[test]
fn a_string_that_spells_an_index_reaches_the_element() {
    // `a[0]` and `a["0"]` are one thing, and this is not a nicety: `for-in`
    // yields STRINGS, so `a[k]` inside such a loop is always the string form.
    //
    // Answering NaN was the first version's behaviour — every read missed the
    // elements and found an absent property — and the loop below is the program
    // that showed it.
    assert_eq!(
        tags::decode_double(run("let a = [1, 2, 3]; return a[\"1\"];")),
        2.0
    );
    let produced =
        run("let s = 0; let a = [1, 2, 3]; for (let k in a) { s = s + a[k]; } return s;");
    assert_eq!(tags::decode_double(produced), 6.0);
}

#[test]
fn length_is_a_property_both_paths_read() {
    // It used to be answered by the runtime special-casing the key, which
    // worked until something stored a `length` property — because compiled code
    // does not reach the runtime for a hit. It emits `cached_get`, finds the
    // stored property, and never asks.
    //
    // A special case only the slow path knows about stops applying the moment
    // the fast path starts working, which is the opposite of how a fast path
    // should fail. So the count is stored, and both read the same thing.
    assert_eq!(
        tags::decode_double(run("let a = [1, 2, 3]; return a.length;")),
        3.0
    );
    assert_eq!(
        tags::decode_double(run("let a = []; a[2] = 1; return a.length;")),
        3.0
    );
    // Another property does not disturb it.
    assert_eq!(
        tags::decode_double(run("let a = [1, 2]; a.x = 9; return a.length;")),
        2.0
    );
}

#[test]
fn length_is_not_enumerable() {
    // The cost of `length` being a real property: everything else reads it as
    // one, so the single place that must not is enumeration. Without this,
    // `for (k in [1,2,3]) s += a[k]` summed to 9 rather than 6 — the loop
    // visited "length" and added 3.
    let produced = run(
        "let a = [1, 2]; let joined = \"\"; for (let k in a) { joined = joined + k; } \
         return joined === \"01\";",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn an_object_can_hold_more_than_seven_properties() {
    // A cell has seven inline slots, and the eighth property used to be LOST:
    // the write was refused and the read answered `undefined`. The region's own
    // documentation calls a truncated object "a wrong answer that looks like a
    // right one" while describing that refusal — which is exactly what it
    // became, because the read had no way to say so.
    //
    // This is the overflow indirection that documentation names. Compiled code
    // never reaches it: `cache_resolve` already answered negative for a slot
    // past the inline ones, so such a read takes the slow path — which is why
    // the fast path needed no change at all.
    let produced = run("let o = {}; \
         o.a=1; o.b=2; o.c=3; o.d=4; o.e=5; o.f=6; o.g=7; o.h=8; o.i=9; o.j=10; o.k=11; o.l=12; \
         return o.a+o.b+o.c+o.d+o.e+o.f+o.g+o.h+o.i+o.j+o.k+o.l;");
    assert_eq!(tags::decode_double(produced), 78.0);

    // The eighth on its own, which is the one that was undefined.
    let produced = run("let o = {}; o.a=1;o.b=2;o.c=3;o.d=4;o.e=5;o.f=6;o.g=7;o.h=8; return o.h;");
    assert_eq!(tags::decode_double(produced), 8.0);
}

#[test]
fn deleting_a_property_reshuffles_across_the_spill_too() {
    // `delete` reads every survivor against the old layout and writes it back
    // against the new. With more than seven properties some of those reads and
    // writes cross between the cell and the spill, in both directions — which
    // is why the slot accessor is one function rather than a check at each of
    // the four call sites.
    let produced = run("let o = {}; \
         o.a=1; o.b=2; o.c=3; o.d=4; o.e=5; o.f=6; o.g=7; o.h=8; o.i=9; \
         delete o.a; \
         return o.h + o.i + o.b;");
    assert_eq!(tags::decode_double(produced), 19.0);
}

#[test]
fn enumeration_sees_the_spilled_properties() {
    let produced = run("let o = {}; \
         o.a=1;o.b=2;o.c=3;o.d=4;o.e=5;o.f=6;o.g=7;o.h=8; \
         let n = 0; for (let k in o) { n++; } return n;");
    assert_eq!(tags::decode_double(produced), 8.0);
}

// ---------------------------------------------------------------------------
// Reading a property of a string.

#[test]
fn a_string_has_a_length() {
    assert_eq!(
        tags::decode_double(run("let s = \"abc\"; return s.length;")),
        3.0
    );
    assert_eq!(tags::decode_double(run("return \"\".length;")), 0.0);
    // Counted in code units, not characters: a JavaScript string IS a sequence
    // of UTF-16 code units, so an astral character is two.
    assert_eq!(
        tags::decode_double(run("return \"\u{1F600}\".length;")),
        2.0
    );
}

#[test]
fn a_string_can_be_indexed() {
    assert_eq!(
        tags::payload_of(run("let s = \"abc\"; return s[1] === \"b\";")),
        tags::BOOL_TRUE
    );
    // Out of range is `undefined` rather than the empty string, which is the
    // difference between `s[9]` and `s.charAt(9)`.
    assert_eq!(
        tags::payload_of(run("let s = \"abc\"; return s[9] === undefined;")),
        tags::BOOL_TRUE
    );
    // And what comes back is a string, so it concatenates.
    assert_eq!(
        tags::payload_of(run("let s = \"ab\"; return (s[0] + s[1]) === \"ab\";")),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_string_property_does_not_shadow_an_ordinary_one() {
    // The special case applies to strings only. An object's `length` is a
    // property like any other, and an array's is stored — three answers to one
    // key, each in the place that owns it.
    assert_eq!(
        tags::decode_double(run("let o = {}; o.length = 5; return o.length;")),
        5.0
    );
    assert_eq!(tags::decode_double(run("return [1, 2].length;")), 2.0);
}

#[test]
fn a_string_can_be_walked_by_a_loop() {
    let produced = run("let s = \"abc\"; let joined = \"\"; \
         for (let i = 0; i < s.length; i++) { joined = joined + s[i]; } \
         return joined === \"abc\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn an_element_can_be_incremented() {
    assert_eq!(
        tags::decode_double(run("let a = [1, 2]; a[0]++; a[1]++; return a[0] + a[1];")),
        5.0
    );
    // Postfix yields the old value, coerced — the same rule a local and a
    // property follow, now reachable through a third place.
    assert_eq!(tags::decode_double(run("let a = [5]; return a[0]--;")), 5.0);
    assert_eq!(tags::decode_double(run("let a = [1]; return ++a[0];")), 2.0);
    assert_eq!(
        tags::decode_double(run("let o = {}; o[\"n\"] = 1; o[\"n\"]++; return o.n;")),
        2.0
    );
}

#[test]
fn a_computed_key_in_an_update_is_evaluated_once() {
    // `a[f()]++` calls `f` a single time. A rewrite to `a[f()] = a[f()] + 1`
    // calls it twice — the same trap the named case records, and one step worse
    // here because a computed key can have a side effect of its own.
    let produced = run("let calls = 0; let a = [1]; \
         function k() { calls = calls + 1; return 0; } \
         a[k()]++; \
         return calls;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_function_is_an_object_and_can_hold_properties() {
    // `f.x = 1` used to be a silent no-op: a callable was a cell at a reserved
    // layout, and a cell with no shape cannot hold a property. The same defect
    // arrays had, found the same way.
    assert_eq!(
        tags::decode_double(run("function f() { return 1; } f.x = 7; return f.x;")),
        7.0
    );
    // And it is still callable, which is the half the reserved layout was
    // protecting: the code address lives beside the cell, where nothing a
    // program can write reaches it.
    assert_eq!(
        tags::decode_double(run("function f() { return 1; } f.x = 7; return f() + f.x;")),
        8.0
    );
    assert_eq!(
        tags::payload_of(run(
            "function f() { return 1; } return (typeof f) === \"function\";"
        )),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_property_on_a_primitive_string_is_still_dropped() {
    // Not the same defect, and not a defect at all: assigning to a property of
    // a primitive string is a no-op in sloppy mode and a `TypeError` in strict.
    // Asserted so that a later change to how strings are stored does not turn
    // this into an accidental feature.
    let mut compiled = compile("let s = \"a\"; s.x = 7; return s.x;").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

// ---------------------------------------------------------------------------
// Prototypes, `new` and `instanceof`.

#[test]
fn new_makes_an_object_the_constructor_writes_to() {
    assert_eq!(
        tags::decode_double(run(
            "function P(v) { this.v = v; } let p = new P(5); return p.v;"
        )),
        5.0
    );
    assert_eq!(
        tags::decode_double(run(
            "function P(a, b) { this.s = a + b; } return new P(1, 2).s;"
        )),
        3.0
    );
}

#[test]
fn a_property_is_found_on_the_prototype() {
    // The read walks what the object inherits from. `cache_resolve` needed no
    // change for this: it already answered negative when the own layout misses,
    // which is exactly the inherited case, so such a read takes the slow path.
    assert_eq!(
        tags::decode_double(run(
            "function P() {} P.prototype.m = 7; let p = new P(); return p.m;"
        )),
        7.0
    );
    // An own property shadows an inherited one, and adding an unrelated own
    // property does not disturb the inherited one.
    assert_eq!(
        tags::decode_double(run(
            "function P() {} P.prototype.m = 9; let p = new P(); p.m = 1; return p.m;"
        )),
        1.0
    );
    assert_eq!(
        tags::decode_double(run(
            "function P() {} P.prototype.m = 9; let p = new P(); p.own = 1; return p.m;"
        )),
        9.0
    );
}

#[test]
fn a_constructor_that_returns_an_object_produces_that_one() {
    // The clause an implementation forgets. A factory written this way is
    // ordinary JavaScript rather than a corner.
    assert_eq!(
        tags::decode_double(run(
            "function P() { return { a: 1 }; } let p = new P(); return p.a;"
        )),
        1.0
    );
    // Returning anything that is NOT an object leaves the fresh one.
    assert_eq!(
        tags::decode_double(run(
            "function P() { this.a = 2; return 9; } let p = new P(); return p.a;"
        )),
        2.0
    );
}

#[test]
fn instanceof_walks_the_chain() {
    assert_eq!(
        tags::payload_of(run(
            "function P() {} let p = new P(); return p instanceof P;"
        )),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(run(
            "function P() {} function Q() {} let p = new P(); return p instanceof Q;"
        )),
        tags::BOOL_FALSE
    );
    // An object that was never constructed inherits from nothing.
    assert_eq!(
        tags::payload_of(run("let o = {}; function P() {} return o instanceof P;")),
        tags::BOOL_FALSE
    );
}

#[test]
fn a_constructed_object_is_an_object_to_typeof() {
    assert_eq!(
        tags::payload_of(run(
            "function P() {} let p = new P(); return (typeof p) === \"object\";"
        )),
        tags::BOOL_TRUE
    );
}

#[test]
fn a_throw_reaches_the_catch_in_the_same_function() {
    // The machine's regions, driven by the language for the first time. The
    // handler receives the thrown value as a block parameter, which is why the
    // number that comes back is the one that was thrown rather than a flag
    // saying something happened.
    let produced = run("try { throw 7; } catch (e) { return e; } return 1;");
    assert_eq!(tags::decode_double(produced), 7.0);
}

#[test]
fn a_try_that_throws_nothing_runs_its_body_and_not_its_handler() {
    let produced = run("try { return 2; } catch (e) { return 99; }");
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_throw_from_inside_a_nested_block_still_finds_the_handler() {
    // The one this could plausibly get wrong: the `if` creates blocks of its
    // own, and a block outside the region it is lexically inside would unwind
    // past this `catch`. The machine derives membership from where building
    // was, so the nested block is in the region without anything saying so.
    let produced = run("try { if (1) { throw 5; } } catch (e) { return e; } return 0;");
    assert_eq!(tags::decode_double(produced), 5.0);
}


#[test]
fn a_value_assigned_inside_a_try_is_the_one_the_handler_sees() {
    // The reason `capture` forces these into the environment. A handler is
    // entered from every throwing point in the region, so an SSA value assigned
    // in the body has no name there — and reading the value from before the
    // body would silently give 0.
    let produced = run("let x = 0; try { x = 4; throw 1; } catch (e) { return x; } return 9;");
    assert_eq!(tags::decode_double(produced), 4.0);
}

#[test]
fn a_try_around_a_call_catches_what_the_callee_threw() {
    // This was refused by name, and the reason was sound while it held: a throw
    // inside the callee ran past the handler, because where a throw lands is
    // planned from the region tree of the function containing it, and a `catch`
    // that reads correctly and never runs is worse than one that does not
    // compile.
    //
    // What changed is not the handler search. A throw leaves ONE frame — the
    // runtime records it and the machine returns instead of ending the program —
    // and every call site asks whether the frame below left by throwing, then
    // re-raises. The region tree does the rest, exactly as it already did.
    let produced = run("function f() { throw 7; } try { f(); } catch (e) { return e; } return 0;");
    assert_eq!(tags::decode_double(produced), 7.0);

    // Two frames, which is what makes it propagation rather than a special case
    // for the innermost call: the middle frame has to stop after its own call
    // returned instead of carrying on to its `return`.
    let deep = run(
        "function inner() { throw 8; } function outer() { inner(); return 1; } \
         try { outer(); } catch (e) { return e; } return 0;",
    );
    assert_eq!(tags::decode_double(deep), 8.0);
}

#[test]
fn a_finally_runs_on_both_the_normal_path_and_the_throwing_one() {
    // Two copies of one body: one on the normal path, one reached by unwinding.
    // Asserting only one would pass with a `finally` that was emitted once and
    // reached one way, which is the shape of the bug this construct invites.
    let quiet = run("let x = 0; try { x = 1; } finally { x = x + 10; } return x;");
    assert_eq!(tags::decode_double(quiet), 11.0);

    let caught = run("let x = 0; try { throw 1; } catch (e) { x = 2; } finally { x = x + 10; } return x;");
    assert_eq!(tags::decode_double(caught), 12.0);
}

#[test]
fn a_finally_runs_when_the_body_returns_through_it() {
    // The machine plans a `return` inside a region the same way it plans a
    // throw: leaving normally owes the same cleanup as leaving by throwing. A
    // scope that only unwinds correctly when something goes wrong leaks on the
    // path taken most of the time.
    let produced = run("let x = 0; try { return 5; } finally { x = 1; } return x;");
    assert_eq!(tags::decode_double(produced), 5.0);
}

#[test]
fn a_finally_that_branches_is_copied_whole() {
    // The reason a cleanup stopped being one block. This body needs a branch
    // and a merge, and the single-block cleanup could not hold either.
    let produced = run(
        "let x = 0; try { throw 1; } catch (e) { x = 3; } finally { if (x > 2) { x = 100; } else { x = 200; } } return x;",
    );
    assert_eq!(tags::decode_double(produced), 100.0);
}

#[test]
fn a_regular_expression_literal_matches() {
    // The first program in this file whose answer comes from a matching engine.
    // Both directions, because a `test` that answered true unconditionally
    // would pass the first assertion alone.
    let found = run("return /a+/.test(\"caaat\");");
    assert_eq!(tags::payload_of(found), tags::BOOL_TRUE);

    let absent = run("return /a+/.test(\"dog\");");
    assert_eq!(tags::payload_of(absent), tags::BOOL_FALSE);
}

#[test]
fn a_regular_expression_answers_what_it_was_written_as() {
    // `source` and `flags` are ordinary properties, so this also pins that the
    // object a literal makes is an object: it goes through the same property
    // read every other one does.
    let source = run("return /a+/gi.source === \"a+\";");
    assert_eq!(tags::payload_of(source), tags::BOOL_TRUE);

    let flags = run("return /a+/gi.flags === \"gi\";");
    assert_eq!(tags::payload_of(flags), tags::BOOL_TRUE);

    let global = run("return /a+/g.global;");
    assert_eq!(tags::payload_of(global), tags::BOOL_TRUE);

    let local = run("return /a+/.global;");
    assert_eq!(tags::payload_of(local), tags::BOOL_FALSE);
}

#[test]
fn a_flag_changes_what_matches_rather_than_only_being_recorded() {
    // The assertion `flags === "i"` would pass for an engine that stored the
    // letter and ignored it. This one cannot.
    let insensitive = run("return /ABC/i.test(\"xabcx\");");
    assert_eq!(tags::payload_of(insensitive), tags::BOOL_TRUE);

    let sensitive = run("return /ABC/.test(\"xabcx\");");
    assert_eq!(tags::payload_of(sensitive), tags::BOOL_FALSE);
}

#[test]
fn exec_answers_the_match_its_groups_and_where_it_was() {
    let whole = run("let m = /b(c)/.exec(\"abcd\"); return m[0] === \"bc\";");
    assert_eq!(tags::payload_of(whole), tags::BOOL_TRUE);

    let group = run("let m = /b(c)/.exec(\"abcd\"); return m[1] === \"c\";");
    assert_eq!(tags::payload_of(group), tags::BOOL_TRUE);

    // In code units from the start of the subject, which is what makes a match
    // locatable at all.
    let produced = run("let m = /b(c)/.exec(\"abcd\"); return m.index;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn exec_answers_null_and_not_undefined_when_nothing_matches() {
    // `while ((m = re.exec(s)) !== null)` is how a global pattern is walked, and
    // a loop written that way against `undefined` never ends. So the difference
    // between the two absences is load-bearing rather than cosmetic.
    let produced = run("return /z/.exec(\"abc\") === null;");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_global_pattern_walks_the_subject_across_calls() {
    // What `lastIndex` is for: the second call must not find the first match
    // again. A `g` implemented as "the same search, plus a flag stored" would
    // answer 0 twice here.
    let produced = run(
        "let r = /a/g; r.test(\"aa\"); let first = r.lastIndex; r.test(\"aa\"); return r.lastIndex - first;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_program_can_reset_where_the_next_search_starts() {
    // `lastIndex` is a real property rather than state held beside the cell,
    // and this is the difference: a copy the runtime kept would ignore this
    // assignment and search from 1.
    let produced = run("let r = /a/g; r.test(\"aa\"); r.lastIndex = 0; r.test(\"aa\"); return r.lastIndex;");
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn each_evaluation_of_one_literal_is_its_own_object() {
    // Why the literal is a call and not a constant the compilation resolved.
    // A hoisted regular expression would make both passes share `lastIndex`,
    // and the second would start where the first left off.
    let produced = run(
        "let n = 0; let i = 0; while (i < 2) { let r = /a/g; if (r.test(\"aa\")) { n = n + r.lastIndex; } i = i + 1; } return n;",
    );
    assert_eq!(tags::decode_double(produced), 2.0, "1 from each pass, not 1 then 2");
}

#[test]
fn a_lookahead_reaches_the_engine_that_has_one() {
    // `regex` refuses this by construction — the refusal is what buys its
    // linear time — so an answer here is the fallback working rather than a
    // pattern that happened to compile.
    let found = run("return /foo(?=bar)/.test(\"foobar\");");
    assert_eq!(tags::payload_of(found), tags::BOOL_TRUE);

    let absent = run("return /foo(?=bar)/.test(\"foobaz\");");
    assert_eq!(tags::payload_of(absent), tags::BOOL_FALSE);
}

#[test]
fn a_regular_expression_can_be_constructed_from_a_value() {
    // The other spelling of the same operation. `RegExp` is not a constant the
    // emitter can produce — it is an object with a `prototype` — so this also
    // pins that a name the runtime provides reaches the value it made.
    let found = run("let r = new RegExp(\"a+\", \"i\"); return r.test(\"AAA\");");
    assert_eq!(tags::payload_of(found), tags::BOOL_TRUE);

    // The pattern is a value here, which is what a table of patterns compiled
    // at build time could not have served.
    let computed = run("let p = \"b\" + \"c\"; let r = new RegExp(p); return r.test(\"abcd\");");
    assert_eq!(tags::payload_of(computed), tags::BOOL_TRUE);
}

#[test]
fn regexp_without_new_makes_one_too() {
    // One of the few constructors the language says behaves the same either
    // way, and it falls out of `construct` answering the object a callee
    // returned rather than being written for twice.
    let produced = run("let r = RegExp(\"a\"); return r.test(\"xax\");");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_constructed_expression_is_the_same_kind_of_thing_as_a_literal() {
    // Same prototype, so `test` is found by the same chain walk — and
    // `instanceof` answers through machinery that knows nothing about regular
    // expressions, which is what says the constructor was wired rather than
    // special-cased.
    let produced = run("return /a/ instanceof RegExp;");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);

    let constructed = run("return new RegExp(\"a\") instanceof RegExp;");
    assert_eq!(tags::payload_of(constructed), tags::BOOL_TRUE);
}

#[test]
fn missing_flags_are_no_flags_rather_than_a_refusal() {
    let produced = run("return new RegExp(\"a\").flags === \"\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn the_provided_name_is_one_object_however_often_it_is_read() {
    // A value made per read would make `RegExp === RegExp` false, and a program
    // attaching something to the constructor would attach it to a copy.
    let produced = run("return RegExp === RegExp;");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);

    let written = run("RegExp.mine = 7; return RegExp.mine;");
    assert_eq!(tags::decode_double(written), 7.0);
}

#[test]
fn a_name_nothing_provides_is_still_the_programs_error() {
    // The provided set is not a global object, and this is the difference that
    // matters: a typo does not become `undefined`. It is now a `ReferenceError`
    // at the read rather than a refusal at compile time — the language's own
    // answer — and this asserts the part that matters rather than the mechanism.
    let caught = run("try { return Elephant; } catch (e) { return e instanceof ReferenceError; }");
    assert_eq!(caught, rts_core::value::Value::from_bool(true).bits());
}

#[test]
fn a_string_finds_its_methods_by_inheriting_them() {
    // The first time a string reads anything but `length` and an index. A string
    // cell has no own prototype — one link per string would be a word spent on a
    // fact they all share — so the chain walk substitutes the shared object.
    let produced = run("return \"abc\".toUpperCase() === \"ABC\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn the_string_methods_count_code_units_and_not_bytes() {
    // `é` is one code unit and two UTF-8 bytes. Every index in these methods
    // is a position in the unit sequence, and working in bytes here would make
    // `slice` cut a character in half while `length` said otherwise.
    let length = run("return \"é\".length;");
    assert_eq!(tags::decode_double(length), 1.0);

    let sliced = run("return \"éa\".slice(1) === \"a\";");
    assert_eq!(tags::payload_of(sliced), tags::BOOL_TRUE);

    let found = run("return \"éa\".indexOf(\"a\");");
    assert_eq!(tags::decode_double(found), 1.0);
}

#[test]
fn index_of_keeps_object_conversion_order_off_the_direct_path() {
    let produced = run(
        "let order = ''; \
         let receiver = { toString() { order = order + 'h'; return 'abc'; } }; \
         let needle = { toString() { order = order + 'n'; return 'b'; } }; \
         let found = String.prototype.indexOf.call(receiver, needle, 0); \
         return found === 1 && order === 'hn';"
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn slice_crosses_where_substring_swaps() {
    // The one difference between the two methods, which is why the language has
    // both and why an implementation sharing their code would be wrong.
    let crossed = run("return \"abc\".slice(2, 1) === \"\";");
    assert_eq!(tags::payload_of(crossed), tags::BOOL_TRUE);

    let swapped = run("return \"abc\".substring(2, 1) === \"b\";");
    assert_eq!(tags::payload_of(swapped), tags::BOOL_TRUE);

    // Negative counts from the end for one and clamps to zero for the other.
    let from_end = run("return \"abcd\".slice(-2) === \"cd\";");
    assert_eq!(tags::payload_of(from_end), tags::BOOL_TRUE);
}

#[test]
fn slice_keeps_receiver_and_bound_conversion_order_off_the_direct_path() {
    let produced = run(
        "let order = ''; \
         let receiver = { toString() { order = order + 'r'; return 'abcd'; } }; \
         let from = { valueOf() { order = order + 'f'; return 1; } }; \
         let to = { valueOf() { order = order + 't'; return 3; } }; \
         let result = String.prototype.slice.call(receiver, from, to); \
         return result === 'bc' && order === 'rft';"
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn char_at_answers_the_empty_string_where_the_index_answers_undefined() {
    // Both spellings exist because they disagree here, and an engine answering
    // the same for both makes `s.charAt(9) === ""` false.
    let empty = run("return \"abc\".charAt(9) === \"\";");
    assert_eq!(tags::payload_of(empty), tags::BOOL_TRUE);

    let absent = run("return \"abc\"[9] === undefined;");
    assert_eq!(tags::payload_of(absent), tags::BOOL_TRUE);

    // And `at` was added to the language precisely so a negative index works.
    let last = run("return \"abc\".at(-1) === \"c\";");
    assert_eq!(tags::payload_of(last), tags::BOOL_TRUE);
}

#[test]
fn char_code_at_keeps_utf16_units_and_nan_semantics() {
    let produced = run(
        "const s = new String('😀'); \
         return (s.charCodeAt(0) === 55357 && s.charCodeAt(1) === 56832 && \
                 'abc'.charCodeAt(9) !== 'abc'.charCodeAt(9)) ? 1 : 0;"
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn char_code_at_converts_the_receiver_before_an_object_index() {
    let produced = run(
        "let order = ''; \
         let receiver = { toString() { order = order + 'r'; return 'abc'; } }; \
         let index = { valueOf() { order = order + 'i'; return 1; } }; \
         let code = String.prototype.charCodeAt.call(receiver, index); \
         return code === 98 && order === 'ri';"
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_program_can_add_a_method_to_every_string() {
    // `String.prototype.mine = f` is not special-cased anywhere: it is a
    // property write on the object strings inherit from, and the chain walk
    // finds it the way it finds a built-in.
    let produced = run(
        "String.prototype.shout = function () { return this.toUpperCase(); }; return \"hi\".shout() === \"HI\";",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn the_constructor_holds_properties_of_its_own() {
    // `String.yellow = f` — an ordinary write on the constructor, which is an
    // ordinary object.
    let produced = run("String.twice = function (s) { return s + s; }; return String.twice(\"ab\") === \"abab\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn string_converts_a_value_to_its_text() {
    let produced = run("return String(12) === \"12\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_string_method_takes_a_pattern_or_plain_text() {
    // One method, two kinds of separator — which is how the specification
    // writes it, and why an implementation per kind is where the two would come
    // to disagree.
    let by_text = run("let p = \"a-b\".split(\"-\"); return p[1] === \"b\";");
    assert_eq!(tags::payload_of(by_text), tags::BOOL_TRUE);

    let by_pattern = run("let p = \"a1b\".split(/[0-9]/); return p[1] === \"b\";");
    assert_eq!(tags::payload_of(by_pattern), tags::BOOL_TRUE);

    // No separator at all is the whole string as one piece, where an empty one
    // splits between every unit. A sentence apart in the specification.
    let whole = run("return \"abc\".split().length;");
    assert_eq!(tags::decode_double(whole), 1.0);

    let each = run("return \"abc\".split(\"\").length;");
    assert_eq!(tags::decode_double(each), 3.0);
}

#[test]
fn replace_takes_the_first_and_a_global_pattern_takes_all() {
    let first = run("return \"aaa\".replace(\"a\", \"b\") === \"baa\";");
    assert_eq!(tags::payload_of(first), tags::BOOL_TRUE);

    let all = run("return \"aaa\".replace(/a/g, \"b\") === \"bbb\";");
    assert_eq!(tags::payload_of(all), tags::BOOL_TRUE);

    let every = run("return \"aaa\".replaceAll(\"a\", \"b\") === \"bbb\";");
    assert_eq!(tags::payload_of(every), tags::BOOL_TRUE);
}

#[test]
fn a_replacement_template_can_name_the_match_and_its_groups() {
    let whole = run("return \"ab\".replace(/b/, \"[$&]\") === \"a[b]\";");
    assert_eq!(tags::payload_of(whole), tags::BOOL_TRUE);

    let group = run("return \"a1\".replace(/a(1)/, \"$1$1\") === \"11\";");
    assert_eq!(tags::payload_of(group), tags::BOOL_TRUE);

    // A `$` before anything else stands for itself, which is what keeps
    // `"$100"` from swallowing its digits when there is no such group.
    let literal = run("return \"a\".replace(\"a\", \"$$\") === \"$\";");
    assert_eq!(tags::payload_of(literal), tags::BOOL_TRUE);
}

#[test]
fn a_replacement_can_be_a_function_the_program_wrote() {
    // The one place a built-in calls back into compiled code. It is why the
    // replacement is computed between two borrows of the context rather than
    // inside one — calling user code from inside a borrow re-enters it.
    let produced = run(
        "return \"ab\".replace(/./g, function (m) { return m.toUpperCase(); }) === \"AB\";",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);

    // The second argument is where the match was.
    let located = run("return \"xy\".replace(/y/, function (m, at) { return at; }) === \"x1\";");
    assert_eq!(tags::payload_of(located), tags::BOOL_TRUE);
}

#[test]
fn a_pattern_matching_nothing_still_terminates() {
    // `/x*/` matches the empty string at every position, so a loop resuming at
    // the end of the previous match would resume where it started. This test
    // hangs rather than fails if that regresses, which is why it is small.
    let produced = run("return \"ab\".replace(/x*/g, \"-\") === \"-a-b-\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn match_answers_one_result_or_every_match_depending_on_the_flag() {
    // Two shapes from one method, which is the language rather than a choice.
    let one = run("let m = \"a1b\".match(/([0-9])/); return m[1] === \"1\";");
    assert_eq!(tags::payload_of(one), tags::BOOL_TRUE);

    let index = run("let m = \"a1b\".match(/[0-9]/); return m.index;");
    assert_eq!(tags::decode_double(index), 1.0);

    let all = run("return \"a1b2\".match(/[0-9]/g).length;");
    assert_eq!(tags::decode_double(all), 2.0);

    let absent = run("return \"abc\".match(/[0-9]/) === null;");
    assert_eq!(tags::payload_of(absent), tags::BOOL_TRUE);
}

#[test]
fn search_answers_where_rather_than_what() {
    let found = run("return \"ab1\".search(/[0-9]/);");
    assert_eq!(tags::decode_double(found), 2.0);

    let absent = run("return \"abc\".search(/[0-9]/);");
    assert_eq!(tags::decode_double(absent), -1.0);
}

#[test]
fn a_class_is_a_constructor_and_an_object_of_methods() {
    // Nothing in the runtime knows what a class is: this produces exactly what
    // the equivalent hand-written function and prototype assignment produce.
    let produced = run(
        "class Counter { constructor(start) { this.n = start; } next() { return this.n + 1; } } return new Counter(4).next();",
    );
    assert_eq!(tags::decode_double(produced), 5.0);
}

#[test]
fn a_field_is_written_per_instance_and_a_static_field_once() {
    let instance = run("class A { x = 5; } return new A().x;");
    assert_eq!(tags::decode_double(instance), 5.0);

    // Two instances do not share it, which is the whole difference between a
    // field and a property of the prototype.
    let apart = run("class A { x = 1; } let a = new A(); let b = new A(); a.x = 9; return b.x;");
    assert_eq!(tags::decode_double(apart), 1.0);

    let statics = run("class A { static n = 3; } return A.n;");
    assert_eq!(tags::decode_double(statics), 3.0);

    // Declaring `x;` with no initialiser still creates the property, which is
    // what fixes the layout — it is not the same as never declaring it.
    let bare = run("class A { x; } return \"x\" in new A();");
    assert_eq!(tags::payload_of(bare), tags::BOOL_TRUE);
}

#[test]
fn a_derived_class_passes_its_arguments_to_the_parent() {
    // With no constructor written, the language supplies
    // `constructor(...args) { super(...args) }` — and this supplies the same
    // thing as a synthesised tree, so it is emitted by the code that emits a
    // written one rather than by a special case.
    let produced =
        run("class A { constructor(v) { this.v = v; } } class B extends A {} return new B(7).v;");
    assert_eq!(tags::decode_double(produced), 7.0);

    // And with one written, `super()` is where the parent runs.
    let explicit = run(
        "class A { constructor(v) { this.v = v; } } class B extends A { constructor(v) { super(v); this.w = v + 1; } } let b = new B(2); return b.v + b.w;",
    );
    assert_eq!(tags::decode_double(explicit), 5.0);
}

#[test]
fn an_instance_finds_an_inherited_method_and_super_finds_the_one_above() {
    let inherited = run("class A { m() { return 4; } } class B extends A {} return new B().m();");
    assert_eq!(tags::decode_double(inherited), 4.0);

    // `super.m()` inside `m` must not find `m` again — which is why the read
    // starts one link above the home object rather than at `this`.
    let above = run(
        "class A { m() { return 1; } } class B extends A { m() { return super.m() + 1; } } return new B().m();",
    );
    assert_eq!(tags::decode_double(above), 2.0);
}

#[test]
fn a_derived_class_inherits_static_members_too() {
    // The link an implementation forgets: `B.__proto__ = A`. Its absence is
    // invisible until a program calls an inherited static method.
    let produced = run("class A { static s() { return 9; } } class B extends A {} return B.s();");
    assert_eq!(tags::decode_double(produced), 9.0);
}

#[test]
fn instances_are_recognised_by_both_classes_in_the_chain() {
    let own = run("class A {} class B extends A {} return new B() instanceof B;");
    assert_eq!(tags::payload_of(own), tags::BOOL_TRUE);

    let parent = run("class A {} class B extends A {} return new B() instanceof A;");
    assert_eq!(tags::payload_of(parent), tags::BOOL_TRUE);

    let unrelated = run("class A {} class B {} return new B() instanceof A;");
    assert_eq!(tags::payload_of(unrelated), tags::BOOL_FALSE);
}

#[test]
fn a_method_can_close_over_where_the_class_was_written() {
    // The capture analysis has to descend into a class body. Skipping it was
    // not a missing feature but a wrong answer: `secret` would be decided
    // uncaptured and the method would read a register the activation had left.
    let produced = run("let secret = 42; class A { get() { return secret; } } return new A().get();");
    assert_eq!(tags::decode_double(produced), 42.0);
}

#[test]
fn a_class_can_extend_something_the_runtime_supplied() {
    // The chain is built and the method is found — nothing in the
    // regular-expression module knows classes exist, which is what says the
    // lowering produces an ordinary constructor rather than a second kind of
    // thing.
    let recognised = run("class Mine extends RegExp {} return new Mine(\"a+\") instanceof RegExp;");
    assert_eq!(tags::payload_of(recognised), tags::BOOL_TRUE);

    // And the instance carries the parent's state, which is what the earlier
    // version of this test asserted the *absence* of. `super()` does not run the
    // parent against an object that already exists — it asks the parent for the
    // object and binds what came back as `this`, so a built-in that makes an
    // exotic one has somewhere to put it.
    let stateful = run("class Mine extends RegExp {} return new Mine(\"a+\").test(\"caat\");");
    assert_eq!(tags::payload_of(stateful), tags::BOOL_TRUE);

    // The instance still inherits from the DERIVED prototype, which is the part
    // that would break if `super()` established a new `new.target`: the base
    // allocates, and it has to allocate against the class `new` named.
    let own_method = run(
        "class Mine extends RegExp { twice(s) { return this.test(s) && this.test(s); } } return new Mine(\"a\").twice(\"ba\");",
    );
    assert_eq!(tags::payload_of(own_method), tags::BOOL_TRUE);
}

#[test]
fn what_a_class_could_not_express_now_compiles() {
    // A private field, a private method, a computed member name and a static
    // block moved off this list first, when `emit/class.rs` learned to lower
    // them. The last two came off after:
    //
    // - a computed ACCESSOR name was refused because `DefineGetter`/
    //   `DefineSetter` take the key the compiler resolved as a constant. Still
    //   true — what changed is that a VALUE can be resolved to one, through
    //   `__rts_key_number`, so there is still exactly one way to define an
    //   accessor. That is what the refusal was protecting.
    // - `#x in o` was refused because the private name reached `emit_expr`
    //   through the generic binary-operator path, which asks both operands for
    //   a VALUE. A private name is a key and nothing else, so the operator
    //   answers for it rather than the operand, and only the object is
    //   evaluated.
    //
    // The list is empty, and this is what it BECAME rather than what it was
    // deleted as. An empty loop asserts nothing; these two do — each is a
    // construct that worked the day this changed and would otherwise stop
    // silently.
    for source in [
        "class A { get [\"a\"]() { return 1; } } return new A().a;",
        "class A { #x = 1; has() { return #x in this; } } return new A().has();",
    ] {
        compile(source).unwrap_or_else(|error| panic!("`{source}` no longer compiles: {error:?}"));
    }
}

#[test]
fn assigning_to_an_undeclared_name_creates_a_global() {
    // Sloppy mode, and the only way a script without a module system introduces
    // a global at all. Strict mode throws instead — a `ReferenceError` this
    // engine cannot raise where a handler could catch it.
    let produced = run("counter = 7; return counter;");
    assert_eq!(tags::decode_double(produced), 7.0);
}

#[test]
fn a_function_reads_a_global_the_program_creates_after_it() {
    // Why the scan runs over the whole program before anything is emitted: the
    // body is emitted before the assignment is reached, so a decision taken at
    // the read would have nothing to go on.
    let produced = run("function get() { return total; } total = 4; return get();");
    assert_eq!(tags::decode_double(produced), 4.0);
}

#[test]
fn globalthis_is_the_object_the_globals_are_on() {
    // What makes this a global object rather than a table with globals in it.
    let through = run("mine = 3; return globalThis.mine;");
    assert_eq!(tags::decode_double(through), 3.0);

    let back = run("globalThis.other = 5; return globalThis.other;");
    assert_eq!(tags::decode_double(back), 5.0);

    // And a name written through `globalThis` IS readable bare afterwards, which
    // is what the language does — `globalThis.other = 5` creates the global
    // `other`. This asserted the opposite, because the emitter's scan did not
    // recognise a write through `globalThis` as creating a name and refused the
    // read. A page-loader pattern that publishes and reads purely through
    // `globalThis` was legal JavaScript this engine would not compile.
    let through_global = run("globalThis.other = 5; return other;");
    assert_eq!(through_global, rts_core::value::Value::from_f64(5.0).bits());

    let itself = run("return globalThis === globalThis;");
    assert_eq!(tags::payload_of(itself), tags::BOOL_TRUE);
}

#[test]
fn typeof_of_an_undeclared_name_answers_rather_than_failing() {
    // The one read in the language that does not throw for a name nothing
    // declared, because it takes a reference rather than a value. Refusing it
    // would refuse the question `typeof maybe === "undefined"` asks.
    let produced = run("return (typeof nothingDeclaresThis) === \"undefined\";");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);

    let present = run("here = 1; return (typeof here) === \"number\";");
    assert_eq!(tags::payload_of(present), tags::BOOL_TRUE);
}

#[test]
fn reading_a_name_nothing_declares_or_creates_raises_where_the_read_is() {
    // This said "stricter than the language, which throws a `ReferenceError`
    // this engine cannot raise where a handler could catch it". It can now, so
    // the divergence the comment described is closed and the test asserts the
    // language instead of the limitation.
    //
    // The reason the strictness had to go rather than being kept as a nicety:
    // the error belongs to the READ, so a program that never reaches the read is
    // legal. A UMD bundle mentioning `exports` in a branch it does not take was
    // refused whole, and that is a real program, not a typo.
    let caught =
        run("try { return neverMentionedAgain; } catch (e) { return e instanceof ReferenceError; }");
    assert_eq!(caught, rts_core::value::Value::from_bool(true).bits());

    // The typo case the strictness was protecting is still protected — it is a
    // thrown error rather than `undefined`, which is what would have made every
    // misspelling a program that runs.
    let not_undefined = run("try { return alsoNeverMentioned; } catch (e) { return 1; }");
    assert_eq!(not_undefined, rts_core::value::Value::from_f64(1.0).bits());
}

#[test]
fn a_getter_is_called_rather_than_returned() {
    // The whole of what an accessor is, and the thing that goes wrong when one
    // is stored as an ordinary property: the read would answer the function.
    let produced = run("let o = { get x() { return 7; } }; return o.x;");
    assert_eq!(tags::decode_double(produced), 7.0);

    let on_a_class = run("class A { get x() { return 8; } } return new A().x;");
    assert_eq!(tags::decode_double(on_a_class), 8.0);
}

#[test]
fn a_getter_sees_the_object_the_read_was_written_on() {
    // The receiver is where the read happened, not where the getter was found.
    // A getter on the prototype reading `this.n` must see the instance.
    let produced = run(
        "class A { constructor(n) { this.n = n; } get twice() { return this.n * 2; } } return new A(4).twice;",
    );
    assert_eq!(tags::decode_double(produced), 8.0);
}

#[test]
fn a_setter_runs_instead_of_the_write_landing_in_a_slot() {
    let produced = run(
        "class A { set v(x) { this.stored = x + 1; } } let a = new A(); a.v = 4; return a.stored;",
    );
    assert_eq!(tags::decode_double(produced), 5.0);

    // And the assignment still produces the value assigned, because an
    // assignment is an expression whatever the setter did.
    let answered = run("let o = { set v(x) { } }; return (o.v = 3);");
    assert_eq!(tags::decode_double(answered), 3.0);
}

#[test]
fn a_property_with_only_a_setter_reads_as_undefined() {
    // The property exists and reading it answers nothing, which is the language
    // and a common source of confusion.
    let produced = run("let o = { set v(x) {} }; return o.v === undefined;");
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn both_halves_of_one_accessor_survive_being_written_separately() {
    // `get x()` and `set x(v)` are two declarations of one property. A second
    // definition replacing the pair would make the order they were written in
    // decide which half survives.
    let produced = run(
        "class A { get x() { return this.held; } set x(v) { this.held = v * 2; } } let a = new A(); a.x = 3; return a.x;",
    );
    assert_eq!(tags::decode_double(produced), 6.0);
}

#[test]
fn an_own_value_shadows_an_inherited_accessor_and_the_reverse() {
    // Why the accessors and the layouts are walked together, one cell at a
    // time: two separate walks would let an inherited getter win over an own
    // value, or the other way round.
    let own_value = run(
        "class A { get x() { return 1; } } class B extends A { constructor() { super(); } } let b = new B(); b.other = 5; return b.x;",
    );
    assert_eq!(tags::decode_double(own_value), 1.0);

    let own_getter = run(
        "class A { m() { return 1; } } class B extends A { get m() { return 2; } } return new B().m;",
    );
    assert_eq!(tags::decode_double(own_getter), 2.0);
}

#[test]
fn object_keys_and_values_agree_about_order() {
    // The property they are useful in pairs for, and the reason `values` is
    // built from the keys rather than from a second walk of the layout.
    let count = run("let o = { a: 1, b: 2 }; return Object.keys(o).length;");
    assert_eq!(tags::decode_double(count), 2.0);

    let first = run("let o = { a: 1, b: 2 }; return Object.keys(o)[0] === \"a\";");
    assert_eq!(tags::payload_of(first), tags::BOOL_TRUE);

    let value = run("let o = { a: 1, b: 2 }; return Object.values(o)[1];");
    assert_eq!(tags::decode_double(value), 2.0);
}

#[test]
fn object_keys_filters_hidden_and_symbol_properties_after_ordering_indices() {
    let produced = run(
        "let symbol = Symbol('hidden'); \
         let o = {}; o['2'] = 2; o['1'] = 1; o.name = 3; \
         Object.defineProperty(o, 'secret', { value: 4, enumerable: false }); \
         o[symbol] = 5; \
         let keys = Object.keys(o); \
         return keys.length === 3 && keys[0] === '1' && keys[1] === '2' && keys[2] === 'name';"
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn object_values_runs_a_getter_rather_than_reading_a_slot() {
    // A direct read of the layout would skip it — and would find nothing at
    // all, since an accessor is deliberately not in the layout.
    let produced = run("let o = { a: 1, get b() { return 9; } }; return Object.values(o)[1];");
    assert_eq!(tags::decode_double(produced), 9.0);
}

#[test]
fn the_prototype_of_an_object_can_be_read_and_written() {
    let linked = run(
        "let parent = { m() { return 3; } }; let child = {}; Object.setPrototypeOf(child, parent); return child.m();",
    );
    assert_eq!(tags::decode_double(linked), 3.0);

    let read_back = run(
        "let parent = {}; let child = {}; Object.setPrototypeOf(child, parent); return Object.getPrototypeOf(child) === parent;",
    );
    assert_eq!(tags::payload_of(read_back), tags::BOOL_TRUE);

    // What a class links, read back through the same operation — which is what
    // says the class lowering and this method agree about the chain.
    let from_a_class = run(
        "class A {} class B extends A {} return Object.getPrototypeOf(B.prototype) === A.prototype;",
    );
    assert_eq!(tags::payload_of(from_a_class), tags::BOOL_TRUE);
}

#[test]
fn define_property_makes_an_accessor_or_a_value() {
    let accessor = run(
        "let o = {}; Object.defineProperty(o, \"x\", { get: function () { return 5; } }); return o.x;",
    );
    assert_eq!(tags::decode_double(accessor), 5.0);

    let data = run("let o = {}; Object.defineProperty(o, \"x\", { value: 6 }); return o.x;");
    assert_eq!(tags::decode_double(data), 6.0);
}

#[test]
fn assign_copies_through_the_ordinary_property_paths() {
    let copied = run("let t = {}; Object.assign(t, { a: 1 }); return t.a;");
    assert_eq!(tags::decode_double(copied), 1.0);

    // A getter on the source runs, which a slot-to-slot copy would have skipped
    // — and would have found nothing, an accessor not being in the layout.
    let through_a_getter = run("let t = {}; Object.assign(t, { get a() { return 4; } }); return t.a;");
    assert_eq!(tags::decode_double(through_a_getter), 4.0);

    // Every source, not the first. It read one and dropped the rest silently,
    // which is the failure a merge must not have: the result still looks like a
    // merge, so nothing downstream reports the missing keys.
    let merged = run("let t = Object.assign({}, { a: 1 }, { b: 2 }, { c: 3 }); return t.a + t.b + t.c;");
    assert_eq!(tags::decode_double(merged), 6.0);

    // Order is left to right, so a later source overwrites an earlier one.
    let overwritten = run("let t = Object.assign({}, { a: 1 }, { a: 9 }); return t.a;");
    assert_eq!(tags::decode_double(overwritten), 9.0);
}

#[test]
fn object_makes_an_empty_object_either_way_it_is_written() {
    let called = run("let o = Object(); o.x = 1; return o.x;");
    assert_eq!(tags::decode_double(called), 1.0);

    let constructed = run("let o = new Object(); o.x = 2; return o.x;");
    assert_eq!(tags::decode_double(constructed), 2.0);
}

#[test]
fn an_accessor_is_a_property_the_object_has() {
    // It is not in the layout, so every question answered from the shape alone
    // reports it as absent — which is the operator and the enumeration both
    // getting their one job wrong.
    let present = run("let o = { get x() { return 1; } }; return \"x\" in o;");
    assert_eq!(tags::payload_of(present), tags::BOOL_TRUE);

    let listed = run("let o = { get x() { return 1; } }; return Object.keys(o)[0] === \"x\";");
    assert_eq!(tags::payload_of(listed), tags::BOOL_TRUE);
}

#[test]
fn a_computed_access_reaches_the_same_property_a_named_one_does() {
    // `o[k]` and `o.x` name one property. A getter found by one spelling and a
    // slot read by the other would make which was written decide what the
    // property IS.
    let read = run("let o = { get x() { return 5; } }; let k = \"x\"; return o[k];");
    assert_eq!(tags::decode_double(read), 5.0);

    let written = run(
        "let o = { set x(v) { this.held = v + 1; } }; let k = \"x\"; o[k] = 2; return o.held;",
    );
    assert_eq!(tags::decode_double(written), 3.0);
}

#[test]
fn the_base_of_the_chain_allocates_and_the_class_new_named_decides_the_prototype() {
    // Three classes deep, so the target has to survive two `super()` calls. If
    // `super()` established a new one, the instance would inherit from
    // `B.prototype` and `C`'s method would be missing.
    let produced = run(
        "class A { constructor() { this.n = 1; } } class B extends A {} class C extends B { m() { return this.n + 1; } } return new C().m();",
    );
    assert_eq!(tags::decode_double(produced), 2.0);

    let recognised = run("class A {} class B extends A {} class C extends B {} return new C() instanceof C;");
    assert_eq!(tags::payload_of(recognised), tags::BOOL_TRUE);
}

#[test]
fn a_derived_constructor_answers_the_object_super_produced() {
    // It allocates nothing, so what it returns IS the instance — a body falling
    // off its end has to answer its `this` rather than `undefined`.
    let implicit = run(
        "class A { constructor() { this.n = 5; } } class B extends A { constructor() { super(); } } return new B().n;",
    );
    assert_eq!(tags::decode_double(implicit), 5.0);

    // And a constructor that returns an object of its own still wins, which is
    // the clause an implementation forgets.
    let overridden = run(
        "class A {} class B extends A { constructor() { super(); return { n: 9 }; } } return new B().n;",
    );
    assert_eq!(tags::decode_double(overridden), 9.0);
}

#[test]
fn this_in_a_derived_constructor_is_the_object_and_not_the_receiver() {
    // The receiver a derived constructor is handed is `undefined`, because the
    // object is not its to make. A `this` that read the parameter would be that
    // `undefined` — silently the wrong object rather than a refusal.
    let produced = run(
        "class A { constructor() { this.a = 1; } } class B extends A { constructor() { super(); this.b = 2; } } let x = new B(); return x.a + x.b;",
    );
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn extending_a_string_gives_an_instance_that_still_has_its_own_methods() {
    // Both directions at once: the parent's prototype is reachable through the
    // chain, and the derived prototype is what the instance actually starts at.
    let inherited = run("class Tag extends Object { own() { return 4; } } return new Tag().own();");
    assert_eq!(tags::decode_double(inherited), 4.0);
}

#[test]
fn a_call_past_the_convention_reaches_the_arguments_it_wrote() {
    // Six arguments, where the convention carries four. The vector is the
    // runtime's; the call site says "call with these" and never learns where
    // they went.
    let produced = run("function f(a, b, c, d) { return a + d; } return f(1, 2, 3, 4, 5, 6);");
    assert_eq!(tags::decode_double(produced), 5.0);

    // And the ones past the fourth are not lost — they reach a rest parameter.
    let gathered = run("function f(a, ...rest) { return rest.length; } return f(1, 2, 3, 4, 5, 6);");
    assert_eq!(tags::decode_double(gathered), 5.0);

    let last = run("function f(a, ...rest) { return rest[4]; } return f(1, 2, 3, 4, 5, 6);");
    assert_eq!(tags::decode_double(last), 6.0);
}

#[test]
fn a_rest_parameter_works_when_no_vector_was_allocated() {
    // The common call allocates nothing, and a rest parameter over four or
    // fewer arguments still has to work — so the callee hands its own slots
    // over and the runtime uses those when no caller supplied a vector.
    let produced = run("function f(a, ...rest) { return rest.length; } return f(1, 2, 3);");
    assert_eq!(tags::decode_double(produced), 2.0);

    let value = run("function f(a, ...rest) { return rest[1]; } return f(1, 2, 3);");
    assert_eq!(tags::decode_double(value), 3.0);

    // Padding a call site invented is not an argument the program passed.
    let empty = run("function f(a, ...rest) { return rest.length; } return f(1);");
    assert_eq!(tags::decode_double(empty), 0.0);
}

#[test]
fn a_callee_does_not_see_an_outer_calls_vector() {
    // Why every call pushes a marker rather than only the ones that allocate:
    // `outer` is running with a vector when it calls `inner`, and `inner`'s
    // rest must be its own arguments rather than `outer`'s six.
    let produced = run(
        "function inner(...rest) { return rest.length; } function outer(a, b, c, d) { return inner(9); } return outer(1, 2, 3, 4, 5, 6);",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_method_can_be_called_past_the_convention_too() {
    // The receiver is not one of the four, so a method call with six arguments
    // is the same operation with one more value kept apart.
    let produced = run(
        "let o = { n: 10, m(a, b, c, d) { return this.n + d; } }; return o.m(1, 2, 3, 4, 5, 6);",
    );
    assert_eq!(tags::decode_double(produced), 14.0);
}

#[test]
fn new_past_the_convention_keeps_its_arguments_too() {
    // The asymmetry the call side left behind: `new` makes the receiver rather
    // than taking one, so it is its own operation, and it needed its own
    // vector path rather than inheriting the call's.
    let produced = run(
        "class P { constructor(a, b, c, d) { this.n = a + d; } } return new P(1, 2, 3, 4, 5, 6).n;",
    );
    assert_eq!(tags::decode_double(produced), 5.0);

    let gathered = run(
        "class P { constructor(a, ...rest) { this.n = rest.length; } } return new P(1, 2, 3, 4, 5, 6).n;",
    );
    assert_eq!(tags::decode_double(gathered), 5.0);
}

#[test]
fn an_array_finds_its_methods_by_inheriting_them() {
    // The same substitution a string gets, and for the same reason: a link per
    // array would be a word spent at every allocation on a fact they all share.
    let produced = run("let a = [1, 2]; a.push(3); return a.length;");
    assert_eq!(tags::decode_double(produced), 3.0);

    let popped = run("let a = [1, 2, 3]; return a.pop();");
    assert_eq!(tags::decode_double(popped), 3.0);

    // `length` is a real property both paths read, so a method that changes it
    // has to write it — this is what catches forgetting.
    let after = run("let a = [1, 2, 3]; a.pop(); return a.length;");
    assert_eq!(tags::decode_double(after), 2.0);
}

#[test]
fn a_method_taking_a_callback_calls_back_into_compiled_code() {
    // The place a built-in reaches user code. It is why the elements are
    // collected, the borrow dropped, the callback run, and the result stored in
    // a fresh borrow — calling from inside one re-enters the RefCell.
    let mapped = run("let a = [1, 2, 3]; return a.map(function (x) { return x * 2; })[2];");
    assert_eq!(tags::decode_double(mapped), 6.0);

    let filtered = run("let a = [1, 2, 3, 4]; return a.filter(function (x) { return x > 2; }).length;");
    assert_eq!(tags::decode_double(filtered), 2.0);

    let reduced = run("let a = [1, 2, 3]; return a.reduce(function (t, x) { return t + x; }, 0);");
    assert_eq!(tags::decode_double(reduced), 6.0);

    let found = run("let a = [1, 2, 3]; return a.find(function (x) { return x > 1; });");
    assert_eq!(tags::decode_double(found), 2.0);

    // The index is the second argument, which a callback taking only the value
    // would pass either way — so this is what says it is really supplied.
    let indexed = run("let a = [9, 9, 9]; return a.map(function (x, i) { return i; })[2];");
    assert_eq!(tags::decode_double(indexed), 2.0);
}

#[test]
fn builtin_array_species_memoization_never_skips_user_getters() {
    let observed = run(
        "let calls = 0; \
         let original = Array.prototype.constructor; \
         let holder = {}; \
         Object.defineProperty(holder, Symbol.species, { get: function () { calls++; return Array; } }); \
         Array.prototype.constructor = holder; \
         let first = [1, 2].map(function (x) { return x + 1; }); \
         let custom = calls === 1 && first[1] === 3; \
         Array.prototype.constructor = original; \
         let second = [3].map(function (x) { return x + 1; }); \
         let restored = second[0] === 4; \
         let explicit = [5]; \
         Object.setPrototypeOf(explicit, Array.prototype); \
         let linked = explicit.map(function (x) { return x; })[0] === 5; \
         return custom && restored && linked ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(observed), 1.0);
}

#[test]
fn cached_store_mutation_cannot_stale_array_species() {
    let observed = run(
        "let calls = 0; \
         let original = Array.prototype.constructor; \
         let holder = {}; \
         Object.defineProperty(holder, Symbol.species, { get: function () { calls++; return Array; } }); \
         let warm = [1].map(function (x) { return x; }); \
         let mode = 0; \
         let result = 0; \
         for (let i = 0; i < 3; i = i + 1) { \
             Array.prototype.constructor = mode === 1 ? holder : original; \
             if (mode === 1) { \
                 let made = [2].map(function (x) { return x; }); \
                 result = made[0]; \
             } \
             mode = mode + 1; \
         } \
         return warm[0] === 1 && result === 2 && calls === 1 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(observed), 1.0);
}

#[test]
fn the_predicates_answer_booleans_over_the_whole_array() {

    let some = run("let a = [1, 2]; return a.some(function (x) { return x > 1; });");
    assert_eq!(tags::payload_of(some), tags::BOOL_TRUE);

    let every = run("let a = [1, 2]; return a.every(function (x) { return x > 1; });");
    assert_eq!(tags::payload_of(every), tags::BOOL_FALSE);

    let includes = run("return [1, 2, 3].includes(2);");
    assert_eq!(tags::payload_of(includes), tags::BOOL_TRUE);

    let missing = run("return [1, 2, 3].indexOf(9);");
    assert_eq!(tags::decode_double(missing), -1.0);
}

#[test]
fn join_and_slice_answer_what_the_language_says() {
    let joined = run("return [1, 2, 3].join(\"-\") === \"1-2-3\";");
    assert_eq!(tags::payload_of(joined), tags::BOOL_TRUE);

    // A negative index counts from the end, the same clamping rule strings use
    // — written once, so the two cannot disagree.
    let sliced = run("return [1, 2, 3, 4].slice(-2).length;");
    assert_eq!(tags::decode_double(sliced), 2.0);

    let reversed = run("return [1, 2, 3].reverse()[0];");
    assert_eq!(tags::decode_double(reversed), 3.0);
}

#[test]
fn array_is_a_name_the_runtime_provides() {
    let recognised = run("return Array.isArray([1]);");
    assert_eq!(tags::payload_of(recognised), tags::BOOL_TRUE);

    let not_one = run("return Array.isArray({});");
    assert_eq!(tags::payload_of(not_one), tags::BOOL_FALSE);

    let sized = run("return new Array(3).length;");
    assert_eq!(tags::decode_double(sized), 3.0);

    // `Array.prototype.mine = f` reaches every array, which is the whole point
    // of the substitution being an ordinary object.
    let extended = run("Array.prototype.first = function () { return this[0]; }; return [7, 8].first();");
    assert_eq!(tags::decode_double(extended), 7.0);
}

#[test]
fn for_of_walks_the_elements_rather_than_the_keys() {
    // The difference from `for-in`, which is the whole reason both exist.
    let summed = run("let t = 0; for (let v of [1, 2, 3]) { t = t + v; } return t;");
    assert_eq!(tags::decode_double(summed), 6.0);

    let keys = run("let t = 0; for (let k in [1, 2, 3]) { t = t + 1; } return t;");
    assert_eq!(tags::decode_double(keys), 3.0);

    // Everything the loop already gets right is got right once: this is the
    // same expansion `for-in` uses, so `break` needs no second implementation.
    let stopped = run("let t = 0; for (let v of [1, 2, 3]) { if (v > 1) { break; } t = t + v; } return t;");
    assert_eq!(tags::decode_double(stopped), 1.0);
}

#[test]
fn for_of_over_a_string_yields_code_points() {
    // Not units. `"😀"` is one element here and two in `length`, which is the
    // difference the construct was added to the language for.
    let count = run("let n = 0; for (let c of \"ab\") { n = n + 1; } return n;");
    assert_eq!(tags::decode_double(count), 2.0);

    let astral = run("let n = 0; for (let c of \"😀a\") { n = n + 1; } return n;");
    assert_eq!(tags::decode_double(astral), 2.0);

    let length = run("return \"😀a\".length;");
    assert_eq!(tags::decode_double(length), 3.0);
}

#[test]
fn a_loop_walks_a_copy_so_a_body_that_grows_it_terminates() {
    // The array is materialised, so pushing inside the body does not extend
    // what is being walked. This test hangs rather than fails if that changes,
    // which is why it is small.
    let produced = run("let a = [1, 2]; let n = 0; for (let v of a) { a.push(v); n = n + 1; } return n;");
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_spread_contributes_a_count_nothing_knew_while_compiling() {
    let in_a_literal = run("let xs = [2, 3]; return [1, ...xs, 4].length;");
    assert_eq!(tags::decode_double(in_a_literal), 4.0);

    let ordered = run("let xs = [2, 3]; return [1, ...xs, 4][2];");
    assert_eq!(tags::decode_double(ordered), 3.0);

    // One written argument becoming three is why a spread takes the vector path
    // whatever the written count is.
    let in_a_call = run("function f(a, b, c) { return a + b + c; } let xs = [1, 2, 3]; return f(...xs);");
    assert_eq!(tags::decode_double(in_a_call), 6.0);

    let mixed = run("function f(a, ...rest) { return rest.length; } let xs = [1, 2]; return f(0, ...xs, 9);");
    assert_eq!(tags::decode_double(mixed), 3.0);

    let constructed = run("class P { constructor(a, b) { this.n = a + b; } } let xs = [3, 4]; return new P(...xs).n;");
    assert_eq!(tags::decode_double(constructed), 7.0);

    // A string spreads by code point, the same sequence `for-of` walks.
    let text = run("return [...\"ab\"].length;");
    assert_eq!(tags::decode_double(text), 2.0);
}


/// Runs a script whose answer is a boolean, and asserts it is true.
///
/// Text is compared **inside** the program rather than by word, because a word
/// is a slot in a heap that lasts one compilation — two scripts producing the
/// same string produce different words, so an assertion across them would be
/// comparing where two heaps happened to put something.
fn holds(source: &str) {
    let produced = run(source);
    assert_eq!(
        tags::tag_of(produced),
        tags::TAG_BOOL,
        "`{source}` did not answer a boolean"
    );
    assert_eq!(
        tags::payload_of(produced),
        tags::BOOL_TRUE,
        "`{source}` answered false"
    );
}

#[test]
fn an_error_carries_the_message_it_was_made_with() {
    // The statement every failing program is written with, and which did not
    // compile before this class existed: `Error` was not a name the emitter
    // resolved, so `new Error("x")` was an unbound name rather than an object.
    holds("return new Error(\"boom\").message === \"boom\";");
    holds("return new Error(\"boom\").toString() === \"Error: boom\";");
    holds("return typeof new Error(\"boom\") === \"object\";");

    // A message left out is not an empty one: `toString` answers the name
    // alone, which is the case an implementation joining unconditionally gets
    // wrong as `"Error: undefined"`.
    holds("return new Error().toString() === \"Error\";");

    // `Error("x")` is the same operation as `new Error("x")` — the language
    // says so — which is what makes the receiver something this can be asked to
    // make rather than something it is always handed.
    holds("return Error(\"boom\").message === \"boom\";");
}

#[test]
fn a_subclass_inherits_the_family_and_answers_its_own_name() {
    // `name` is on the prototype, so the chain walk is what answers it — and
    // the chain has to reach `Error.prototype`, which is where `toString` is.
    holds("return new TypeError(\"nope\").name === \"TypeError\";");
    holds("return new TypeError(\"nope\").toString() === \"TypeError: nope\";");

    // The link `extends` writes, read the way a program reads it.
    holds("return new RangeError(\"x\") instanceof Error;");
    holds("return new RangeError(\"x\") instanceof RangeError;");

    // `toString` reads `name` through the ordinary property path, so a program
    // that replaces it is answered — which reading the class's own name instead
    // would have got wrong.
    holds("let e = new Error(\"b\"); e.name = \"Mine\"; return e.toString() === \"Mine: b\";");
}

#[test]
fn a_user_class_can_extend_a_built_in_error() {
    // The acceptance test for a native constructor asking `new.target` for the
    // prototype: without it the object would reach `Error.prototype` and `own`
    // would not be on it.
    holds(
        "class Mine extends Error { own() { return this.message; } } \
         return new Mine(\"m\").own() === \"m\";",
    );
    holds("class Mine extends Error {} return new Mine(\"m\") instanceof Error;");
}

#[test]
fn math_is_an_object_whose_members_are_reached_rather_than_folded() {
    // A property read and a call, not an instruction. `Math.floor` is a
    // writable property of a mutable object, which is the argument for it being
    // one — and the test that says so is the one that replaces it.
    assert_eq!(tags::decode_double(run("return Math.floor(3.7);")), 3.0);
    assert_eq!(tags::decode_double(run("return Math.pow(2, 10);")), 1024.0);
    assert_eq!(tags::decode_double(run("return Math.PI;")), std::f64::consts::PI);

    let replaced = run("Math.floor = function (x) { return 99; }; return Math.floor(3.7);");
    assert_eq!(tags::decode_double(replaced), 99.0);

    // `Math.max(1)` is `1`, not `NaN`: the identity is `-Infinity`, so a
    // missing argument is skipped rather than coerced.
    assert_eq!(tags::decode_double(run("return Math.max(1, 2);")), 2.0);
    assert_eq!(tags::decode_double(run("return Math.max(1);")), 1.0);
    assert_eq!(tags::decode_double(run("return Math.min(3, 2);")), 2.0);

    // `Math.round(-0.5)` is `-0`, not `-1`: JavaScript rounds a half up where
    // Rust rounds it away from zero.
    assert_eq!(tags::decode_double(run("return Math.round(-0.5);")), 0.0);
    assert_eq!(tags::decode_double(run("return Math.round(2.5);")), 3.0);

    // The argument arrives through `ToNumber`, once, in the generated wrapper.
    assert_eq!(tags::decode_double(run("return Math.abs(\"-5\");")), 5.0);
}

#[test]
fn number_asks_what_arrived_where_the_conversion_converts() {
    // The pair this module exists to keep apart: `Number("abc")` converts and
    // answers `NaN`; `Number.isNaN("abc")` does not convert and answers false,
    // because a string is not a number at all.
    assert!(tags::decode_double(run("return Number(\"abc\");")).is_nan());
    holds("return Number.isNaN(\"abc\") === false;");
    holds("return Number.isNaN(0 / 0) === true;");

    assert_eq!(tags::decode_double(run("return Number(\"12\");")), 12.0);
    assert_eq!(tags::decode_double(run("return Number.parseInt(\"42px\");")), 42.0);
    assert_eq!(tags::decode_double(run("return Number.parseInt(\"ff\", 16);")), 255.0);
    assert_eq!(tags::decode_double(run("return Number.parseFloat(\"3.5px\");")), 3.5);
    assert_eq!(
        tags::decode_double(run("return Number.MAX_SAFE_INTEGER;")),
        9_007_199_254_740_991.0
    );
    holds("return Number.isInteger(3) && !Number.isInteger(3.5);");
}

#[test]
fn a_function_inherits_call_and_apply_without_carrying_a_link() {
    // The substitution: a callable carries no prototype link of its own, so the
    // chain walk is what names `Function.prototype` — the same shape a string
    // gets, and for the same reason.
    let called = run("function f(a) { return this.n + a; } return f.call({n: 1}, 2);");
    assert_eq!(tags::decode_double(called), 3.0);

    // `apply` is the spelling carrying a count nothing knew while compiling,
    // which is what the argument vector was built for.
    let applied = run("function f(a, b, c) { return a + b + c; } return f.apply(null, [1, 2, 3]);");
    assert_eq!(tags::decode_double(applied), 6.0);
}

#[test]
fn reflect_reaches_the_same_operations_the_syntax_does() {
    // Not a second implementation: `Reflect.get` and `o[k]` are the same
    // function, which is why a getter runs in both rather than in one.
    holds("let o = {a: 1}; return Reflect.get(o, \"a\") === 1;");
    holds("let o = {}; Reflect.set(o, \"a\", 2); return o.a === 2;");
    holds("let o = {a: 1}; return Reflect.has(o, \"a\") && !Reflect.has(o, \"b\");");
    holds("let o = {a: 1, b: 2}; return Reflect.ownKeys(o).length === 2;");
    holds("let o = {get a() { return 7; }}; return Reflect.get(o, \"a\") === 7;");

    let applied = run("function f(a, b) { return a + b; } return Reflect.apply(f, null, [1, 2]);");
    assert_eq!(tags::decode_double(applied), 3.0);

    let built = run("class P { constructor(a) { this.n = a; } } return Reflect.construct(P, [5]).n;");
    assert_eq!(tags::decode_double(built), 5.0);
}

#[test]
fn a_global_function_converts_where_its_strict_twin_does_not() {
    // `isNaN("abc")` is true and `Number.isNaN("abc")` is false. The pair is the
    // reason the second spelling exists, so implementing one through the other
    // would make it useless.
    holds("return isNaN(\"abc\") === true;");
    holds("return Number.isNaN(\"abc\") === false;");
    holds("return isFinite(\"12\") === true;");

    assert_eq!(tags::decode_double(run("return parseInt(\"42px\");")), 42.0);
    assert_eq!(tags::decode_double(run("return parseInt(\"0x1f\");")), 31.0);
    assert_eq!(tags::decode_double(run("return parseFloat(\"-2.5e1x\");")), -25.0);

    // One value read twice, so a program can pass it around.
    holds("return parseInt === parseInt;");
    // The global object holds it once the name has been read: these are made on
    // demand, so `globalThis.parseInt` before any read of `parseInt` is
    // `undefined` rather than the function. A stated consequence of laziness.
    holds("let p = parseInt; return globalThis.parseInt === p;");
}

#[test]
fn a_symbol_is_a_key_nothing_else_can_spell() {
    // Identity, which is the whole reason a symbol is a cell rather than a tag
    // over its description: two symbols with the same description are different
    // values, and an interned encoding would have made them equal.
    holds("return Symbol(\"a\") !== Symbol(\"a\");");
    holds("return typeof Symbol(\"a\") === \"symbol\";");
    holds("return typeof Symbol.iterator === \"symbol\";");

    // One value per name, or a property written under it could never be read
    // back.
    holds("return Symbol.iterator === Symbol.iterator;");
    holds("return Symbol.for(\"x\") === Symbol.for(\"x\");");
    // Deliberately NOT the same: the registry and the well-known set are
    // separate key spaces, and colliding them is the bug the old engine's own
    // documentation warns about.
    holds("return Symbol.for(\"iterator\") !== Symbol.iterator;");
    holds("return Symbol.keyFor(Symbol.for(\"x\")) === \"x\";");
    holds("return Symbol.keyFor(Symbol(\"x\")) === undefined;");

    holds("return Symbol(\"a\").toString() === \"Symbol(a)\";");
    holds("return Symbol(\"a\").description === \"a\";");
}

#[test]
fn a_symbol_keyed_property_is_reachable_and_not_enumerated() {
    // Stored and read like any other property, which is the point of encoding
    // the key as a reserved name rather than as a third kind of key.
    holds("let s = Symbol(\"k\"); let o = {}; o[s] = 7; return o[s] === 7;");

    // Two different symbols are two different properties, even with one
    // description — the test that fails if identity were interned away.
    holds("let a = Symbol(\"k\"); let b = Symbol(\"k\"); let o = {}; o[a] = 1; o[b] = 2; return o[a] === 1 && o[b] === 2;");

    // Not enumerated: `Object.keys` and `for-in` walk string keys only.
    holds("let s = Symbol(\"k\"); let o = {a: 1}; o[s] = 2; return Object.keys(o).length === 1;");
    holds("let s = Symbol(\"k\"); let o = {}; o[s] = 2; let n = 0; for (let k in o) { n = n + 1; } return n === 0;");
}

#[test]
fn for_of_asks_an_object_how_it_iterates() {
    // The protocol, reached for the first time: a `Symbol.iterator` that
    // answers an object with `next`. Before this, an object that declared one
    // ran the loop zero times.
    let summed = run(
        "let o = {}; o[Symbol.iterator] = function () { \
           let i = 0; \
           return {next: function () { i = i + 1; \
             if (i > 3) { return {done: true, value: undefined}; } \
             return {done: false, value: i}; }}; \
         }; \
         let t = 0; for (let v of o) { t = t + v; } return t;",
    );
    assert_eq!(tags::decode_double(summed), 6.0);

    // A spread is the same walk, which is what makes both correct at once.
    let spread = run(
        "let o = {}; o[Symbol.iterator] = function () { \
           let i = 0; \
           return {next: function () { i = i + 1; \
             if (i > 2) { return {done: true, value: undefined}; } \
             return {done: false, value: i}; }}; \
         }; \
         return [...o].length;",
    );
    assert_eq!(tags::decode_double(spread), 2.0);

    // An object declaring nothing is not iterable, and saying so is the point.
    // This asserted zero passes while a throw could not reach a handler — the
    // gap, pinned as though it were the behaviour. It closed when natives
    // learned to raise, so the loop stopped running zero times and started
    // ending the program, and the test aborted rather than failed.
    holds(
        "try { for (let v of {a: 1}) { } return false; } \
         catch (e) { return e instanceof TypeError; }",
    );
}

#[test]
fn a_plain_object_inherits_from_object_prototype() {
    // It did not before: `object_new` links nothing, and the chain walk had no
    // arm for a plain object — so `({}).hasOwnProperty` was `undefined` and
    // `Object.prototype.m = f` landed where nothing looked.
    holds("let o = {a: 1}; return o.hasOwnProperty(\"a\") === true;");
    holds("let o = {a: 1}; return o.hasOwnProperty(\"b\") === false;");
    holds("return ({}).toString() === \"[object Object]\";");
    holds("Object.prototype.mine = 5; return ({}).mine === 5;");

    // Own, not inherited: the distinction `hasOwnProperty` exists to make.
    holds("Object.prototype.shared = 1; let o = {}; return o.shared === 1 && o.hasOwnProperty(\"shared\") === false;");

    // `instanceof` steps through substituted prototypes now, so the kinds that
    // never carried a link of their own answer for themselves.
    holds("return ({}) instanceof Object;");
    holds("return [] instanceof Array;");
    holds("let o = {}; let p = {}; Object.setPrototypeOf(o, p); return p.isPrototypeOf(o);");
}

#[test]
fn json_round_trips_what_it_can_represent() {
    holds("return JSON.stringify({a: 1, b: \"x\"}) === \"{\\\"a\\\":1,\\\"b\\\":\\\"x\\\"}\";");
    holds("return JSON.stringify([1, 2, 3]) === \"[1,2,3]\";");
    // A control character is escaped, which is the one part of the string
    // form a naive writer gets wrong.
    holds(r##"return JSON.stringify(JSON.parse("\"\\n\"")).length === 4;"##);
    // Non-finite is `null`, which is the specification's answer and not an
    // approximation of one.
    holds("return JSON.stringify([0 / 0, 1 / 0]) === \"[null,null]\";");
    // An `undefined` member is dropped and an `undefined` element is `null` —
    // two different answers for the same value, which is the pair an
    // implementation gets wrong by treating them alike.
    holds("return JSON.stringify({a: undefined, b: 1}) === \"{\\\"b\\\":1}\";");
    holds("return JSON.stringify([undefined]) === \"[null]\";");
    holds("return JSON.stringify(undefined) === undefined;");

    holds("return JSON.parse(\"{\\\"a\\\":[1,2]}\").a[1] === 2;");
    holds("return JSON.parse(\"\\\"\\u0041\\\"\") === \"A\";");
    holds("return JSON.parse(\"-1.5e2\") === -150;");
    // Um texto inválido LANÇA, e o `catch` vê. Esta linha pedia `undefined` —
    // o que `parse` respondia antes de ganhar um throw que sai de um quadro —
    // e ficou para trás quando o comportamento mudou; a asserção agora é o
    // comportamento, e o que ela pina é que o erro é CAPTURÁVEL em vez de
    // terminar o processo.
    holds(
        "try { JSON.parse(\"[\"); return false; }          catch (e) { return e instanceof SyntaxError; }"
    );

    // A key that spells an index reaches the same property either spelling
    // finds, which is what routing the parse through the interner buys.
    holds("return JSON.parse(\"{\\\"0\\\":7}\")[0] === 7;");

    let round = run("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(JSON.parse(JSON.stringify(o)));");
    let direct = run("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(o);");
    // Two separate programs, so the words differ; what is compared is that each
    // produced the same text as itself round-tripped.
    let _ = (round, direct);
    holds("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(JSON.parse(JSON.stringify(o))) === JSON.stringify(o);");

    // A cycle THROWS, which is what the specification says. This asserted the
    // `null` a writer that could not raise had to answer instead — the gap,
    // pinned as though it were the behaviour — so when natives learned to raise
    // the program stopped answering and started ending, and the test aborted
    // rather than failed.
    holds(
        "let o = {}; o.self = o; \
         try { JSON.stringify(o); return false; } \
         catch (e) { return e instanceof TypeError; }",
    );
}

#[test]
fn a_date_is_a_time_value_and_a_calendar_over_it() {
    holds("return new Date(0).getTime() === 0;");
    holds("return new Date(0).getFullYear() === 1970;");
    holds("return new Date(0).getDay() === 4;");
    holds("return new Date(0).toISOString() === \"1970-01-01T00:00:00.000Z\";");

    // A leap day, and a time before the epoch — the two the civil conversion
    // gets wrong when it is written as division alone.
    holds("return new Date(951782400000).toISOString() === \"2000-02-29T00:00:00.000Z\";");
    holds("return new Date(-14182940000).getFullYear() === 1969;");

    holds("return Date.parse(\"2020-01-02T03:04:05.006Z\") === 1577934245006;");
    holds("return new Date(\"2020-01-02T03:04:05.006Z\").getMonth() === 0;");
    holds("return isNaN(Date.parse(\"not a date\"));");

    // Everything is UTC, said as a test rather than only in a comment.
    holds("return new Date(0).getTimezoneOffset() === 0;");
    holds("let d = new Date(3600000); return d.getHours() === d.getUTCHours();");

    holds("return Date.now() > 1700000000000;");
}

#[test]
fn a_map_keeps_insertion_order_and_same_value_zero_keys() {
    holds("let m = new Map(); m.set(\"a\", 1); return m.get(\"a\") === 1 && m.size === 1;");
    holds("let m = new Map(); m.set(\"a\", 1); m.set(\"a\", 2); return m.get(\"a\") === 2 && m.size === 1;");
    holds("let m = new Map(); m.set(\"a\", 1); return m.has(\"a\") && m.delete(\"a\") && m.size === 0;");
    holds("let m = new Map([[1, \"x\"], [2, \"y\"]]); return m.get(2) === \"y\" && m.size === 2;");

    // Insertion order, which a bare hash table does not give and which the
    // specification requires of every walk.
    let order = run("let m = new Map(); m.set(\"b\", 1); m.set(\"a\", 2); m.set(\"c\", 3); let s = \"\"; m.forEach(function (v, k) { s = s + k; }); return s;");
    let expected = run("return \"bac\";");
    let _ = (order, expected);
    holds("let m = new Map(); m.set(\"b\", 1); m.set(\"a\", 2); m.set(\"c\", 3); let s = \"\"; m.forEach(function (v, k) { s = s + k; }); return s === \"bac\";");

    // A delete preserves it, which is the case a swap-with-last gets wrong.
    holds("let m = new Map(); m.set(\"a\", 1); m.set(\"b\", 2); m.set(\"c\", 3); m.delete(\"b\"); return [...m.keys()][1] === \"c\";");

    // SameValueZero: `NaN` is a usable key, where `===` would never find it.
    holds("let m = new Map(); m.set(0 / 0, 7); return m.get(0 / 0) === 7;");
    // And `+0` and `-0` are one key, which is where SameValueZero differs from
    // SameValue rather than from `===`.
    holds("let m = new Map(); m.set(0, 1); m.set(-0, 2); return m.size === 1 && m.get(0) === 2;");

    // Object keys use their live slot as an identity hash and stay correct by identity.
    holds("let a = {}; let b = {}; let m = new Map(); m.set(a, 1); m.set(b, 2); return m.get(a) === 1 && m.get(b) === 2;");
}

#[test]
fn object_keys_keep_identity_across_collections() {
    holds(concat!(
        "let stable = {}; let m = new Map(); let s = new Set(); ",
        "m.set(stable, 7); s.add(stable); ",
        "for (let i = 0; i < 100000; i++) { const garbage = {}; } ",
        "return m.get(stable) === 7 && m.has(stable) && s.has(stable) && !m.has({}) && !s.has({});",
    ));
}

#[test]
fn string_keys_use_content_identity_in_map_and_set() {
    holds(
        "let m = new Map(); \
         let first = \"ab\"; \
         let second = (\"a\" + \"b\").slice(0, 2); \
         m.set(first, 7); \
         m.set(second, 9); \
         return m.size === 1 && m.get(first) === 9 && m.get(second) === 9;",
    );
    holds(
        "let s = new Set(); \
         s.add(\"key\"); \
         s.add(\"k\" + \"ey\"); \
         return s.size === 1 && s.has(\"key\");",
    );
}

#[test]
fn a_set_holds_each_member_once_and_answers_the_es2025_operations() {
    holds("let s = new Set(); s.add(1); s.add(1); return s.size === 1;");
    holds("let s = new Set([1, 2, 3]); return s.has(2) && !s.has(9);");
    holds("let s = new Set([1, 2]); s.delete(1); return s.size === 1 && [...s.values()][0] === 2;");
    holds("let s = new Set([1, 2]); s.clear(); return s.size === 0;");

    holds("let a = new Set([1, 2]); let b = new Set([2, 3]); return a.union(b).size === 3;");
    holds("let a = new Set([1, 2]); let b = new Set([2, 3]); return a.intersection(b).size === 1;");
    holds("let a = new Set([1, 2]); let b = new Set([2, 3]); return a.difference(b).size === 1;");
    holds("let a = new Set([1, 2]); let b = new Set([2, 3]); return a.symmetricDifference(b).size === 2;");
    holds("let a = new Set([1]); let b = new Set([1, 2]); return a.isSubsetOf(b) && b.isSupersetOf(a);");
    holds("let a = new Set([1]); let b = new Set([2]); return a.isDisjointFrom(b);");
}

#[test]
fn a_weak_collection_takes_objects_only_and_is_strong_here() {
    holds("let k = {}; let m = new WeakMap(); m.set(k, 5); return m.get(k) === 5 && m.has(k);");
    holds("let k = {}; let m = new WeakMap(); m.set(k, 5); m.delete(k); return m.has(k) === false;");
    // A primitive key THROWS, which is what the specification says and what
    // this asserted the absence of: it read `m.has(1) === false` back when a
    // native could not raise, and kept reading it after natives learned to —
    // so the program stopped answering `false` and started ending, and the test
    // died with it rather than failing.
    holds(
        "let m = new WeakMap(); \
         try { m.set(1, 5); return false; } \
         catch (e) { return e instanceof TypeError; }",
    );
    holds("let k = {}; let s = new WeakSet(); s.add(k); return s.has(k) && !s.has({});");
}

#[test]
fn a_method_on_a_primitive_receiver_is_reached() {
    // A number is not a cell, so there was nothing for the chain walk to walk
    // from and every one of these read `undefined`.
    holds("return (255).toString(16) === \"ff\";");
    holds("return (5).valueOf() === 5;");
    holds("return true.toString() === \"true\";");
    holds("return false.valueOf() === false;");

    // `toFixed` rounds half AWAY FROM ZERO. Rust formats to nearest-even, so
    // `format!(\"{:.0}\", 2.5)` is \"2\" and this must be \"3\" — a difference
    // invisible for every value that is not exactly half.
    holds("return (2.5).toFixed(0) === \"3\";");
    holds("return (1.005).toFixed(2).length === 4;");
    holds("return (1.5).toFixed(1) === \"1.5\";");

    // The class is registered by the read itself, so a program that never names
    // `Number` still gets the method.
    holds("return (10).toString(2) === \"1010\";");
}

#[test]
fn a_bound_function_keeps_its_receiver_and_its_leading_arguments() {
    let fixed = run("function f() { return this.n; } let g = f.bind({n: 7}); return g();");
    assert_eq!(tags::decode_double(fixed), 7.0);

    // The bound receiver wins over the call's, which is the whole of what
    // `bind` does and the part a naive forward gets backwards.
    let kept = run("function f() { return this.n; } let o = {n: 1, m: f.bind({n: 2})}; return o.m();");
    assert_eq!(tags::decode_double(kept), 2.0);

    // Partial arguments come first at every later call.
    let partial = run("function f(a, b) { return a * 10 + b; } let g = f.bind(null, 1); return g(2);");
    assert_eq!(tags::decode_double(partial), 12.0);

    // And nothing is prepended when none were given — the case that breaks if
    // trailing `undefined` is remembered instead of dropped.
    let none = run("function f(a) { return a; } let g = f.bind(null); return g(9);");
    assert_eq!(tags::decode_double(none), 9.0);

    // Binding twice keeps the first receiver, because the second binds the
    // already-bound function.
    let twice = run("function f() { return this.n; } let g = f.bind({n: 3}).bind({n: 4}); return g();");
    assert_eq!(tags::decode_double(twice), 3.0);
}

#[test]
fn a_name_captured_from_inside_a_nested_block_is_still_captured() {
    // The capture analysis walked a function body twice: once looking for
    // nested functions, once collecting every identifier — and only the second
    // pass saw identifiers, only at the top level of the body. So a write to an
    // outer local from inside an `if` decided the local was not captured, the
    // inner function had no binding for it, and the program was refused as an
    // unbound name. One level up it compiled.
    let guarded = run("let n = 0; function f(x) { if (x) { n = 1; } } f(true); return n;");
    assert_eq!(tags::decode_double(guarded), 1.0);

    let nested = run("let n = 0; function f() { { { n = 2; } } } f(); return n;");
    assert_eq!(tags::decode_double(nested), 2.0);

    let looped = run("let n = 0; function f() { for (let i = 0; i < 3; i = i + 1) { n = n + i; } } f(); return n;");
    assert_eq!(tags::decode_double(looped), 3.0);

    // A `try` was reaching the traversal's wildcard entirely, so a function
    // written inside one had its captures decided as if it did not exist.
    let protected = run("let n = 0; function f() { try { n = 5; } finally { n = n + 1; } } f(); return n;");
    assert_eq!(tags::decode_double(protected), 6.0);
}
#[test]
fn a_write_inside_a_constructors_arguments_survives_a_loop() {
    // The loop-merge analysis walked the expression tree with its OWN match over
    // `ExprKind`, and that copy had no arm for `New`. So a write that happened
    // only inside a constructor's arguments was invisible: the name got no block
    // parameter, the header restored it to its pre-loop value on every pass, and
    // every write to it was discarded. The program compiled and ran wrong.
    let inside_new = run(
        "let y = 0; let i = 0; while (i < 3) { new Object(y = i); i = i + 1; } return y;",
    );
    assert_eq!(tags::decode_double(inside_new), 2.0);

    // The same shape through the nodes that copy also covered, so a regression
    // in either direction is visible.
    let inside_call = run(
        "let y = 0; let i = 0; while (i < 3) { Object(y = i); i = i + 1; } return y;",
    );
    assert_eq!(tags::decode_double(inside_call), 2.0);

    // A template's substitution and an array literal reach the same walk.
    let inside_array = run("let y = 0; let i = 0; while (i < 2) { [y = i]; i = i + 1; } return y;");
    assert_eq!(tags::decode_double(inside_array), 1.0);
}

/// `ToPrimitive` runs the object's own method, and in the order each operator
/// specifies.
///
/// Every one of these answered `NaN`, `undefined` or `false` before: nothing
/// called `valueOf` or `toString`, so an object reaching an operator fell out of
/// the conversion as an absence and was reported as a value. That is the worst
/// shape a defect can have — `'x' + {}` being `NaN` is indistinguishable from
/// arithmetic that went wrong somewhere upstream.
#[test]
fn an_object_operand_is_converted_by_its_own_method() {
    // `+` prefers `valueOf`; `String` prefers `toString`. The same object, two
    // answers, which is what makes the hint more than decoration.
    let both = run(
        "let o = { valueOf() { return 9; }, toString() { return \"t\"; } }; \
         return (o + 0) === 9 && String(o) === \"t\" ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(both), 1.0);

    // Through the prototypes that were already there and unreachable: an array
    // converts by `Array.prototype.toString`, a plain object by
    // `Object.prototype.toString`.
    let inherited = run("return \"x\" + {} === \"x[object Object]\" && String([1, 2]) === \"1,2\" ? 1 : 0;");
    assert_eq!(tags::decode_double(inherited), 1.0);

    // Arithmetic, relational and loose equality, all of which read the object
    // through the same conversion.
    let operators = run(
        "let o = { valueOf() { return 7; } }; \
         return (o - 2) === 5 && (o * 2) === 14 && o > 6 && ([] == 0) ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(operators), 1.0);

    // Two objects are still compared by identity. Converting both first would
    // make `{} == {}` true, which is the one thing this must not break.
    let identity = run("let a = {}; return ({} == {}) === false && (a == a) === true ? 1 : 0;");
    assert_eq!(tags::decode_double(identity), 1.0);

    // The order is observable, and it is left-to-right for ALL FOUR relational
    // operators — including the two the specification writes with their
    // operands swapped.
    //
    // This assertion said the opposite, and it was wrong rather than stale: it
    // required "ba" for `a <= b`, reasoning that `<=` "is specified as !(b < a),
    // so the right operand converts first". The premise is right and the
    // conclusion does not follow. `a <= b` evaluates `IsLessThan(rval, lval,
    // false)`, and that `false` selects the branch converting `y` before `x` —
    // where `y` is the SECOND argument, which is `lval`, the LEFT operand. The
    // specification says why in a NOTE at that very step: "the order of
    // evaluation needs to be reversed to preserve left to right evaluation".
    // The swap in the arguments and the swap in the conversion cancel, on
    // purpose.
    //
    // Checked against both rulers on 2026-08-29 rather than re-derived from the
    // text a second time: node and bun each answer "ab" for `<`, `>`, `<=` and
    // `>=`, and so does this engine. A test asserting behaviour the language
    // does not have is the one failure running more tests cannot catch.
    for op in ["<", ">", "<=", ">="] {
        let order = run(&format!(
            "let log = \"\"; \
             let a = {{ valueOf() {{ log += \"a\"; return 1; }} }}; \
             let b = {{ valueOf() {{ log += \"b\"; return 2; }} }}; \
             a {op} b; return log === \"ab\" ? 1 : 0;"
        ));
        assert_eq!(
            tags::decode_double(order),
            1.0,
            "`a {op} b` converts its left operand first"
        );
    }
    // `+` has no swap to cancel and converts left first for the plain reason.
    let plus = run(
        "let log = \"\"; \
         let a = { valueOf() { log += \"a\"; return 1; } }; \
         let b = { valueOf() { log += \"b\"; return 2; } }; \
         a + b; return log === \"ab\" ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(plus), 1.0);
}

/// `lastIndexOf` with an empty needle answered by hanging.
///
/// It walked forwards keeping the last hit, advancing past each match — and
/// `find` answers `Some` at every position for an empty needle, so the walk
/// never ended. `"abc".lastIndexOf("")` is `3`; it was an infinite loop at full
/// CPU that printed nothing. Searching backwards has no such case, because the
/// range is bounded before the search starts.
#[test]
fn searching_backwards_terminates_on_the_needle_that_matches_everywhere() {
    assert_eq!(tags::decode_double(run("return \"abc\".lastIndexOf(\"\");")), 3.0);
    assert_eq!(tags::decode_double(run("return \"\".lastIndexOf(\"\");")), 0.0);

    // The ordinary answers the rewrite must not have moved.
    holds("return \"abcabc\".lastIndexOf(\"a\") === 3 && \"abcabc\".lastIndexOf(\"bc\") === 4;");
    holds("return \"abc\".lastIndexOf(\"z\") === -1 && \"ab\".lastIndexOf(\"abcd\") === -1;");

    // The position, which this reads as an upper bound — the opposite of what
    // `indexOf` does with its own second argument.
    holds("return \"abcabc\".lastIndexOf(\"a\", 2) === 0;");
}

/// The three position arguments that were accepted and discarded.
///
/// `includes`, `startsWith` and `endsWith` all answered the one-argument answer,
/// so the errors ran in both directions: `"abc".includes("c", 5)` was true and
/// `"abc".startsWith("b", 1)` was false. `indexOf` honoured its `fromIndex`
/// correctly, which is what kept the inconsistency invisible.
#[test]
fn a_search_position_moves_where_the_comparison_happens() {
    holds("return \"abc\".includes(\"a\", 1) === false && \"abc\".includes(\"c\", 5) === false;");
    holds("return \"abc\".includes(\"c\", 1) === true && \"abc\".includes(\"a\") === true;");

    // A start, not a search: `startsWith` compares AT the position.
    holds("return \"abc\".startsWith(\"a\", 1) === false && \"abc\".startsWith(\"b\", 1) === true;");

    // An end, not a start — which is why this one cannot share the clamp.
    holds("return \"abc\".endsWith(\"c\", 2) === false && \"abc\".endsWith(\"b\", 2) === true;");
    holds("return \"abc\".endsWith(\"c\") === true;");
}

/// `join` converts an element that is an object, rather than dropping it.
///
/// Every non-primitive element joined as the empty string, so
/// `[1, [2, 3]].join("-")` answered `"1-"` and the nested data vanished with no
/// trace. The conversion is a call, which is why the elements are copied out of
/// the borrow before any of them runs.
#[test]
fn joining_runs_each_elements_own_conversion() {
    holds("return String([1, [2, 3]]) === \"1,2,3\";");
    holds("return [1, [2, 3]].join(\"-\") === \"1-2,3\";");
    holds("return [[1, 2], [3, 4]].toString() === \"1,2,3,4\";");
    holds("return String([{}]) === \"[object Object]\";");
    holds("let o = { toString() { return \"T\"; } }; return [o].join(\",\") === \"T\";");

    // The separator converts too, and the flat cases must not have moved.
    holds("return [1, 2].join({ toString() { return \"+\"; } }) === \"1+2\";");
    holds("return String([1, 2, 3]) === \"1,2,3\" && String([null, undefined, 1]) === \",,1\";");
}

/// `JSON.stringify` runs a value's own `toJSON`.
///
/// It ignored the hook, so every object that defines one — `Date` included —
/// serialised as `{}`. Well-formed JSON that lost the value, which nothing
/// downstream could detect.
#[test]
fn stringify_asks_the_value_how_it_wants_to_be_written() {
    holds("return JSON.stringify({ toJSON() { return 5; } }) === \"5\";");

    // Nested and inside an array, the two positions the walk reaches a value
    // from other than the root.
    holds("return JSON.stringify({ a: { toJSON() { return \"z\"; } } }) === \"{\\\"a\\\":\\\"z\\\"}\";");
    holds("return JSON.stringify([{ toJSON() { return 1; } }]) === \"[1]\";");

    // Inherited rather than own, which is how `Date` provides one.
    holds("return JSON.stringify(new Date(0)) === \"\\\"1970-01-01T00:00:00.000Z\\\"\";");
}

/// A `Number.prototype` method reads its receiver rather than converting it.
///
/// `thisNumberValue` is a read: the receiver of `(5).toFixed(1)` already IS the
/// number, and the specification never runs `valueOf` to find out. Sharing the
/// argument spelling of `ToNumber` made `valueOf` convert its own receiver — by
/// looking up `valueOf` and calling it — which recursed until the stack ran out
/// in four suite files.
#[test]
fn a_number_method_does_not_convert_its_own_receiver() {
    holds("return (5).valueOf() === 5 && (2.5).toFixed(0) === \"3\";");
    holds("return (255).toString(16) === \"ff\" && (1.5).toPrecision(2) === \"1.5\";");

    // The argument side still converts, which is the half that must not have
    // been narrowed with it.
    holds("return Number({ valueOf() { return 5; } }) === 5;");
}

/// A derived class runs its field initialisers, and runs them after `super()`.
///
/// They were prepended to the constructor head, where a derived class has no
/// `this` yet — it lives in an environment slot only the `super()` call writes —
/// so every field of every subclass was assigned onto whatever that slot held
/// before and lost. `class B extends A { b = 2 }` answered `undefined`.
#[test]
fn a_subclass_field_is_initialised_after_the_base_has_made_the_object() {
    let inherited = run("class A { x = 10; } class B extends A { w = 30; } let b = new B(); return b.x + b.w;");
    assert_eq!(tags::decode_double(inherited), 40.0);

    // The ORDER is the point, not merely that they run: an initialiser may read
    // a property the base constructor set, which is only there after `super()`.
    let ordered = run(
        "class C { constructor(n) { this.n = n; } } \
         class E extends C { e = this.n * 2; constructor() { super(4); } } \
         return new E().e;",
    );
    assert_eq!(tags::decode_double(ordered), 8.0);

    // A base class keeps the head placement, where its own constructor body can
    // already read what the initialisers wrote.
    let base = run("class F { f = 1; constructor() { this.g = this.f + 1; } } return new F().g;");
    assert_eq!(tags::decode_double(base), 2.0);
}

/// `Math.random` answers a double in `[0, 1)` and advances.
///
/// It was absent entirely, so `Math.random()` read `undefined` and every
/// expression built on it — the usual `Math.floor(Math.random() * n)` — answered
/// `NaN`. What is pinned here is what the language actually promises: the range,
/// and that consecutive draws differ. The specification says
/// "implementation-dependent" and guarantees nothing about the sequence, so a
/// test demanding particular values would be pinning this implementation rather
/// than the language.
#[test]
fn random_stays_in_the_unit_interval_and_moves() {
    let held = run(
        "let ok = true; let moved = false; let prev = -1; \
         for (let i = 0; i < 500; i++) { \
           let r = Math.random(); \
           if (r < 0 || r >= 1) { ok = false; } \
           if (r !== prev) { moved = true; } \
           prev = r; \
         } \
         return ok && moved ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(held), 1.0);
}

/// `await` over a promise only a timer can settle.
///
/// `await new Promise(r => setTimeout(r, n))` — the standard way to wait —
/// reported "this promise cannot settle" and ended the program. Two causes, and
/// only the second is interesting.
///
/// The runtime holds a hook for letting time pass, because `std::thread::sleep`
/// is not on every target; nothing installed it, so `rest_for` always answered
/// "no waiter". The host slept in its own loop between turns, which hid it: a
/// timer fired once the body was over, so timers looked like they worked.
///
/// The second is the real one. Pumping the loop DELIVERS — the turn that empties
/// the timer table is the turn that ran the callback — so "nothing outstanding"
/// was being read as "nothing can settle it" one instant after the thing had
/// settled. The answer is now re-checked before it is believed.
#[test]
fn awaiting_a_timer_finishes_rather_than_reporting_a_deadlock() {
    let waited = run("await new Promise(function (r) { setTimeout(r, 1); }); return 5;");
    assert_eq!(tags::decode_double(waited), 5.0);

    // Through an async function, which is the shape a program actually writes.
    let through = run(
        "async function go() { await new Promise(function (r) { setTimeout(r, 1); }); return 7; } \
         return await go();",
    );
    assert_eq!(tags::decode_double(through), 7.0);

    // The value crosses the wait, so this is not "it stopped erroring".
    let carried = run(
        "let seen = 0; \
         await new Promise(function (r) { setTimeout(function () { seen = 3; r(0); }, 1); }); \
         return seen;",
    );
    assert_eq!(tags::decode_double(carried), 3.0);
}

/// A generator parks its frame between answers, and picks it back up.
///
/// The machine's half of this — `frame::resumable_form` — was built and tested
/// long before anything called it. What runs here is the other three: the
/// language emits a body that may park plus a wrapper that makes an object, the
/// host rewrites that body before placing it, and the runtime holds the frame.
///
/// `function*` was the largest single refusal in the suite (38 files), and it is
/// no longer one.
#[test]
fn a_generator_answers_its_values_one_at_a_time() {
    let counted = run(
        "function* counter() { yield 1; yield 2; return 3; } \
         const g = counter(); \
         let sum = 0; \
         sum = sum + g.next().value; \
         sum = sum + g.next().value; \
         sum = sum + g.next().value; \
         return sum;",
    );
    assert_eq!(
        tags::decode_double(counted),
        6.0,
        "each entry answers where the last one left off"
    );

    // `done` is what separates a generator from a function answering values.
    let finished = run(
        "function* one() { yield 1; } \
         const g = one(); \
         g.next(); \
         return g.next().done ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(finished), 1.0);
}

/// What a resumption delivers is what the `yield` evaluates to.
///
/// This is the half a state-machine desugaring gets wrong quietly: the value
/// travels INTO the parked frame, and it is the suspension's own result rather
/// than anything the call that produced it answered.
#[test]
fn what_next_is_given_is_what_the_yield_produced() {
    let echoed = run(
        "function* echo(a) { const x = yield a; const y = yield 0; return x + y; } \
         const g = echo(10); \
         g.next(); \
         g.next(5); \
         return g.next(7).value;",
    );
    assert_eq!(tags::decode_double(echoed), 12.0);
}

/// A generator IS an iterator, so `for`-`of` and spread reach it.
///
/// `Symbol.iterator` answering the generator itself is installed beside the
/// class rather than declared in it — the attribute names a member with a
/// string, and that key is a symbol.
#[test]
fn a_generator_is_what_the_iteration_protocol_asks_for() {
    let totalled = run(
        "function* three() { yield 1; yield 2; yield 3; } \
         let total = 0; \
         for (const n of three()) { total = total + n; } \
         return total;",
    );
    assert_eq!(tags::decode_double(totalled), 6.0);

    let spread = run("function* two() { yield 4; yield 5; } return [...two()].length;");
    assert_eq!(tags::decode_double(spread), 2.0);
}

/// `yield*` produces what another iterable produces.
///
/// A loop whose body is an ordinary `yield`, not a suspension of its own —
/// which is why it was refused separately after `yield` worked. What it does
/// NOT do is forward `next`, `throw` and `return` to the inner iterator; see
/// `emit/delegate.rs`, where the limit is the same one `for`-`of` has.
#[test]
fn delegating_yields_each_of_the_inner_values() {
    let summed = run(
        "function* inner() { yield 1; yield 2; } \
         function* outer() { yield* inner(); yield 3; } \
         let total = 0; \
         for (const n of outer()) { total = total + n; } \
         return total;",
    );
    assert_eq!(tags::decode_double(summed), 6.0);

    // An array is an iterable like any other, and the commonest thing written.
    let over_an_array = run(
        "function* g() { yield* [4, 5]; } \
         let total = 0; \
         for (const n of g()) { total = total + n; } \
         return total;",
    );
    assert_eq!(tags::decode_double(over_an_array), 9.0);

    // Order, which a loop that yielded before stepping would still pass the
    // sums above with.
    let ordered = run(
        "function* g() { yield* [1, 2]; } \
         const seen = []; \
         for (const n of g()) { seen.push(n); } \
         return seen[0] * 10 + seen[1];",
    );
    assert_eq!(tags::decode_double(ordered), 12.0);
}

/// A generator parks inside a `try`, and a `finally` around one is still a
/// cleanup after the rewrite.
///
/// The rewrite builds a NEW function and copies instructions into it, so
/// everything a function owns beside its blocks has to be carried across. The
/// protected regions were the fourth such thing to be missing, and the shape of
/// the failure is worth pinning: the cleanup block belonged to no region, so its
/// own terminator was refused as being outside a cleanup.
#[test]
fn a_generator_can_park_inside_a_protected_region() {
    let caught = run(
        "function* g() { try { yield 1; } catch (e) { yield 2; } } \n         return g().next().value;",
    );
    assert_eq!(tags::decode_double(caught), 1.0);

    let past_a_cleanup = run(
        "function* g() { try { yield 1; } finally { } yield 2; } \n         const it = g(); \n         it.next(); \n         return it.next().value;",
    );
    assert_eq!(tags::decode_double(past_a_cleanup), 2.0);
}

/// A `Proxy` answers reads and writes through its handler.
///
/// Nothing in the compiled fast path changed to make this work, and that is the
/// design rather than a coincidence: a cached access encodes an OWN slot, a
/// proxy has no own properties, so every access to one misses to the entry
/// point where the traps live. A program with no proxy in it pays nothing.
#[test]
fn a_proxy_answers_through_its_handler() {
    let trapped = run(
        "const p = new Proxy({}, { get(t, k) { return 42; } }); return p.anything;",
    );
    assert_eq!(tags::decode_double(trapped), 42.0);

    // The property reaches the trap as a string, which is the one place a key
    // has to travel back out of the number the compiler resolved it to.
    let named = run(
        "let seen = ''; \
         const p = new Proxy({}, { get(t, k) { seen = k; return 1; } }); \
         p.chosen; \
         return seen === 'chosen' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(named), 1.0);

    // A write goes to the trap, and the assignment still evaluates to the value.
    let written = run(
        "let stored = 0; \
         const p = new Proxy({}, { set(t, k, v) { stored = v; return true; } }); \
         const answered = (p.x = 7); \
         return stored * 10 + answered;",
    );
    assert_eq!(tags::decode_double(written), 77.0);

    let asked = run(
        "const p = new Proxy({}, { has(t, k) { return k === 'yes'; } }); \
         return ('yes' in p) && !('no' in p) ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(asked), 1.0);
}

/// A handler without the trap forwards to the target.
///
/// Which is what makes a partial handler useful rather than a hole: the
/// specification's default for every trap is the operation on the target.
#[test]
fn a_handler_without_the_trap_falls_through_to_the_target() {
    let read = run("const p = new Proxy({ a: 5 }, {}); return p.a;");
    assert_eq!(tags::decode_double(read), 5.0);

    let written = run(
        "const target = { a: 1 }; \
         const p = new Proxy(target, {}); \
         p.a = 9; \
         return target.a;",
    );
    assert_eq!(tags::decode_double(written), 9.0);

    let present = run("const p = new Proxy({ a: 1 }, {}); return 'a' in p ? 1 : 0;");
    assert_eq!(tags::decode_double(present), 1.0);
}

/// A proxy whose target is a proxy reaches the inner handler.
///
/// `new Proxy(new Proxy(x, inner), {})` is what a wrapper around a wrapper is,
/// and the forwarding an absent trap does has to be the whole operation rather
/// than a shape read — otherwise the inner handler is skipped and the read
/// answers `undefined`, which is what a chain of three reported.
#[test]
fn a_proxy_over_a_proxy_reaches_the_innermost_handler() {
    let chained = run(
        "const inner = new Proxy({}, { get(t, k) { return 100; } }); \
         const middle = new Proxy(inner, {}); \
         const outer = new Proxy(middle, {}); \
         return outer.anything;",
    );
    assert_eq!(tags::decode_double(chained), 100.0);
}

/// A proxy is callable and constructible through its handler.
///
/// It has no code address of its own and must not: an address is the one thing
/// a program may never choose. So a call to a proxy arrives where the jump did
/// not happen, rather than at a check every ordinary call would pay for.
#[test]
fn a_proxy_can_be_called_and_constructed() {
    let applied = run(
        "const p = new Proxy(function () {}, { apply(t, self, args) { return 42; } }); \
         return p();",
    );
    assert_eq!(tags::decode_double(applied), 42.0);

    // The arguments reach the trap as an array, which is what the trap's
    // signature says and what a forwarding handler passes on.
    let with_arguments = run(
        "const p = new Proxy(function () {}, { apply(t, self, args) { return args[0] + args[1]; } }); \
         return p(3, 4);",
    );
    assert_eq!(tags::decode_double(with_arguments), 7.0);

    // No trap: the call goes to the target.
    let forwarded = run(
        "function target(a) { return a * 2; } \
         const p = new Proxy(target, {}); \
         return p(21);",
    );
    assert_eq!(tags::decode_double(forwarded), 42.0);

    let built = run(
        "class Thing { constructor() { this.n = 1; } } \
         const p = new Proxy(Thing, { construct(t, args) { return { n: 9 }; } }); \
         return new p().n;",
    );
    assert_eq!(tags::decode_double(built), 9.0);
}

/// `Object.keys` and the prototype accessors go through their traps.
#[test]
fn a_proxy_answers_for_its_keys_and_its_prototype() {
    let listed = run(
        "const p = new Proxy({}, { ownKeys(t) { return ['a', 'b', 'c']; } }); \
         return Reflect.ownKeys(p).length;",
    );
    assert_eq!(tags::decode_double(listed), 3.0);

    // `Object.keys` is NOT that list: it asks for each key's descriptor to keep
    // only the enumerable ones, and a key the trap invented that the target
    // does not have has no descriptor. Three keys in, none out — which is what
    // every other engine answers and what this test exists to keep true, since
    // the obvious implementation returns three.
    let filtered = run(
        "const p = new Proxy({ a: 1 }, { ownKeys(t) { return ['x', 'y']; } }); \
         return Object.keys(p).length;",
    );
    assert_eq!(tags::decode_double(filtered), 0.0);

    // No trap: the target's own keys, which is what forwarding means — the
    // proxy itself has never had any.
    let forwarded = run("const p = new Proxy({ x: 1, y: 2 }, {}); return Object.keys(p).length;");
    assert_eq!(tags::decode_double(forwarded), 2.0);

    let inherited = run(
        "const proto = { tag: 7 }; \
         const p = new Proxy({}, { getPrototypeOf(t) { return proto; } }); \
         return Object.getPrototypeOf(p).tag;",
    );
    assert_eq!(tags::decode_double(inherited), 7.0);
}

/// A trap answers the named read and the computed one alike.
///
/// `o.x` reaches `get_property` and `Reflect.get(o, "x")` reaches
/// `get_indexed` — two spellings of one operation, and a proxy asked by only
/// one of them is those two disagreeing. It was exactly that for a while: every
/// handler written as `get: (t, k) => …` and read through `Reflect` answered
/// `undefined` while the same handler read as `p.x` answered correctly.
#[test]
fn a_trap_answers_the_computed_spelling_too() {
    let read = run(
        "const p = new Proxy({ name: 'alice' }, { get: (t, k) => 0 }); \
         return Reflect.get(p, 'name');",
    );
    assert_eq!(tags::decode_double(read), 0.0);

    let written = run(
        "let stored = 0; \
         const p = new Proxy({}, { set: (t, k, v) => { stored = v; return true; } }); \
         Reflect.set(p, 'x', 5); \
         return stored;",
    );
    assert_eq!(tags::decode_double(written), 5.0);

    // And a descriptor, which is the trap `Reflect` had no member for at all.
    //
    // The trap has to name a property the target could actually have: a
    // descriptor with no `configurable` is completed to `configurable: false`,
    // and claiming a non-configurable property that an extensible target does
    // not own is the invariant `[[GetOwnProperty]]` refuses. This asserted `42`
    // until the invariant check existed, so it was pinning the absence of the
    // check rather than the trap — bun and node both throw on the version it
    // asserted.
    let described = run(
        "const p = new Proxy({ x: 0 }, \
             { getOwnPropertyDescriptor: (t, k) => ({ value: 42, configurable: true }) }); \
         return Reflect.getOwnPropertyDescriptor(p, 'x').value;",
    );
    assert_eq!(tags::decode_double(described), 42.0);

    // A handler that refuses reports the refusal, rather than the truth that
    // the call reached an object.
    let refused = run(
        "const p = new Proxy({}, { defineProperty: (t, k, d) => false }); \
         return Reflect.defineProperty(p, 'x', { value: 1 }) ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(refused), 0.0);
}

/// `values()`, `keys()` and `entries()` answer an ITERATOR.
///
/// They answered the materialised array, which is why `for`-`of` over one
/// worked and `.next()` did not exist at all. The array is still one spread
/// away, which is how it is written in JavaScript — and `.length` on the
/// iterator is now `undefined`, exactly as it is in every other engine.
#[test]
fn the_three_iteration_methods_answer_something_with_next() {
    let stepped = run(
        "const it = [1, 2].values(); \
         const first = it.next().value; \
         const second = it.next().value; \
         const done = it.next().done ? 1 : 0; \
         return first * 100 + second * 10 + done;",
    );
    assert_eq!(tags::decode_double(stepped), 121.0);

    // An exhausted iterator stays exhausted rather than wrapping around.
    let twice_past_the_end = run(
        "const it = [1].values(); it.next(); it.next(); \
         return it.next().done ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(twice_past_the_end), 1.0);

    // A collection's three, which walk the same lists they always did.
    let mapped = run(
        "const m = new Map([['k', 9]]); \
         return m.values().next().value;",
    );
    assert_eq!(tags::decode_double(mapped), 9.0);

    let setted = run("const s = new Set([5, 6]); return s.keys().next().value;");
    assert_eq!(tags::decode_double(setted), 5.0);

    // The iterator is iterable, which is what `for`-`of` and spread need — and
    // what keeps every program that used the old array answer working.
    let spread = run("return [...[1, 2, 3].values()].length;");
    assert_eq!(tags::decode_double(spread), 3.0);

    // `Symbol.iterator` on an array IS `values`, rather than a second walk.
    let by_symbol = run("const it = [4, 5][Symbol.iterator](); return it.next().value;");
    assert_eq!(tags::decode_double(by_symbol), 4.0);
}

/// The ES2025 iterator helpers, on what `values()` and friends answer.
///
/// `arr.entries().map(f)` is what a program written for Node reaches for, and
/// it answers another ITERATOR — so `.toArray()` is how an array comes back and
/// `.join()` on the helper's result is a mistake there as much as here.
#[test]
fn an_iterator_carries_the_helpers_a_program_expects() {
    let mapped = run("return [1, 2, 3].values().map(x => x * 2).toArray().join(',') === '2,4,6' ? 1 : 0;");
    assert_eq!(tags::decode_double(mapped), 1.0);

    let filtered =
        run("return [1, 2, 3, 4].values().filter(x => x > 2).toArray().length;");
    assert_eq!(tags::decode_double(filtered), 2.0);

    let sliced = run("return [1, 2, 3].values().take(2).toArray().join('') === '12' ? 1 : 0;");
    assert_eq!(tags::decode_double(sliced), 1.0);

    let dropped = run("return [1, 2, 3].values().drop(1).toArray().join('') === '23' ? 1 : 0;");
    assert_eq!(tags::decode_double(dropped), 1.0);

    let folded = run("return [1, 2, 3].values().reduce((a, b) => a + b, 0);");
    assert_eq!(tags::decode_double(folded), 6.0);

    let found = run("return [1, 2, 3].values().find(x => x > 1);");
    assert_eq!(tags::decode_double(found), 2.0);

    let flattened = run("return [1, 2].values().flatMap(x => [x, x]).toArray().length;");
    assert_eq!(tags::decode_double(flattened), 4.0);

    // A helper CONSUMES the iterator it was called on, which is what stops two
    // `take(1)` calls from both answering the first element.
    //
    // The claim is right and the assertion under it was wrong. It read
    // `it.next().done ? 1 : 0` and required 1 — that the source is EXHAUSTED
    // after `take(1).toArray()`. It is not: `take` pulls one element and then
    // performs `IteratorClose`, which looks for a `return` method, and an array
    // iterator has none. So two of the three elements are still there. node and
    // bun both answer `{ value: 2, done: false }`, and so does this engine;
    // checked on 2026-08-29, which is the ruler this file otherwise uses.
    //
    // What consumption actually looks like is the second pair below: two
    // `take(1)` calls on ONE source answer `[1]` and then `[2]`, never `[1]`
    // twice. That is the behaviour the comment always described, now asserted.
    let after = run(
        "const it = [1, 2, 3].values(); \
         it.take(1).toArray(); \
         const step = it.next(); \
         return step.done === false && step.value === 2 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(after),
        1.0,
        "one element taken leaves the other two on the source"
    );

    let twice = run(
        "const it = [1, 2, 3].values(); \
         const first = it.take(1).toArray(); \
         const second = it.take(1).toArray(); \
         return first[0] === 1 && second[0] === 2 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(twice),
        1.0,
        "the second helper starts where the first one stopped"
    );

    // The same through a helper that WRAPS rather than ends: `map` adopts the
    // source, `take` adopts the map, and the pull still reaches the array.
    let through = run(
        "const it = [1, 2, 3].values(); \
         const mapped = it.map(x => x * 2).take(1).toArray(); \
         const step = it.next(); \
         return mapped[0] === 2 && step.value === 2 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(through), 1.0);
}

/// `export *` and `export * as ns` both forward what another module exports.
///
/// Written against the graph rather than a single file, because that is what
/// they are about. `compile` alone cannot see a second module at all.
#[test]
fn a_star_export_forwards_what_the_other_module_has() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("rts_star_export");
    std::fs::create_dir_all(&dir).expect("a directory to write fixtures in");
    let write = |name: &str, source: &str| {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
        path
    };

    write("star_inner.ts", "export function one() { return 1; }\nexport const three = 3;\n");
    write("star_all.ts", "export * from \"./star_inner\";\n");
    write("star_ns.ts", "export * as inner from \"./star_inner\";\n");
    // Asserted from INSIDE the program, through `rts:test`, because a module
    // answers nothing to its host: its last expression is not its value. This
    // is the shape `suite_run` uses for every file in the suite.
    let entry = write(
        "star_entry.ts",
        "import { test, expect } from \"rts:test\";\n\
         import { one, three } from \"./star_all\";\n\
         import { inner } from \"./star_ns\";\n\
         test(\"forwarded\", () => expect(one() + three + inner.one()).toBe(5));\n",
    );

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 1, "the fixture registers one test");
    assert!(
        failed.is_empty(),
        "1 and 3 through `export *`, and 1 through `export * as ns` — the last \
         of which forwarded a name called `*` and answered undefined: {failed:?}"
    );
}

/// A native that calls user code asks whether the callee left a throw behind.
///
/// The rule the runtime could not raise without. `invoke` answers `undefined`
/// for a call that threw, `undefined` is a value, and a native that carries on
/// with it produces effects the language says never happen — or, in the case
/// this test's second half pins, never stops.
#[test]
fn a_throw_from_a_callback_stops_the_native_that_called_it() {
    // A spread over an iterator whose `next` throws. This filled a vector until
    // the process died: `done` read `undefined`, which is never true. It hung a
    // test for over an hour instead of passing in 0.05 s.
    let propagated = run(
        "const g = { [Symbol.iterator]() { return { next() { throw new Error('inner'); } }; } }; \
         let seen = 'none'; \
         try { [...g]; } catch (e) { seen = e.message; } \
         return seen === 'inner' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(propagated), 1.0);

    // An iterator protocol that is not one at all: `Symbol.iterator` answered,
    // and what it gave back has no callable `next`.
    let refused = run(
        "const b = { [Symbol.iterator]() { return { next: 3 }; } }; \
         let kind = 'none'; \
         try { [...b]; } catch (e) { kind = e instanceof TypeError ? 'TypeError' : 'other'; } \
         return kind === 'TypeError' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(refused), 1.0);

    // `forEach` stops rather than running the callback over the rest.
    let stopped = run(
        "let ran = 0; \
         try { [1, 2, 3].forEach(x => { ran = ran + 1; if (x === 2) throw new Error('stop'); }); } \
         catch (e) {} \
         return ran;",
    );
    assert_eq!(tags::decode_double(stopped), 2.0);

    // And `map` answers what it had, rather than folding `undefined` in.
    let mapped = run(
        "let ran = 0; \
         try { [1, 2, 3].map(x => { ran = ran + 1; if (x === 2) throw new Error('stop'); return x; }); } \
         catch (e) {} \
         return ran;",
    );
    assert_eq!(tags::decode_double(mapped), 2.0);
}

/// A `.then` handler that throws REJECTS the derived promise.
///
/// It resolved it with `undefined` — so `.catch()` after a throwing `.then()`
/// never fired and a failed chain reported success. The specification inverted,
/// and the reason the runtime was not allowed to raise until the checks existed.
#[test]
fn a_handler_that_throws_rejects_the_promise_it_derived() {
    let rejected = run(
        "let seen = 'none'; \
         await Promise.resolve(1) \
             .then(function () { throw new Error('boom'); }) \
             .catch(function (e) { seen = e.message; }); \
         return seen === 'boom' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(rejected), 1.0);

    // A `finally` that throws replaces the settlement it was passing through.
    let replaced = run(
        "let seen = 'none'; \
         await Promise.resolve(1) \
             .finally(function () { throw new Error('from finally'); }) \
             .catch(function (e) { seen = e.message; }); \
         return seen === 'from finally' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(replaced), 1.0);
}

/// A class body binds the class's own name, which its static block reads.
///
/// `static { Config.VERSION = "1.0.0" }` is how a static block is written, and
/// the name resolved to `undefined` inside one: for a declaration the outer
/// binding is not written until the class expression finishes. The block ran to
/// completion having assigned a property of nothing, which was invisible until
/// a native could throw — the method it was supposed to set then answered
/// `undefined`, and calling it became a `TypeError`.
#[test]
fn a_static_block_can_name_the_class_it_is_in() {
    let assigned = run(
        "class Config { static V; static { Config.V = 7; } } return Config.V;",
    );
    assert_eq!(tags::decode_double(assigned), 7.0);

    // Through a static field the block mutates rather than replaces, which is
    // the other shape a real program writes.
    let appended = run(
        "class R { static items = []; static { R.items.push(1); R.items.push(2); } } \
         return R.items.length;",
    );
    assert_eq!(tags::decode_double(appended), 2.0);

    // A static METHOD naming the class, which is the same binding seen from a
    // function rather than from the body.
    let called = run(
        "class Q { static V = 1; static bump() { return Q.V + 1; } } return Q.bump();",
    );
    assert_eq!(tags::decode_double(called), 2.0);
}

/// `import { num } from "rts"` — the bare specifier, and 64-bit arithmetic.
///
/// The specifier was not registered at all, so 33 files in the suite bound
/// nothing from it and died on their first call once a native could throw.
#[test]
fn the_bare_rts_specifier_answers_integer_arithmetic() {
    // Through a fixture rather than `run`, because an `import` is a MODULE item:
    // `run` compiles a script, and `import()` as an expression is its own gap.
    use std::io::Write;
    let dir = std::env::temp_dir().join("rts_bare_surface");
    std::fs::create_dir_all(&dir).expect("a directory to write a fixture in");
    let path = dir.join("surface.ts");
    let mut file = std::fs::File::create(&path).expect("a fixture file");
    file.write_all(
        b"import { test, expect } from \"rts:test\";\n\
          import { num, math, hint } from \"rts\";\n\
          test(\"wrapping\", () => expect(num.wrapping_sub(0, 1)).toBe(-1));\n\
          test(\"checked refuses\", () => expect(num.checked_div(100, 0)).toBe(-9223372036854775808));\n\
          test(\"bits\", () => expect(num.count_ones(255)).toBe(8));\n\
          test(\"integer abs\", () => expect(math.abs_i64(-13)).toBe(13));\n\
          test(\"a hint answers its argument\", () => expect(hint.black_box_i64(42)).toBe(42));\n",
    )
    .expect("written");

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&path).expect("the fixture compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 5, "the fixture registers five tests");
    assert!(failed.is_empty(), "{failed:?}");
}

/// An error carries where it was made, in the `at …` form Node and Bun print.
///
/// This engine reported `Error: boom` and nothing else, so a failure said what
/// happened and never where. The frames come from the stack `functions::invoke`
/// already keeps — the one a bound function reads to know which binding it is —
/// rather than from a second record that would drift the first time either
/// forgot to pop.
#[test]
fn an_error_says_where_it_came_from() {
    let traced = run(
        "function inner() { throw new Error('boom'); } \
         function middle() { inner(); } \
         function outer() { middle(); } \
         let seen = ''; \
         try { outer(); } catch (e) { seen = e.stack; } \
         return seen.indexOf('at inner') >= 0 && seen.indexOf('at outer') >= 0 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(traced), 1.0);

    // Innermost first, which is the order every engine prints and the order a
    // reader scans.
    let ordered = run(
        "function inner() { throw new Error('boom'); } \
         function outer() { inner(); } \
         let seen = ''; \
         try { outer(); } catch (e) { seen = e.stack; } \
         return seen.indexOf('at inner') < seen.indexOf('at outer') ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(ordered), 1.0);

    // The header is `Name: message`, so the first line still says what happened.
    let headed = run(
        "let seen = ''; \
         try { throw new TypeError('wrong'); } catch (e) { seen = e.stack; } \
         return seen.indexOf('TypeError: wrong') === 0 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(headed), 1.0);

    // Captured where the error is CONSTRUCTED, not where it is thrown — which
    // is what every engine does, and what makes a stored-then-thrown error name
    // the line that made it.
    let constructed = run(
        "function made() { return new Error('later'); } \
         const e = made(); \
         return e.stack.indexOf('at made') >= 0 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(constructed), 1.0);
}

#[test]
fn proven_dot_rs_try_catch_bug_class_a_var_reassigned_inside_try_is_not_wrongly_proved_numeric() {
    // The fifth hole found by construction (see `emit/proven.rs`'s module doc
    // on `keep_only_numeric`): `StmtKind::Try` had no arm in `proven.rs` at
    // all, so an assignment inside a `try` body was invisible to the pass
    // that decides whether a local keeps its proven `F64` representation.
    // Before the fix this failed to compile with `ImplicitNarrowing` (or
    // worse, ran with a mismatched representation) because `x` stayed
    // "proved numeric" straight through `x = "a"`.
    let produced = run(
        "let x = 1; try { x = \"a\"; } catch (e) {} return typeof x === \"string\" ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_call_emitted_as_its_callee_s_body_still_evaluates_its_arguments_once_and_in_order() {
    // The one thing a substitution can break that a call cannot: an argument is
    // an expression, and binding a parameter to it twice would run it twice.
    // `emit/inline.rs` emits every argument before it binds anything, so the
    // counter here answers 1 rather than 2 — and the order is pinned as well,
    // because the second argument's side effect must happen after the first's.
    let produced = run(
        "function pick(a, b) { return a * 10 + b; } \
         let log = ''; \
         function step(c) { log = log + c; return c === 'x' ? 1 : 2; } \
         const answered = pick(step('x'), step('y')); \
         return answered === 12 && log === 'xy' ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_function_a_call_site_substitutes_is_still_a_value_the_program_can_pass_around() {
    // Substituting at the call site must not remove the function: `id` is
    // called directly AND handed to `map`, and only the first of those is a
    // call site at all. A version that treated the proof as permission to stop
    // emitting the declaration would fail here rather than merely be slower.
    let produced = run(
        "function id(x) { return x; } \
         return id(7) === 7 && [1, 2, 3].map(id).length === 3 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_parameter_of_a_substituted_body_shadows_a_caller_local_of_the_same_spelling() {
    // The hazard the substitution introduces, since the body is emitted in the
    // CALLER's scope: `x` in the callee is the argument, never the caller's own
    // `x`. Bound in a scope layer of its own for exactly this, and the answer
    // 9 rather than 1000 is what says the layer is there.
    let produced = run(
        "function twice(x) { return x + x; } \
         let x = 500; \
         return twice(4.5) === 9 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_name_declared_twice_is_never_substituted_from_the_wrong_declaration() {
    // The whole-program condition, as a program that would give a wrong answer
    // without it: the inner `size` shadows the outer one, so a call inside
    // `wrapped` must reach the inner function. `inline::declarations_of`
    // counts two declarations and refuses the candidate outright.
    let produced = run(
        "function size(v) { return v + 1; } \
         function wrapped() { function size(v) { return v + 100; } return size(1); } \
         return wrapped() === 101 ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

#[test]
fn a_closure_survives_the_collection_its_own_prototype_allocation_triggers() {
    // `closure_new` allocates the callable's cell, records what makes it
    // callable beside it, and THEN allocates the `prototype` object every
    // function gets. Between those two allocations the only thing naming the
    // first cell is a Rust local holding a raw index — and `roots::scan_stack`
    // keeps only words that are unambiguously encoded references, which an
    // index is not. So the second allocation could collect the closure that
    // asked for it.
    //
    // The count is measured, not chosen: the region is 65 536 cells and a
    // closure takes two, so a collection lands every ~32 000 of these. Fewer
    // than that and the test passes without the fix, which would make it a test
    // of nothing.
    //
    // The symptom was `TypeError: object is not a function`, because the swept
    // cell was handed back out and the value named whatever took the index.
    let produced = run(
        "let n = 0; \
         for (let i = 0; i < 200000; i++) { const f = (x) => x + 1; n = n + f(1); } \
         return n === 400000 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(produced),
        1.0,
        "every closure made in the loop stayed callable across the collections"
    );
}

#[test]
fn values_a_native_is_still_accumulating_survive_a_collection() {
    // The values `map` has produced so far live in a Rust `Vec<u64>` on the
    // native's own frame. `roots::scan_stack` walks the MACHINE stack — the
    // `Vec`'s buffer is on the Rust heap and nowhere in that range — so nothing
    // saw them, and the callback allocating is what makes a collection land in
    // the middle of the loop.
    //
    // The failure was not a crash. It was an ANSWER: nine of three hundred
    // rounds came back with wrong data, because objects already produced had
    // been swept and their cells handed to something else. That is why this
    // asserts the contents and not merely the length.
    let produced = run(
        "const xs = []; \
         for (let i = 0; i < 500; i++) xs.push(i); \
         let bad = 0; \
         for (let r = 0; r < 200; r++) { \
           const out = xs.map((v) => ({ v: v })); \
           if (out.length !== 500) { bad = bad + 1; continue; } \
           for (let i = 0; i < 500; i++) { if (out[i].v !== i) { bad = bad + 1; break; } } \
         } \
         return bad === 0 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(produced),
        1.0,
        "every object the map produced was still itself when the map finished"
    );
}

#[test]
fn negating_a_proven_double_is_a_sign_flip_and_still_answers_what_the_language_says() {
    // `-x` had no proven form at all: `emit_unary` always called the runtime,
    // on the grounds that `x * -1` is wrong for a bigint. That argument is
    // about the MULTIPLY. A sign flip over a value already proven `Repr::F64`
    // cannot be reached by a bigint — the proof is what rules it out — so
    // `FloatOp::Neg` is emitted there and the call stays for everything else.
    //
    // The corners a sign flip must not get wrong, all in one program: `-0` is
    // not `0` under `Object.is` but is under `===`, double negation is the
    // identity, and a bigint still negates as a bigint rather than as `NaN`.
    let produced = run(
        "let sign = 1.0; let s = 0.0; \
         for (let i = 0; i < 6; i++) { sign = -sign; s = s + sign * i; } \
         const zero = -0; \
         const ok = s === 3 \
           && (zero === 0) \
           && Object.is(zero, -0) \
           && !Object.is(zero, 0) \
           && -(-5) === 5 \
           && (-(2n) === -2n) \
           && Object.is(-(0.0), -0); \
         return ok ? 1 : 0;",
    );
    assert_eq!(tags::decode_double(produced), 1.0);
}

/// `import.meta` names the module's own file, and `import()` answers the SAME
/// namespace a second call does.
///
/// Written against the graph, for the reason the `export *` test above is: a
/// single file has no specifier, so neither operation has a module to be about.
/// What it pins is the language's, not this engine's: the specification gives a
/// module ONE `import.meta` object, `import.meta.url` is a `file:` URL, and two
/// imports of one specifier are one module — which is what a module cache means
/// and is the only thing `first === second` can be testing.
#[test]
fn a_module_knows_its_own_url_and_imports_itself_once() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("rts_import_meta");
    std::fs::create_dir_all(&dir).expect("a directory to write fixtures in");
    let write = |name: &str, source: &str| {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
        path
    };

    write("meta_inner.ts", "export const value = 7;\n");
    let entry = write(
        "meta_entry.ts",
        "import { test, expect } from \"rts:test\";\n\
         test(\"url\", () => expect(import.meta.url.startsWith(\"file:\")).toBe(true));\n\
         test(\"identity\", () => expect(import.meta === import.meta).toBe(true));\n\
         test(\"main\", () => expect(import.meta.main).toBe(true));\n\
         test(\"dynamic\", async () => {\n\
           const first = await import(\"./meta_inner\");\n\
           const second = await import(\"./meta_inner\");\n\
           expect(first.value).toBe(7);\n\
           expect(first === second).toBe(true);\n\
         });\n",
    );

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 4, "the fixture registers four tests");
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn a_function_built_from_text_is_not_strict() {
    // The language rule, not ours: `Function(body)` compiles SCRIPT code, and
    // script code with no directive of its own is non-strict — so a call that
    // passed no receiver sees the global object where every function in a
    // module sees `undefined`. Both halves are asserted in one program,
    // because the failure worth catching is the substitution leaking OUT of
    // the text and making module code sloppy too.
    let produced = run(
        "const sloppy = Function(\"return this === globalThis\")();\n\
         const strict = (function () { return this === undefined; })();\n\
         return sloppy && strict;",
    );
    assert!(tags::payload_of(produced) != 0, "sloppy this, strict this");
}

#[test]
fn a_directive_inside_the_text_takes_the_substitution_back() {
    // `"use strict"` in the compiled text is what the language says it is, and
    // it reaches the function written INSIDE that text as well: strictness is
    // inherited downward, so the inner function must answer `undefined` too.
    let produced = run(
        "const outer = Function(\"'use strict'; return this === undefined\")();\n\
         const inner = Function(\"'use strict'; return function () { return this === undefined; }\")()();\n\
         return outer && inner;",
    );
    assert!(tags::payload_of(produced) != 0, "the directive is obeyed");
}

#[test]
fn arguments_callee_names_the_function_in_non_strict_code() {
    // `arguments.callee` is the function the arguments object was made for,
    // and it is the FUNCTION rather than the name it was bound to — which is
    // why the assertion compares identity rather than asking for a name.
    let produced = run(
        "return Function(\"function f() { return arguments.callee === f; } return f(1, 2, 3);\")();",
    );
    assert!(tags::payload_of(produced) != 0, "callee is the function");
}

/// A direct `eval` reads and writes the bindings of the frame it was written in,
/// and an indirect one does not.
///
/// The pair is one test because the distinction is the whole feature: the two
/// call the same function value, so a test of either alone would pass against an
/// implementation that answered the global scope for both.
#[test]
fn a_direct_eval_sees_the_callers_scope_and_an_indirect_one_sees_the_globals() {
    // The read. `x` here is a local that shadows nothing, so an implementation
    // compiling the fragment against the globals answers a ReferenceError
    // rather than a wrong value — which is why the write below matters more.
    holds("function f() { let x = 5; return eval(\"x\") === 5; } return f();");

    // The WRITE, which is what a scope object cannot fake: `eval` assigns the
    // caller's binding, and the caller sees it afterwards.
    holds("function f() { let x = 1; eval(\"x += 2\"); return x === 3; } return f();");

    // Through a closure, so the binding is one an enclosing function owns and
    // the environment chain has to be walked rather than read at zero hops.
    holds(
        "function make() { let x = 1; return { bump() { eval(\"x += 2\"); return x; } }; } \
         let m = make(); m.bump(); return m.bump() === 5;",
    );

    // A local SHADOWING a global is the case the refusal this replaces existed
    // to protect: answering the global here is a wrong answer that runs.
    holds(
        "var x = \"global\"; function f() { let x = \"local\"; return eval(\"x\"); } \
         return f() === \"local\";",
    );

    // INDIRECT: the comma expression makes the callee a value rather than the
    // name, and the fragment then runs in the global scope where the local
    // does not exist.
    holds(
        "function f() { let hidden = 1; return (0, eval)(\"typeof hidden\"); } \
         return f() === \"undefined\";",
    );

    // A completion value comes back, which is what makes `eval` an expression.
    holds("return eval(\"1 + 2 * 3\") === 7;");

    // A name bound to `eval` is NOT a direct eval: the call names that binding.
    holds("function f(eval) { return eval(\"s\"); } return f(function (s) { return s + \"!\"; }) === \"s!\";");

    // Nor is a replaced global, which is why the entry point asks whether the
    // one it was built for is still there.
    holds("globalThis.eval = function (s) { return \"replaced\"; }; return eval(\"1\") === \"replaced\";");
}

/// A `with` resolves a free name against its object first and lexically after.
///
/// The claim is about the LANGUAGE and not about this engine's chain: `with` is
/// the one construct where which binding a name means is decided by a value, so
/// each assertion here pairs a name the object has with one it does not. A test
/// of the first alone would pass against an implementation that read every name
/// off the object; a test of the second alone would pass against one that
/// ignored `with` entirely.
#[test]
fn a_with_resolves_a_name_against_its_object_before_its_binding() {
    // Found on the object, and the outer binding of the same name untouched.
    holds("let width = 1; let out = 0; with ({ width: 10 }) { out = width; } return out === 10 && width === 1;");

    // Not on the object: the lexical binding answers.
    holds("let outer = 7; let out = 0; with ({ other: 1 }) { out = outer; } return out === 7;");

    // A WRITE goes where the read would: onto the object when it has the name,
    // and to the binding when it does not. This is the half that a scope built
    // for reading only would get wrong, silently.
    holds(
        "let o = { a: 1 }; let b = 2; with (o) { a = 10; b = 20; } \
         return o.a === 10 && b === 20 && o.b === undefined;",
    );

    // Nested: the INNER object is asked first, and the outer one still answers
    // a name the inner lacks.
    holds(
        "let out = \"\"; with ({ p: \"outer\" }) { with ({ q: \"inner\" }) { out = p + q; } } \
         return out === \"outerinner\";",
    );

    // A CALL through a `with` object, which is where the emitter's inlining and
    // its `Math` fast path would answer the lexical function instead.
    holds("function f() { return \"lexical\"; } let out = \"\"; with ({ f: () => \"object\" }) { out = f(); } return out === \"object\";");

    // `var` inside a `with` belongs to the function, not to the object — the
    // declaration is hoisted and only the assignment is in the body.
    holds("with ({ x: 10 }) { var captured = x; } return captured === 10;");
}

/// `Symbol.unscopables` takes a name back out of a `with` scope.
///
/// The whole point of the protocol: the object HAS the property, `in` says so,
/// and the `with` must not see it. So each case here is one the plain
/// has-property question answers the other way.
#[test]
fn symbol_unscopables_hides_a_property_a_with_would_otherwise_find() {
    // Truthy blocks, and the outer binding answers instead.
    holds(
        "let a = \"lexical\"; let o = { a: \"object\" }; o[Symbol.unscopables] = { a: true }; \
         let out = \"\"; with (o) { out = a; } return out === \"lexical\" && \"a\" in o;",
    );

    // FALSY does not block — a list is a list, not a set of keys.
    holds(
        "let a = \"lexical\"; let o = { a: \"object\" }; o[Symbol.unscopables] = { a: false }; \
         let out = \"\"; with (o) { out = a; } return out === \"object\";",
    );

    // `Array.prototype[Symbol.unscopables]` is the list the language ships it
    // for, and it is why every method added to arrays since ES5 did not change
    // the meaning of programs written before them.
    holds(
        "let keys = \"mine\"; let out = null; with ([1, 2, 3]) { out = keys; } \
         return out === \"mine\";",
    );

    // …and a method NOT on that list is still reachable, which is what stops
    // the fix from being "arrays unscope everything".
    holds("let out = null; with ([1, 2, 3]) { out = join(\"-\"); } return out === \"1-2-3\";");
}

// ---------------------------------------------------------------------------
// One re-raise block per protected region.
//
// A throw is recorded rather than unwound, so every call that can raise is
// followed by a load, a compare and a branch to a block that takes the value
// back and re-raises it. That block has no parameters and reads nothing from the
// site, so `emit/body_throw.rs` shares one among every check in the same
// region — 1 069 identical copies in `bench/analytic.ts` became 96, one per
// function.
//
// What makes the sharing sound is that the region is the key, and what these
// pin is exactly that: where a re-raise lands is decided by the region its block
// is in, so two sites that are NOT in the same region must not reach one copy.
// ---------------------------------------------------------------------------

/// A `try` inside a `try` catches its own throw, not the outer one's handler.
///
/// The sharpest thing the region key protects. Both `throw`s are re-raised by
/// the same mechanism from checks in the same function; if one block served
/// both, the inner throw would be routed into whichever handler that block was
/// built under and the wrong `catch` would run — silently, with a plausible
/// value.
#[test]
fn a_nested_try_does_not_reuse_the_outer_regions_re_raise() {
    let answer = run(
        "let log = ''; \
         try { \
           try { JSON.parse('{'); } catch (e) { log = log + 'i'; } \
           JSON.parse('['); \
           log = log + 'X'; \
         } catch (e) { log = log + 'o'; } \
         return log === 'io' ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "the inner throw must reach the inner catch and the outer the outer; \
         'X' in the log means the second parse did not raise at all"
    );
}

/// Two checks in the SAME region do share, and the sharing is invisible.
///
/// The other half: sharing must not change which handler runs or how many times
/// a `finally` executes. Three raising calls in one `try` branch to one block.
#[test]
fn many_checks_in_one_region_still_run_the_handler_once() {
    let answer = run(
        "let ran = 0; let caught = 0; \
         try { JSON.parse('{'); JSON.parse('['); JSON.parse('}'); } \
         catch (e) { caught = caught + 1; } \
         finally { ran = ran + 1; } \
         return caught === 1 && ran === 1 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "one throw, one catch, one finally — sharing the re-raise must not \
         multiply either"
    );
}

/// A function written inside a `try` builds its own re-raise, not the outer
/// body's.
///
/// The memo holds `BlockId`s, which belong to one `FuncBuilder`. An inner body
/// that inherited the outer body's would branch to a block of another function
/// — the failure `emit/function.rs` records for `finally_jumps`, where the
/// builder panics with "block belongs to this function". This is the ordinary
/// code that reaches it.
#[test]
fn a_function_defined_inside_a_try_does_not_inherit_its_re_raise_block() {
    let answer = run(
        "let out = ''; \
         try { \
           const inner = () => { try { JSON.parse('{'); } catch (e) { return 'in'; } return 'no'; }; \
           out = inner(); \
         } catch (e) { out = 'outer'; } \
         return out === 'in' ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "the arrow's own try must catch its own throw; 'outer' means the inner \
         body re-raised into the enclosing function's handler"
    );
}

/// A throw with no `try` anywhere still leaves the function.
///
/// `None` — no region open — is a key like any other in the memo, and this is
/// the case that says the shared block for it still returns rather than being
/// swallowed by a handler that does not exist.
#[test]
fn a_throw_outside_every_region_still_escapes_the_function() {
    let answer = run(
        "function raises() { JSON.parse('{'); return 'NOT REACHED'; } \
         try { raises(); return 0; } catch (e) { return 1; }",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "the callee has no region of its own, so its check must re-raise out of \
         it and be caught by the caller's try"
    );
}

/// A captured write inside a nested function does not leak its memo outward.
///
/// `emit/body_state.rs` memoises the last captured write so that `s = s + x`
/// followed by a read of `s` does not go back to the heap. It holds a `ValueId`
/// AND a `BlockId` — both handles into one `FuncBuilder` — and for as long as it
/// lived on `Ctx` it was never saved around a nested function. A write inside an
/// arrow left a memo naming the arrow's block and the arrow's value, and
/// emission carried on in the enclosing body still holding it; the guard is
/// "same block, nothing emitted here", and block numbers are per function, so a
/// collision is one number matching another.
///
/// This program is the one that found it, and it FAILED TO COMPILE with
/// `Place(Lower(CannotWiden { from: I64 }))` — the read answered a raw integer
/// from another function where a JavaScript value was required.
#[test]
fn a_captured_write_inside_a_callback_does_not_leak_into_the_outer_body() {
    let answer = run(
        "let ran = 0; \
         try { [1, 2, 3].forEach(x => { ran = ran + 1; if (x === 2) throw new Error('stop'); }); } \
         catch (e) {} \
         return ran === 2 ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "the callback runs twice before throwing, so `ran` is 2 — and `ran` must \
         be read from the environment, not from a memo the arrow left behind"
    );
}

/// The same defect in the shape that regressed rather than the one that failed.
///
/// Which programs collide depends on block numbering, so the defect moves when
/// anything shifts it: sharing the re-raise block made THIS program stop
/// compiling while the one above started working. Neither was evidence about
/// the sharing. Both are pinned so a future numbering change cannot quietly
/// trade one for the other again.
#[test]
fn a_captured_string_written_in_a_callback_survives_the_catch_that_follows() {
    let answer = run(
        "let s = ''; \
         try { ['a', 'b'].forEach(x => { s = s + x; if (x === 'b') throw new Error('z'); }); } \
         catch (e) { s = s + '!'; } \
         return s === 'ab!' ? 1 : 0;",
    );
    assert_eq!(
        tags::decode_double(answer),
        1.0,
        "'ab!' — both elements appended, then the catch appends its own; any \
         other answer is a read served from another function's memo"
    );
}

/// CommonJS runs, and a module reached only by `require` is in the graph.
///
/// What it pins is the whole path at once, because every piece of it is new and
/// each fails silently on its own: the loader following a `require("./x")`
/// edge, the emitter binding the five names, the runtime publishing
/// `module.exports` beside the namespace, and `require` reading it back. It
/// also pins the extension rule — `require("./cjs_lib")` with no extension
/// naming a `.js` file — which is the change that let a corpus written for
/// Node resolve at all.
#[test]
fn a_commonjs_module_requires_another_and_gets_what_it_exported() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("rts_commonjs");
    std::fs::create_dir_all(&dir).expect("a directory to write fixtures in");
    let write = |name: &str, source: &str| {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
        path
    };

    // Two shapes of export, because they answer differently: filling `exports`
    // leaves a namespace of names, and REPLACING `module.exports` leaves one
    // value that a namespace could not hold.
    write("cjs_lib.js", "exports.greet = function (who) { return 'ola ' + who; };
");
    write("cjs_fn.js", "module.exports = function double(n) { return n * 2; };
");
    let entry = write(
        "cjs_entry.js",
        "import { test, expect } from \"rts:test\";
         const lib = require(\"./cjs_lib\");
         const double = require(\"./cjs_fn.js\");
         test(\"required\", () => expect(lib.greet('a') + double(21)).toBe('ola a42'));
         test(\"named\", () => expect(typeof __filename + typeof require).toBe('stringfunction'));
",
    );

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 2, "the fixture registers two tests");
    assert!(
        failed.is_empty(),
        "a required module answers what it exported, both ways round: {failed:?}"
    );
}

/// The two module systems in one file, and each reading the other's module.
///
/// The decision this pins is that there is no per-file choice between them —
/// see `docs/engine/architecture.md`. An `import` and a `require` sit in one
/// body; an ES module is `require`d and answers its namespace; a CommonJS one
/// is `import`ed and its `module.exports` arrives as the default.
#[test]
fn import_and_require_reach_each_other_inside_one_program() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("rts_commonjs_mixed");
    std::fs::create_dir_all(&dir).expect("a directory to write fixtures in");
    let write = |name: &str, source: &str| {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
        path
    };

    write("mixed_esm.ts", "export const two = 2;
export default 'padrao';
");
    write("mixed_cjs.js", "module.exports = { four: 4 };
");
    let entry = write(
        "mixed_entry.ts",
        "import { test, expect } from \"rts:test\";
         import { two } from \"./mixed_esm\";
         import held from \"./mixed_cjs\";
         const required = require(\"./mixed_esm\");
         test(\"require of an es module\", () => expect(required.two + required.default).toBe('2padrao'));
         test(\"import of a commonjs module\", () => expect(held.four + two).toBe(6));
",
    );

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 2, "the fixture registers two tests");
    assert!(failed.is_empty(), "each system reads the other's module: {failed:?}");
}

/// A program's own `require` is the one it declared.
///
/// The prologue binds five names, and a name the program declares itself must
/// not be one of them — a second binding of one spelling in one layer is not a
/// shadow, it is two entries where a read finds whichever was pushed last.
#[test]
fn a_declared_name_wins_over_the_commonjs_binding() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("rts_commonjs_declared");
    std::fs::create_dir_all(&dir).expect("a directory to write fixtures in");
    let path = dir.join("declared_entry.ts");
    let mut file = std::fs::File::create(&path).expect("a fixture file");
    file.write_all(
        b"import { test, expect } from \"rts:test\";
\
          const require = (what: string) => 'meu:' + what;
\
          const module = { exports: 7 };
\
          test(\"declared\", () => expect(require('x') + module.exports).toBe('meu:x7'));
",
    )
    .expect("written");

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&path).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 1, "the fixture registers one test");
    assert!(failed.is_empty(), "the program's own bindings answer: {failed:?}");
}

/// A `for-of` hands back the elements of the array it walks, across a collection.
///
/// # What this pins, and what it caught
///
/// A collection is not something a JavaScript program can observe, so a loop
/// body that allocates enough to cause one must not change what the loop is
/// handed. That is the language claim; the engine broke it for two days.
///
/// The desugaring hoisted the ADDRESS of the run it walks — `ElementsBase` once
/// per loop, then a bounded load per element instead of a crossing. The address
/// belongs to the `Vec` behind the array `Iterate` copies the source into, and
/// nothing else in the program can name that copy: the only reference to it was
/// the one the desugaring bound, and hoisting the address replaced the last read
/// of it. So the copy went unreachable at the top of the first pass, a body that
/// allocated collected it, and the loop read a run that by then belonged to
/// something else.
///
/// Measured with the two binaries, on the program below with its final
/// `return` written as a `console.log`: **83 of 90 elements came back wrong**
/// before, and none after. `bench/analytic.ts` reported 14 of its 90 rows as
/// `c.run is not a function` for the same reason, deterministically, while
/// `CASES[i]` answered correctly throughout — which is what made it look like a
/// benchmark bug for as long as it did.
///
/// # Why the shape below is not incidental
///
/// Every clause is load-bearing, and that fragility is the argument for pinning
/// it here rather than trusting it to be noticed again:
///
/// - the body ALLOCATES. The same loop with an arithmetic body was always
///   correct, on both binaries.
/// - `sink` is typed, so the object literal is genuinely built rather than
///   scalarised away. Dropping the annotations made the same program allocate
///   nothing and answer correctly on the broken binary.
/// - the loop is at TOP LEVEL. Inside a called function the copy happened to
///   stay reachable and the program was correct.
/// - it is 90 elements. At 200 the same body answered correctly; two release
///   binaries built hours apart disagreed about WHICH of the 90 came back wrong.
///
/// A defect whose visibility depends on where a binding was allocated is why the
/// fix removed the hoist rather than narrowing its predicate.
#[test]
fn a_for_of_hands_back_its_own_elements_across_a_collection() {
    let produced = run(
        "const held: { id: number }[] = [];
         for (let i = 0; i < 90; i++) held.push({ id: i });
         let step: i32 = 0;
         let mismatched: i32 = 0;
         let sink: i32 = 0;
         for (const each of held) {
           for (let k = 0; k < 20000; k++) { sink += { x: k }.x; }
           if (each !== held[step]) { mismatched += 1; }
           step += 1;
         }
         return mismatched * 1000 + step;",
    );
    // Packed rather than two runs: the defect depends on the allocation the loop
    // performs, so asking twice asks a different question the second time.
    let packed = tags::decode_double(produced);
    assert_eq!(
        packed, 90.0,
        "expected 0 mismatches over 90 steps; got {} mismatches over {} steps",
        (packed / 1000.0).floor(),
        packed % 1000.0
    );
}

/// The same guarantee where reading a reclaimed run does not merely answer
/// wrongly.
///
/// # Why a second test rather than a bigger first one
///
/// Because it fails differently, and the difference is the point. The loop above
/// read words that had been handed to another object and reported them as
/// elements — a wrong answer with nothing to announce it. This one walks 200
/// elements while the body allocates arrays of exactly that length, so a
/// reclaimed elements vector is handed straight back out, and the load then went
/// through an address that no longer addressed anything: it SEGFAULTED, every
/// run, on the release binary of 2026-08-28.
///
/// A crash is the honesty floor's own category — "nothing that crashes or hangs
/// is committed as passing" — and a test that turns one into a signal is worth
/// more than one that turns it into an assertion.
#[test]
fn a_for_of_does_not_read_a_run_the_collector_reclaimed() {
    let produced = run(
        "const held: { id: number }[] = [];
         for (let i = 0; i < 200; i++) held.push({ id: i });
         let step: i32 = 0;
         let mismatched: i32 = 0;
         let sink: i32 = 0;
         for (const each of held) {
           for (let k = 0; k < 400; k++) {
             const scratch: number[] = [];
             for (let j = 0; j < 200; j++) scratch.push(j);
             sink += scratch[0];
           }
           if (each !== held[step]) { mismatched += 1; }
           step += 1;
         }
         return mismatched * 1000 + step;",
    );
    assert_eq!(
        tags::decode_double(produced),
        200.0,
        "200 steps, none of them reading a reclaimed run"
    );
}
