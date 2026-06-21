//! `&&` / `||` with non-boolean operands: JS returns one of the OPERANDS (not a
//! bool) with true short-circuit evaluation.

use super::assert_stdout;

#[test]
fn or_returns_truthy_operand() {
    assert_stdout(
        r#"console.log(0 || "x");
           console.log("" || "fallback");
           console.log(null || 42);"#,
        "x\nfallback\n42\n",
    );
}

#[test]
fn and_returns_operand() {
    assert_stdout(
        r#"console.log("a" && "b");
           console.log(5 && 0);"#,
        "b\n0\n",
    );
}

#[test]
fn proven_bool_still_bool() {
    // Both-bool effect-free operands keep the proven-Bool fast path.
    assert_stdout(
        r#"console.log(true && false);
           console.log(true || false);"#,
        "false\ntrue\n",
    );
}

#[test]
fn and_short_circuits_side_effect() {
    // `false && f()` must NOT evaluate `f()` (the call has a side effect).
    assert_stdout(
        r#"let n = 0;
           function side(): boolean { n = n + 1; return true; }
           const r = false && side();
           console.log(r, "n=" + n);"#,
        "false n=0\n",
    );
}

#[test]
fn or_short_circuits_side_effect() {
    // `true || f()` must NOT evaluate `f()`.
    assert_stdout(
        r#"let n = 0;
           function side(): boolean { n = n + 1; return true; }
           const r = true || side();
           console.log(r, "n=" + n);"#,
        "true n=0\n",
    );
}
