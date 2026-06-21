//! `switch` statement: dispatch, fall-through, default, string discriminant.

use super::assert_stdout;

#[test]
fn switch_basic_match_and_break() {
    assert_stdout(
        r#"let x = 2;
           switch (x) {
             case 1: console.log("um"); break;
             case 2: console.log("dois"); break;
             default: console.log("outro");
           }"#,
        "dois\n",
    );
}

#[test]
fn switch_fall_through() {
    // `case 1` has no break → falls through into `case 2`, which breaks.
    assert_stdout(
        r#"let x = 1;
           switch (x) {
             case 1: console.log("a");
             case 2: console.log("b"); break;
             case 3: console.log("c");
           }"#,
        "a\nb\n",
    );
}

#[test]
fn switch_default_no_match() {
    assert_stdout(
        r#"let x = 9;
           switch (x) {
             case 1: console.log("um"); break;
             default: console.log("def");
           }"#,
        "def\n",
    );
}

#[test]
fn switch_string_discriminant_with_return() {
    // String discriminant; each case `return`s (terminates without explicit break).
    assert_stdout(
        r#"function f(s: string): string {
             switch (s) {
               case "a": return "AA";
               case "b": return "BB";
               default: return "??";
             }
           }
           console.log(f("b"), f("x"));"#,
        "BB ??\n",
    );
}

#[test]
fn switch_no_default_no_match_is_noop() {
    // No matching case and no default → the whole switch is skipped.
    assert_stdout(
        r#"let x = 5;
           switch (x) {
             case 1: console.log("one"); break;
             case 2: console.log("two"); break;
           }
           console.log("after");"#,
        "after\n",
    );
}
