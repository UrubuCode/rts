//! `void x` — evaluates the operand (side effects run) and yields `undefined`.

use super::assert_stdout;

#[test]
fn void_yields_undefined() {
    assert_stdout("console.log(void 0);", "undefined\n");
    assert_stdout("console.log(void \"x\");", "undefined\n");
}

#[test]
fn typeof_void_is_undefined() {
    assert_stdout("console.log(typeof void 0);", "undefined\n");
}

#[test]
fn void_evaluates_operand_side_effect() {
    // The operand still runs (its side effect is observed); the value is dropped.
    assert_stdout(
        "let n = 0; function bump(): number { n = n + 1; return 9; } void bump(); console.log(n);",
        "1\n",
    );
}
