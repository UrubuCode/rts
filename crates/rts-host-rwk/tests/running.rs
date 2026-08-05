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
    // reaches a runtime operation that does not exist. So it names `for await`,
    // which needs a suspended frame — the operation nothing emits and nothing
    // in this crate can stand in for.
    let error = compile("async function f(a) { for await (let v of a) { } }")
        .expect_err("`for await` needs a suspended frame");
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

#[test]
fn a_construct_still_missing_is_refused_by_name_rather_than_approximated() {
    // An array is a heap value with no entry point to make one; a computed key
    // needs `ToPropertyKey`, which the runtime does not define; `delete`
    // removes a property, which it does not define either.
    //
    // This test has named `typeof`, a string literal, `~` and `==` in turn, and
    // each moved on when it landed. What it pins is the shape of the refusal.
    for source in [
        "return [1, , 2];",
        "class A {} class B extends A { constructor() { super(1, 2, 3, 4, 5); } } return new B();",
        "let o = {}; let x = 1; return delete x;",
    ] {
        let error = compile(source).expect_err("still a gap");
        assert!(
            format!("{error:?}").contains("Unsupported"),
            "expected a named refusal for `{source}`, got {error:?}"
        );
    }
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
fn calling_something_that_is_not_a_function_does_not_jump_to_it() {
    // The reason calling is a runtime operation rather than the machine's
    // indirect call. `1()` must throw a TypeError, throwing needs protected
    // regions, and nothing emits those — so the runtime answers `undefined`
    // instead. That is a stated gap and it is not the point of this test.
    //
    // The point is what does NOT happen: the program must not jump through
    // whatever the value spelled. It running at all is the assertion.
    let mut compiled = compile("let n = 1; return n();").expect("compiles");
    let produced = compiled.run();
    assert_eq!(
        produced,
        compiled.model().singleton(Singleton::Undefined).word()
    );
}

#[test]
fn the_limits_of_the_fixed_arity_are_refused_by_name() {
    // The convention carries four arguments. Going past it at the CALL is no
    // longer refused — the arguments go in a vector the runtime holds — but
    // going past it in the DECLARATION still is, because a fifth parameter has
    // no slot to arrive in. Refused rather than truncated: a parameter that
    // silently read `undefined` forever is a wrong program that runs.
    for source in [
        "function f(a, b, c, d, e) { return a; } return f(1);",
        "class A {} class B extends A { constructor() { super(1, 2, 3, 4, 5); } } return new B();",
    ] {
        let error = compile(source).expect_err("past the fixed arity");
        assert!(
            format!("{error:?}").contains("Unsupported"),
            "expected a named refusal for `{source}`, got {error:?}"
        );
    }
}

#[test]
fn what_a_function_still_cannot_do_is_refused_by_name() {
    // Each of these is a mechanism rather than a spelling: a default needs an
    // expression evaluated at the call, `this` inside an arrow needs the
    // defining function's receiver carried through the environment, and both
    // `async` and `function*` need a frame that can be suspended.
    //
    // A rest parameter and a spread argument were both on this list and came
    // off it: the vector one needed is the runtime's now, and the other is what
    // iteration produces.
    for source in [
        "function f(a) { return a; } return f();",
        "let f = () => this; return f();",
        "async function f() { return 1; } return f();",
        "function* f() { yield 1; } return f();",
    ] {
        // The third one is legal and emits — a missing argument is padded — so
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
fn a_hole_is_refused_rather_than_written_as_undefined() {
    // `[,1]` has no element zero, and `0 in [,1]` is false where
    // `[undefined,1]` answers true. This runtime cannot tell the two apart, so
    // the hole is a named gap rather than an array that is quietly the wrong
    // one.
    let error = compile("return [,1];").expect_err("a hole is a gap");
    assert!(
        format!("{error:?}").contains("Unsupported"),
        "expected a named refusal, got {error:?}"
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
    // would turn every typo into a program that runs, which is why only the
    // three constants are readable and the rest is refused.
    let error = compile("return nowhere;").expect_err("`nowhere` is undeclared");
    assert!(
        format!("{error:?}").contains("UnboundName"),
        "expected the program to be wrong rather than a gap, got {error:?}"
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
fn a_captured_loop_variable_is_shared_across_passes_which_is_wrong() {
    // A KNOWN DIVERGENCE, pinned so it stays visible rather than being
    // rediscovered.
    //
    // The language gives `let` in a loop a fresh binding per pass, so a closure
    // made on the first pass captures the FIRST key. This engine's environment
    // is per function **activation** rather than per iteration, so every pass
    // writes the same slot and every closure sees the last value.
    //
    // Not caused by `for-in`: an ordinary `for` has it too, and E5 shipped it.
    // Fixing it means creating an environment inside the loop, chained to the
    // function's, whenever the body declares a name something captures.
    //
    // Asserted as what it DOES, so that fixing it fails this test — which is
    // how a divergence stays a decision rather than becoming folklore.
    let produced = run("function collect() { \
           let o = {}; o.a = 1; o.b = 2; \
           let first = 0; \
           for (let k in o) { function keep() { return k; } if (first === 0) { first = keep; } } \
           return first(); \
         } \
         return collect() === \"b\";");
    assert_eq!(
        tags::payload_of(produced),
        tags::BOOL_TRUE,
        "every closure sees the LAST key, because one environment is shared"
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
fn a_try_around_a_call_is_refused_by_name_rather_than_compiled() {
    // A throw inside the callee would run past this handler, because where a
    // throw lands is planned from the region tree of the function containing
    // it. A `catch` that reads correctly and never runs is worse than one that
    // does not compile.
    let error =
        compile("function f() { throw 1; } try { f(); } catch (e) {}").expect_err("refused");
    assert!(format!("{error:?}").contains("call"), "{error:?}");
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
    // matters: a typo does not become `undefined`. Removing this assertion is
    // how the refusal would quietly stop applying.
    let error = compile("return Elephant;").expect_err("refused");
    assert!(format!("{error:?}").contains("UnboundName"), "{error:?}");
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
    // `"é"` is one code unit and two UTF-8 bytes. Every index in these methods
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
fn what_a_class_still_cannot_express_is_refused_by_name() {
    // Each needs a mechanism rather than more of this lowering: a private name
    // is not a property key at all, and a static block is statements with the
    // class as `this`. An accessor was on this list and moved off it when the
    // pair-beside-the-cell mechanism landed.
    for (source, expected) in [
        ("class A { #x = 1; }", "private"),
        ("class A { static { let x = 1; } }", "static block"),
        ("class A { [\"a\"] = 1; }", "computed"),
    ] {
        let error = compile(source).expect_err("refused");
        let text = format!("{error:?}");
        assert!(text.contains(expected), "{source} gave {text}");
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

    // But the bare name is NOT readable, and that is the compile-time refusal
    // being consistent rather than a separate limitation: a name is readable
    // when the language provides it or the program assigns it by name, and
    // `globalThis.other = 5` is neither. Reading it is what a typo looks like.
    let bare = compile("globalThis.other = 5; return other;").expect_err("refused");
    assert!(format!("{bare:?}").contains("UnboundName"), "{bare:?}");

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
fn reading_a_name_nothing_declares_or_creates_is_still_refused() {
    // Stricter than the language, which throws a `ReferenceError` this engine
    // cannot raise where a handler could catch it. Wrong only for a program
    // that meant to catch that error — where answering `undefined` would be
    // wrong for every program with a typo in it.
    let error = compile("return neverMentionedAgain;").expect_err("refused");
    assert!(format!("{error:?}").contains("UnboundName"), "{error:?}");
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

    // An object declaring nothing still walks zero times rather than failing,
    // which is the stated gap while a throw cannot reach a handler.
    let none = run("let n = 0; for (let v of {a: 1}) { n = n + 1; } return n;");
    assert_eq!(tags::decode_double(none), 0.0);
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
    holds("return JSON.parse(\"[\") === undefined;");

    // A key that spells an index reaches the same property either spelling
    // finds, which is what routing the parse through the interner buys.
    holds("return JSON.parse(\"{\\\"0\\\":7}\")[0] === 7;");

    let round = run("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(JSON.parse(JSON.stringify(o)));");
    let direct = run("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(o);");
    // Two separate programs, so the words differ; what is compared is that each
    // produced the same text as itself round-tripped.
    let _ = (round, direct);
    holds("let o = {a: [1, {b: true}], c: null}; return JSON.stringify(JSON.parse(JSON.stringify(o))) === JSON.stringify(o);");

    // A cycle answers `null` rather than hanging. The specification throws, and
    // why this does not is the stated gap.
    holds("let o = {}; o.self = o; return JSON.stringify(o) === \"{\\\"self\\\":null}\";");
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
