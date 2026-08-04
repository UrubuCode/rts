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
    compile(source)
        .unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"))
        .run()
}

#[test]
fn a_script_that_does_nothing_returns_undefined() {
    let compiled = compile("").expect("an empty script compiles");
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
    let compiled = compile("return 1 === 1;").expect("compiles");
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
    let error = compile("return 1 - 2;").expect_err("`-` has no runtime operation");
    assert!(
        format!("{error:?}").contains("Unsupported"),
        "expected a named refusal, got {error:?}"
    );
}
