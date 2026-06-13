//! Increment 4 proof: REAL `.ts` programs run end to end and print correctly.
//!
//! Each test feeds an ACTUAL TS source string to [`super::run_source`] — parse →
//! rts-hir → run-lowering (Tagged path) → whole-module JIT → execute — and
//! asserts the EXACT captured stdout against what Node/Bun would print. These are
//! the first programs the new engine runs to completion with output.
//!
//! Out-of-subset constructs bail with an explicit `Unsupported` (the negative
//! tests at the bottom), never a silent wrong value.

use super::run_source;

/// Run `src` and assert its captured stdout equals `expected`.
fn assert_stdout(src: &str, expected: &str) {
    match run_source(src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("run_source failed for:\n{src}\n  -> {e}"),
    }
}

// ===========================================================================
// Numeric + string `+` through the generic path.
// ===========================================================================

#[test]
fn add_numbers() {
    assert_stdout("console.log(1 + 2);", "3\n");
}

#[test]
fn concat_strings() {
    assert_stdout(r#"console.log("a" + "b");"#, "ab\n");
}

#[test]
fn number_plus_string() {
    assert_stdout(r#"console.log(1 + "x");"#, "1x\n");
}

// ===========================================================================
// typeof — multiple args, mixed kinds.
// ===========================================================================

#[test]
fn typeof_mixed() {
    assert_stdout(
        r#"console.log(typeof 1, typeof "s", typeof true);"#,
        "number string boolean\n",
    );
}

// ===========================================================================
// JS number formatting: fractional vs integer-valued.
// ===========================================================================

#[test]
fn float_formatting() {
    assert_stdout("console.log(1.5);", "1.5\n");
}

#[test]
fn integer_valued_float_formatting() {
    // 3.0 prints as "3" (no decimal) — the headline JS Number→String case.
    assert_stdout("console.log(3.0);", "3\n");
}

// ===========================================================================
// Function defs + cross-function calls.
// ===========================================================================

#[test]
fn single_function_call() {
    assert_stdout(
        "function sq(x: number){ return x*x; } console.log(sq(5));",
        "25\n",
    );
}

#[test]
fn cross_function_call_chain() {
    assert_stdout(
        r#"
        function inc(x: number){ return x + 1; }
        function dbl(x: number){ return x * 2; }
        console.log(dbl(inc(4)));
        "#,
        "10\n",
    );
}

// ===========================================================================
// A loop printing each iteration (top-level control flow + ToBoolean cond).
// ===========================================================================

#[test]
fn loop_printing() {
    assert_stdout(
        "let i = 0; while (i < 3) { console.log(i); i = i + 1; }",
        "0\n1\n2\n",
    );
}

// ===========================================================================
// Strict equality returning booleans.
// ===========================================================================

#[test]
fn strict_eq_booleans() {
    assert_stdout("console.log(true === true, 1 === 2);", "true false\n");
}

// ===========================================================================
// Extra coverage — combinations.
// ===========================================================================

#[test]
fn string_eq() {
    assert_stdout(r#"console.log("ab" === "ab", "a" === "b");"#, "true false\n");
}

#[test]
fn typeof_of_variable() {
    // typeof over a runtime value (not a literal) → runtime tag inspection.
    assert_stdout(
        r#"let s = "hi"; let n = 42; console.log(typeof s, typeof n);"#,
        "string number\n",
    );
}

#[test]
fn if_over_number_truthiness() {
    // `if (n)` with a number condition exercises inline ToBoolean.
    assert_stdout(
        "let n = 5; if (n) { console.log(\"yes\"); } else { console.log(\"no\"); }",
        "yes\n",
    );
}

#[test]
fn string_concat_in_loop() {
    assert_stdout(
        r#"
        let i = 0;
        while (i < 2) {
            console.log("row" + i);
            i = i + 1;
        }
        "#,
        "row0\nrow1\n",
    );
}

#[test]
fn multiple_log_lines() {
    assert_stdout(
        r#"console.log(1); console.log(2); console.log("three");"#,
        "1\n2\nthree\n",
    );
}

#[test]
fn negative_number_formatting() {
    assert_stdout("console.log(-0.0, -5, -2.5);", "0 -5 -2.5\n");
}

// ===========================================================================
// Negative: out-of-subset constructs bail EXPLICITLY (soundness floor).
// ===========================================================================

#[test]
fn object_literal_bails() {
    // Object literals are a later increment — must bail, not miscompile.
    let res = run_source("let o = { a: 1 }; console.log(o);");
    assert!(res.is_err(), "object literal must bail, got {res:?}");
}

#[test]
fn unknown_method_call_bails() {
    let res = run_source(r#"console.log("a".toUpperCase());"#);
    assert!(res.is_err(), "unknown method call must bail, got {res:?}");
}

#[test]
fn array_literal_bails() {
    let res = run_source("let a = [1, 2, 3]; console.log(a);");
    assert!(res.is_err(), "array literal must bail, got {res:?}");
}

// ---------------------------------------------------------------------------
// Soundness bails forced by HIR ambiguity (the engine REFUSES rather than guess).
// ---------------------------------------------------------------------------

#[test]
fn cross_kind_equality_bails() {
    // `0 == ""` is `true` loose / `false` strict; swc collapses `==`/`===` onto
    // one HIR op, so the engine cannot tell them apart for cross-kind operands.
    // It must bail, not emit a (possibly wrong) boolean.
    let res = run_source(r#"console.log(0 == "");"#);
    assert!(res.is_err(), "cross-kind equality must bail, got {res:?}");
}

#[test]
fn unary_plus_or_not_bails() {
    // swc lowers BOTH unary `+` and `!` to `HirUnOp::Not`; `+"42"` is `42` while
    // `!"42"` is `false`. Indistinguishable in HIR → bail.
    let res = run_source(r#"console.log(+"42");"#);
    assert!(res.is_err(), "unary +/! must bail, got {res:?}");
    let res2 = run_source("console.log(!0);");
    assert!(res2.is_err(), "unary +/! must bail, got {res2:?}");
}
