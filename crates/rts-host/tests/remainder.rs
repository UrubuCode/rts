//! `%`, actually run — and the proof that proving it did not change it.
//!
//! A separate file from `running.rs` rather than appended to it: that one is
//! 4 494 lines, and rule 8 says new code lands in a small focused module rather
//! than growing something already large.
//!
//! # What these tests are for
//!
//! `%` on two proven doubles now reaches [`__rts_number_remainder`], a second
//! entry point that computes the same arithmetic as the generic one in a
//! different shape — unboxed both ways. Two things follow that a test has to
//! pin, and they pull in opposite directions:
//!
//! - the answers must be **identical** to the generic path's, in every case
//!   where the two differ in most languages: the sign of the result, a zero
//!   divisor, and negative zero;
//! - the site must actually **take** the new path, or the change is a second
//!   entry point nothing reaches.
//!
//! The first is what the bulk of this file is, because two implementations of
//! one operator is exactly the shape that drifts. The arithmetic is written
//! once — both entries end in the same `%` on two `f64` — and these say so from
//! the outside, where a reader can check it without taking that on trust.

use rts_cranelift::tags;
use rts_host::compile;

/// Runs a script and hands back the encoded word it produced.
fn run(source: &str) -> u64 {
    let mut program =
        compile(source).unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"));
    program.run()
}

/// Runs a script and reads the double it answered.
fn number(source: &str) -> f64 {
    tags::decode_double(run(source))
}

#[test]
fn the_remainder_takes_the_sign_of_the_dividend() {
    // The case that separates remainder from modulo, and the one most languages
    // answer differently. `-5 % 3` is `-2` in JavaScript, not `1`.
    //
    // Written through locals so the operands are PROVEN and the proven path is
    // what answers — a literal pair could be folded, and folding would make
    // this test pass without the entry point existing.
    assert_eq!(number("let a = -5; let b = 3; return a % b;"), -2.0);
    assert_eq!(number("let a = 5; let b = -3; return a % b;"), 2.0);
    assert_eq!(number("let a = -5; let b = -3; return a % b;"), -2.0);
}

#[test]
fn a_remainder_by_zero_is_not_a_number_rather_than_a_trap() {
    // The machine's integer remainder TRAPS on a zero divisor, which is why
    // `rts_cranelift`'s builder refuses one it cannot settle. This is the
    // language's own answer, and the reason the double path could never be
    // that instruction: a process that stops is not `NaN`.
    assert!(number("let a = 5; let b = 0; return a % b;").is_nan());
    assert!(number("let a = 0; let b = 0; return a % b;").is_nan());
}

#[test]
fn zero_divided_into_something_keeps_its_sign() {
    // `-0 % 5` is `-0`, and `-0` is not distinguishable from `0` by `===` — so
    // the test divides into it, which is the only way the language lets a
    // program tell the two apart.
    assert_eq!(
        number("let a = -0.0; let b = 5; return 1 / (a % b);"),
        f64::NEG_INFINITY
    );
    assert_eq!(
        number("let a = 0.0; let b = 5; return 1 / (a % b);"),
        f64::INFINITY
    );
}

#[test]
fn a_fractional_remainder_is_not_truncated() {
    // `%` is defined on the doubles themselves, not on their integer parts.
    assert_eq!(number("let a = 5.5; let b = 2; return a % b;"), 1.5);
    assert_eq!(number("let a = 5.5; let b = 2.5; return a % b;"), 0.5);
}

#[test]
fn an_infinite_dividend_is_not_a_number_and_an_infinite_divisor_is_the_dividend() {
    // The two directions answer differently, which is the asymmetry worth
    // pinning: `Infinity % 2` is `NaN`, but `2 % Infinity` is `2`.
    assert!(number("let a = 1 / 0; let b = 2; return a % b;").is_nan());
    assert_eq!(number("let a = 2; let b = 1 / 0; return a % b;"), 2.0);
}

#[test]
fn the_generic_path_still_answers_the_same_thing() {
    // The claim that the two entry points compute one operation. These operands
    // are strings, so nothing is proven and the GENERIC entry runs — the one
    // that coerces, consults the bigint path and answers a tagged value.
    //
    // If the two ever drift, this file is where it shows: the same three
    // programs above, written so the other implementation answers them.
    assert_eq!(number("let a = '-5'; let b = 3; return a % b;"), -2.0);
    assert_eq!(number("let a = 5; let b = '-3'; return a % b;"), 2.0);
    assert_eq!(number("let a = '5.5'; let b = '2'; return a % b;"), 1.5);
    assert!(number("let a = '5'; let b = 0; return a % b;").is_nan());
}

#[test]
fn a_local_reassigned_through_a_remainder_stays_proven() {
    // The whole reason for the change, stated as a behaviour rather than as a
    // shape: this is the LCG step every hash and every seeded generator is
    // written with, and before `%` entered the proven set the `%` made
    // `state` unprovable — which made the `*` and the `+` before it generic
    // too, and everything downstream of `state` after it.
    //
    // What it asserts is only the ANSWER. That the answer now comes from
    // instructions rather than from four runtime calls is not something a
    // running program can see, which is why it is measured separately and not
    // claimed here.
    let source = "
        let state = 1;
        let i = 0;
        while (i < 3) {
            state = (state * 1664525 + 1013904223) % 4294967296;
            i = i + 1;
        }
        return state;
    ";
    // 1 -> 1015568748 -> 1586005467 -> 2165703038, taken from Node and Bun
    // running this same program rather than worked out by hand.
    //
    // Not a stylistic preference: the hand-worked third step was written here
    // first and was WRONG, and the engine was right. Rule 6 says a test states
    // what the language means, and the only honest source for what this
    // recurrence means is a runtime that already implements it.
    assert_eq!(number(source), 2165703038.0);
}

/// The LCG step, which is what every seeded generator and every hash is
/// written with, and the shape the whole change was made for.
const LCG: &str = "
    function step(): number {
        let state = 1;
        let i = 0;
        while (i < 3) {
            state = (state * 1664525 + 1013904223) % 4294967296;
            i = i + 1;
        }
        return state;
    }
    step();
";

#[test]
fn a_proven_remainder_loop_reaches_no_generic_operator() {
    // The demonstration, as a SHAPE rather than as a time. A timing assertion
    // would be the wrong instrument here — it would be flaky on a loaded
    // machine, and it would pass for a build that got faster for an unrelated
    // reason. What the change actually claims is narrower and is decidable:
    // in this loop, no operator reaches the runtime's generic path any more.
    let ir = rts_host::describe::describe_source(LCG).expect("compiles");

    // The generic operators, which every one of these sites used to be. `%`
    // is the one that was never provable; the other three were provable in
    // principle and were not, because `state` had been poisoned by the `%`.
    for generic in [
        "__rts_remainder",
        "__rts_multiply",
        "__rts_add",
        "__rts_less",
    ] {
        assert!(
            !ir.contains(generic),
            "`{generic}` is still emitted for a loop whose every local is a \
             number with a literal initialiser. Before `%` entered the proven \
             set, all four were — one unprovable operator made the local \
             unprovable, and that made every operator reading it unprovable \
             too.\n\n{ir}"
        );
    }

    // And the remainder itself is no longer a call of ANY kind. This assertion
    // said the opposite when it was written — it required
    // `__rts_number_remainder` to be present, which was true and was the best
    // available then. `4294967296` is a power of two, and dividing by one of
    // those rounds nothing, so the machine has an exact five-instruction
    // sequence for it and takes that instead.
    assert!(
        !ir.contains("__rts_number_remainder"),
        "`% 2^k` is exact as instructions, so not even the unboxed call should \
         be left in this loop.\n\n{ir}"
    );
    assert!(
        ir.contains("Arith"),
        "the multiply and the add either side of it are instructions now.\n\n{ir}"
    );
}

#[test]
fn a_proven_remainder_by_an_odd_divisor_is_the_unboxed_call() {
    // The divisor decides, not the operator. This is the same loop with `% 7`,
    // where no exact instruction sequence exists — so the answer is the
    // unboxed entry point, which is still far better than the generic one:
    // no widening, no narrowing, no thrown-value check.
    let ir = rts_host::describe::describe_source(
        "
        function step(): number {
            let state = 1;
            let i = 0;
            while (i < 3) { state = (state * 31 + 7) % 7; i = i + 1; }
            return state;
        }
        step();
        ",
    )
    .expect("compiles");

    assert!(
        ir.contains("__rts_number_remainder"),
        "7 is not a power of two, so the sequence would be inexact and the \
         unboxed call is what is left.\n\n{ir}"
    );
    assert!(
        !ir.contains("__rts_remainder\n") && !ir.contains("__rts_remainder "),
        "but it must not fall all the way back to the GENERIC entry — the \
         operands are still proven.\n\n{ir}"
    );
}

#[test]
fn an_unproven_remainder_still_reaches_the_generic_operator() {
    // The other side of the same claim, and the one that says the proof is
    // applied where it holds rather than everywhere. A parameter is not
    // provable — `emit/proven.rs` refuses to prove anything arriving from
    // outside the function — so this `%` must still be the generic call.
    let ir = rts_host::describe::describe_source(
        "function f(a, b) { return a % b; } f(5, 3);",
    )
    .expect("compiles");

    assert!(
        ir.contains("__rts_remainder"),
        "operands nothing proved must still coerce, which is what the generic \
         entry is for.\n\n{ir}"
    );
}

#[test]
fn a_remainder_still_coerces_an_object_through_value_of() {
    // The generic entry can run user code, and that is the reason it exists.
    // Proving the operands is what makes the fast entry safe — so a test that
    // the slow one still calls back is what says the proof was not applied
    // where it does not hold.
    let source = "
        let calls = 0;
        const o = { valueOf() { calls = calls + 1; return 7; } };
        const r = o % 4;
        return r + calls;
    ";
    // 7 % 4 is 3, and `valueOf` ran once.
    assert_eq!(number(source), 4.0);
}
