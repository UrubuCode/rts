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
    // This test has named `-`, `**`, `in` and `instanceof` in turn, and each
    // moved on when the runtime defined it — which is the right way for it to
    // fail. What it pins is the SHAPE of the refusal, so it follows whatever is
    // still missing rather than being deleted with the gap it happened to name.
    //
    // Nothing is left in its original category: no operator the language spells
    // reaches a runtime operation that does not exist. So it names `for-of`,
    // which needs the iterator protocol — a call to user code from inside the
    // enumeration, which is the thing an entry point cannot do.
    let error = compile("let a = [1]; for (let v of a) { }")
        .expect_err("`for-of` needs the iterator protocol");
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
    // An array is a heap value with no entry point to make one; a computed key
    // needs `ToPropertyKey`, which the runtime does not define; `delete`
    // removes a property, which it does not define either.
    //
    // This test has named `typeof`, a string literal, `~` and `==` in turn, and
    // each moved on when it landed. What it pins is the shape of the refusal.
    for source in [
        "let a = [1]; return [...a];",
        "class C {}",
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
        tags::decode_double(run("function add(a, b) { return a + b; } return add(2, 3);")),
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
    let produced = run(
        "function pair() { \
           let n = 0; \
           function write() { n = 41; } \
           function read() { return n + 1; } \
           write(); \
           return read(); \
         } \
         return pair();",
    );
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
    let produced = run(
        "function outer() { \
           let k = 5; \
           function middle() { \
             let m = 2; \
             function inner() { return k * m; } \
             return inner(); \
           } \
           return middle(); \
         } \
         return outer();",
    );
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
    let produced = run(
        "let o = {}; \
         o.n = 12; \
         o.get = function () { return this.n; }; \
         return o.get();",
    );
    assert_eq!(tags::decode_double(produced), 12.0);
}

#[test]
fn a_plain_call_has_no_receiver() {
    let mut compiled =
        compile("function f() { return this; } return f();").expect("compiles");
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
    let produced = run(
        "let calls = 0; \
         let o = {}; \
         o.answer = function () { return 1; }; \
         function get() { calls = calls + 1; return o; } \
         get().answer(); \
         return calls;",
    );
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
    // The convention carries four arguments, and going past it is refused
    // rather than truncated: a call whose fifth argument silently vanished is a
    // wrong program that runs. Both directions are named — too many at the call
    // and too many in the declaration.
    for source in [
        "function f(a) { return a; } return f(1, 2, 3, 4, 5);",
        "function f(a, b, c, d, e) { return a; } return f(1);",
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
    // Each of these is a mechanism rather than a spelling: a rest parameter and
    // a spread argument both need the argument vector the fixed arity exists in
    // place of, a default needs an expression evaluated at the call, and `this`
    // inside an arrow needs the defining function's receiver carried through the
    // environment.
    for source in [
        "function f(...rest) { return rest; } return f(1);",
        "function f(a) { return a; } let xs = 0; return f(...xs);",
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
    assert_eq!(tags::decode_double(run("if (\"\") { return 1; } return 2;")), 2.0);
    assert_eq!(tags::decode_double(run("if (\"x\") { return 1; } return 2;")), 1.0);
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
    let produced = run(
        "function f() { return 1; } \
         let o = {}; \
         return ((typeof f) === \"function\") && ((typeof o) === \"object\");",
    );
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
    assert_eq!(tags::decode_double(run("return 2147483648 | 0;")), -2147483648.0);
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
    assert_eq!(tags::payload_of(run("return 1 === \"1\";")), tags::BOOL_FALSE);
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
    assert_eq!(tags::payload_of(run("return 1 != \"1\";")), tags::BOOL_FALSE);
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
        tags::payload_of(run("let a = 1; let b = 2; return `a${a}b${b}c` === \"a1b2c\";")),
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
    let mut compiled =
        compile("let o = {}; return o[\"missing\"];").expect("compiles");
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
    let produced = run(
        "let log = \"\"; \
         let o = {}; \
         function a() { log = log + \"a\"; return o; } \
         function b() { log = log + \"b\"; return \"k\"; } \
         function c() { log = log + \"c\"; return 1; } \
         a()[b()] = c(); \
         return log === \"abc\";",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn a_compound_assignment_to_a_computed_key_reads_it_first() {
    assert_eq!(
        tags::decode_double(run("let o = {}; o[\"n\"] = 10; o[\"n\"] += 5; return o[\"n\"];")),
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
    let outer_left = run(
        "let count = 0; \
         outer: for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; break outer; } \
         } \
         return count;",
    );
    assert_eq!(tags::decode_double(outer_left), 1.0);

    let inner_left = run(
        "let count = 0; \
         for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; break; } \
         } \
         return count;",
    );
    assert_eq!(tags::decode_double(inner_left), 3.0);
}

#[test]
fn a_labelled_continue_resumes_the_loop_it_names() {
    // `continue outer` abandons the rest of the inner loop AND the rest of the
    // outer body, so the increment after it never runs.
    let produced = run(
        "let count = 0; \
         outer: for (let i = 0; i < 3; i++) { \
           for (let j = 0; j < 2; j++) { count++; continue outer; } \
           count = count + 100; \
         } \
         return count;",
    );
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_label_on_a_block_can_be_broken_out_of() {
    // Not a loop, so there is nothing to continue and only `break` reaches it.
    let produced = run(
        "let n = 1; \
         done: { \
           n = 2; \
           break done; \
         } \
         return n;",
    );
    assert_eq!(tags::decode_double(produced), 2.0);
}

#[test]
fn a_continue_inside_a_labelled_block_belongs_to_the_loop_around_it() {
    // The reason `Loops::target` skips a frame with nothing to continue to. A
    // search that stopped at the innermost frame would find the block, which
    // cannot be continued, and refuse a program that is legal.
    let produced = run(
        "let count = 0; \
         for (let i = 0; i < 3; i++) { \
           inner: { count++; continue; } \
         } \
         return count;",
    );
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
    let produced = run(
        "let out = 0; \
         switch (1) { \
           case 1: out = out + 1; \
           case 2: out = out + 10; break; \
           case 3: out = out + 100; \
         } \
         return out;",
    );
    assert_eq!(tags::decode_double(produced), 11.0);
}

#[test]
fn default_runs_where_it_sits_rather_than_last() {
    // `default` is not a fallback appended to the end. Nothing matches 9, so
    // control enters at `default` and falls through into the clause written
    // after it — an implementation that ran `default` last would return 1.
    let produced = run(
        "let out = 0; \
         switch (9) { \
           case 1: out = out + 1; break; \
           default: out = out + 10; \
           case 2: out = out + 100; \
         } \
         return out;",
    );
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
    let produced = run(
        "let out = 0; \
         for (let i = 0; i < 3; i++) { \
           switch (i) { case 0: out = out + 1; break; case 1: out = out + 10; break; \
                        default: out = out + 100; } \
         } \
         return out;",
    );
    assert_eq!(tags::decode_double(produced), 111.0);
}

#[test]
fn a_continue_inside_a_switch_belongs_to_the_loop_around_it() {
    // A switch takes `break` and is not a loop, so a `continue` written inside
    // one has to pass through its frame to reach the loop. The same rule a
    // labelled block follows, and the reason both record `continue_to: None`.
    let produced = run(
        "let count = 0; \
         for (let i = 0; i < 4; i++) { \
           switch (i) { case 0: continue; default: count++; } \
         } \
         return count;",
    );
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_switch_clause_can_hold_a_closure() {
    // The capture analysis had to learn about switch: a nested function inside
    // a clause is a function like any other, and a name it captures that was
    // not marked would be two closures disagreeing about a variable.
    let produced = run(
        "function make() { \
           let n = 0; \
           switch (1) { case 1: { function bump() { n = n + 5; } bump(); bump(); } } \
           return n; \
         } \
         return make();",
    );
    assert_eq!(tags::decode_double(produced), 10.0);
}

// ---------------------------------------------------------------------------
// Arrays.

#[test]
fn an_array_literal_holds_its_elements() {
    assert_eq!(tags::decode_double(run("let a = [10, 20, 30]; return a[1];")), 20.0);
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
    assert_eq!(tags::decode_double(run("let a = []; a[2] = 1; return a[2];")), 1.0);
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
    let produced = run(
        "let a = [1, 2, 3, 4]; \
         let total = 0; \
         for (let i = 0; i < a.length; i++) { total = total + a[i]; } \
         return total;",
    );
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
    let produced = run(
        "let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         delete o.b; \
         return o.a + o.c;",
    );
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
        tags::payload_of(run("let o = {}; o.x = 1; let k = \"x\"; delete o[k]; return \"x\" in o;")),
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
    assert_eq!(tags::decode_double(run("let a = 3; let o = { a }; return o.a;")), 3.0);
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
    let produced = run(
        "let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         let total = 0; \
         for (let k in o) { total = total + o[k]; } \
         return total;",
    );
    assert_eq!(tags::decode_double(produced), 6.0);
}

#[test]
fn for_in_yields_keys_as_strings() {
    // Even for an array index. `for (k in [1,2])` yields "0" and "1", so a body
    // comparing `k === 0` finds nothing — which is the language rather than a
    // quirk of this implementation, and the reason the test asserts the string.
    let produced = run(
        "let a = [7, 8]; \
         let joined = \"\"; \
         for (let k in a) { joined = joined + k; } \
         return joined === \"01\";",
    );
    assert_eq!(tags::payload_of(produced), tags::BOOL_TRUE);
}

#[test]
fn for_in_visits_them_in_the_order_they_were_added() {
    let produced = run(
        "let o = {}; o.z = 1; o.a = 2; o.m = 3; \
         let joined = \"\"; \
         for (let k in o) { joined = joined + k; } \
         return joined === \"zam\";",
    );
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
    let produced = run(
        "let o = {}; o.a = 1; o.b = 2; o.c = 3; \
         let count = 0; \
         for (let k in o) { if (k === \"b\") { continue; } count++; } \
         return count;",
    );
    assert_eq!(tags::decode_double(produced), 2.0);

    let produced = run(
        "let o = {}; o.a = 1; o.b = 2; \
         let count = 0; \
         outer: for (let k in o) { for (let j in o) { count++; break outer; } } \
         return count;",
    );
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
    let produced = run(
        "function collect() { \
           let o = {}; o.a = 1; o.b = 2; \
           let first = 0; \
           for (let k in o) { function keep() { return k; } if (first === 0) { first = keep; } } \
           return first(); \
         } \
         return collect() === \"b\";",
    );
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
    let produced = run(
        "function f() { \
           let n = 0; \
           function g() { return n; } \
           while (n < 3) { n = n + 1; } \
           return g(); \
         } \
         return f();",
    );
    assert_eq!(tags::decode_double(produced), 3.0);
}

#[test]
fn a_captured_name_assigned_in_a_switch_or_a_labelled_block_is_the_same_case() {
    // The other two constructs that carry names across a join, for the same
    // reason and through the same `plan`.
    let produced = run(
        "function f() { \
           let n = 0; \
           function g() { return n; } \
           switch (1) { case 1: n = 7; } \
           return g(); \
         } \
         return f();",
    );
    assert_eq!(tags::decode_double(produced), 7.0);

    let produced = run(
        "function f() { \
           let n = 0; \
           function g() { return n; } \
           done: { n = 9; break done; } \
           return g(); \
         } \
         return f();",
    );
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
    let produced = run("let s = 0; let a = [1, 2, 3]; for (let k in a) { s = s + a[k]; } return s;");
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
    assert_eq!(tags::decode_double(run("let a = [1, 2, 3]; return a.length;")), 3.0);
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
    let produced = run(
        "let o = {}; \
         o.a=1; o.b=2; o.c=3; o.d=4; o.e=5; o.f=6; o.g=7; o.h=8; o.i=9; o.j=10; o.k=11; o.l=12; \
         return o.a+o.b+o.c+o.d+o.e+o.f+o.g+o.h+o.i+o.j+o.k+o.l;",
    );
    assert_eq!(tags::decode_double(produced), 78.0);

    // The eighth on its own, which is the one that was undefined.
    let produced = run(
        "let o = {}; o.a=1;o.b=2;o.c=3;o.d=4;o.e=5;o.f=6;o.g=7;o.h=8; return o.h;",
    );
    assert_eq!(tags::decode_double(produced), 8.0);
}

#[test]
fn deleting_a_property_reshuffles_across_the_spill_too() {
    // `delete` reads every survivor against the old layout and writes it back
    // against the new. With more than seven properties some of those reads and
    // writes cross between the cell and the spill, in both directions — which
    // is why the slot accessor is one function rather than a check at each of
    // the four call sites.
    let produced = run(
        "let o = {}; \
         o.a=1; o.b=2; o.c=3; o.d=4; o.e=5; o.f=6; o.g=7; o.h=8; o.i=9; \
         delete o.a; \
         return o.h + o.i + o.b;",
    );
    assert_eq!(tags::decode_double(produced), 19.0);
}

#[test]
fn enumeration_sees_the_spilled_properties() {
    let produced = run(
        "let o = {}; \
         o.a=1;o.b=2;o.c=3;o.d=4;o.e=5;o.f=6;o.g=7;o.h=8; \
         let n = 0; for (let k in o) { n++; } return n;",
    );
    assert_eq!(tags::decode_double(produced), 8.0);
}

// ---------------------------------------------------------------------------
// Reading a property of a string.

#[test]
fn a_string_has_a_length() {
    assert_eq!(tags::decode_double(run("let s = \"abc\"; return s.length;")), 3.0);
    assert_eq!(tags::decode_double(run("return \"\".length;")), 0.0);
    // Counted in code units, not characters: a JavaScript string IS a sequence
    // of UTF-16 code units, so an astral character is two.
    assert_eq!(tags::decode_double(run("return \"\u{1F600}\".length;")), 2.0);
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
    let produced = run(
        "let s = \"abc\"; let joined = \"\"; \
         for (let i = 0; i < s.length; i++) { joined = joined + s[i]; } \
         return joined === \"abc\";",
    );
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
    let produced = run(
        "let calls = 0; let a = [1]; \
         function k() { calls = calls + 1; return 0; } \
         a[k()]++; \
         return calls;",
    );
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
        tags::payload_of(run("function f() { return 1; } return (typeof f) === \"function\";")),
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
        tags::decode_double(run("function P(v) { this.v = v; } let p = new P(5); return p.v;")),
        5.0
    );
    assert_eq!(
        tags::decode_double(run("function P(a, b) { this.s = a + b; } return new P(1, 2).s;")),
        3.0
    );
}

#[test]
fn a_property_is_found_on_the_prototype() {
    // The read walks what the object inherits from. `cache_resolve` needed no
    // change for this: it already answered negative when the own layout misses,
    // which is exactly the inherited case, so such a read takes the slow path.
    assert_eq!(
        tags::decode_double(run("function P() {} P.prototype.m = 7; let p = new P(); return p.m;")),
        7.0
    );
    // An own property shadows an inherited one, and adding an unrelated own
    // property does not disturb the inherited one.
    assert_eq!(
        tags::decode_double(
            run("function P() {} P.prototype.m = 9; let p = new P(); p.m = 1; return p.m;")
        ),
        1.0
    );
    assert_eq!(
        tags::decode_double(
            run("function P() {} P.prototype.m = 9; let p = new P(); p.own = 1; return p.m;")
        ),
        9.0
    );
}

#[test]
fn a_constructor_that_returns_an_object_produces_that_one() {
    // The clause an implementation forgets. A factory written this way is
    // ordinary JavaScript rather than a corner.
    assert_eq!(
        tags::decode_double(run("function P() { return { a: 1 }; } let p = new P(); return p.a;")),
        1.0
    );
    // Returning anything that is NOT an object leaves the fresh one.
    assert_eq!(
        tags::decode_double(
            run("function P() { this.a = 2; return 9; } let p = new P(); return p.a;")
        ),
        2.0
    );
}

#[test]
fn instanceof_walks_the_chain() {
    assert_eq!(
        tags::payload_of(run("function P() {} let p = new P(); return p instanceof P;")),
        tags::BOOL_TRUE
    );
    assert_eq!(
        tags::payload_of(
            run("function P() {} function Q() {} let p = new P(); return p instanceof Q;")
        ),
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
        tags::payload_of(run("function P() {} let p = new P(); return (typeof p) === \"object\";")),
        tags::BOOL_TRUE
    );
}
