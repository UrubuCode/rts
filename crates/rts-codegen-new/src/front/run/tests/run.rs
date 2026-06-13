//! Core run tests: numeric/string `+`, `typeof`, JS number formatting,
//! cross-function calls, control flow, equality — plus the negative HIR-ambiguity
//! bails the engine REFUSES rather than guess.

use super::{assert_bails, assert_stdout, run_source};

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

/// Smoke the REAL-stdout path (`run_source`, NOT the capture path): it must run
/// to completion without SIGILL/crash, proving `__rtsadp_print_line` forwards to
/// the REAL `__RTS_FN_NS_IO_PRINT(ptr, len)` correctly (the line lands on the
/// test's stdout; this asserts only that the real IO_PRINT branch executes).
#[test]
fn run_source_real_stdout_smoke() {
    let res = run_source(r#"console.log("real-stdout-path", 1 + 2);"#);
    assert!(res.is_ok(), "run_source (real IO_PRINT path) failed: {res:?}");
}

// ===========================================================================
// Negative: out-of-subset constructs bail EXPLICITLY (soundness floor).
// ===========================================================================

#[test]
fn whole_object_log_now_inspects() {
    // P3.6 (intentional, justified change from the prior bail): printing a WHOLE
    // object value now renders `{ a: 1 }` (Node/util.inspect single-line) via the
    // slot-0 global shape-id key recovery (see `value/inspect.rs`).
    assert_stdout("let o = { a: 1 }; console.log(o);", "{ a: 1 }\n");
}

#[test]
fn whole_array_log_now_inspects() {
    // P3.5 (intentional, justified change from the prior bail): printing a whole
    // array value now renders Bun/Node's `util.inspect` form `[ 1, 2, 3 ]`
    // (see `value/inspect.rs` + the `inspect` test module). Objects render too.
    assert_stdout("let a = [1, 2, 3]; console.log(a);", "[ 1, 2, 3 ]\n");
}

// ---------------------------------------------------------------------------
// Soundness bails forced by HIR ambiguity (the engine REFUSES rather than guess).
// ---------------------------------------------------------------------------

#[test]
fn cross_kind_equality_bails() {
    // `0 == ""` is `true` loose / `false` strict; swc collapses `==`/`===` onto
    // one HIR op, so the engine cannot tell them apart for cross-kind operands.
    // It must bail, not emit a (possibly wrong) boolean.
    assert_bails(r#"console.log(0 == "");"#);
}

#[test]
fn unary_plus_or_not_bails() {
    // swc lowers BOTH unary `+` and `!` to `HirUnOp::Not`; `+"42"` is `42` while
    // `!"42"` is `false`. Indistinguishable in HIR → bail.
    assert_bails(r#"console.log(+"42");"#);
    assert_bails("console.log(!0);");
}
