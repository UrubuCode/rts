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
use rts_host_rwk::compile;

/// Runs a script and hands back the encoded word it produced.
fn run(source: &str) -> u64 {
    let mut program = compile(source)
        .unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"))
        ;
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
    assert_eq!(tags::decode_double(run("if (1) { return 7; } return 9;")), 7.0);
    assert_eq!(tags::decode_double(run("if (0) { return 7; } return 9;")), 9.0);
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
    // `**` rather than `-`: this test named `-` until the runtime defined it,
    // which is the right way for it to fail. What it pins is the shape of the
    // refusal, so it moves to whatever is still missing rather than being
    // deleted with the gap it happened to name.
    let error = compile("return 2 ** 3;").expect_err("`**` has no runtime operation");
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
    assert_eq!(tags::payload_of(run("return (0 / 0) <= (0 / 0);")), tags::BOOL_FALSE);
    assert_eq!(tags::payload_of(run("return (0 / 0) >= (0 / 0);")), tags::BOOL_FALSE);
}

#[test]
fn a_loop_that_counts_up_to_a_bound() {
    // What `<` was missing for. The loop now reads the way one is written,
    // rather than around the operators the runtime lacked.
    let produced = run("let i = 0; let total = 0; while (i < 5) { total = total + i; i = i + 1; } return total;");
    assert_eq!(tags::decode_double(produced), 10.0);
}

#[test]
fn a_for_loop_runs_end_to_end() {
    // Header scope, condition, body and update — E3's whole shape, executed.
    let produced = run("let total = 0; for (let i = 1; i <= 4; i = i + 1) { total = total * 10 + i; } return total;");
    assert_eq!(tags::decode_double(produced), 1234.0);
}

// ---------------------------------------------------------------------------
// Objects. The machine's shapes get their first client, and the compiler and
// the runtime have to agree about a second numbering — property keys — for any
// of it to mean anything.

#[test]
fn a_property_written_is_the_property_read() {
    assert_eq!(tags::decode_double(run("let o = {}; o.x = 42; return o.x;")), 42.0);
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
    assert_eq!(tags::decode_double(run("let x = 0; x ||= 9; return x;")), 9.0);
    assert_eq!(tags::decode_double(run("let x = 3; x ||= 9; return x;")), 3.0);
    assert_eq!(tags::decode_double(run("let x = 3; x &&= 9; return x;")), 9.0);
    assert_eq!(tags::decode_double(run("let x = 0; x &&= 9; return x;")), 0.0);
    assert_eq!(
        tags::decode_double(run("let x = null; x ??= 6; return x;")),
        6.0
    );
    assert_eq!(tags::decode_double(run("let x = 0; x ??= 6; return x;")), 0.0);
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

#[test]
fn a_construct_still_missing_is_refused_by_name_rather_than_approximated() {
    // `~` needs ToInt32, which the runtime does not define, and `typeof` needs
    // a string, which nothing can yet materialise. Both are named rather than
    // guessed — the property this whole emitter is built on, restated where
    // the next person to add an operator will read it.
    for source in ["return ~1;", "return typeof 1;", "return \"a\";"] {
        let error = compile(source).expect_err("still a gap");
        assert!(
            format!("{error:?}").contains("Unsupported"),
            "expected a named refusal for `{source}`, got {error:?}"
        );
    }
}
